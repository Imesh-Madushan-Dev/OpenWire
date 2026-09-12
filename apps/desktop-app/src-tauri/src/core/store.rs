//! Tiny JSON store used for settings and the server catalog.
//!
//! Writes are atomic (temp file + rename) so a crash mid-write can never leave a
//! truncated config behind - the previous good file survives.

use std::fs;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::error::{AppError, AppResult};

pub fn read_json<T: DeserializeOwned>(path: &Path) -> AppResult<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(value) => Ok(Some(value)),
            Err(err) => {
                // A corrupt or hand-broken file must not brick the app. Park it
                // and carry on with defaults.
                log::warn!(
                    "{} is not valid JSON ({err}); quarantining it",
                    path.display()
                );
                let _ = fs::rename(path, path.with_extension("json.bak"));
                Ok(None)
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(AppError::Config(format!("read {}: {err}", path.display()))),
    }
}

pub fn write_json<T: Serialize>(path: &Path, value: &T) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| AppError::Config(format!("create {}: {e}", parent.display())))?;
    }

    let body = serde_json::to_vec_pretty(value)
        .map_err(|e| AppError::Config(format!("serialize {}: {e}", path.display())))?;

    let tmp: PathBuf = path.with_extension("json.tmp");
    fs::write(&tmp, &body)
        .map_err(|e| AppError::Config(format!("write {}: {e}", tmp.display())))?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        AppError::Config(format!("commit {}: {e}", path.display()))
    })
}
