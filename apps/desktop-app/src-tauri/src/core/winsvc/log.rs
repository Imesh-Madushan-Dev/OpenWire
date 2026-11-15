//! A service's log file.
//!
//! A service has no console and no Tauri logger, so problems go to a small
//! file beside its binary. Program Files and not ProgramData on purpose: a
//! user can pre-create paths under ProgramData, and a SYSTEM process writing
//! wherever a user pointed it is a classic way to overwrite protected files.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Mutex, OnceLock};

use super::ServiceSpec;

struct FileLog;

static LOG: OnceLock<Mutex<File>> = OnceLock::new();
const LOG_LIMIT: u64 = 512 * 1024;

impl log::Log for FileLog {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let Some(file) = LOG.get() else { return };
        let Ok(mut file) = file.lock() else { return };

        // Truncated rather than rotated: this is for diagnosing the current
        // problem, not for keeping history.
        if file
            .metadata()
            .map(|meta| meta.len() > LOG_LIMIT)
            .unwrap_or(false)
        {
            let _ = file.set_len(0);
        }
        let _ = writeln!(
            file,
            "{} {:<5} {}",
            crate::core::usage::unix_seconds(),
            record.level(),
            record.args()
        );
    }

    fn flush(&self) {}
}

static LOGGER: FileLog = FileLog;

pub fn init(spec: &ServiceSpec) {
    let Some(path) = spec.log_path() else {
        return;
    };
    let Ok(file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    if LOG.set(Mutex::new(file)).is_ok() && log::set_logger(&LOGGER).is_ok() {
        log::set_max_level(log::LevelFilter::Info);
    }
}
