//! Check the preconditions for native repository creation.
//!
//! ## Recommendations
//! Keep the destination namespace stable throughout creation and engine shutdown.
use super::{LocalOwnerRepository, StorageError};
use crate::storage::{Limits, WriteTransaction};
use libsql::Value;
use std::fs::Metadata;
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

/// These inputs initialize a native repository without an implicit checkout or preference database.
pub struct InitializeOptions<'a> {
    /// Forty lowercase hexadecimal project-code bytes.
    pub repository_id: &'a str,
    /// Stable local owner identity, never a remote credential.
    pub owner_id: &'a str,
    /// SQLite date expression, normalized for the native D card.
    pub date: &'a str,
    /// Explicit initial administrator and U-card bytes, valid nonempty UTF-8 without NUL.
    pub user: &'a [u8],
    /// Initial remote password without NUL, stored as the native project/login/password SHA1 digest.
    pub password: &'a str,
    /// Select SHA1 for the initial artifact and native hash policy; false uses SHA3-256.
    pub sha1: bool,
    /// Finite content reconstruction ceilings.
    pub content_limits: Limits,
    /// Finite artifact parser ceilings.
    pub parse_limits: crate::artifact::ParseLimits,
}

/// Conservatively clean up the exclusively created main file before possible engine entry.
pub(super) struct NewFileCleanup {
    /// Destination never followed during cleanup.
    path: PathBuf,
    /// Saved identity supports a no-follow observation under caller cooperation.
    identity: (u64, u64),
    /// Existing bound creation retains its historical failed-file behavior.
    armed: bool,
    /// A possible engine open cannot be certified disposed by the safe provider.
    engine_may_have_opened: bool,
    /// Distinguish precommit, ambiguous terminal attempt and known durable commit.
    phase: CommitPhase,
}
/// Durability phase is independent of generic operation errors or autocommit.
#[derive(Clone, Copy)]
enum CommitPhase {
    /// No terminal COMMIT has been attempted.
    Precommit,
    /// COMMIT may have run; outcome is not yet known.
    Attempted,
    /// COMMIT returned success with idle restoration.
    Committed,
}
impl NewFileCleanup {
    /// Retain metadata captured once from the exclusively opened descriptor.
    /// # Arguments
    /// * `path` - Exclusively created native main filename.
    /// * `meta` - Actual descriptor metadata captured before its close.
    /// * `armed` - Permit conservative failed-file cleanup for unbound creation when true.
    /// # Returns
    /// A precommit cleanup guard with no possible engine open recorded.
    pub(super) fn new(path: &Path, meta: &Metadata, armed: bool) -> Self {
        use std::os::unix::fs::MetadataExt;
        Self {
            path: path.into(),
            identity: (meta.dev(), meta.ino()),
            armed,
            engine_may_have_opened: false,
            phase: CommitPhase::Precommit,
        }
    }
    /// Return the operation result with a separately reported checked cleanup failure.
    /// # Arguments
    /// * `result` - Original initialization result, retained as the primary outcome.
    /// # Returns
    /// The original result, with a separate cleanup diagnostic appended to an armed failure.
    /// # Errors
    /// Preserves the operation error and records refusal or failure of conservative main-only cleanup.
    pub(super) fn finish<T>(mut self, result: Result<T, StorageError>) -> Result<T, StorageError> {
        let result = match result {
            Err(mut error) if self.armed => {
                if let Err(cleanup) = self.remove_created() {
                    error.cleanup = Some(match error.cleanup {
                        Some(prior) => format!("{prior}; {cleanup}"),
                        None => cleanup.to_string(),
                    });
                }
                Err(error)
            }
            other => other,
        };
        self.armed = false;
        result
    }
    /// Mark the conservative boundary before a possibly opening engine call.
    pub(super) fn engine_opening(&mut self) {
        self.engine_may_have_opened = true;
    }
    /// Mark an uncertain durability boundary before polling COMMIT.
    pub(super) fn commit_attempted(&mut self) {
        self.phase = CommitPhase::Attempted;
    }
    /// Preserve a successfully committed database even when final observation fails.
    pub(super) fn committed(&mut self) {
        self.phase = CommitPhase::Committed;
    }
    /// Remove only when every phase, disposal, identity and suffix prerequisite holds.
    /// # Returns
    /// Success when the owned pre-engine main was removed or was already absent with every prerequisite satisfied.
    /// # Errors
    /// Refuses sidecar entries, metadata failures, identity disagreement, possible engine opening or uncertain/known commit; returns removal IO errors.
    fn remove_created(&self) -> Result<(), StorageError> {
        use std::os::unix::fs::MetadataExt;
        // Inspect EVERY suffix even when main is absent or an earlier check fails.
        let mut sidecar_failures = Vec::new();
        for suffix in ["-journal", "-wal", "-shm"] {
            match cleanup_metadata(&sidecar_path(&self.path, suffix), Some(suffix)) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Ok(_) => {
                    sidecar_failures.push(format!("repository sidecar {suffix} exists; retained"))
                }
                Err(error) => sidecar_failures.push(format!(
                    "repository sidecar {suffix} observation failed: {error}"
                )),
            }
        }
        let sidecars = if sidecar_failures.is_empty() {
            Ok(())
        } else {
            Err(StorageError::state(sidecar_failures.join("; ")))
        };
        let main = cleanup_metadata(&self.path, None);
        let mut failures = Vec::new();
        if let Err(error) = sidecars {
            failures.push(error.to_string());
        }
        match self.phase {
            CommitPhase::Precommit => {}
            CommitPhase::Attempted => {
                failures.push("repository commit outcome uncertain; retained".into())
            }
            CommitPhase::Committed => {
                failures.push("repository committed; retained for explicit reopen".into())
            }
        }
        if self.engine_may_have_opened {
            failures.push(match self.phase {
                CommitPhase::Precommit => "actual engine disposal and rollback completion unconfirmed; repository retained".into(),
                CommitPhase::Attempted | CommitPhase::Committed => "actual engine disposal unconfirmed; repository retained".into(),
            });
        }
        let present = match main {
            Ok(meta)
                if crate::storage::FileIdentity::matches(
                    self.identity,
                    meta.is_file(),
                    (meta.dev(), meta.ino()),
                ) =>
            {
                true
            }
            Ok(_) => {
                failures.push("failed repository cleanup refused substituted destination".into());
                false
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => {
                failures.push(format!("failed repository cleanup metadata: {error}"));
                false
            }
        };
        if !failures.is_empty() {
            return Err(StorageError::state(failures.join("; ")));
        }
        if present {
            // This observation then unlink is deliberately not claimed atomic.
            cleanup_remove(&self.path)
                .map_err(|e| StorageError::caused("failed repository cleanup removal", e))?;
        }
        Ok(())
    }
}
impl Drop for NewFileCleanup {
    /// Apply the same conservative removal predicate best effort without reporting success.
    fn drop(&mut self) {
        if self.armed {
            let _ = self.remove_created();
        }
    }
}

/// Observe a cleanup entry without following it; finite tests can supply ordinary IO failures.
/// # Arguments
/// * `path` - Native entry whose metadata is needed for the conservative removal predicate.
/// * `suffix` - Recovery suffix, or `None` for the owned main.
/// # Returns
/// Native no-follow metadata.
/// # Errors
/// Retains the actual OS error or a scoped ordinary test IO error.
fn cleanup_metadata(path: &Path, suffix: Option<&str>) -> std::io::Result<Metadata> {
    #[cfg(test)]
    if let Some(code) = cleanup_injection::METADATA
        .with(|slot| *slot.borrow())
        .filter(|(selected, _)| *selected == suffix)
        .map(|(_, code)| code)
    {
        return Err(std::io::Error::from_raw_os_error(code));
    }
    #[cfg(not(test))]
    let _ = suffix;
    std::fs::symlink_metadata(path)
}
/// Remove the owned main after every conservative cleanup prerequisite succeeds.
/// # Arguments
/// * `path` - Owned and checked native main filename.
/// # Returns
/// Success after the main entry is removed.
/// # Errors
/// Retains the actual unlink error or a scoped ordinary test IO error.
fn cleanup_remove(path: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if let Some(code) = cleanup_injection::REMOVE.with(|slot| *slot.borrow()) {
        return Err(std::io::Error::from_raw_os_error(code));
    }
    std::fs::remove_file(path)
}
#[cfg(test)]
/// Scoped ordinary IO errors exercise caller cleanup decisions without namespace races.
mod cleanup_injection {
    use std::cell::RefCell;
    thread_local! {
        /// One selected cleanup metadata syscall returns this ordinary OS error.
        pub(super) static METADATA: RefCell<Option<(Option<&'static str>, i32)>> = const { RefCell::new(None) };
        /// The checked main unlink returns this ordinary OS error.
        pub(super) static REMOVE: RefCell<Option<i32>> = const { RefCell::new(None) };
    }
    /// Restore ordinary syscalls when the finite fixture ends.
    pub(super) struct Reset;
    impl Drop for Reset {
        /// Clear each scoped error even after an assertion fails.
        fn drop(&mut self) {
            METADATA.with(|slot| *slot.borrow_mut() = None);
            REMOVE.with(|slot| *slot.borrow_mut() = None);
        }
    }
}
/// Decode the actual owned native identity result without manufacturing missing values.
/// # Arguments
/// * `rows` - Fully materialized project-code/server-code query result.
/// # Returns
/// Exactly one pair of valid native identities.
/// # Errors
/// Rejects wrong row counts, typed cells, lengths and nonhexadecimal bytes.
fn decode_initialization_identity(rows: Vec<Vec<Value>>) -> Result<(String, String), StorageError> {
    match rows.as_slice() {
        [row] => match row.as_slice() {
            [Value::Text(project), Value::Text(server)]
                if [project, server].iter().all(|s| {
                    s.len() == 40
                        && s.bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                }) =>
            {
                Ok((project.clone(), server.clone()))
            }
            _ => Err(StorageError::state(
                "native initialization identity unavailable",
            )),
        },
        _ => Err(StorageError::state(
            "native initialization identity unavailable",
        )),
    }
}

impl WriteTransaction<'_> {
    /// Normalize DATETIME using Fossil checkin.c:date_in_standard_format SQL.
    /// # Arguments
    /// * `date` - Native SQLite DATETIME expression.
    /// # Returns
    /// The nonempty native strftime result for the root date card.
    /// # Errors
    /// Returns SQL or invalid/null date conversion failures and latches them on the owner.
    pub(super) async fn initial_date(&mut self, date: &str) -> Result<String, StorageError> {
        let result = async {
            let rows = self
                .query_bound(
                    "SELECT strftime('%Y-%m-%dT%H:%M:%f',?1)",
                    vec![Value::Text(date.into())],
                )
                .await?;
            match rows.first().map(Vec::as_slice) {
                Some([Value::Text(s)]) if rows.len() == 1 && !s.is_empty() => Ok(s.clone()),
                _ => Err(StorageError::state(format!(
                    "unrecognized date format ({date}): use \"YYYY-MM-DD HH:MM:SS.SSS\""
                ))),
            }
        }
        .await;
        self.record(result)
    }
    /// Install the implemented native defaults, administrator and hash policy.
    /// # Arguments
    /// * `options` - Explicit administrator/password and native SHA1 selection.
    /// # Returns
    /// Whether the explicitly selected native hash policy uses SHA1.
    /// # Errors
    /// Returns invalid administrator/password bytes or SQL failures and latches them on the owner.
    pub(super) async fn initialize_native_settings(
        &mut self,
        options: &InitializeOptions<'_>,
    ) -> Result<bool, StorageError> {
        let result=async {
        if options.user.contains(&0) || options.password.contains('\0') { return Err(StorageError::state("invalid native administrator or password")); }
        self.connection().execute_batch(include_str!("default-reports.sql")).await?;
        self.execute_bound("INSERT INTO config(name,value,mtime) VALUES('rebuilt',?1,unixepoch()),('ignore-glob','.git,*/.git,.git/*,*/.git/*',unixepoch())",vec![Value::Text(format!("gossil {}",env!("CARGO_PKG_VERSION")))]).await?;
        if options.sha1 { self.execute_bound("INSERT INTO config(name,value,mtime) VALUES('hash-policy','0',unixepoch())",vec![]).await?; }
        let mut secret=Vec::new();secret.extend_from_slice(options.repository_id.as_bytes());secret.push(b'/');secret.extend_from_slice(options.user);secret.push(b'/');secret.extend_from_slice(options.password.as_bytes());
        let secret=crate::content_hash::hex(&crate::content_hash::sha1(&secret));
        self.execute_bound("INSERT INTO user(login,pw,cap,info,mtime) VALUES(CAST(?1 AS TEXT),?2,'s','',unixepoch())",vec![Value::Blob(options.user.into()),Value::Text(secret)]).await?;
        self.execute_bound("INSERT OR IGNORE INTO user(login,pw,cap,info) VALUES('anonymous',hex(randomblob(8)),'hz','Anon'),('nobody','','gjorz','Nobody'),('developer','','ei','Dev'),('reader','','kptw','Reader')",vec![]).await?;
        Ok(options.sha1)
        }.await;
        self.record(result)
    }
}

impl LocalOwnerRepository {
    /// Read the actual native initialization identities after checking admission.
    /// # Returns
    /// Native project-code and server-code, in that order.
    /// # Examples
    /// ```no_run
    /// async fn identities(owner: &mut gossil::storage::LocalOwnerRepository) {
    ///     let (project, server) = owner.initialization_identity().await.unwrap();
    ///     assert_eq!(project.len(), 40);
    ///     assert_eq!(server.len(), 40);
    /// }
    /// ```
    /// # Errors
    /// Rejects stale admission, missing or malformed native identities and transaction errors.
    pub async fn initialization_identity(&mut self) -> Result<(String, String), StorageError> {
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission(&self.principal, &self.admission).await?;
            let rows = tx.query_bound("SELECT (SELECT value FROM config WHERE name='project-code'),(SELECT value FROM config WHERE name='server-code')", vec![]).await?;
            decode_initialization_identity(rows)
        }.await;
        let result = tx.record(result);
        tx.commit().await?;
        result
    }
}

#[cfg(test)]
/// Check conservative cleanup and native identity decoding with finite fixtures.
mod tests {
    use super::*;
    use std::fs::File;
    /// An unopened owned main is removable while the first operation error survives unchanged.
    #[test]
    fn cleanup_before_engine_removes_only_owned_main_and_preserves_first_error() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let file = File::create_new(&path).unwrap();
        let cleanup = NewFileCleanup::new(&path, &file.metadata().unwrap(), true);
        drop(file);
        let first = StorageError::caused("first failure", std::io::Error::from_raw_os_error(5));
        let error = cleanup.finish::<()>(Err(first)).unwrap_err();
        assert_eq!(error.operation, "first failure");
        assert!(std::error::Error::source(&error).is_some());
        assert!(error.cleanup.is_none());
        assert!(!path.exists());
    }
    /// Every recovery neighbor is checked and preserved during conservative cleanup refusal.
    #[test]
    fn cleanup_reports_all_sidecars_without_replacing_primary_failure() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let file = File::create_new(&path).unwrap();
        let cleanup = NewFileCleanup::new(&path, &file.metadata().unwrap(), true);
        drop(file);
        for suffix in ["-journal", "-wal", "-shm"] {
            std::fs::write(sidecar_path(&path, suffix), b"recovery").unwrap();
        }
        let error = cleanup
            .finish::<()>(Err(StorageError::state("first")))
            .unwrap_err();
        assert_eq!(error.operation, "first");
        let diagnostic = error.cleanup.unwrap();
        for suffix in ["-journal", "-wal", "-shm"] {
            assert!(diagnostic.contains(suffix));
            assert_eq!(
                std::fs::read(sidecar_path(&path, suffix)).unwrap(),
                b"recovery"
            );
        }
        assert!(path.exists());
    }
    /// Possible engine opening and attempted/successful commits each forbid main removal.
    #[test]
    fn cleanup_retains_possible_engine_and_commit_outcomes() {
        for phase in ["opening", "attempted", "committed"] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("repo");
            let file = File::create_new(&path).unwrap();
            let mut cleanup = NewFileCleanup::new(&path, &file.metadata().unwrap(), true);
            drop(file);
            match phase {
                "opening" => cleanup.engine_opening(),
                "attempted" => cleanup.commit_attempted(),
                _ => cleanup.committed(),
            }
            let error = cleanup
                .finish::<()>(Err(StorageError::state("first")))
                .unwrap_err();
            assert_eq!(error.operation, "first");
            assert!(error.cleanup.is_some());
            assert!(path.exists());
        }
    }
    /// Preserve both the first error and its prior cleanup diagnostic when a later cleanup fails.
    #[test]
    fn cleanup_appends_diagnostic_and_preserves_io_cause_on_remove_failure() {
        use std::error::Error;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let file = File::create_new(&path).unwrap();
        let cleanup = NewFileCleanup::new(&path, &file.metadata().unwrap(), true);
        drop(file);
        let _reset = cleanup_injection::Reset;
        cleanup_injection::REMOVE.with(|slot| *slot.borrow_mut() = Some(5));
        let mut first = StorageError::caused("first", std::io::Error::from_raw_os_error(13));
        first.cleanup = Some("prior cleanup".into());
        let error = cleanup.finish::<()>(Err(first)).unwrap_err();
        assert_eq!(error.operation, "first");
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .raw_os_error(),
            Some(13)
        );
        let diagnostic = error.cleanup.unwrap();
        assert!(diagnostic.starts_with("prior cleanup; "));
        assert!(diagnostic.contains("cleanup removal"));
        assert!(path.exists());
    }
    /// Ordinary metadata failures retain the main and never authorize a suffix sweep.
    #[test]
    fn cleanup_refuses_each_metadata_io_error_without_deleting_main() {
        for suffix in [None, Some("-journal"), Some("-wal"), Some("-shm")] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("repo");
            let file = File::create_new(&path).unwrap();
            let cleanup = NewFileCleanup::new(&path, &file.metadata().unwrap(), true);
            drop(file);
            let _reset = cleanup_injection::Reset;
            cleanup_injection::METADATA.with(|slot| *slot.borrow_mut() = Some((suffix, 5)));
            let error = cleanup
                .finish::<()>(Err(StorageError::state("first")))
                .unwrap_err();
            assert_eq!(error.operation, "first");
            let diagnostic = error.cleanup.unwrap();
            assert!(diagnostic.contains(if suffix.is_some() {
                "observation failed"
            } else {
                "cleanup metadata"
            }));
            assert!(path.exists());
        }
    }
    /// Identity disagreement refuses cleanup using two stable owned files.
    #[test]
    fn cleanup_refuses_other_descriptor_identity_and_accepts_absent_owned_main() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let file = File::create_new(&path).unwrap();
        let other = File::create_new(temp.path().join("other")).unwrap();
        let cleanup = NewFileCleanup::new(&path, &other.metadata().unwrap(), true);
        let error = cleanup
            .finish::<()>(Err(StorageError::state("first")))
            .unwrap_err();
        assert!(error.cleanup.unwrap().contains("substituted destination"));
        assert!(path.exists());
        let cleanup = NewFileCleanup::new(&path, &file.metadata().unwrap(), true);
        drop(file);
        std::fs::remove_file(&path).unwrap();
        let error = cleanup
            .finish::<()>(Err(StorageError::state("first")))
            .unwrap_err();
        assert!(error.cleanup.is_none());
        assert!(!path.exists());
    }
    /// Drop uses the same pre-engine removal predicate; engine/commit uncertainty always retains.
    #[test]
    fn cleanup_drop_and_engine_commit_phase_combinations_preserve_ownership() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let file = File::create_new(&path).unwrap();
        let cleanup = NewFileCleanup::new(&path, &file.metadata().unwrap(), true);
        drop(file);
        drop(cleanup);
        assert!(!path.exists());
        for committed in [false, true] {
            let file = File::create_new(&path).unwrap();
            let mut cleanup = NewFileCleanup::new(&path, &file.metadata().unwrap(), true);
            drop(file);
            cleanup.engine_opening();
            cleanup.commit_attempted();
            if committed {
                cleanup.committed();
            }
            let error = cleanup
                .finish::<()>(Err(StorageError::state("first")))
                .unwrap_err();
            let diagnostic = error.cleanup.unwrap();
            assert!(diagnostic.contains("actual engine disposal unconfirmed"));
            assert!(diagnostic.contains(if committed {
                "committed; retained"
            } else {
                "commit outcome uncertain"
            }));
            assert!(path.exists());
            std::fs::remove_file(&path).unwrap();
        }
    }
    /// Malformed owned native result rows cannot produce fabricated identities.
    #[test]
    fn initialization_identity_decoder_rejects_cardinality_types_and_encoding() {
        let valid = Value::Text("0123456789abcdef0123456789abcdef01234567".into());
        assert!(decode_initialization_identity(vec![vec![valid.clone(), valid.clone()]]).is_ok());
        for rows in [
            vec![],
            vec![
                vec![valid.clone(), valid.clone()],
                vec![valid.clone(), valid.clone()],
            ],
            vec![vec![]],
            vec![vec![valid.clone(), Value::Null]],
            vec![vec![valid.clone(), Value::Integer(40)]],
            vec![vec![valid.clone(), Value::Text("bad".into())]],
            vec![vec![valid.clone(), Value::Text("z".repeat(40))]],
        ] {
            assert_eq!(
                decode_initialization_identity(rows).unwrap_err().operation,
                "native initialization identity unavailable"
            );
        }
    }
}
