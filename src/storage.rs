//! Sole-connection local storage. File opens verify WAL, foreign keys, FULL and 5000 ms.
//! Unexpected owner destruction poisons the store and closes its connection.
//! Local SQL blocks its current poll; cancellation after COMMIT cannot undo it.
//!
//! The core-only native provider has a reviewed date backport. Its source identity
//! is recorded in `evidence/libsql-provider/provenance.json`. Native repository
//! creation and admission parse the empty root on this same connection. General
//! artifact crosslinks, runtime clocks and broader date compatibility remain unqualified.
//! Catalog6 supports native initialization, one registration and exact private recorded history; bound catalog1 retains private writes.
//! A marker of an older Gossil catalog (2, 4 or 5) is refused before any effect, without migration.
//!
//! ## Recommendations
//! Use one owner for a complete mutation and explicitly commit or roll back.
//! Use LocalOwnerRepository for admitted native content and explicit finite limits.
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the local record readers that use the codec are not built yet"
    )
)]
mod command_codec;
mod content;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the local record readers that meter queries are not built yet"
    )
)]
mod metered_query;
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the file operations that use these limits are not built yet"
    )
)]
mod operation_limits;
mod policy;
mod repository;
mod schema;
pub use content::Limits;
pub use repository::{
    AdmissionLimits, CheckoutView, CreateOptions, InitializeOptions, LocalOwnerRepository,
    PathScope, PathScopeSelection, PendingAdd, PrivateRecordingOutcome, PrivateRecordingRequest,
    RecordedCheckin, SharedView, TrackingLimits,
};
mod lifecycle;
use lifecycle::FileIdentity;

use libsql::{Builder, Connection, Value};
use std::{fmt, path::Path, sync::Arc, time::Duration};

/// An operation failure, with a separate checked-cleanup failure when present.
#[derive(Debug, Clone)]
pub struct StorageError {
    /// First operation or state failure.
    pub operation: String,
    /// Rollback failure; absence does not establish power-loss durability.
    pub cleanup: Option<String>,
    /// Original input or engine failure retained for error-chain inspection.
    source: Option<Arc<dyn std::error::Error + Send + Sync>>,
}
impl fmt::Display for StorageError {
    /// Format the operation failure and any separate cleanup failure.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.operation)?;
        if let Some(cleanup) = &self.cleanup {
            write!(f, "; cleanup: {cleanup}")?;
        }
        Ok(())
    }
}
impl std::error::Error for StorageError {
    /// Expose the original input or engine error when one is available.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref().map(|error| error as _)
    }
}
impl From<libsql::Error> for StorageError {
    /// Preserve the original engine error and its diagnostic.
    fn from(error: libsql::Error) -> Self {
        Self {
            operation: error.to_string(),
            cleanup: None,
            source: Some(Arc::new(error)),
        }
    }
}
impl StorageError {
    /// Preserve a non-SQL failure as the original operation cause.
    /// # Arguments
    /// * `message` - Context describing the failed operation.
    /// * `error` - Underlying error to retain.
    /// # Returns
    /// A storage error retaining the original cause.
    fn caused(
        message: impl Into<String>,
        error: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            operation: message.into(),
            cleanup: None,
            source: Some(Arc::new(error)),
        }
    }
    /// Construct a state error without a cleanup failure.
    /// # Arguments
    /// * `message` - Explanation of the failed ownership or configuration check.
    /// # Returns
    /// An error without an engine source.
    fn state(message: impl Into<String>) -> Self {
        Self {
            operation: message.into(),
            cleanup: None,
            source: None,
        }
    }
    /// Refuse a repository whose marker names an older Gossil catalog (CAT-07).
    /// # Arguments
    /// * `found` - The older catalog that the one stored marker names.
    /// # Returns
    /// An error whose source is [`OlderCatalog`], so that the CLI can exit 78.
    fn older_catalog(found: i64) -> Self {
        let older = OlderCatalog { found };
        Self {
            operation: older.to_string(),
            cleanup: None,
            source: Some(Arc::new(older)),
        }
    }
    /// Name the repository in an older-catalog refusal, with the full CAT-07 message.
    /// # Arguments
    /// * `repository` - The repository path that the caller selected.
    /// # Returns
    /// The same error. An older-catalog refusal gets the three lines of CAT-07.
    fn naming_repository(mut self, repository: &Path) -> Self {
        if let Some(older) = self.older() {
            self.operation = format!(
                "gossil: repository {} is at catalog {}, and this build needs catalog {}.\n\
                 Gossil has no upgrade path before its first real release.\n\
                 Create a new repository with gos v init or gos i init.",
                repository.display(),
                older.found,
                schema::NEWEST_CATALOG
            );
        }
        self
    }
    /// Return the older-catalog refusal that this error carries, if any.
    /// # Returns
    /// The refusal of CAT-07, when the stored marker named an older Gossil catalog.
    fn older(&self) -> Option<OlderCatalog> {
        self.source
            .as_deref()
            .and_then(|source| source.downcast_ref::<OlderCatalog>())
            .copied()
    }
}

/// Reports a repository at a Gossil catalog older than the newest catalog of this build.
///
/// Gossil has no upgrade path before its first real release (CAT-05), so every
/// command refuses such a repository before any effect, and the CLI exits 78.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OlderCatalog {
    /// The catalog that the one stored marker names.
    pub(crate) found: i64,
}
impl fmt::Display for OlderCatalog {
    /// Name the catalog found and the catalog that this build needs.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "repository is at catalog {}, and this build needs catalog {}",
            self.found,
            schema::NEWEST_CATALOG
        )
    }
}
impl std::error::Error for OlderCatalog {}

/// A checked commit failure retains separate operation and cleanup causes.
/// A cleanup failure means the uncertain connection was discarded and its store poisoned.
#[derive(Debug)]
pub struct CommitFailure {
    /// The original failure comes from a latched operation, COMMIT or idle restoration.
    pub operation: StorageError,
    /// A cleanup failure comes from an independent rollback or idle-restoration error.
    pub cleanup: Option<StorageError>,
}

/// Open the selected native provider while preserving non-UTF-8 Unix filename bytes.
/// SQLite URI percent encoding bridges the Rust driver's UTF-8 filename boundary.
/// It does not change repository bytes or introduce another storage engine.
/// # Arguments
/// * `path` - Native filesystem name, rejected if it contains NUL.
/// * `flags` - Existing native read/write/create flags.
/// # Returns
/// The configured database factory for that exact filename.
/// # Errors
/// Returns filename, current-directory or original libsql open failures.
pub(crate) async fn open_native_database(
    path: &Path,
    flags: libsql::OpenFlags,
) -> Result<libsql::Database, StorageError> {
    std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|e| StorageError::caused("invalid local database path", e))?;
    if path.as_os_str().is_empty() {
        return Ok(Builder::new_local(path).flags(flags).build().await?);
    }
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| StorageError::caused("local database current directory unavailable", e))?
            .join(path)
    };
    // Native command paths are filesystem names. An absolute filename cannot
    // accidentally activate SQLite's compiled-in `file:` URI interpretation.
    if absolute.to_str().is_some() {
        return Ok(Builder::new_local(&absolute).flags(flags).build().await?);
    }
    // An explicit empty authority preserves even a native leading double slash.
    let mut uri = String::from("file://");
    for byte in absolute.as_os_str().as_encoded_bytes() {
        use std::fmt::Write;
        if byte.is_ascii_alphanumeric() || b"/-._~".contains(byte) {
            uri.push(*byte as char);
        } else {
            write!(&mut uri, "%{byte:02X}").unwrap();
        }
    }
    // SQLITE_OPEN_URI, from the pinned provider's sqlite3.h. Preserve all selected flags.
    let flags = libsql::OpenFlags::from_bits_retain(flags.bits() | 0x00000040);
    Ok(Builder::new_local(uri).flags(flags).build().await?)
}

/// A configured local database with exactly one unexposed connection.
pub struct LocalStore {
    /// Sole connection, absent while a mutation owns it or after poisoning.
    connection: Option<Connection>,
    /// Whether an abnormal exit prevents reuse.
    poisoned: bool,
    /// Cooperative pathname expectation for repository constructors.
    identity: Option<FileIdentity>,
}
impl Drop for LocalStore {
    /// Observe before releasing the safe provider; this does not certify engine disposal.
    fn drop(&mut self) {
        if let Some(identity) = &self.identity {
            let _ = identity.observe();
        }
        drop(self.connection.take());
    }
}
impl LocalStore {
    /// Open and verify a local file database; never fall back to memory.
    /// Nonempty paths are literal native filenames, including `file:` and URI punctuation.
    /// Raw Unix filename bytes remain subject to the filesystem's acceptance rules.
    /// # Arguments
    /// * `path` - Local database file, whose parent must exist.
    /// # Returns
    /// A store with one verified idle connection.
    /// # Errors
    /// Returns an error for invalid input, open, configuration or verification failure.
    /// An embedded NUL returns an error with a [`std::ffi::NulError`] source.
    /// Driver failures retain their original [`libsql::Error`] source.
    /// # Examples
    /// ```no_run
    /// # async fn lifecycle(path: &std::path::Path) -> Result<(), gossil::storage::StorageError> {
    /// use gossil::storage::LocalStore;
    /// let mut store = LocalStore::open(path).await?;
    /// assert!(!store.is_poisoned());
    /// store.begin_write().await?.commit().await?;
    /// store.begin_write().await?.rollback().await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        Self::open_inner(path, true).await
    }
    /// Construct and verify one retained connection.
    /// # Arguments
    /// * `path` - Database path, or the private in-memory fixture name.
    /// * `file` - Require WAL for files; fixtures instead verify memory mode.
    /// # Returns
    /// A configured store or the opening or verification error.
    /// # Errors
    /// Returns an input error for an embedded NUL, engine errors for opening or
    /// configuration failures, or a state error when required settings differ.
    async fn open_inner(path: impl AsRef<Path>, file: bool) -> Result<Self, StorageError> {
        Self::open_observed(path.as_ref(), file, None).await
    }
    /// Open with cooperative regular-file observations before and after engine entry.
    /// # Arguments
    /// * `path` - Literal database filename.
    /// * `file` - Require file WAL configuration.
    /// * `identity` - Previously captured regular-file identity.
    /// # Returns
    /// A configured store retaining the optional identity.
    /// # Errors
    /// Retains pathname, configuration and engine failures.
    async fn open_observed(
        path: &Path,
        file: bool,
        identity: Option<FileIdentity>,
    ) -> Result<Self, StorageError> {
        if let Some(expected) = &identity {
            expected.observe()?;
        }
        let database = if file {
            open_native_database(path, libsql::OpenFlags::default()).await?
        } else {
            // The pinned driver unwraps CString construction at connect.
            std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).map_err(|error| {
                StorageError {
                    operation: format!("invalid local database path: {error}"),
                    cleanup: None,
                    source: Some(Arc::new(error)),
                }
            })?;
            Builder::new_local(path).build().await?
        };
        let connection = database.connect()?;
        connection.busy_timeout(Duration::from_millis(5000))?;
        connection.execute("PRAGMA foreign_keys=ON", ()).await?;
        connection.execute("PRAGMA synchronous=FULL", ()).await?;
        let journal = if file { "wal" } else { "memory" };
        if file {
            materialize(&connection, "PRAGMA journal_mode=WAL").await?;
        }
        for (sql, expected) in [
            ("PRAGMA journal_mode", Value::Text(journal.into())),
            ("PRAGMA foreign_keys", Value::Integer(1)),
            ("PRAGMA synchronous", Value::Integer(2)),
            ("PRAGMA busy_timeout", Value::Integer(5000)),
        ] {
            if materialize(&connection, sql).await? != vec![vec![expected]] {
                return Err(StorageError::state(format!(
                    "configuration disagreement: {sql}"
                )));
            }
        }
        if let Some(expected) = &identity {
            expected.observe()?;
        }
        Ok(Self {
            connection: Some(connection),
            poisoned: false,
            identity,
        })
    }
    /// Report whether unexpected destruction or failed cleanup prevents reuse.
    /// # Returns
    /// True when reconstruction is required.
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    /// Begin IMMEDIATE before returning the uniquely borrowed mutation owner.
    /// # Returns
    /// The sole write owner after SQLite acquired its write reservation.
    /// # Errors
    /// Returns an error for a poisoned store or failed BEGIN; failed BEGIN poisons it.
    /// # Examples
    /// A second owner cannot borrow the store while the first owner remains active.
    /// ```compile_fail
    /// async fn overlapping(store: &mut gossil::storage::LocalStore) {
    ///     let first = store.begin_write().await.unwrap();
    ///     let second = store.begin_write().await.unwrap();
    ///     first.rollback().await.unwrap();
    /// }
    /// ```
    pub async fn begin_write(&mut self) -> Result<WriteTransaction<'_>, StorageError> {
        let connection = self
            .connection
            .take()
            .ok_or_else(|| StorageError::state("store poisoned"))?;
        let mut owner = WriteTransaction {
            store: self,
            connection: Some(connection),
            failure: None,
            #[cfg(test)]
            fail_control: None,
        };
        owner.execute("BEGIN IMMEDIATE").await?;
        Ok(owner)
    }
}
/// Own an exclusive write boundary without exposing a connection, statement or lazy result.
///
/// # Examples
/// The raw connection and arbitrary SQL helpers are private to this module.
/// ```compile_fail
/// fn raw_alias(owner: &gossil::storage::WriteTransaction<'_>) {
///     let alias = owner.connection();
/// }
/// ```
/// ```compile_fail
/// async fn arbitrary_sql(owner: &mut gossil::storage::WriteTransaction<'_>) {
///     owner.execute("DELETE FROM tasks").await.unwrap();
/// }
/// ```
pub struct WriteTransaction<'a> {
    /// Exclusively borrowed destination for a confirmed idle connection.
    store: &'a mut LocalStore,
    /// Sole connection until explicit checked termination.
    connection: Option<Connection>,
    /// First failure, retained even when an internal caller catches its result.
    failure: Option<StorageError>,
    #[cfg(test)]
    /// Narrow control-result failure seam; it does not simulate disk failure.
    fail_control: Option<&'static str>,
}
impl WriteTransaction<'_> {
    /// Read the sole connection without exposing it outside this module.
    /// # Returns
    /// The armed owner's connection; terminal methods never call this after restoration.
    /// # Panics
    /// Panics if an internal caller uses the owner after its connection was restored.
    fn connection(&self) -> &Connection {
        self.connection.as_ref().expect("armed owner")
    }
    /// Latch the first operation failure before returning it.
    /// # Arguments
    /// * `result` - Completed execution, stepping or decoding result.
    /// # Returns
    /// The unchanged result after retaining its first error.
    /// # Errors
    /// Returns the supplied error unchanged.
    fn record<T>(&mut self, result: Result<T, StorageError>) -> Result<T, StorageError> {
        if let Err(error) = &result {
            self.failure.get_or_insert_with(|| error.clone());
        }
        result
    }
    /// Verify a full native content name before any storage statement.
    /// # Arguments
    /// * `name` - Full native content hash to verify.
    /// * `bytes` - Input bytes for this operation.
    /// # Returns
    /// Success when the bytes match their full native name.
    /// # Errors
    /// Rejects unsupported names, hash mismatch or a prior latched failure.
    fn verify_content_name(&mut self, name: &str, bytes: &[u8]) -> Result<(), StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = (|| {
            let digest = match name.len() {
                40 => crate::content_hash::hex(&crate::content_hash::sha1(bytes)),
                64 => crate::content_hash::hex(&crate::content_hash::sha3(bytes, 256).unwrap()),
                _ => return Err(StorageError::state("unsupported native content name")),
            };
            if name != digest {
                return Err(StorageError::state("native content hash mismatch"));
            }
            Ok(())
        })();
        self.record(result)
    }
    /// Execute one internal statement; a caught failure still prevents further SQL.
    /// # Arguments
    /// * `sql` - Fixed internal SQL or private fixture SQL; no caller-supplied transaction control.
    /// # Returns
    /// Success after execution, or the latched failure.
    /// # Errors
    /// Returns the first latched error or the current engine execution error.
    async fn execute(&mut self, sql: &str) -> Result<(), StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = self
            .connection()
            .execute(sql, ())
            .await
            .map(|_| ())
            .map_err(Into::into);
        self.record(result)
    }
    /// Execute fixed domain SQL with owned bound parameters and latch every failure.
    /// # Arguments
    /// * `sql` - Fixed internal parameterized SQL statement.
    /// * `values` - Owned SQL values bound to that statement.
    /// # Returns
    /// Success after the fixed statement completes.
    /// # Errors
    /// Returns the prior latched failure or the current SQL execution error.
    async fn execute_bound(
        &mut self,
        sql: &'static str,
        values: Vec<Value>,
    ) -> Result<u64, StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = self
            .connection()
            .execute(sql, values)
            .await
            .map_err(Into::into);
        self.record(result)
    }
    /// Step a private parameterized query completely before releasing owned values.
    /// # Arguments
    /// * `sql` - Internal SQL, either fixed or assembled from private schema names.
    /// * `values` - Owned SQL values bound to that statement.
    /// # Returns
    /// Fully materialized owned result rows.
    /// # Errors
    /// Returns the prior latched failure or SQL preparation, stepping or value-decoding errors.
    async fn query_bound(
        &mut self,
        sql: &str,
        values: Vec<Value>,
    ) -> Result<Vec<Vec<Value>>, StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            let statement = self.connection().prepare(sql).await?;
            let mut rows = statement.query(values).await?;
            let mut output = Vec::new();
            while let Some(row) = rows.next().await? {
                let mut values = Vec::new();
                for index in 0..row.column_count() {
                    values.push(row.get_value(index)?);
                }
                output.push(values);
            }
            Ok(output)
        }
        .await;
        self.record(result)
    }
    #[cfg(test)]
    /// Fully materialize fixture queries and latch preparation, step and decode errors.
    /// # Errors
    /// Returns the first latched error or the current materialization error.
    async fn query(&mut self, sql: &str) -> Result<Vec<Vec<Value>>, StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = materialize(self.connection(), sql).await;
        self.record(result)
    }
    /// Execute fixed termination SQL, bypassing only the rollback-only latch.
    /// # Arguments
    /// * `sql` - COMMIT or ROLLBACK from a terminal method.
    /// # Returns
    /// The engine or narrow fixture-control result.
    /// # Errors
    /// Returns an engine error or the private injected control failure.
    async fn control(&self, sql: &'static str) -> Result<(), StorageError> {
        #[cfg(test)]
        if self.fail_control == Some(sql) {
            return Err(StorageError::caused(
                "injected control failure",
                std::io::Error::other(format!("injected {sql} failure")),
            ));
        }
        self.connection().execute(sql, ()).await?;
        Ok(())
    }
    /// Confirm idle state before restoring the sole connection.
    /// # Returns
    /// Success after restoration, or an inconsistent-state error.
    /// # Errors
    /// Returns a state error if SQLite still reports an active transaction.
    fn restore(&mut self) -> Result<(), StorageError> {
        if !self.connection().is_autocommit() {
            return Err(StorageError::state("termination left transaction active"));
        }
        self.store.connection = self.connection.take();
        Ok(())
    }
    /// Roll back when active and confirm idle state before reuse.
    /// # Returns
    /// Success after confirmed idle restoration, or a cleanup failure.
    /// # Errors
    /// Returns the rollback engine error or an inconsistent-state error.
    async fn cleanup(&mut self) -> Result<(), StorageError> {
        if !self.connection().is_autocommit() {
            self.control("ROLLBACK").await?;
        }
        self.restore()
    }
    /// Commit all changes while preserving independent operation and cleanup causes.
    /// # Returns
    /// The commit succeeds only after COMMIT and confirmed idle restoration.
    /// # Errors
    /// Returns [`CommitFailure::operation`] for the original latched, COMMIT or
    /// idle-restoration error. [`CommitFailure::cleanup`] retains any independent
    /// rollback or idle-restoration error. An uncertain connection is discarded
    /// and the store poisoned.
    /// # Examples
    /// ```
    /// use gossil::storage::{CommitFailure, LocalStore, StorageError};
    ///
    /// async fn finish(store: &mut LocalStore) -> Result<(), StorageError> {
    ///     let transaction = store.begin_write().await?;
    ///     match transaction.commit_checked().await {
    ///         Ok(()) => Ok(()),
    ///         Err(CommitFailure { operation, cleanup }) => {
    ///             if let Some(cleanup) = cleanup {
    ///                 eprintln!("cleanup failed: {cleanup}");
    ///             }
    ///             Err(operation)
    ///         }
    ///     }
    /// }
    /// ```
    // Preserve both owned, independent failure causes in the public error contract.
    #[allow(clippy::result_large_err)]
    pub async fn commit_checked(mut self) -> Result<(), CommitFailure> {
        let outcome = if let Some(error) = self.failure.take() {
            Err(error)
        } else {
            self.control("COMMIT").await.and_then(|()| self.restore())
        };
        match outcome {
            Ok(()) => Ok(()),
            Err(operation) => Err(CommitFailure {
                operation,
                cleanup: self.cleanup().await.err(),
            }),
        }
    }
    /// Commit all changes, or reject a rollback-only owner with its original error.
    /// Use [`Self::commit_checked`] to retain an independently typed cleanup cause.
    /// # Returns
    /// Success only after COMMIT and confirmed idle restoration.
    /// # Errors
    /// Returns the original error with cleanup diagnostics as text. Uncertain connections
    /// are discarded. The original operation source remains available.
    pub async fn commit(self) -> Result<(), StorageError> {
        self.commit_checked().await.map_err(|failure| {
            let mut error = failure.operation;
            if let Some(cleanup) = failure.cleanup {
                error.cleanup = Some(cleanup.operation);
            }
            error
        })
    }
    /// Explicitly roll back and restore the connection only after checked cleanup.
    /// # Returns
    /// Success after rollback when no earlier operation failed.
    /// # Errors
    /// Returns the original operation error if one occurred, with any cleanup error attached.
    pub async fn rollback(mut self) -> Result<(), StorageError> {
        let original = self.failure.take();
        match (original, self.cleanup().await) {
            (None, result) => result,
            (Some(mut error), cleanup) => {
                if let Err(cleanup) = cleanup {
                    error.cleanup = Some(cleanup.operation);
                }
                Err(error)
            }
        }
    }
}
impl Drop for WriteTransaction<'_> {
    /// Poison the store and close its connection without executing SQL.
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            self.store.poisoned = true;
            drop(connection);
        }
    }
}
/// Materialize owned values before returning; no statement, row or alias survives.
/// # Arguments
/// * `connection` - Sole connection borrowed within its owner.
/// * `sql` - Fixed configuration readback or fixture query.
/// # Returns
/// Owned values, or the preparation, query, stepping or decoding failure.
/// # Errors
/// Returns engine errors from preparation, query execution, stepping or value decoding.
async fn materialize(connection: &Connection, sql: &str) -> Result<Vec<Vec<Value>>, StorageError> {
    let statement = connection.prepare(sql).await?;
    let mut rows = statement.query(()).await?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().await? {
        let mut values = Vec::new();
        for index in 0..row.column_count() {
            values.push(row.get_value(index)?);
        }
        result.push(values);
    }
    Ok(result)
}

#[cfg(test)]
/// Check local engine capabilities, ownership and transaction cleanup with fixture tables.
mod tests {
    use super::*;
    /// Verify all required settings on both the first and a later file open.
    #[tokio::test]
    async fn settings_every_open() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("store.db");
        for _ in 0..2 {
            let store = LocalStore::open(&path).await.unwrap();
            let mut rows = store
                .connection
                .as_ref()
                .unwrap()
                .query("PRAGMA foreign_keys", ())
                .await
                .unwrap();
            assert_eq!(
                rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
                1
            );
            let mut rows = store
                .connection
                .as_ref()
                .unwrap()
                .query("PRAGMA journal_mode", ())
                .await
                .unwrap();
            assert_eq!(
                rows.next()
                    .await
                    .unwrap()
                    .unwrap()
                    .get::<String>(0)
                    .unwrap(),
                "wal"
            );
            for (sql, expected) in [("PRAGMA synchronous", 2), ("PRAGMA busy_timeout", 5000)] {
                assert_eq!(
                    materialize(store.connection.as_ref().unwrap(), sql)
                        .await
                        .unwrap(),
                    vec![vec![Value::Integer(expected)]]
                );
            }
        }
    }
    /// Verify BEGIN reserves the writer before any data modification.
    #[tokio::test]
    async fn immediate_before_dml() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("store.db");
        let mut first = LocalStore::open(&path).await.unwrap();
        let second = LocalStore::open(&path).await.unwrap();
        second
            .connection
            .as_ref()
            .unwrap()
            .busy_timeout(std::time::Duration::ZERO)
            .unwrap();
        let owner = first.begin_write().await.unwrap();
        assert!(
            second
                .connection
                .as_ref()
                .unwrap()
                .execute("BEGIN IMMEDIATE", ())
                .await
                .is_err()
        );
        owner.rollback().await.unwrap();
    }
    /// Verify a caught constraint error blocks later SQL and commit.
    #[tokio::test]
    async fn caught_error_rejects_commit_and_later_helper() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("store.db");
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("CREATE TABLE item(id INTEGER UNIQUE)")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("INSERT INTO item VALUES(1)").await.unwrap();
        assert!(tx.execute("INSERT INTO item VALUES(1)").await.is_err());
        assert!(tx.execute("INSERT INTO item VALUES(2)").await.is_err());
        assert!(tx.commit().await.is_err());
        let mut tx = store.begin_write().await.unwrap();
        assert_eq!(
            tx.query("SELECT count(*) FROM item").await.unwrap(),
            vec![vec![Value::Integer(0)]]
        );
        tx.rollback().await.unwrap();
    }

    /// Create fixture tables through the same owner used by domain operations.
    async fn fixture(store: &mut LocalStore) {
        let mut tx = store.begin_write().await.unwrap();
        for sql in [
            "CREATE TABLE parent(id INTEGER PRIMARY KEY, name TEXT UNIQUE, bytes BLOB)",
            "CREATE TABLE child(parent INTEGER REFERENCES parent(id) DEFERRABLE INITIALLY DEFERRED)",
        ] {
            tx.execute(sql).await.unwrap();
        }
        tx.commit().await.unwrap();
    }
    /// Return owned results without retaining connection aliases.
    async fn read(store: &mut LocalStore, sql: &str) -> Vec<Vec<Value>> {
        let mut tx = store.begin_write().await.unwrap();
        let result = tx.query(sql).await.unwrap();
        tx.rollback().await.unwrap();
        result
    }
    /// Verify engine capabilities, persisted commits and complete explicit rollback.
    #[tokio::test]
    async fn capabilities_commit_reopen_and_explicit_rollback() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("store.db");
        let mut store = LocalStore::open(&path).await.unwrap();
        fixture(&mut store).await;
        println!(
            "native version/source: {:?}",
            read(&mut store, "SELECT sqlite_version(),sqlite_source_id()").await
        );
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("INSERT INTO parent VALUES(1,'owned',x'00ff80')")
            .await
            .unwrap();
        tx.execute("INSERT INTO child VALUES(1)").await.unwrap();
        assert_eq!(
            tx.query("SELECT name,bytes,json_extract('{\"n\":7}', '$.n') FROM parent")
                .await
                .unwrap(),
            vec![vec![
                Value::Text("owned".into()),
                Value::Blob(vec![0, 255, 128]),
                Value::Integer(7)
            ]]
        );
        tx.commit().await.unwrap();
        drop(store);
        let mut store = LocalStore::open(&path).await.unwrap();
        assert_eq!(
            read(&mut store, "SELECT count(*) FROM child").await,
            vec![vec![Value::Integer(1)]]
        );
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("INSERT INTO parent VALUES(2,'rollback',NULL)")
            .await
            .unwrap();
        tx.execute("INSERT INTO child VALUES(2)").await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(
            read(&mut store, "SELECT id FROM parent").await,
            vec![vec![Value::Integer(1)]]
        );
        assert_eq!(
            read(&mut store, "PRAGMA integrity_check").await,
            vec![vec![Value::Text("ok".into())]]
        );
        assert!(!store.is_poisoned());
        assert!(
            LocalStore::open(temp.path().join("absent").join("db"))
                .await
                .is_err()
        );
    }
    /// A caught pre-SQL hash error must roll back the whole borrowed operation.
    #[tokio::test]
    async fn caught_hash_mismatch_rolls_back_prior_mutation_after_reopen() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("db");
        let mut store = LocalStore::open(&path).await.unwrap();
        fixture(&mut store).await;
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("INSERT INTO parent VALUES(1,'must roll back',NULL)")
            .await
            .unwrap();
        let error = tx
            .verify_content_name(&"0".repeat(64), b"different bytes")
            .unwrap_err();
        assert_eq!(error.operation, "native content hash mismatch");
        let later = tx
            .execute("INSERT INTO parent VALUES(2,'also forbidden',NULL)")
            .await;
        let commit = tx.commit().await;
        drop(store);
        let mut reopened = LocalStore::open(&path).await.unwrap();
        assert_eq!(
            read(&mut reopened, "SELECT count(*) FROM parent").await,
            vec![vec![Value::Integer(0)]]
        );
        assert!(later.is_err());
        assert!(commit.is_err());
    }
    #[tokio::test]
    /// Every private domain entry point preserves a prior error without issuing further SQL.
    async fn latched_domain_helpers_preserve_the_first_failure() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = LocalStore::open(temp.path().join("db")).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let sha1 = crate::content_hash::hex(&crate::content_hash::sha1(b"abc"));
        tx.verify_content_name(&sha1, b"abc").unwrap();
        let error = tx.verify_content_name("invalid", b"abc").unwrap_err();
        assert_eq!(
            tx.verify_content_name(&sha1, b"abc").unwrap_err().operation,
            error.operation
        );
        assert_eq!(
            tx.execute_bound("CREATE TABLE forbidden(id)", vec![])
                .await
                .unwrap_err()
                .operation,
            error.operation
        );
        assert_eq!(
            tx.query_bound("SELECT 1", vec![])
                .await
                .unwrap_err()
                .operation,
            error.operation
        );
        assert_eq!(
            tx.install_schema().await.unwrap_err().operation,
            error.operation
        );
        assert_eq!(
            tx.validate_catalog().await.unwrap_err().operation,
            error.operation
        );
        let principal = policy::Principal {
            repository: "wrong".into(),
            owner: "wrong".into(),
        };
        assert_eq!(
            tx.validate_principal(&principal)
                .await
                .unwrap_err()
                .operation,
            error.operation
        );
        assert_eq!(tx.commit().await.unwrap_err().operation, error.operation);
    }
    /// Verify a deferred constraint failure rolls back before connection reuse.
    #[tokio::test]
    async fn commit_failure_rolls_back_and_reuses_idle_connection() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = LocalStore::open(temp.path().join("db")).await.unwrap();
        fixture(&mut store).await;
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("INSERT INTO child VALUES(99)").await.unwrap();
        let error = tx.commit().await.unwrap_err();
        assert!(error.operation.contains("FOREIGN KEY"));
        assert_eq!(error.cleanup, None);
        assert!(!store.is_poisoned());
        assert!(store.connection.as_ref().unwrap().is_autocommit());
        assert_eq!(
            read(&mut store, "SELECT count(*) FROM child").await,
            vec![vec![Value::Integer(0)]]
        );
    }
    /// Preserve native COMMIT and independent typed cleanup causes after real writes.
    /// The rollback seam is a control failure, not a simulated disk failure.
    #[tokio::test]
    async fn checked_commit_retains_native_and_cleanup_causes_and_discards_connection() {
        use std::error::Error;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("db");
        let mut store = LocalStore::open(&path).await.unwrap();
        fixture(&mut store).await;
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("INSERT INTO parent VALUES(1,'lost',NULL)")
            .await
            .unwrap();
        tx.execute("INSERT INTO child VALUES(99)").await.unwrap();
        tx.fail_control = Some("ROLLBACK");
        let failure = tx.commit_checked().await.unwrap_err();
        assert!(failure.operation.operation.contains("FOREIGN KEY"));
        assert!(failure.operation.source().unwrap().is::<libsql::Error>());
        let cleanup = failure.cleanup.unwrap();
        assert_eq!(cleanup.operation, "injected control failure");
        let cause = cleanup
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap();
        assert_eq!(cause.kind(), std::io::ErrorKind::Other);
        assert_eq!(cause.to_string(), "injected ROLLBACK failure");
        assert!(store.is_poisoned());
        assert!(store.connection.is_none());
        assert!(store.begin_write().await.is_err());
        let mut reopened = LocalStore::open(&path).await.unwrap();
        assert_eq!(
            read(&mut reopened, "SELECT count(*) FROM parent").await,
            vec![vec![Value::Integer(0)]]
        );
        assert_eq!(
            read(&mut reopened, "SELECT count(*) FROM child").await,
            vec![vec![Value::Integer(0)]]
        );
        reopened
            .begin_write()
            .await
            .unwrap()
            .commit_checked()
            .await
            .unwrap();
    }
    /// Compose a real generic COMMIT failure with complete prepared-locator cleanup.
    /// This helper fixture does not establish native repository or registration admission.
    /// # Panics
    /// Panics if preparation, deferred-FK rollback, connection reuse or cleanup differs.
    #[tokio::test]
    async fn commit_failure_discards_prepared_locator_without_publication() {
        use crate::checkout_locator::{Locator, Prepared, prepare_directory};
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("db");
        let marker = temp.path().join("marker");
        let mut store = LocalStore::open(&path).await.unwrap();
        fixture(&mut store).await;
        prepare_directory(&marker).unwrap();
        let value = Locator::new(
            &path,
            "0123456789abcdef0123456789abcdef01234567".into(),
            "work".into(),
        )
        .unwrap();
        let prepared = Prepared::new(&marker, &value).unwrap();
        let entries = std::fs::read_dir(&marker)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        assert_eq!(crate::checkout_locator::read(&entries[0]).unwrap(), value);
        assert!(!marker.join("checkout.json").exists());
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("INSERT INTO child VALUES(99)").await.unwrap();
        let error = tx.commit().await.unwrap_err();
        assert!(error.operation.contains("FOREIGN KEY"));
        assert_eq!(error.cleanup, None);
        assert!(!store.is_poisoned());
        assert!(store.connection.as_ref().unwrap().is_autocommit());
        assert_eq!(
            read(&mut store, "SELECT count(*) FROM child").await,
            vec![vec![Value::Integer(0)]]
        );
        drop(prepared);
        assert_eq!(std::fs::read_dir(&marker).unwrap().count(), 0);
        assert!(!marker.join("checkout.json").exists());
    }
    /// Verify cleanup failures retain the first error and poison the store.
    #[tokio::test]
    async fn failed_cleanup_preserves_original_and_poison_policy() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("db");
        let mut store = LocalStore::open(&path).await.unwrap();
        fixture(&mut store).await;
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("INSERT INTO parent VALUES(1,'lost',NULL)")
            .await
            .unwrap();
        let original = tx
            .execute("INSERT INTO parent VALUES(1,'duplicate',NULL)")
            .await
            .unwrap_err();
        tx.fail_control = Some("ROLLBACK");
        let failure = tx.commit().await.unwrap_err();
        assert_eq!(failure.operation, original.operation);
        assert_eq!(failure.cleanup.as_deref(), Some("injected control failure"));
        assert!(failure.to_string().contains("cleanup:"));
        assert!(store.is_poisoned());
        assert!(store.begin_write().await.is_err());
        let mut other = LocalStore::open(&path).await.unwrap();
        assert_eq!(
            read(&mut other, "SELECT count(*) FROM parent").await,
            vec![vec![Value::Integer(0)]]
        );
        let mut tx = other.begin_write().await.unwrap();
        tx.fail_control = Some("ROLLBACK");
        assert!(tx.rollback().await.is_err());
        assert!(other.is_poisoned());
    }
    /// Verify owner destruction releases the writer and prevents store reuse.
    #[tokio::test]
    async fn dropped_owner_releases_aliases_and_poisoned_store() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("db");
        let mut store = LocalStore::open(&path).await.unwrap();
        fixture(&mut store).await;
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("INSERT INTO parent VALUES(1,'first',x'00ff')")
            .await
            .unwrap();
        tx.execute("INSERT INTO parent VALUES(2,'second',x'80')")
            .await
            .unwrap();
        let owned = tx
            .query("SELECT name,bytes FROM parent ORDER BY id")
            .await
            .unwrap();
        drop(tx);
        assert!(store.is_poisoned());
        assert_eq!(owned.len(), 2);
        let mut other = LocalStore::open(&path).await.unwrap();
        assert_eq!(
            read(&mut other, "SELECT count(*) FROM parent").await,
            vec![vec![Value::Integer(0)]]
        );
        other.begin_write().await.unwrap().commit().await.unwrap();
    }
    /// Verify memory fixtures retain settings and transaction behavior without WAL.
    #[tokio::test]
    async fn memory_retains_one_connection_and_excludes_wal_only() {
        let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
        fixture(&mut store).await;
        assert_eq!(
            read(&mut store, "PRAGMA journal_mode").await,
            vec![vec![Value::Text("memory".into())]]
        );
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("INSERT INTO parent VALUES(1,'memory',x'ff00')")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert_eq!(
            read(&mut store, "SELECT bytes FROM parent").await,
            vec![vec![Value::Blob(vec![255, 0])]]
        );
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("DELETE FROM parent").await.unwrap();
        tx.rollback().await.unwrap();
        assert_eq!(
            read(&mut store, "SELECT count(*) FROM parent").await,
            vec![vec![Value::Integer(1)]]
        );
        assert_eq!(
            read(&mut store, "SELECT json_extract('{\"n\":7}', '$.n')").await,
            vec![vec![Value::Integer(7)]]
        );
        assert_eq!(
            read(&mut store, "PRAGMA integrity_check").await,
            vec![vec![Value::Text("ok".into())]]
        );
        let mut tx = store.begin_write().await.unwrap();
        assert!(
            tx.execute("INSERT INTO parent VALUES(1,'duplicate',NULL)")
                .await
                .is_err()
        );
        assert!(tx.commit().await.is_err());
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("INSERT INTO child VALUES(99)").await.unwrap();
        assert!(tx.commit().await.is_err());
        assert_eq!(
            read(&mut store, "SELECT count(*) FROM child").await,
            vec![vec![Value::Integer(0)]]
        );
        drop(store.begin_write().await.unwrap());
        assert!(store.is_poisoned());
    }
    /// Verify cancellation rolls back before commit and preserves completed commits.
    #[tokio::test]
    async fn cancellation_at_controlled_await_rolls_back_but_after_commit_preserves() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("db");
        let mut initial = LocalStore::open(&path).await.unwrap();
        fixture(&mut initial).await;
        drop(initial);
        for (id, commit, cancel) in [
            (1, false, true),
            (2, true, true),
            (5, false, false),
            (6, true, false),
        ] {
            let task_path = path.clone();
            let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
            let (resume_tx, resume_rx) = tokio::sync::oneshot::channel();
            let task = tokio::spawn(async move {
                let mut store = LocalStore::open(task_path).await.unwrap();
                let mut tx = store.begin_write().await.unwrap();
                tx.execute(&format!(
                    "INSERT INTO parent VALUES({id},'owner-{id}',NULL)"
                ))
                .await
                .unwrap();
                if commit {
                    tx.commit().await.unwrap();
                    ready_tx.send(()).unwrap();
                    resume_rx.await.unwrap();
                } else {
                    ready_tx.send(()).unwrap();
                    resume_rx.await.unwrap();
                    tx.rollback().await.unwrap();
                }
            });
            ready_rx.await.unwrap();
            if cancel {
                task.abort();
                assert!(task.await.unwrap_err().is_cancelled());
            } else {
                resume_tx.send(()).unwrap();
                task.await.unwrap();
            }
            let mut other = LocalStore::open(&path).await.unwrap();
            let mut tx = other.begin_write().await.unwrap();
            let competitor = id + 2;
            tx.execute(&format!(
                "INSERT INTO parent VALUES({competitor},'competitor-{competitor}',NULL)"
            ))
            .await
            .unwrap();
            tx.commit().await.unwrap();
        }
        let mut reopened = LocalStore::open(&path).await.unwrap();
        assert_eq!(
            read(&mut reopened, "SELECT id FROM parent ORDER BY id").await,
            vec![
                vec![Value::Integer(2)],
                vec![Value::Integer(3)],
                vec![Value::Integer(4)],
                vec![Value::Integer(6)],
                vec![Value::Integer(7)],
                vec![Value::Integer(8)]
            ]
        );
    }
    /// Verify independent writers contend while a WAL reader sees committed state.
    #[test]
    fn writers_on_os_threads_and_wal_reader() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("db");
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let mut store = LocalStore::open(&path).await.unwrap();
            fixture(&mut store).await;
            let mut tx = store.begin_write().await.unwrap();
            tx.execute("INSERT INTO parent VALUES(1,'pending',NULL)")
                .await
                .unwrap();
            let thread_path = path.clone();
            let (observed_tx, observed_rx) = std::sync::mpsc::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let writer = std::thread::spawn(move || {
                tokio::runtime::Runtime::new().unwrap().block_on(async {
                    let mut other = LocalStore::open(&thread_path).await.unwrap();
                    other
                        .connection
                        .as_ref()
                        .unwrap()
                        .busy_timeout(Duration::ZERO)
                        .unwrap();
                    assert_eq!(
                        materialize(
                            other.connection.as_ref().unwrap(),
                            "SELECT count(*) FROM parent"
                        )
                        .await
                        .unwrap(),
                        vec![vec![Value::Integer(0)]]
                    );
                    assert!(other.begin_write().await.is_err());
                    assert!(other.is_poisoned());
                    observed_tx.send(()).unwrap();
                    release_rx.recv().unwrap();
                    let mut other = LocalStore::open(&thread_path).await.unwrap();
                    assert_eq!(
                        read(&mut other, "SELECT count(*) FROM parent").await,
                        vec![vec![Value::Integer(1)]]
                    );
                    let mut tx = other.begin_write().await.unwrap();
                    tx.execute("INSERT INTO parent VALUES(2,'thread',NULL)")
                        .await
                        .unwrap();
                    tx.commit().await.unwrap();
                });
            });
            observed_rx.recv().unwrap();
            tx.commit().await.unwrap();
            release_tx.send(()).unwrap();
            writer.join().unwrap();
            assert_eq!(
                read(&mut store, "SELECT count(*) FROM parent").await,
                vec![vec![Value::Integer(2)]]
            );
        });
    }
    /// Verify query failures latch and explicit rollback retains the engine cause.
    #[tokio::test]
    async fn preparation_and_step_failures_latch_and_explicit_rollback_keeps_cause() {
        use std::error::Error;
        let temp = tempfile::tempdir().unwrap();
        let mut store = LocalStore::open(temp.path().join("db")).await.unwrap();
        fixture(&mut store).await;
        for query in [
            "SELECT no_such_column FROM parent",
            "SELECT json_extract('malformed', '$.n')",
        ] {
            let mut tx = store.begin_write().await.unwrap();
            tx.execute("INSERT INTO parent VALUES(1,'must roll back',NULL)")
                .await
                .unwrap();
            let error = tx.query(query).await.unwrap_err();
            assert!(error.source().is_some());
            assert_eq!(error.to_string(), error.operation);
            assert!(tx.query("SELECT 1").await.is_err());
            assert_eq!(tx.rollback().await.unwrap_err().operation, error.operation);
            assert!(!store.is_poisoned());
            assert_eq!(
                read(&mut store, "SELECT count(*) FROM parent").await,
                vec![vec![Value::Integer(0)]]
            );
        }
        let mut tx = store.begin_write().await.unwrap();
        tx.fail_control = Some("COMMIT");
        assert!(tx.commit().await.is_err());
        assert!(!store.is_poisoned());
        let tx = store.begin_write().await.unwrap();
        drop(tx.commit());
        assert!(store.is_poisoned());
        let mut store = LocalStore::open(temp.path().join("db")).await.unwrap();
        let tx = store.begin_write().await.unwrap();
        drop(tx.rollback());
        assert!(store.is_poisoned());
    }
    /// Verify configuration disagreement and inconsistent termination reject reuse.
    #[tokio::test]
    async fn configuration_mismatch_and_inconsistent_state_are_errors() {
        let temp = tempfile::tempdir().unwrap();
        assert!(
            LocalStore::open_inner(temp.path().join("file"), false)
                .await
                .is_err()
        );
        // The native filename adapter treats this as a literal file, not URI/memory syntax.
        let literal = temp.path().join(":memory:");
        let mut literal_store = LocalStore::open(&literal).await.unwrap();
        let mut tx = literal_store.begin_write().await.unwrap();
        tx.execute("CREATE TABLE literal_path(value)")
            .await
            .unwrap();
        tx.execute("INSERT INTO literal_path VALUES(17)")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        drop(literal_store);
        assert!(literal.is_file());
        let mut reopened = LocalStore::open(&literal).await.unwrap();
        assert_eq!(
            read(&mut reopened, "SELECT value FROM literal_path").await,
            vec![vec![Value::Integer(17)]]
        );
        drop(reopened);
        let mut store = LocalStore::open(temp.path().join("db")).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        assert!(tx.restore().is_err());
        tx.rollback().await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("CREATE TABLE auto_rollback(id UNIQUE)")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("INSERT INTO auto_rollback VALUES(1)")
            .await
            .unwrap();
        assert!(
            tx.execute("INSERT OR ROLLBACK INTO auto_rollback VALUES(1)")
                .await
                .is_err()
        );
        assert!(tx.connection().is_autocommit());
        assert!(tx.rollback().await.is_err());
        assert!(!store.is_poisoned());
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("INSERT INTO auto_rollback VALUES(1)")
            .await
            .unwrap();
        assert!(
            tx.execute("INSERT OR ROLLBACK INTO auto_rollback VALUES(1)")
                .await
                .is_err()
        );
        tx.fail_control = Some("ROLLBACK");
        // SQLite already rolled back, so no rollback command should be attempted.
        assert!(tx.commit().await.unwrap_err().cleanup.is_none());
        assert!(!store.is_poisoned());
    }
    /// Verify explicit rollback reports both the first error and cleanup failure.
    #[tokio::test]
    async fn explicit_rollback_cleanup_failure_retains_first_error() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("db");
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let error = tx.query("SELECT absent").await.unwrap_err();
        tx.fail_control = Some("ROLLBACK");
        let failure = tx.rollback().await.unwrap_err();
        assert_eq!(failure.operation, error.operation);
        assert_eq!(failure.cleanup.as_deref(), Some("injected control failure"));
        assert!(store.is_poisoned());
        let mut reopened = LocalStore::open(&path).await.unwrap();
        reopened
            .begin_write()
            .await
            .unwrap()
            .rollback()
            .await
            .unwrap();
    }
    #[tokio::test]
    /// Reject embedded NUL filenames on file and memory routes with their original cause.
    async fn embedded_nul_path_returns_error_with_original_cause() {
        use std::error::Error;
        for file in [true, false] {
            let result = LocalStore::open_inner("bad\0name.db", file).await;
            let error = result
                .err()
                .expect("embedded NUL path must return an error");
            assert!(error.operation.contains("path"));
            let cause = error
                .source()
                .unwrap()
                .downcast_ref::<std::ffi::NulError>()
                .unwrap();
            assert_eq!(cause.nul_position(), 3);
            assert!(error.cleanup.is_none());
        }
    }

    #[tokio::test]
    /// Preserve SQLite temporary-file semantics and ordinary native file behavior.
    async fn path_validation_preserves_empty_and_valid_file_behavior() {
        use std::error::Error;
        let error = LocalStore::open("")
            .await
            .err()
            .expect("temporary database cannot satisfy required WAL mode");
        assert_eq!(
            error.operation,
            "configuration disagreement: PRAGMA journal_mode"
        );
        assert!(error.source().is_none());
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("valid.db");
        let mut store = LocalStore::open(&path).await.unwrap();
        store.begin_write().await.unwrap().commit().await.unwrap();
        assert!(path.is_file());
    }
}

#[cfg(test)]
#[path = "../tests/support/storage_composition.rs"]
mod composition_tests;

#[cfg(test)]
#[path = "../tests/support/storage_dates.rs"]
mod date_tests;
