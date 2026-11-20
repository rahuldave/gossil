//! Check the preconditions for native repository creation.
//!
//! ## Recommendations
//! Keep the destination namespace stable throughout creation and engine shutdown.
use super::StorageError;
use std::path::{Path, PathBuf};

/// Refuse existing recovery entries before entering the native database engine.
/// # Arguments
/// * `path` - Literal native destination filename.
/// # Returns
/// Success after observing all three recovery entries absent.
/// # Errors
/// Retains the first observation error, or rejects an existing entry of any kind.
pub(super) fn check_sidecars(path: &Path) -> Result<(), StorageError> {
    for suffix in ["-journal", "-wal", "-shm"] {
        match std::fs::symlink_metadata(sidecar_path(path, suffix)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Ok(_) => {
                return Err(StorageError::state(format!(
                    "repository sidecar {suffix} exists; retained"
                )));
            }
            Err(error) => {
                return Err(StorageError::caused(
                    format!("repository sidecar {suffix} observation failed"),
                    error,
                ));
            }
        }
    }
    Ok(())
}

/// Append a recovery suffix to native filename bytes without URI interpretation.
/// # Arguments
/// * `path` - Native destination filename.
/// * `suffix` - SQLite recovery suffix.
/// # Returns
/// The literal neighboring recovery filename.
fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}
