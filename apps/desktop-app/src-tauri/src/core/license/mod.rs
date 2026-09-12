//! Licensing.
//!
//! A key is a signed statement: "this machine, this person, until this date".
//! It is signed with an Ed25519 private key that exists only in `apps/key-gen`;
//! the app embeds nothing but the matching *public* key. So a key cannot be
//! forged or edited - changing any byte of the name, machine or expiry breaks
//! the signature - and there is no secret in this binary for a reverse engineer
//! to extract. A symmetric (HMAC) scheme would not have that property: its
//! verification secret would have to ship in the app.
//!
//! What no client-side check can do is stop someone patching the check out of
//! the binary. Enforcement sits here in native code, at the single point a
//! tunnel comes up, rather than in the webview - so bypassing it means patching
//! a stripped, LTO'd release build rather than editing JavaScript. That raises
//! the bar a long way; it does not make it impossible, and nothing here claims
//! otherwise.
//!
//! Key format: `MZY1.<payload>.<signature>`, both base64url without padding.
//! The signature covers `MZY1.<payload>` exactly as it appears in the key, so
//! there is no JSON canonicalisation to get wrong between the Node signer and
//! this verifier.

use std::path::Path;
use std::sync::OnceLock;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::core::store;

pub const KEY_PREFIX: &str = "MZY1";
const PAYLOAD_VERSION: u32 = 1;

/// Written by `apps/key-gen` (`npm run setup`). Public by design: it can only
/// check signatures, never make them.
const PUBLIC_KEY: &str = include_str!("../../../license-public.key");

const KEY_FILE: &str = "license.key";
const CLOCK_FILE: &str = "license-clock.json";

/// How far the clock may run backwards before it counts as tampering. Generous
/// on purpose: a timezone change or an NTP correction is not an attack.
const CLOCK_TOLERANCE_SECS: i64 = 48 * 3600;
/// The last-seen time is only rewritten when it has moved this far, so the
/// once-a-minute re-check does not turn into a once-a-minute disk write.
const CLOCK_WRITE_EVERY_SECS: i64 = 3600;

const DAY: i64 = 86_400;

const UNSUPPORTED: &str = "Licensing isn't available on this platform yet.";
const CLOCK_TAMPERED: &str =
    "Your system clock appears to have been set back. Correct the date and time, then try again.";

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// What the UI renders. Tagged by `state`, so TypeScript can narrow on it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum LicenseStatus {
    /// No key activated on this machine yet.
    #[serde(rename_all = "camelCase")]
    Unlicensed { machine_id: String },

    #[serde(rename_all = "camelCase")]
    Active {
        machine_id: String,
        name: String,
        issued_at: i64,
        expires_at: i64,
        days_left: i64,
    },

    /// Genuine, and for this machine, but past its date.
    #[serde(rename_all = "camelCase")]
    Expired {
        machine_id: String,
        name: String,
        expires_at: i64,
    },

    /// A stored key that no longer checks out, with a reason written for a person.
    #[serde(rename_all = "camelCase")]
    Invalid { machine_id: String, reason: String },

    /// No stable machine identifier on this platform.
    Unsupported,
}

/// A key that passed verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LicenseInfo {
    pub machine_id: String,
    pub name: String,
    pub issued_at: i64,
    pub expires_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyError {
    Malformed,
    BadSignature,
    WrongMachine,
}

impl KeyError {
    fn message(self) -> &'static str {
        match self {
            Self::Malformed => {
                "That doesn't look like a Maizy license key. Paste the whole key exactly as you received it."
            }
            // Deliberately vague: explaining *why* a signature failed helps
            // nobody but someone trying to make one pass.
            Self::BadSignature => "This license key isn't valid.",
            Self::WrongMachine => {
                "This key was issued for a different computer. Send your Machine ID to get a key for this one."
            }
        }
    }
}

#[derive(Deserialize)]
struct Payload {
    v: u32,
    mid: String,
    name: String,
    iat: i64,
    exp: i64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClockRecord {
    last_seen: i64,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// The license state of this machine right now.
pub fn status(config_dir: &Path, now: i64) -> LicenseStatus {
    let Some(machine_id) = machine_id() else {
        return LicenseStatus::Unsupported;
    };

    let Some(key) = std::fs::read_to_string(config_dir.join(KEY_FILE))
        .ok()
        .filter(|key| !key.trim().is_empty())
    else {
        return LicenseStatus::Unlicensed { machine_id };
    };

    evaluate(&key, &machine_id, config_dir, now)
}

/// Checks a pasted key and, only if it is valid for this machine today, stores
/// it. Returns a message written for a person on failure.
pub fn activate(config_dir: &Path, key: &str, now: i64) -> Result<LicenseStatus, String> {
    let Some(machine_id) = machine_id() else {
        return Err(UNSUPPORTED.to_string());
    };

    match evaluate(key, &machine_id, config_dir, now) {
        status @ LicenseStatus::Active { .. } => {
            let compact: String = key.chars().filter(|c| !c.is_whitespace()).collect();
            write_key(config_dir, &compact)
                .map_err(|_| "Couldn't save the license key to disk.".to_string())?;
            Ok(status)
        }
        LicenseStatus::Expired { .. } => {
            Err("This license key has already expired. Ask for a renewed key.".to_string())
        }
        LicenseStatus::Invalid { reason, .. } => Err(reason),
        LicenseStatus::Unlicensed { .. } => Err(KeyError::Malformed.message().to_string()),
        LicenseStatus::Unsupported => Err(UNSUPPORTED.to_string()),
    }
}

/// `Ok` only while an unexpired, genuine key for this machine is active.
pub fn require(config_dir: &Path, now: i64) -> Result<(), String> {
    match status(config_dir, now) {
        LicenseStatus::Active { .. } => Ok(()),
        LicenseStatus::Expired { .. } => Err(
            "Your Maizy license has expired. Enter a renewed key to keep using the VPN.".to_string(),
        ),
        LicenseStatus::Unlicensed { .. } => {
            Err("Activate Maizy with a license key before connecting.".to_string())
        }
        LicenseStatus::Invalid { reason, .. } => Err(reason),
        LicenseStatus::Unsupported => Err(UNSUPPORTED.to_string()),
    }
}

/// This computer's ID as shown to the user, e.g. `A1B2-C3D4-E5F6-0718-293A`.
pub fn machine_id() -> Option<String> {
    static ID: OnceLock<Option<String>> = OnceLock::new();
    ID.get_or_init(|| platform_machine_guid().map(|guid| derive_machine_id(&guid)))
        .clone()
}

// ---------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------

/// Verifies a key's signature and binding. Expiry is judged separately, so an
/// expired key can still say whose it was.
pub fn verify(key: &str, public_key: &[u8], machine_id: &str) -> Result<LicenseInfo, KeyError> {
    // Keys get pasted out of emails and chat apps, which wrap long lines.
    let compact: String = key.chars().filter(|c| !c.is_whitespace()).collect();

    let mut parts = compact.split('.');
    let (Some(prefix), Some(payload_b64), Some(signature_b64), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(KeyError::Malformed);
    };
    if prefix != KEY_PREFIX {
        return Err(KeyError::Malformed);
    }

    let signature = URL_SAFE_NO_PAD
        .decode(signature_b64)
        .map_err(|_| KeyError::Malformed)?;

    // Prefix and payload are ASCII by now (the prefix matched, and the split
    // point is a '.'), so this byte slice always lands on a char boundary.
    let signed = &compact[..prefix.len() + 1 + payload_b64.len()];

    // Authenticate before interpreting: nothing in the payload is trusted, or
    // even parsed, until the signature says the key generator wrote it.
    ring::signature::UnparsedPublicKey::new(&ring::signature::ED25519, public_key)
        .verify(signed.as_bytes(), &signature)
        .map_err(|_| KeyError::BadSignature)?;

    let bytes = URL_SAFE_NO_PAD
        .decode(payload_b64)
        .map_err(|_| KeyError::Malformed)?;
    let payload: Payload = serde_json::from_slice(&bytes).map_err(|_| KeyError::Malformed)?;

    if payload.v != PAYLOAD_VERSION || payload.name.trim().is_empty() || payload.exp <= payload.iat
    {
        return Err(KeyError::Malformed);
    }

    let issued_for = normalize_machine_id(&payload.mid).ok_or(KeyError::Malformed)?;
    if Some(issued_for.as_str()) != normalize_machine_id(machine_id).as_deref() {
        return Err(KeyError::WrongMachine);
    }

    Ok(LicenseInfo {
        machine_id: issued_for,
        name: payload.name.trim().to_string(),
        issued_at: payload.iat,
        expires_at: payload.exp,
    })
}

/// Active or expired, from a verified key and the current time.
pub fn classify(info: LicenseInfo, now: i64) -> LicenseStatus {
    if now >= info.expires_at {
        return LicenseStatus::Expired {
            machine_id: info.machine_id,
            name: info.name,
            expires_at: info.expires_at,
        };
    }

    // Rounded up: a key with three hours left is on its last day, not its zeroth.
    let days_left = (info.expires_at - now + DAY - 1) / DAY;

    LicenseStatus::Active {
        machine_id: info.machine_id,
        name: info.name,
        issued_at: info.issued_at,
        expires_at: info.expires_at,
        days_left,
    }
}

/// `A1B2C3D4e5f6-0718 293a` -> `A1B2-C3D4-E5F6-0718-293A`, or `None`.
///
/// Must agree with `normalizeMachineId` in `apps/key-gen/keygen.mjs`.
pub fn normalize_machine_id(raw: &str) -> Option<String> {
    let hex: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();

    if hex.len() != 20 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }

    Some(
        hex.as_bytes()
            .chunks(4)
            .map(|chunk| std::str::from_utf8(chunk).unwrap_or_default())
            .collect::<Vec<_>>()
            .join("-"),
    )
}

fn evaluate(key: &str, machine_id: &str, config_dir: &Path, now: i64) -> LicenseStatus {
    let info = match verify(key, &public_key(), machine_id) {
        Ok(info) => info,
        Err(err) => {
            return LicenseStatus::Invalid {
                machine_id: machine_id.to_string(),
                reason: err.message().to_string(),
            }
        }
    };

    if clock_rolled_back(config_dir, now) {
        return LicenseStatus::Invalid {
            machine_id: machine_id.to_string(),
            reason: CLOCK_TAMPERED.to_string(),
        };
    }

    classify(info, now)
}

fn public_key() -> Vec<u8> {
    // A missing or garbled key decodes to something ring rejects, which fails
    // every key closed rather than open. A test pins that it is well formed.
    URL_SAFE_NO_PAD.decode(PUBLIC_KEY.trim()).unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Clock rollback
// ---------------------------------------------------------------------------

/// Whether the clock has gone backwards past the tolerance since last seen.
///
/// Defence in depth, not a wall: the record is a local file, and deleting it
/// resets the check. It stops the casual "set the date back a month" trick,
/// which is the one people actually try.
fn clock_rolled_back(config_dir: &Path, now: i64) -> bool {
    let path = config_dir.join(CLOCK_FILE);
    let mut record: ClockRecord = store::read_json(&path).ok().flatten().unwrap_or_default();

    if rolled_back(record.last_seen, now) {
        return true;
    }

    if now >= record.last_seen + CLOCK_WRITE_EVERY_SECS {
        record.last_seen = now;
        // Best effort: failing to note the time must never lock a paying user out.
        let _ = store::write_json(&path, &record);
    }

    false
}

fn rolled_back(last_seen: i64, now: i64) -> bool {
    last_seen > 0 && now + CLOCK_TOLERANCE_SECS < last_seen
}

fn write_key(config_dir: &Path, key: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(config_dir)?;
    let path = config_dir.join(KEY_FILE);
    let tmp = path.with_extension("key.tmp");
    std::fs::write(&tmp, key)?;
    std::fs::rename(&tmp, &path)
}

// ---------------------------------------------------------------------------
// Machine identity
// ---------------------------------------------------------------------------

/// Hashed, so the raw OS identifier is never shown or sent anywhere; and
/// namespaced, so this machine's ID is unrelated to what any other product
/// derives from the same GUID. 80 bits: ample to tell customers' machines
/// apart, short enough to read out over a call.
fn derive_machine_id(guid: &str) -> String {
    let input = format!("maizy-vpn/machine-id/v1:{}", guid.trim().to_ascii_lowercase());
    let digest = ring::digest::digest(&ring::digest::SHA256, input.as_bytes());
    let hex: String = digest.as_ref()[..10]
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect();
    normalize_machine_id(&hex).unwrap_or(hex)
}

/// Windows' per-installation GUID. Readable without elevation, stable across
/// reboots and hardware changes; it changes only when Windows is reinstalled.
#[cfg(windows)]
fn platform_machine_guid() -> Option<String> {
    use winreg::enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY};
    use winreg::RegKey;

    // The 64-bit registry view explicitly: a 32-bit build would otherwise be
    // redirected to WOW6432Node, where MachineGuid does not exist.
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey_with_flags(r"SOFTWARE\Microsoft\Cryptography", KEY_READ | KEY_WOW64_64KEY)
        .and_then(|key| key.get_value::<String, _>("MachineGuid"))
        .ok()
        .filter(|guid| !guid.trim().is_empty())
}

#[cfg(target_os = "linux")]
fn platform_machine_guid() -> Option<String> {
    ["/etc/machine-id", "/var/lib/dbus/machine-id"]
        .iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
}

#[cfg(not(any(windows, target_os = "linux")))]
fn platform_machine_guid() -> Option<String> {
    // ponytail: IOPlatformUUID via IOKit on macOS. The product ships on Windows
    // first, and an unsupported platform is reported as such, never guessed.
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{Ed25519KeyPair, KeyPair};

    const MID: &str = "A1B2-C3D4-E5F6-0718-293A";
    const NOW: i64 = 1_800_000_000;

    fn keypair() -> Ed25519KeyPair {
        let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new()).unwrap();
        Ed25519KeyPair::from_pkcs8(document.as_ref()).unwrap()
    }

    fn payload(mid: &str, name: &str, iat: i64, exp: i64) -> String {
        serde_json::json!({ "v": 1, "mid": mid, "name": name, "iat": iat, "exp": exp }).to_string()
    }

    /// Signs exactly as `apps/key-gen/keygen.mjs` does: over `MZY1.<payload>`.
    fn issue(pair: &Ed25519KeyPair, payload: &str) -> String {
        let body = format!("{KEY_PREFIX}.{}", URL_SAFE_NO_PAD.encode(payload));
        let signature = pair.sign(body.as_bytes());
        format!("{body}.{}", URL_SAFE_NO_PAD.encode(signature.as_ref()))
    }

    fn thirty_day_key(pair: &Ed25519KeyPair, mid: &str) -> String {
        issue(pair, &payload(mid, "Jane Doe", NOW, NOW + 30 * DAY))
    }

    #[test]
    fn a_genuine_key_verifies_and_carries_its_details() {
        let pair = keypair();
        let info = verify(&thirty_day_key(&pair, MID), pair.public_key().as_ref(), MID).unwrap();

        assert_eq!(info.name, "Jane Doe");
        assert_eq!(info.machine_id, MID);
        assert_eq!(info.expires_at, NOW + 30 * DAY);
    }

    #[test]
    fn editing_any_part_of_the_payload_breaks_the_signature() {
        let pair = keypair();
        let genuine = thirty_day_key(&pair, MID);
        let signature = genuine.rsplit('.').next().unwrap();

        // Same signature, better terms: a longer expiry and someone else's name.
        let forged_body = URL_SAFE_NO_PAD.encode(payload(MID, "Mallory", NOW, NOW + 9999 * DAY));
        let forged = format!("{KEY_PREFIX}.{forged_body}.{signature}");

        assert_eq!(
            verify(&forged, pair.public_key().as_ref(), MID),
            Err(KeyError::BadSignature)
        );
    }

    #[test]
    fn a_key_from_any_other_signer_is_rejected() {
        let ours = keypair();
        let theirs = keypair();

        assert_eq!(
            verify(&thirty_day_key(&theirs, MID), ours.public_key().as_ref(), MID),
            Err(KeyError::BadSignature)
        );
    }

    #[test]
    fn a_key_is_useless_on_another_machine() {
        let pair = keypair();
        let key = thirty_day_key(&pair, MID);

        assert_eq!(
            verify(&key, pair.public_key().as_ref(), "FFFF-FFFF-FFFF-FFFF-FFFF"),
            Err(KeyError::WrongMachine)
        );
    }

    #[test]
    fn a_key_pasted_with_line_breaks_still_verifies() {
        let pair = keypair();
        let key = thirty_day_key(&pair, MID);
        let (head, tail) = key.split_at(key.len() / 2);
        let wrapped = format!("  {head}\r\n\t{tail}\n");

        assert!(verify(&wrapped, pair.public_key().as_ref(), MID).is_ok());
    }

    #[test]
    fn machine_ids_compare_regardless_of_case_and_dashes() {
        assert_eq!(normalize_machine_id("a1b2c3d4e5f60718293a").as_deref(), Some(MID));
        assert_eq!(normalize_machine_id(" A1B2-c3d4 E5F6-0718-293A ").as_deref(), Some(MID));
        assert_eq!(normalize_machine_id("A1B2-C3D4"), None, "too short");
        assert_eq!(normalize_machine_id("ZZZZ-C3D4-E5F6-0718-293A"), None, "not hex");
    }

    #[test]
    fn garbage_is_reported_as_malformed_never_a_panic() {
        let pair = keypair();
        let public_key = pair.public_key().as_ref().to_vec();

        for junk in ["", "MZY1", "MZY1.abc", "MZY1.a.b.c", "MZY2.eyJ9.AAAA", "MZY1.\u{e9}.\u{e9}"] {
            assert_eq!(verify(junk, &public_key, MID), Err(KeyError::Malformed), "{junk:?}");
        }
    }

    #[test]
    fn a_signed_but_nonsensical_payload_is_rejected() {
        let pair = keypair();
        let public_key = pair.public_key().as_ref();

        let expires_before_issue = issue(&pair, &payload(MID, "Jane", NOW, NOW - 1));
        let nameless = issue(&pair, &payload(MID, "   ", NOW, NOW + DAY));

        assert_eq!(verify(&expires_before_issue, public_key, MID), Err(KeyError::Malformed));
        assert_eq!(verify(&nameless, public_key, MID), Err(KeyError::Malformed));
    }

    #[test]
    fn expiry_and_days_left_follow_the_clock() {
        let info = LicenseInfo {
            machine_id: MID.to_string(),
            name: "Jane".to_string(),
            issued_at: NOW,
            expires_at: NOW + 30 * DAY,
        };

        assert!(matches!(
            classify(info.clone(), NOW),
            LicenseStatus::Active { days_left: 30, .. }
        ));
        // Three hours from the end is still a day left, not zero.
        assert!(matches!(
            classify(info.clone(), NOW + 30 * DAY - 3 * 3600),
            LicenseStatus::Active { days_left: 1, .. }
        ));
        assert!(matches!(
            classify(info, NOW + 30 * DAY),
            LicenseStatus::Expired { .. }
        ));
    }

    #[test]
    fn winding_the_clock_back_is_caught_beyond_the_tolerance() {
        let last_seen = NOW;

        assert!(!rolled_back(0, NOW), "a first run has nothing to compare against");
        assert!(!rolled_back(last_seen, NOW - 3600), "an hour back is a correction");
        assert!(!rolled_back(last_seen, NOW - CLOCK_TOLERANCE_SECS), "the tolerance itself");
        assert!(rolled_back(last_seen, NOW - 30 * DAY), "a month back is the trick");
    }

    /// A key minted by the real Node generator (`apps/key-gen`) with the real
    /// signing key, verified against the public key actually compiled into this
    /// build. The tests above prove the scheme; this one proves the two halves
    /// agree byte for byte - base64url flavour, exactly what is signed, field
    /// names. It is bound to a machine ID no real computer has, so it grants
    /// nothing, and `verify` does not judge dates, so its expiry is irrelevant.
    ///
    /// If the signing key is ever replaced (`npm run setup -- --force`), this
    /// fails by design. Mint a new fixture with:
    ///   npm start -- --machine 0123-4567-89AB-CDEF-0123 --name "Cross-language fixture" --days 1
    #[test]
    fn a_key_from_the_node_generator_verifies_against_the_embedded_public_key() {
        const FIXTURE: &str = "MZY1.eyJ2IjoxLCJtaWQiOiIwMTIzLTQ1NjctODlBQi1DREVGLTAxMjMiLCJuYW1lIjoiQ3Jvc3MtbGFuZ3VhZ2UgZml4dHVyZSIsImlhdCI6MTc4OTI0NDQwNiwiZXhwIjoxNzg5MzMwODA2fQ.Dy3EQcyB-x4j819rLyknnDcTi0LiSZAeV0rnCCegM_0L8NGN8tLNhdTwKCEAX6dznzqTJs8ItSbyUT3BTdCKAA";

        let info = verify(FIXTURE, &public_key(), "0123456789abcdef0123")
            .expect("the generator and the app must agree on the key format");

        assert_eq!(info.name, "Cross-language fixture");
        assert_eq!(info.machine_id, "0123-4567-89AB-CDEF-0123");
        assert!(info.expires_at > info.issued_at);

        // And the same key is still refused anywhere else.
        assert_eq!(
            verify(FIXTURE, &public_key(), "FFFF-FFFF-FFFF-FFFF-FFFF"),
            Err(KeyError::WrongMachine)
        );
    }

    /// Prints this computer's Machine ID, to issue a real key for local testing.
    /// Run with: cargo test print_machine_id -- --ignored --nocapture
    #[test]
    #[ignore]
    fn print_machine_id() {
        println!("\nMachine ID: {}\n", machine_id().expect("machine id"));
    }

    #[test]
    fn the_embedded_public_key_is_a_real_ed25519_key() {
        // Fails the build's tests if `npm run setup` was never run or the file
        // was damaged - otherwise every genuine key would be refused in the field.
        assert_eq!(public_key().len(), 32);
    }

    #[test]
    fn the_machine_id_is_stable_and_well_formed() {
        let derived = derive_machine_id("4C4C4544-0035-3010-8052-B4C04F333732");
        assert_eq!(derived, derive_machine_id("4c4c4544-0035-3010-8052-b4c04f333732 "));
        assert_eq!(normalize_machine_id(&derived).as_deref(), Some(derived.as_str()));

        #[cfg(windows)]
        {
            let live = machine_id().expect("Windows always has a MachineGuid");
            assert_eq!(normalize_machine_id(&live).as_deref(), Some(live.as_str()));
        }
    }
}
