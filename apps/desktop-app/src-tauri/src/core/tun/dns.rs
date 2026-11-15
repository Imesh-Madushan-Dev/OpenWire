//! DNS for everything on the adapter.
//!
//! Windows is told the adapter's DNS server is [`super::DNS_V4`], and the
//! engine catches every query sent to port 53, to that address or any other,
//! and hands it here. Queries go through the tunnel to a public resolver,
//! over TCP: every server carries TCP, while UDP support varies, and one
//! resolver seen from the server's side answers for the server's location.
//!
//! Queries share one TCP connection, pipelined (RFC 7766). Each gets a fresh
//! ID on the way out, so two apps that happen to pick the same ID cannot
//! receive each other's answers, and the original goes back on the reply.
//! Windows caches what comes back, so nothing is cached here.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::sync::{mpsc, oneshot};
use tokio::time::timeout;

use super::upstream;
use super::wire::{self, FlowRequest, Network};

/// How long one query may take before the asker is given up on. Windows
/// retries on its own well before its overall timeout.
const QUERY_TIMEOUT: Duration = Duration::from_secs(5);
/// Reaching a resolver crosses the server; past this, try the next one.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// An unused connection is closed rather than left for the resolver to drop.
const IDLE: Duration = Duration::from_secs(20);
/// Queries in flight at once; more than this and new ones are dropped, which
/// a resolver client treats as a timeout and retries.
const MAX_PENDING: usize = 512;
/// The smallest legal message: a header.
const HEADER: usize = 12;

#[derive(Clone)]
pub struct Forwarder {
    jobs: mpsc::Sender<Job>,
}

struct Job {
    query: Vec<u8>,
    reply: oneshot::Sender<Vec<u8>>,
}

impl Forwarder {
    /// `source` is what the flows to the resolver are attributed to.
    pub fn spawn(proxy: SocketAddr, source: SocketAddr, resolvers: Vec<SocketAddr>) -> Self {
        let (jobs, rx) = mpsc::channel(256);
        tokio::spawn(run(proxy, source, resolvers, rx));
        Self { jobs }
    }

    /// The answer to `query`, or `None` if none came in time.
    pub async fn resolve(&self, query: Vec<u8>) -> Option<Vec<u8>> {
        if query.len() < HEADER {
            return None;
        }
        let (reply, answer) = oneshot::channel();
        self.jobs.try_send(Job { query, reply }).ok()?;
        timeout(QUERY_TIMEOUT, answer).await.ok()?.ok()
    }
}

struct Pending {
    original_id: u16,
    reply: oneshot::Sender<Vec<u8>>,
    deadline: Instant,
}

struct Connection {
    writer: OwnedWriteHalf,
    /// Complete answers, read by a task of their own: a read cut short by
    /// `select!` would lose its place in the stream.
    answers: mpsc::Receiver<Vec<u8>>,
    last_used: Instant,
}

async fn run(
    proxy: SocketAddr,
    source: SocketAddr,
    resolvers: Vec<SocketAddr>,
    mut jobs: mpsc::Receiver<Job>,
) {
    if resolvers.is_empty() {
        return;
    }
    let mut resolver = 0usize;
    let mut connection: Option<Connection> = None;
    let mut pending: HashMap<u16, Pending> = HashMap::new();
    let mut next_id = initial_id();
    let mut ticker = tokio::time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            job = jobs.recv() => {
                let Some(Job { mut query, reply }) = job else { break };
                if pending.len() >= MAX_PENDING {
                    continue;
                }
                if connection.is_none() {
                    connection = connect(proxy, source, resolvers[resolver]).await;
                    if connection.is_none() {
                        log::warn!("resolver {} unreachable through the tunnel", resolvers[resolver]);
                        resolver = (resolver + 1) % resolvers.len();
                        continue;
                    }
                }
                let Some(conn) = connection.as_mut() else { continue };

                while pending.contains_key(&next_id) {
                    next_id = next_id.wrapping_add(1);
                }
                let id = next_id;
                next_id = next_id.wrapping_add(1);
                let original_id = u16::from_be_bytes([query[0], query[1]]);
                query[..2].copy_from_slice(&id.to_be_bytes());

                let Ok(frame) = wire::frame(&query) else { continue };
                if conn.writer.write_all(&frame).await.is_err() {
                    // Everything in flight on it is lost; the askers retry.
                    connection = None;
                    pending.clear();
                    resolver = (resolver + 1) % resolvers.len();
                    continue;
                }
                conn.last_used = Instant::now();
                pending.insert(id, Pending { original_id, reply, deadline: Instant::now() + QUERY_TIMEOUT });
            }
            answer = next_answer(&mut connection) => match answer {
                Some(mut answer) if answer.len() >= 2 => {
                    let id = u16::from_be_bytes([answer[0], answer[1]]);
                    if let Some(waiting) = pending.remove(&id) {
                        answer[..2].copy_from_slice(&waiting.original_id.to_be_bytes());
                        let _ = waiting.reply.send(answer);
                    }
                }
                Some(_) => {}
                None => {
                    // Closed under us - normal for an idle resolver connection.
                    // Only a close with questions outstanding counts against it.
                    if !pending.is_empty() {
                        resolver = (resolver + 1) % resolvers.len();
                    }
                    connection = None;
                    pending.clear();
                }
            },
            _ = ticker.tick() => {
                let now = Instant::now();
                pending.retain(|_, waiting| waiting.deadline > now);
                if pending.is_empty()
                    && connection.as_ref().is_some_and(|conn| conn.last_used.elapsed() > IDLE)
                {
                    connection = None;
                }
            }
        }
    }
}

async fn next_answer(connection: &mut Option<Connection>) -> Option<Vec<u8>> {
    match connection {
        Some(conn) => conn.answers.recv().await,
        None => std::future::pending().await,
    }
}

async fn connect(
    proxy: SocketAddr,
    source: SocketAddr,
    resolver: SocketAddr,
) -> Option<Connection> {
    let request = FlowRequest {
        network: Network::Tcp,
        source,
        destination: resolver,
    };
    let stream = timeout(CONNECT_TIMEOUT, upstream::open(proxy, request))
        .await
        .ok()?
        .ok()?;
    let (mut reader, writer) = stream.into_split();
    let (tx, answers) = mpsc::channel(64);
    tokio::spawn(async move {
        let mut buffer = Vec::new();
        while let Ok(true) = wire::read_datagram(&mut reader, &mut buffer).await {
            if tx.send(std::mem::take(&mut buffer)).await.is_err() {
                break;
            }
        }
    });
    Some(Connection {
        writer,
        answers,
        last_used: Instant::now(),
    })
}

fn initial_id() -> u16 {
    use ring::rand::SecureRandom;
    let mut bytes = [0u8; 2];
    let _ = ring::rand::SystemRandom::new().fill(&mut bytes);
    u16::from_le_bytes(bytes)
}

// ---------------------------------------------------------------------------
// Fitting a TCP answer into a UDP reply
// ---------------------------------------------------------------------------

/// The answer as it may go back over UDP. An answer bigger than the asker
/// said it can take over UDP is cut to its question with the truncated flag
/// set, which sends a resolver back to ask again over TCP - and TCP queries
/// to port 53 are caught and carried too.
pub fn fit_for_udp(query: &[u8], answer: Vec<u8>, ceiling: usize) -> Vec<u8> {
    let limit = udp_limit(query).min(ceiling);
    if answer.len() <= limit {
        return answer;
    }
    truncated(&answer).unwrap_or(answer)
}

/// What the asker accepts over UDP: 512 bytes, or what its EDNS record says.
fn udp_limit(query: &[u8]) -> usize {
    const CLASSIC: usize = 512;
    let Some(counts) = counts(query) else {
        return CLASSIC;
    };
    let mut at = HEADER;
    for _ in 0..counts[0] {
        let Some(next) = skip_name(query, at).map(|end| end + 4) else {
            return CLASSIC;
        };
        at = next;
    }
    // Answer and authority records do not belong in a query, but skip them
    // rather than misread the additional section if they are there.
    for _ in 0..(counts[1] as usize + counts[2] as usize) {
        let Some(next) = skip_record(query, at) else {
            return CLASSIC;
        };
        at = next;
    }
    for _ in 0..counts[3] {
        let Some(name_end) = skip_name(query, at) else {
            return CLASSIC;
        };
        let Some(fixed) = query.get(name_end..name_end + 10) else {
            return CLASSIC;
        };
        // An OPT record (type 41) carries the UDP size in its class field.
        if u16::from_be_bytes([fixed[0], fixed[1]]) == 41 {
            return (u16::from_be_bytes([fixed[2], fixed[3]]) as usize).max(CLASSIC);
        }
        let Some(next) = skip_record(query, at) else {
            return CLASSIC;
        };
        at = next;
    }
    CLASSIC
}

/// The header and question of `answer`, flagged as truncated.
fn truncated(answer: &[u8]) -> Option<Vec<u8>> {
    let questions = counts(answer)?[0];
    let mut end = HEADER;
    for _ in 0..questions {
        end = skip_name(answer, end)? + 4;
    }
    let mut cut = answer.get(..end)?.to_vec();
    cut[2] |= 0x02; // TC
    cut[6..12].fill(0); // no answer, authority or additional records
    Some(cut)
}

/// QDCOUNT, ANCOUNT, NSCOUNT, ARCOUNT.
fn counts(message: &[u8]) -> Option<[u16; 4]> {
    let header = message.get(..HEADER)?;
    let at = |i: usize| u16::from_be_bytes([header[i], header[i + 1]]);
    Some([at(4), at(6), at(8), at(10)])
}

/// The offset just past the name starting at `at`.
fn skip_name(message: &[u8], mut at: usize) -> Option<usize> {
    // Bounded: a name has at most 127 labels.
    for _ in 0..128 {
        let len = *message.get(at)? as usize;
        match len {
            0 => return Some(at + 1),
            // A compression pointer ends the name.
            l if l & 0xC0 == 0xC0 => return message.get(at + 1).map(|_| at + 2),
            l if l & 0xC0 == 0 => at += 1 + l,
            _ => return None,
        }
    }
    None
}

fn skip_record(message: &[u8], at: usize) -> Option<usize> {
    let name_end = skip_name(message, at)?;
    let fixed = message.get(name_end..name_end + 10)?;
    let data = u16::from_be_bytes([fixed[8], fixed[9]]) as usize;
    let end = name_end + 10 + data;
    (end <= message.len()).then_some(end)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A query for `example.com` A, with or without an EDNS record.
    fn query(edns: Option<u16>) -> Vec<u8> {
        let mut q = vec![
            0xAB,
            0xCD,
            0x01,
            0x00,
            0,
            1,
            0,
            0,
            0,
            0,
            0,
            edns.is_some() as u8,
        ];
        q.extend_from_slice(b"\x07example\x03com\x00\x00\x01\x00\x01");
        if let Some(size) = edns {
            q.extend_from_slice(&[0, 0, 41]);
            q.extend_from_slice(&size.to_be_bytes());
            q.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        }
        q
    }

    #[test]
    fn the_udp_limit_comes_from_edns_or_defaults_to_512() {
        assert_eq!(udp_limit(&query(None)), 512);
        assert_eq!(udp_limit(&query(Some(1232))), 1232);
        assert_eq!(
            udp_limit(&query(Some(100))),
            512,
            "never below the classic size"
        );
        assert_eq!(udp_limit(&[0; 5]), 512, "garbage reads as classic");
    }

    #[test]
    fn an_answer_too_big_for_udp_is_cut_to_its_question_and_flagged() {
        let mut answer = query(None);
        answer[2] |= 0x80; // QR
        answer[7] = 1; // one answer record follows
        answer.extend(std::iter::repeat_n(0u8, 600));

        let small = fit_for_udp(&query(Some(1232)), answer.clone(), 1452);
        assert_eq!(small, answer, "fits the asker's EDNS size");

        let cut = fit_for_udp(&query(None), answer.clone(), 1452);
        assert_eq!(cut.len(), 12 + 17, "header and question only");
        assert_eq!(cut[2] & 0x02, 0x02, "truncated flag set");
        assert_eq!(&cut[6..12], &[0; 6]);
        assert_eq!(&cut[..2], &[0xAB, 0xCD], "same id");

        let capped = fit_for_udp(&query(Some(4096)), answer, 300);
        assert_eq!(
            capped.len(),
            29,
            "the packet ceiling wins over a generous EDNS size"
        );
    }

    #[test]
    fn names_are_walked_safely() {
        assert_eq!(skip_name(b"\x03abc\x00", 0), Some(5));
        assert_eq!(
            skip_name(b"\x03abc\xC0\x0C", 0),
            Some(6),
            "pointer ends the name"
        );
        assert_eq!(skip_name(b"\x03ab", 0), None, "truncated");
        assert_eq!(skip_name(b"\x80", 0), None, "reserved label type");
    }

    /// End to end against a fake proxy that is also the resolver: IDs are
    /// rewritten on the way out and restored on the way back.
    #[tokio::test]
    async fn queries_are_pipelined_with_their_ids_restored() {
        use tokio::io::AsyncReadExt;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut greeting = [0u8; 3];
            stream.read_exact(&mut greeting).await.unwrap();
            assert_eq!(greeting, [0x05, 0x01, wire::METHOD]);
            let request = FlowRequest::read(&mut stream).await.unwrap();
            assert_eq!(request.network, Network::Tcp);
            assert_eq!(request.destination, "1.1.1.1:53".parse().unwrap());
            stream.write_all(&[0x05, wire::METHOD]).await.unwrap();
            stream
                .write_all(&wire::status_reply(wire::Status::Ok))
                .await
                .unwrap();

            // Echo every query back as its own answer, upper-casing nothing:
            // the ID is all that matters here.
            let mut buffer = Vec::new();
            while let Ok(true) = wire::read_datagram(&mut stream, &mut buffer).await {
                wire::write_datagram(&mut stream, &buffer).await.unwrap();
            }
        });

        let forwarder = Forwarder::spawn(
            proxy,
            "172.19.0.2:53".parse().unwrap(),
            vec!["1.1.1.1:53".parse().unwrap()],
        );
        let mut a = query(None);
        let mut b = query(None);
        a[..2].copy_from_slice(&[0x11, 0x11]);
        b[..2].copy_from_slice(&[0x11, 0x11]); // the same ID from two askers

        let (answer_a, answer_b) =
            tokio::join!(forwarder.resolve(a.clone()), forwarder.resolve(b.clone()));
        assert_eq!(answer_a.unwrap(), a);
        assert_eq!(answer_b.unwrap(), b);
        assert_eq!(
            forwarder.resolve(vec![1, 2, 3]).await,
            None,
            "too short to be DNS"
        );
    }
}
