//! Local OS access establishes owner trust separately from remote request principals.
//!
//! ## Recommendations
//! Open this handle only for local CLI/library work; never derive HTTP request authority from it.
use super::{LocalStore, StorageError, content::Limits, policy::Principal, schema::NEWEST_CATALOG};
use libsql::Value;
use std::path::Path;

/// Register and authenticate the sole supported native empty-root checkout.
mod checkout;
/// Persist and read immutable explicit tracked-path scope choices.
mod path_scope;
/// Read complete authenticated check-in metadata without changing repository state.
mod reading;
/// Validate the complete supported shared-root and private manifest chain.
mod recorded_history;
/// Capture and record all explicitly private tracked files atomically.
mod recording;
/// Admit exact native and local projections for supported private history.
mod recording_admission;
pub use reading::RecordedCheckin;
/// Encode canonical full-tree private manifests within borrowed budgets.
mod recording_manifest;
mod tracking;
pub use path_scope::{PathScope, PathScopeSelection};
pub use recording::{PrivateRecordingOutcome, PrivateRecordingRequest};
pub use tracking::{PendingAdd, TrackingLimits};
mod initialization;
/// Store and replay private issue tickets. The file is `src/storage/issue_ticket.rs`.
/// The module is a child of this one so that it shares the owner's admission.
#[path = "issue_ticket.rs"]
mod issue_ticket;
pub use checkout::CheckoutView;
pub use initialization::InitializeOptions;
pub use issue_ticket::{
    IssueLimits, IssueRecord, IssueTicket, PrivateIssueRequest, ProjectInitRequest,
};
#[cfg(test)]
pub(crate) use issue_ticket::{IssuePhase, inject_issue_failure};
/// Run the full proof of the repository, or of one issue, on demand.
mod verification;
pub use verification::{IssueMatch, VerifySummary};

/// Fixed scalar predicates bound returned data, not total SQLite scan work.
/// The admitted root cannot produce other artifact types or a current file/merge selection.
/// Catalog1 historical rows, diagnostics, opaque content and target-only references retain no root authority.
/// Catalog6 admission separately requires an exact zero-or-one registration and one native root.
const ROOT_PROJECTION_CONFLICTS: &str = concat!(
    "SELECT EXISTS(SELECT 1 FROM plink WHERE pid=?1 OR cid=?1 OR baseid=?1),",
    "EXISTS(SELECT 1 FROM mlink WHERE mid=?1 OR fid=?1 OR pmid=?1 OR pid=?1),",
    "EXISTS(SELECT 1 FROM cherrypick WHERE parentid=?1 OR childid=?1),",
    "EXISTS(SELECT 1 FROM phantom WHERE rid=?1),",
    "EXISTS(SELECT 1 FROM orphan WHERE rid=?1 OR baseline=?1),",
    "EXISTS(SELECT 1 FROM backlink WHERE srcid=?1),",
    "EXISTS(SELECT 1 FROM forumpost WHERE fpid=?1 OR froot=?1 OR fprev=?1 OR firt=?1),",
    "EXISTS(SELECT 1 FROM g_dependency WHERE source_hash=?2 OR (target_hash=?2 AND required=1)),",
    "EXISTS(SELECT 1 FROM attachment WHERE attachid=?1),",
    "EXISTS(SELECT 1 FROM ticketchng WHERE tkt_rid=?1),",
    "EXISTS(SELECT 1 FROM modreq WHERE objid=?1),",
    "EXISTS(SELECT 1 FROM tagxref WHERE (srcid=?1 OR origid=?1) AND rid IS NOT ?1),",
    "EXISTS(SELECT 1 FROM g_admission WHERE native_hash=?2),",
    "EXISTS(SELECT 1 FROM g_contributor WHERE native_hash=?2 AND status='valid'),",
    "EXISTS(SELECT 1 FROM g_descriptor WHERE (descriptor_hash=?2 OR attachment_artifact_hash=?2) AND status='valid'),",
    "EXISTS(SELECT 1 FROM g_discussion_message WHERE version_hash=?2),",
    "EXISTS(SELECT 1 FROM g_message_context WHERE descriptor_hash=?2 OR version_hash=?2),",
    "EXISTS(SELECT 1 FROM g_message_context_target WHERE descriptor_hash=?2 OR version_hash=?2),",
    "EXISTS(SELECT 1 FROM g_code_link WHERE attachment_artifact_hash=?2),",
    "EXISTS(SELECT 1 FROM g_discussion_target WHERE attachment_artifact_hash=?2),",
    "EXISTS(SELECT 1 FROM g_vfile UNION ALL SELECT 1 FROM g_vmerge UNION ALL SELECT 1 FROM g_path_scope UNION ALL SELECT 1 FROM g_pending_path UNION ALL SELECT 1 FROM g_journal)",
);
/// Number of independently sealed scalar root-conflict categories.
const ROOT_PROJECTION_FLAGS: usize = 21;

/// Finite content and parser bounds for complete supported repository admission.
#[derive(Clone, Copy)]
pub struct AdmissionLimits {
    /// Envelope, reconstruction and cumulative work bounds, including catalog6 metadata and history.
    pub content: Limits,
    /// Structural parser bounds, shared across root and private manifest validation in catalog6.
    pub parser: crate::artifact::ParseLimits,
}
/// Sealed authority snapshot and mutation counters from successful admission.
struct AdmissionProof {
    /// Connection-local observation of external commits.
    data_version: Vec<Vec<Value>>,
    /// Same-connection mutation count at the last checked boundary.
    total_changes: u64,
    /// Exact durable policy, checkout and supported root authority rows.
    authority: Vec<Vec<Vec<Value>>>,
    /// Explicit C4 caller limits retained for checked compatibility entry points.
    content_limits: Option<Limits>,
}

/// Explicit local identities and bounds for a new catalog1 repository with a checkout.
/// Owner identity is bound to this new local file; it is not an authentication credential.
pub struct CreateOptions<'a> {
    /// Existing native project identity encoding, forty lowercase hexadecimal bytes.
    pub repository_id: &'a str,
    /// Stable local owner identity to bind to this replica.
    pub owner_id: &'a str,
    /// Local checkout registration identity.
    pub checkout_id: &'a str,
    /// Existing checkout root directory, with no tracked paths at initialization.
    pub checkout_root: &'a Path,
    /// Native D-card date token; the selected SQL provider validates it.
    pub date: &'a [u8],
    /// Native U-card user bytes.
    pub user: &'a [u8],
    /// Explicit finite content ceilings for the initial root.
    pub content_limits: Limits,
    /// Explicit finite parser ceilings for the initial root.
    pub parse_limits: crate::artifact::ParseLimits,
}

/// A sealed local-owner handle bound to the identities read from its own database.
///
/// Opening the local file uses operating-system file permissions. A server must
/// use a separate shared/team principal path for network requests; this handle
/// must never be passed to a request merely because the server can open the file.
pub struct LocalOwnerRepository {
    /// Sole owned connection and transaction lifecycle.
    store: LocalStore,
    /// Sealed repository and owner binding.
    principal: Principal,
    /// Validated authority and counters checked before content lookup.
    admission: AdmissionProof,
}
/// A restricted shared view selected by a trusted local caller.
/// This conveys no network authentication and has no route back to owner scope.
pub struct SharedView<'a> {
    /// Borrowed owner restricted to the supported public root read API.
    repository: &'a mut LocalOwnerRepository,
}
impl SharedView<'_> {
    /// Read only content with supported committed public closure.
    /// # Arguments
    /// * `native_name` - Full native identity, with no private lookup authority.
    /// * `limits` - Explicit finite envelope, output and work ceilings.
    /// # Returns
    /// Owned verified bytes from the supported public initial-root closure.
    /// # Examples
    /// ```no_run
    /// async fn shared(owner: &mut gossil::storage::LocalOwnerRepository, hash: &str, limits: gossil::storage::Limits) {
    ///     let bytes = owner.shared_view().read_content(hash, limits).await.unwrap();
    ///     drop(bytes);
    /// }
    /// ```
    /// # Errors
    /// Missing and privately excluded names return the same unavailable error.
    /// Unsupported required public graphs, corruption and limits reject the operation.
    pub async fn read_content(
        &mut self,
        native_name: &str,
        limits: Limits,
    ) -> Result<Vec<u8>, StorageError> {
        let mut tx = self.repository.store.begin_write().await?;
        let mut c4_work = if self.repository.admission.content_limits.is_some() {
            Some(super::content::Budget::new(limits)?)
        } else {
            None
        };
        let result = async {
            if let Some(work)=&mut c4_work {tx.check_admission_budgeted(&self.repository.principal,&self.repository.admission,work).await?;} else {tx.check_admission(&self.repository.principal,&self.repository.admission).await?;}
            let rows = tx.query_bound("SELECT b.uuid FROM blob b JOIN g_public_receipt p ON p.native_hash=b.uuid AND p.supporting_root=b.uuid WHERE b.uuid=?1 AND NOT EXISTS(SELECT 1 FROM private x WHERE x.rid=b.rid) AND NOT EXISTS(SELECT 1 FROM delta d WHERE d.rid=b.rid)",vec![Value::Text(native_name.into())]).await?;
            if rows.is_empty() { return Err(StorageError::state("shared content unavailable")); }
            if let Some(work)=&mut c4_work {tx.read_recording_content_budget(&self.repository.principal,native_name,work).await} else {tx.read_owner_content(&self.repository.principal,native_name,limits).await}
        }.await;
        let result = tx.record(result);
        tx.commit().await?;
        result
    }
}

impl LocalOwnerRepository {
    /// Select a restricted local shared view without minting a remote principal.
    /// # Returns
    /// A borrowed view that exposes only supported public content.
    /// # Examples
    /// ```no_run
    /// fn restricted(owner: &mut gossil::storage::LocalOwnerRepository) {
    ///     let _shared = owner.shared_view();
    /// }
    /// ```
    pub fn shared_view(&mut self) -> SharedView<'_> {
        SharedView { repository: self }
    }
    /// Create a catalog1 repository bound to an existing checkout directory.
    /// # Arguments
    /// * `path` - New native database path; existing destinations and recovery sidecars are rejected.
    /// * `options` - Local bindings, native root fields and explicit finite bounds.
    /// # Returns
    /// A sealed local owner after complete root admission and commit.
    /// # Examples
    /// ```no_run
    /// async fn create(path: &std::path::Path, options: gossil::storage::CreateOptions<'_>) {
    ///     let owner = gossil::storage::LocalOwnerRepository::create(path, options).await.unwrap();
    ///     drop(owner);
    /// }
    /// ```
    /// # Errors
    /// Rejects an existing destination, any `-journal`, `-wal` or `-shm` entry,
    /// invalid root/bindings, or any initialization failure. Sidecar observation
    /// failures retain their operating-system cause. Sidecar observations do not
    /// prevent concurrent pathname changes; callers must retain a stable local namespace.
    pub async fn create(
        path: impl AsRef<Path>,
        options: CreateOptions<'_>,
    ) -> Result<Self, StorageError> {
        Self::create_internal(
            path,
            options,
            #[cfg(test)]
            None,
        )
        .await
    }
    /// Initialize a native repository without registering a filesystem checkout.
    /// # Arguments
    /// * `path` - New literal native filename; keep its namespace stable through engine cleanup.
    /// * `options` - Explicit administrator, password, date, hash policy and finite limits.
    /// # Returns
    /// An admitted owner of the canonical empty catalog6 repository.
    /// Initialization stores no file tree. [`Self::register_checkout`] can register one existing directory; then explicitly track paths and select private scopes before [`Self::commit_private_tracked_files`].
    /// No environment variables, preference database or templates supply defaults.
    /// # Examples
    /// ```no_run
    /// async fn initialize(path: &std::path::Path, options: gossil::storage::InitializeOptions<'_>) {
    ///     let owner = gossil::storage::LocalOwnerRepository::initialize(path, options).await.unwrap();
    ///     drop(owner);
    /// }
    /// ```
    /// # Errors
    /// Rejects invalid identities or administrator bytes before destination creation; rejects
    /// existing destinations/sidecars, invalid dates, limits and engine failures. Cleanup
    /// retains any possibly opened engine file and reports that uncertainty separately.
    pub async fn initialize(
        path: impl AsRef<Path>,
        options: InitializeOptions<'_>,
    ) -> Result<Self, StorageError> {
        if options.user.is_empty() || options.user.contains(&0) || options.password.contains('\0') {
            return Err(StorageError::state(
                "invalid native administrator or password",
            ));
        }
        std::str::from_utf8(options.user)
            .map_err(|e| StorageError::caused("local native user is not UTF-8", e))?;
        let create = CreateOptions {
            repository_id: options.repository_id,
            owner_id: options.owner_id,
            checkout_id: "",
            checkout_root: Path::new(""),
            date: options.date.as_bytes(),
            user: options.user,
            content_limits: options.content_limits,
            parse_limits: options.parse_limits,
        };
        Self::create_configured(
            path,
            create,
            Some(&options),
            #[cfg(test)]
            None,
        )
        .await
    }
    /// Keep deterministic phase failures private to the module's tests.
    /// # Arguments
    /// * `path` - New local database destination.
    /// * `options` - Local identities, root fields and finite admission bounds.
    /// # Returns
    /// A sealed owner after complete initialization and admission.
    /// # Errors
    /// Rejects existing destinations or recovery sidecars, invalid identities or roots,
    /// resource limits and filesystem or SQL failures.
    async fn create_internal(
        path: impl AsRef<Path>,
        options: CreateOptions<'_>,
        #[cfg(test)] failure: Option<&str>,
    ) -> Result<Self, StorageError> {
        Self::create_configured(
            path,
            options,
            None,
            #[cfg(test)]
            failure,
        )
        .await
    }
    /// Construct either the existing bound catalog1 or the unbound native catalog6.
    /// # Arguments
    /// * `path` - Exclusively created literal destination.
    /// * `options` - Canonical root fields and optional bound checkout fields.
    /// * `native` - Implemented native settings, selecting unbound catalog6 when present.
    /// # Returns
    /// A complete admitted owner.
    /// # Errors
    /// Preserves the first initialization failure and checked conservative cleanup diagnostics.
    async fn create_configured(
        path: impl AsRef<Path>,
        options: CreateOptions<'_>,
        native: Option<&InitializeOptions<'_>>,
        #[cfg(test)] failure: Option<&str>,
    ) -> Result<Self, StorageError> {
        use std::os::unix::fs::MetadataExt;
        let path = path.as_ref();
        if options.repository_id.len() != 40
            || !options
                .repository_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || options.owner_id.is_empty()
            || (native.is_none() && options.checkout_id.is_empty())
        {
            return Err(StorageError::state(
                "invalid local repository identity binding",
            ));
        }
        let binding = if native.is_none() {
            let root = std::fs::canonicalize(options.checkout_root)
                .map_err(|e| StorageError::caused("checkout root unavailable", e))?;
            let metadata = std::fs::metadata(&root)
                .map_err(|e| StorageError::caused("checkout root metadata unavailable", e))?;
            if !metadata.is_dir() {
                return Err(StorageError::state("checkout root is not a directory"));
            }
            checkout_root_text(&root)?;
            Some((root, metadata))
        } else {
            None
        };
        let root_ceiling = usize::try_from(
            u64::from(options.content_limits.max_output_bytes)
                .min(options.content_limits.max_total_decoded_bytes),
        )
        .unwrap_or(usize::MAX)
        .min(options.parse_limits.input_bytes);
        if native.is_none() {
            empty_root_bytes(options.date, options.user, root_ceiling)?;
        }
        initialization::check_sidecars(path)?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| StorageError::caused("exclusive repository creation failed", e))?;
        let metadata = file
            .metadata()
            .map_err(|error| StorageError::caused("new repository identity unavailable", error))?;
        let mut cleanup = initialization::NewFileCleanup::new(path, &metadata, native.is_some());
        let identity = super::FileIdentity::from_metadata(path, false, &metadata)?;
        drop(file);
        let result = async {
        initialization::check_sidecars(path)?;
        identity.observe()?;
        cleanup.engine_opening();
        let mut store = LocalStore::open_observed(path, true, Some(identity.clone())).await?;
        let mut tx = store.begin_write().await?;
        let result = async {
            if native.is_some() { tx.install_schema_catalog6().await?; } else { tx.install_schema().await?; }
            let date_token = if let Some(native) = native { tx.initial_date(native.date).await? } else { String::from_utf8(options.date.to_vec()).map_err(|e| StorageError::caused("native date UTF-8", e))? };
            let bytes = empty_root_bytes(date_token.as_bytes(), options.user, root_ceiling)?;
            #[cfg(test)]
            if failure == Some("schema") { return Err(StorageError::state("injected initialization phase failure")); }
            let parsed = parse_owned(&tx, &bytes, options.parse_limits).await?;
            validate_empty_root(&parsed)?;
            let date = parsed.date().ok_or_else(|| StorageError::state("empty root has no date"))?;
            std::str::from_utf8(options.user).map_err(|e| StorageError::caused("local native user is not UTF-8", e))?;
            let sha1 = if let Some(native) = native { tx.initialize_native_settings(native).await? } else { false };
            let hash = if sha1 { crate::content_hash::hex(&crate::content_hash::sha1(&bytes)) } else { crate::content_hash::hex(&crate::content_hash::sha3(&bytes, 256).unwrap()) };
            tx.verify_content_name(&hash, &bytes)?;
            let packed = crate::compression::pack_bounded(&bytes, -1, usize::try_from(options.content_limits.max_stored_bytes.min(options.content_limits.max_total_stored_bytes)).unwrap_or(usize::MAX))
                .map_err(|e| StorageError::caused("empty root compression failed", e))?;
            tx.decode_full_content(&hash, bytes.len() as i64, &packed, options.content_limits)?;
            tx.execute_bound("INSERT INTO config(name,value,mtime) VALUES('project-code',?1,unixepoch()),('content-schema','2',unixepoch()),('aux-schema','2015-01-24',unixepoch()),('server-code',lower(hex(randomblob(20))),unixepoch())", vec![Value::Text(options.repository_id.into())]).await?;
            if native.is_some() {
                tx.execute_bound("INSERT INTO g_storage VALUES(1,?1,?2,'gest-ticket-v1',0,0)", vec![Value::Text(options.repository_id.into()), Value::Integer(NEWEST_CATALOG)]).await?;
            } else {
            tx.execute_bound("INSERT INTO g_storage VALUES(1,?1,1,'gest-ticket-v1',0)", vec![Value::Text(options.repository_id.into())]).await?;
            }
            tx.execute_bound("INSERT INTO g_local_owner VALUES(1,?1)", vec![Value::Text(options.owner_id.into())]).await?;
            tx.execute_bound("INSERT INTO rcvfrom(rcvid,uid,mtime) VALUES(1,0,?1)", vec![Value::Real(date)]).await?;
            tx.execute_bound("INSERT INTO blob(rid,rcvid,size,uuid,content) VALUES(1,1,?1,?2,?3)", vec![Value::Integer(bytes.len() as i64), Value::Text(hash.clone()), Value::Blob(packed)]).await?;
            tx.execute_bound("INSERT INTO unclustered VALUES(?1)", vec![Value::Integer(1)]).await?;
            #[cfg(test)]
            if failure == Some("content") { return Err(StorageError::state("injected initialization phase failure")); }
            tx.crosslink_supported_checkin(1, &parsed).await?;
            #[cfg(test)]
            if failure == Some("crosslink") { return Err(StorageError::state("injected initialization phase failure")); }
            tx.execute_bound("INSERT INTO g_public_receipt VALUES(?1,'local-explicit-publication',?1)", vec![Value::Text(hash.clone())]).await?;
            #[cfg(test)]
            if failure == Some("receipt") { return Err(StorageError::state("injected initialization phase failure")); }
            if let Some((root, metadata)) = &binding {
                let root_text = root.to_str().unwrap();
            tx.execute_bound("INSERT INTO g_checkout VALUES(?1,?2,?3,NULL,0,0,'ready')", vec![Value::Text(options.checkout_id.into()), Value::Text(root_text.into()), Value::Text(hash)]).await?;
            tx.execute_bound("INSERT INTO g_checkout_owner VALUES(?1,?2,?3,?4)", vec![Value::Text(options.checkout_id.into()), Value::Text(options.owner_id.into()), Value::Text(metadata.dev().to_string()), Value::Text(metadata.ino().to_string())]).await?;
            }
            tx.validate_catalog().await?;
            #[cfg(test)]
            if failure == Some("bindings") { return Err(StorageError::state("injected initialization phase failure")); }
            tx.execute_bound("UPDATE g_storage SET policy_complete=1 WHERE singleton=1", vec![]).await?;
            #[cfg(test)]
            if failure == Some("marker") { return Err(StorageError::state("injected initialization phase failure")); }
            let principal=Principal { repository: options.repository_id.into(), owner: options.owner_id.into() };
            let limits=AdmissionLimits {content:options.content_limits,parser:options.parse_limits};
            if native.is_some() {
                let mut budget=super::content::Budget::new(limits.content)?;
                let mut parse=ParseWork::new(limits.parser)?;
                tx.validate_recording_repository(&principal,limits,&mut budget,&mut parse).await?;
                tx.capture_admission_budgeted(&mut budget).await
            } else {
                tx.validate_initial_repository(&principal,limits).await?;
                tx.capture_admission(limits).await
            }
        }.await;
        let result = tx.record(result);
        if result.is_err() {
            return Err(tx.commit().await.unwrap_err());
        }
        let admission = result;
        cleanup.commit_attempted();
        tx.commit().await?;
        cleanup.committed();
        let admission = admission?;
        identity.observe()?;
        Ok(Self {
            store,
            principal: Principal {
                repository: options.repository_id.into(),
                owner: options.owner_id.into(),
            },
            admission,
        })
        }.await;
        cleanup.finish(result)
    }
    /// Open an existing complete repository under local operating-system authority.
    ///
    /// The durable repository and owner identities are read from this file rather
    /// than accepted as caller-selected capability strings.
    /// Supports bound catalog1 and the exact catalog6 initialized, tracked and private recorded-history family with zero or one checkout registration, without migration.
    /// A marker of an older Gossil catalog (2, 4 or 5) is refused before any effect, with the three-line message of CAT-07 that names `path`.
    /// Catalog6 requires the immutable shared root and exact supported registration, tracking, scope and native history projections; unrelated local tables remain empty.
    /// # Arguments
    /// * `path` - Existing local database under operating-system owner authority.
    /// * `admission_limits` - Explicit finite root decode and parser budgets.
    /// # Returns
    /// An admitted owner bound to this file, its repository and its stored local owner.
    /// # Examples
    /// ```no_run
    /// async fn open(path: &std::path::Path, limits: gossil::storage::AdmissionLimits) {
    ///     let owner = gossil::storage::LocalOwnerRepository::open(path, limits).await.unwrap();
    ///     drop(owner);
    /// }
    /// ```
    /// # Errors
    /// Rejects missing policy, older and unknown markers, corrupt bindings and engine errors.
    pub async fn open(
        path: impl AsRef<Path>,
        admission_limits: AdmissionLimits,
    ) -> Result<Self, StorageError> {
        let path = path.as_ref();
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|error| StorageError::caused("local repository file unavailable", error))?;
        let metadata = repository_metadata(file.metadata())?;
        if !metadata.is_file() {
            return Err(StorageError::state(
                "local repository is not a regular file",
            ));
        }
        let identity = super::FileIdentity::from_metadata(path, true, &metadata)?;
        drop(file);
        let mut store = LocalStore::open_observed(path, true, Some(identity.clone())).await?;
        let mut tx = store.begin_write().await?;
        let result = async {
            if tx.supported_catalog_version().await? == NEWEST_CATALOG {
                let mut budget=super::content::Budget::new(admission_limits.content)?;
                let mut parse=ParseWork::new(admission_limits.parser)?;
                let principal=tx.read_recording_principal_budgeted(&mut budget).await?;
                tx.validate_recording_repository(&principal,admission_limits,&mut budget,&mut parse).await?;
                let admission=tx.capture_admission_budgeted(&mut budget).await?;
                return Ok((principal,admission));
            }
            let rows = tx.query_bound("SELECT s.repository_id,o.owner_id FROM g_storage s CROSS JOIN g_local_owner o WHERE s.singleton=1 AND o.singleton=1", vec![]).await?;
            let [row] = rows.as_slice() else { return Err(StorageError::state("local repository binding unavailable")); };
            let [Value::Text(repository), Value::Text(owner)] = row.as_slice() else { return Err(StorageError::state("corrupt local repository binding")); };
            let principal = Principal { repository: repository.clone(), owner: owner.clone() };
            tx.validate_principal(&principal).await?;
            tx.validate_initial_repository(&principal,admission_limits).await?;
            let admission=tx.capture_admission(admission_limits).await?;
            Ok((principal,admission))
        }.await;
        let result = tx.record(result);
        tx.commit()
            .await
            .map_err(|error| error.naming_repository(path))?;
        let (principal, admission) = result.map_err(|error| error.naming_repository(path))?;
        identity.observe()?;
        Ok(Self {
            store,
            principal,
            admission,
        })
    }
    /// Store verified private file bytes, independently of artifact interpretation.
    /// # Arguments
    /// * `native_name` - Full lowercase supported native digest of `bytes`.
    /// * `bytes` - Exact file content; no artifact grammar is imposed.
    /// * `limits` - Explicit finite content and work bounds.
    /// # Returns
    /// Success after verified storage or supported deduplication commits with its required classification.
    /// # Examples
    /// ```no_run
    /// async fn put(owner: &mut gossil::storage::LocalOwnerRepository, hash: &str, bytes: &[u8], limits: gossil::storage::Limits) {
    ///     owner.put_private_file_content(hash, bytes, limits).await.unwrap();
    /// }
    /// ```
    /// # Errors
    /// Rejects policy, hash, size, codec and limit errors, or required unresolved crosslinks.
    /// Private writes are unsupported for the empty-root catalog6 scope.
    pub async fn put_private_file_content(
        &mut self,
        native_name: &str,
        bytes: &[u8],
        limits: Limits,
    ) -> Result<(), StorageError> {
        let mut tx = self.store.begin_write().await?;
        let validation = tx.check_admission(&self.principal, &self.admission).await;
        if validation.is_err() {
            return tx.commit().await;
        }
        let supported = async {
            if tx
                .query_bound(
                    "SELECT storage_version FROM g_storage WHERE singleton=1",
                    vec![],
                )
                .await?
                != vec![vec![Value::Integer(1)]]
            {
                return Err(StorageError::state(
                    "private writes unsupported for empty catalog6 scope",
                ));
            }
            Ok(())
        }
        .await;
        if tx.record(supported).is_err() {
            return tx.commit().await;
        }
        let result = tx
            .put_private_file_content(&self.principal, native_name, bytes, limits)
            .await;
        let termination = tx.commit().await;
        if let Some(connection) = &self.store.connection {
            self.admission.total_changes = connection.total_changes();
        }
        termination?;
        result.map(|_| ())
    }
    /// Read exact full content as the local owner using explicit finite limits.
    /// # Arguments
    /// * `native_name` - Full native name requested under this local owner's scope.
    /// * `limits` - Explicit finite envelope, output and work ceilings.
    /// # Returns
    /// Owned exact bytes after all intermediate hashes and sizes validate.
    /// # Examples
    /// ```no_run
    /// async fn get(owner: &mut gossil::storage::LocalOwnerRepository, hash: &str, limits: gossil::storage::Limits) {
    ///     let bytes = owner.read_content(hash, limits).await.unwrap();
    ///     drop(bytes);
    /// }
    /// ```
    /// # Errors
    /// Rejects policy changes, unavailable/shunned content, corruption and exhausted limits.
    pub async fn read_content(
        &mut self,
        native_name: &str,
        limits: Limits,
    ) -> Result<Vec<u8>, StorageError> {
        let mut tx = self.store.begin_write().await?;
        if self.admission.content_limits.is_some() {
            let mut work = super::content::Budget::new(limits)?;
            let result = async {
                tx.check_admission_budgeted(&self.principal, &self.admission, &mut work)
                    .await?;
                tx.read_recording_content_budget(&self.principal, native_name, &mut work)
                    .await
            }
            .await;
            let result = tx.record(result);
            tx.commit().await?;
            return result;
        }
        let validation = tx.check_admission(&self.principal, &self.admission).await;
        if let Err(error) = validation {
            let cleanup = tx.commit().await;
            return Err(cleanup.err().unwrap_or(error));
        }
        let bytes = tx
            .read_owner_content(&self.principal, native_name, limits)
            .await;
        tx.commit().await?;
        bytes
    }
}

/// Lists the older Gossil catalogs that a marker may name.
///
/// Each is refused before any effect (CAT-07). Catalog 3 was never defined, so its
/// marker is unknown, as is every marker above [`NEWEST_CATALOG`].
const OLDER_CATALOGS: [i64; 3] = [2, 4, 5];

/// Decode the fixed-size supported-catalog aggregate without owning raw markers.
/// # Arguments
/// * `rows` - Count, integer-singleton count and its integer version from the fixed query.
/// # Returns
/// Exactly catalog1 or the newest catalog for one valid marker.
/// # Errors
/// Refuses one marker of an older Gossil catalog with an [`super::OlderCatalog`] source, and
/// refuses missing, ambiguous, malformed and unknown markers.
fn supported_catalog_from_rows(rows: &[Vec<Value>]) -> Result<i64, StorageError> {
    match rows {
        [row] => match row.as_slice() {
            [
                Value::Integer(1),
                Value::Integer(1),
                Value::Integer(version),
            ] if *version == 1 || *version == NEWEST_CATALOG => Ok(*version),
            [
                Value::Integer(1),
                Value::Integer(1),
                Value::Integer(version),
            ] if OLDER_CATALOGS.contains(version) => Err(StorageError::older_catalog(*version)),
            _ => Err(StorageError::state("unsupported storage version")),
        },
        _ => Err(StorageError::state("unsupported storage version")),
    }
}

impl super::WriteTransaction<'_> {
    /// Select an exact supported catalog using only fixed-size scalar results.
    /// # Returns
    /// INTEGER1 or the newest catalog for exactly one INTEGER singleton marker.
    /// # Errors
    /// Refuses missing/extra markers and every unsupported type/value before any
    /// raw marker or principal bytes can be owned by the provider.
    async fn supported_catalog_version(&mut self) -> Result<i64, StorageError> {
        let result=async {
            let rows=self.query_bound("SELECT count(*),coalesce(sum(CASE WHEN typeof(singleton)='integer' AND singleton=1 AND typeof(storage_version)='integer' THEN 1 ELSE 0 END),0),max(CASE WHEN typeof(singleton)='integer' AND singleton=1 AND typeof(storage_version)='integer' THEN storage_version ELSE NULL END) FROM g_storage",vec![]).await?;
            supported_catalog_from_rows(&rows)
        }.await;
        self.record(result)
    }

    /// Capture durable authority metadata without reading any content envelope.
    /// # Returns
    /// Owned exact durable authority rows, including catalog6 checkout keys and native receive records.
    /// # Errors
    /// Returns SQL execution or owned value-decoding errors.
    async fn authority_snapshot(&mut self) -> Result<Vec<Vec<Vec<Value>>>, StorageError> {
        let mut snapshot = Vec::new();
        for sql in [
            "SELECT * FROM g_storage ORDER BY singleton",
            "SELECT * FROM g_local_owner ORDER BY singleton",
            "SELECT * FROM g_public_receipt ORDER BY native_hash,validation_class,supporting_root",
            "SELECT * FROM g_checkout ORDER BY checkout_id",
            "SELECT * FROM g_checkout_owner ORDER BY checkout_id",
            "SELECT b.rid,b.size,b.uuid,length(b.content),p.rid,d.srcid FROM blob b LEFT JOIN private p ON p.rid=b.rid LEFT JOIN delta d ON d.rid=b.rid WHERE b.uuid IN(SELECT supporting_root FROM g_public_receipt) ORDER BY b.rid",
            "SELECT * FROM event WHERE objid IN(SELECT rid FROM blob WHERE uuid IN(SELECT supporting_root FROM g_public_receipt)) ORDER BY objid",
            "SELECT * FROM tagxref WHERE rid IN(SELECT rid FROM blob WHERE uuid IN(SELECT supporting_root FROM g_public_receipt)) ORDER BY rid,tagid LIMIT 3",
            "SELECT * FROM leaf WHERE rid IN(SELECT rid FROM blob WHERE uuid IN(SELECT supporting_root FROM g_public_receipt)) ORDER BY rid",
        ] {
            snapshot.push(self.query_bound(sql, vec![]).await?);
        }
        if self
            .query_bound(
                "SELECT storage_version FROM g_storage WHERE singleton=1",
                vec![],
            )
            .await?
            == vec![vec![Value::Integer(NEWEST_CATALOG)]]
        {
            snapshot.push(
                self.query_bound("SELECT * FROM g_vvar ORDER BY checkout_id,name", vec![])
                    .await?,
            );
            snapshot.push(
                self.query_bound("SELECT * FROM g_vfile ORDER BY checkout_id,id", vec![])
                    .await?,
            );
            snapshot.push(
                self.query_bound("SELECT * FROM rcvfrom ORDER BY rcvid", vec![])
                    .await?,
            );
        }
        let roots = self.query_bound("SELECT rid,uuid FROM blob WHERE uuid IN(SELECT supporting_root FROM g_public_receipt) ORDER BY rid LIMIT 2",vec![]).await?;
        for root in roots {
            snapshot.push(self.query_bound(ROOT_PROJECTION_CONFLICTS, root).await?);
        }
        Ok(snapshot)
    }
    /// Capture authority and connection counters after successful validation.
    /// # Arguments
    /// * `limits` - Finite admission ceilings for the bounded catalog6 authority snapshot.
    /// # Returns
    /// The sealed authority snapshot and current connection counters.
    /// # Errors
    /// Returns SQL execution or owned value-decoding errors while capturing authority.
    /// For catalog6, also rejects invalid limits, exhausted work, invalid authority
    /// metadata, allocation failures and failed provider observations.
    async fn capture_admission(
        &mut self,
        limits: AdmissionLimits,
    ) -> Result<AdmissionProof, StorageError> {
        if self.supported_catalog_version().await? == NEWEST_CATALOG {
            let mut budget = super::content::Budget::new(limits.content)?;
            return self.capture_admission_budgeted(&mut budget).await;
        }
        let authority = self.authority_snapshot().await?;
        let data_version = self.query_bound("PRAGMA data_version", vec![]).await?;
        Ok(AdmissionProof {
            authority,
            data_version,
            total_changes: self.connection().total_changes(),
            content_limits: None,
        })
    }
    /// Any unaccounted connection mutation or external commit requires explicit readmission.
    /// # Arguments
    /// * `principal` - Sealed local repository and owner binding.
    /// * `proof` - Previously admitted authority and connection counters.
    /// # Returns
    /// Success when no unaccounted mutation has invalidated admission.
    /// # Errors
    /// Rejects mismatched connection counters or durable authority, invalid policy and SQL failures.
    async fn check_admission(
        &mut self,
        principal: &Principal,
        proof: &AdmissionProof,
    ) -> Result<(), StorageError> {
        if let Some(limits) = proof.content_limits {
            let mut budget = super::content::Budget::new(limits)?;
            return self
                .check_admission_budgeted(principal, proof, &mut budget)
                .await;
        }
        let result = async {
            if self.connection().total_changes() != proof.total_changes {
                return Err(StorageError::state(
                    "repository admission stale; reopen with explicit admission limits",
                ));
            }
            let version = self.query_bound("PRAGMA data_version", vec![]).await?;
            if version != proof.data_version {
                return Err(StorageError::state(
                    "repository admission stale; reopen with explicit admission limits",
                ));
            }
            self.validate_principal(principal).await?;
            if self.authority_snapshot().await? != proof.authority {
                return Err(StorageError::state(
                    "repository authority changed; explicit readmission required",
                ));
            }
            Ok(())
        }
        .await;
        self.record(result)
    }
    /// Native crosslink path for the currently supported complete check-in closure.
    /// # Arguments
    /// * `rid` - Native artifact row identifier.
    /// * `parsed` - Bounded parsed artifact to validate.
    /// # Returns
    /// Success after installing the supported empty-root crosslinks.
    /// # Errors
    /// Rejects unsupported root semantics, native date failures and SQL errors.
    async fn crosslink_supported_checkin(
        &mut self,
        rid: i64,
        parsed: &crate::artifact::ParsedArtifact,
    ) -> Result<(), StorageError> {
        let result = async {
            validate_empty_root(parsed)?;
            let date = parsed.date().ok_or_else(|| StorageError::state("check-in date unavailable"))?;
            let user = parsed.user().ok_or_else(|| StorageError::state("check-in user unavailable"))?;
            let user = std::str::from_utf8(&user.bytes[..user.c_string_len]).map_err(|e| StorageError::caused("native check-in user UTF-8",e))?;
            // manifest_add_checkin_linkages has no plink/mlink/filename effects for this no-parent/no-file manifest.
            self.execute_bound("INSERT INTO event(type,mtime,objid,user,comment,omtime) VALUES('ci',?1,?2,?3,'initial empty check-in',?1)", vec![Value::Real(date),Value::Integer(rid),Value::Text(user.into())]).await?;
            self.execute_bound("INSERT OR IGNORE INTO tag(tagname) VALUES('sym-trunk')",vec![]).await?;
            self.execute_bound("REPLACE INTO tagxref(tagid,tagtype,srcid,origid,value,mtime,rid) VALUES(8,2,?1,?1,'trunk',?2,?1)",vec![Value::Integer(rid),Value::Real(date)]).await?;
            self.execute_bound("REPLACE INTO tagxref(tagid,tagtype,srcid,origid,value,mtime,rid) SELECT tagid,2,?1,?1,NULL,?2,?1 FROM tag WHERE tagname='sym-trunk'",vec![Value::Integer(rid),Value::Real(date)]).await?;
            // Native leaf_check after the branch tag; the validated closure has no descendants.
            self.execute_bound("INSERT OR IGNORE INTO leaf VALUES(?1)",vec![Value::Integer(rid)]).await?;
            Ok(())
        }.await;
        self.record(result)
    }
    /// Refuse every unimplemented local/native state in the unregistered catalog6 scope.
    ///
    /// The repository holds the native empty root at RID 1 and receipt 1, and any
    /// number of private issue creations. Every other row is refused.
    /// # Arguments
    /// * `root` - Authenticated canonical native hash.
    /// * `principal` - Sealed repository and owner binding.
    /// * `limits` - Native root, issue and registration text ceilings.
    /// * `work` - The operation's content ledger.
    /// * `parse` - The operation's parser ledger.
    /// # Returns
    /// Success for one native empty root with only admitted issues and no checkout registration.
    /// # Errors
    /// Rejects populated tables, extra native history, issue rows that differ from
    /// replay and unexplained receipts.
    async fn validate_empty_catalog6(
        &mut self,
        root: &str,
        principal: &Principal,
        limits: AdmissionLimits,
        work: &mut super::content::Budget,
        parse: &mut ParseWork,
    ) -> Result<(), StorageError> {
        if self
            .query_bound(
                "SELECT singleton FROM g_local_owner ORDER BY singleton LIMIT 2",
                vec![],
            )
            .await?
            != vec![vec![Value::Integer(1)]]
        {
            return Err(StorageError::state(
                "unsupported catalog6 owner cardinality",
            ));
        }
        /// Complete local and native table inventory that must be empty for catalog6 admission.
        const EMPTY_TABLES: &[(&str, &str)] = &[
            ("g_admission", "SELECT EXISTS(SELECT 1 FROM g_admission)"),
            (
                "g_checkout_project",
                "SELECT EXISTS(SELECT 1 FROM g_checkout_project)",
            ),
            ("g_code_link", "SELECT EXISTS(SELECT 1 FROM g_code_link)"),
            (
                "g_command_audit",
                "SELECT EXISTS(SELECT 1 FROM g_command_audit)",
            ),
            (
                "g_contributor",
                "SELECT EXISTS(SELECT 1 FROM g_contributor)",
            ),
            ("g_delivery", "SELECT EXISTS(SELECT 1 FROM g_delivery)"),
            (
                "g_delivery_lane",
                "SELECT EXISTS(SELECT 1 FROM g_delivery_lane)",
            ),
            ("g_dependency", "SELECT EXISTS(SELECT 1 FROM g_dependency)"),
            ("g_descriptor", "SELECT EXISTS(SELECT 1 FROM g_descriptor)"),
            ("g_diagnostic", "SELECT EXISTS(SELECT 1 FROM g_diagnostic)"),
            (
                "g_discussion_message",
                "SELECT EXISTS(SELECT 1 FROM g_discussion_message)",
            ),
            (
                "g_discussion_target",
                "SELECT EXISTS(SELECT 1 FROM g_discussion_target)",
            ),
            (
                "g_discussion_thread",
                "SELECT EXISTS(SELECT 1 FROM g_discussion_thread)",
            ),
            ("g_file_undo", "SELECT EXISTS(SELECT 1 FROM g_file_undo)"),
            (
                "g_gest_artifacts",
                "SELECT EXISTS(SELECT 1 FROM g_gest_artifacts)",
            ),
            (
                "g_gest_authors",
                "SELECT EXISTS(SELECT 1 FROM g_gest_authors)",
            ),
            (
                "g_gest_entity_tags",
                "SELECT EXISTS(SELECT 1 FROM g_gest_entity_tags)",
            ),
            (
                "g_gest_iteration_tasks",
                "SELECT EXISTS(SELECT 1 FROM g_gest_iteration_tasks)",
            ),
            (
                "g_gest_iterations",
                "SELECT EXISTS(SELECT 1 FROM g_gest_iterations)",
            ),
            ("g_gest_notes", "SELECT EXISTS(SELECT 1 FROM g_gest_notes)"),
            (
                "g_gest_relationships",
                "SELECT EXISTS(SELECT 1 FROM g_gest_relationships)",
            ),
            ("g_gest_tags", "SELECT EXISTS(SELECT 1 FROM g_gest_tags)"),
            (
                "g_git_checkout",
                "SELECT EXISTS(SELECT 1 FROM g_git_checkout)",
            ),
            ("g_identity", "SELECT EXISTS(SELECT 1 FROM g_identity)"),
            (
                "g_inverse_chain",
                "SELECT EXISTS(SELECT 1 FROM g_inverse_chain)",
            ),
            ("g_journal", "SELECT EXISTS(SELECT 1 FROM g_journal)"),
            (
                "g_journal_file",
                "SELECT EXISTS(SELECT 1 FROM g_journal_file)",
            ),
            (
                "g_local_command",
                "SELECT EXISTS(SELECT 1 FROM g_local_command)",
            ),
            ("g_mergestat", "SELECT EXISTS(SELECT 1 FROM g_mergestat)"),
            (
                "g_message_context",
                "SELECT EXISTS(SELECT 1 FROM g_message_context)",
            ),
            (
                "g_message_context_target",
                "SELECT EXISTS(SELECT 1 FROM g_message_context_target)",
            ),
            ("g_path_scope", "SELECT EXISTS(SELECT 1 FROM g_path_scope)"),
            (
                "g_pending_path",
                "SELECT EXISTS(SELECT 1 FROM g_pending_path)",
            ),
            (
                "g_project_binding",
                "SELECT EXISTS(SELECT 1 FROM g_project_binding)",
            ),
            (
                "g_project_workspaces",
                "SELECT EXISTS(SELECT 1 FROM g_project_workspaces)",
            ),
            (
                "g_record_diagnostic",
                "SELECT EXISTS(SELECT 1 FROM g_record_diagnostic)",
            ),
            ("g_selector", "SELECT EXISTS(SELECT 1 FROM g_selector)"),
            (
                "g_selector_member",
                "SELECT EXISTS(SELECT 1 FROM g_selector_member)",
            ),
            (
                "g_selector_revision",
                "SELECT EXISTS(SELECT 1 FROM g_selector_revision)",
            ),
            ("g_stash", "SELECT EXISTS(SELECT 1 FROM g_stash)"),
            ("g_stashfile", "SELECT EXISTS(SELECT 1 FROM g_stashfile)"),
            ("g_suggestion", "SELECT EXISTS(SELECT 1 FROM g_suggestion)"),
            (
                "g_suggestion_application",
                "SELECT EXISTS(SELECT 1 FROM g_suggestion_application)",
            ),
            (
                "g_target_entity_member",
                "SELECT EXISTS(SELECT 1 FROM g_target_entity_member)",
            ),
            ("g_undo_stash", "SELECT EXISTS(SELECT 1 FROM g_undo_stash)"),
            (
                "g_undo_stashfile",
                "SELECT EXISTS(SELECT 1 FROM g_undo_stashfile)",
            ),
            ("g_undo_vfile", "SELECT EXISTS(SELECT 1 FROM g_undo_vfile)"),
            (
                "g_undo_vmerge",
                "SELECT EXISTS(SELECT 1 FROM g_undo_vmerge)",
            ),
            ("g_vmerge", "SELECT EXISTS(SELECT 1 FROM g_vmerge)"),
            ("accesslog", "SELECT EXISTS(SELECT 1 FROM accesslog)"),
            ("admin_log", "SELECT EXISTS(SELECT 1 FROM admin_log)"),
            ("attachment", "SELECT EXISTS(SELECT 1 FROM attachment)"),
            ("backlink", "SELECT EXISTS(SELECT 1 FROM backlink)"),
            ("chat", "SELECT EXISTS(SELECT 1 FROM chat)"),
            ("cherrypick", "SELECT EXISTS(SELECT 1 FROM cherrypick)"),
            ("concealed", "SELECT EXISTS(SELECT 1 FROM concealed)"),
            ("delta", "SELECT EXISTS(SELECT 1 FROM delta)"),
            ("filename", "SELECT EXISTS(SELECT 1 FROM filename)"),
            ("forumpost", "SELECT EXISTS(SELECT 1 FROM forumpost)"),
            ("mlink", "SELECT EXISTS(SELECT 1 FROM mlink)"),
            ("modreq", "SELECT EXISTS(SELECT 1 FROM modreq)"),
            ("orphan", "SELECT EXISTS(SELECT 1 FROM orphan)"),
            ("phantom", "SELECT EXISTS(SELECT 1 FROM phantom)"),
            ("plink", "SELECT EXISTS(SELECT 1 FROM plink)"),
            ("shun", "SELECT EXISTS(SELECT 1 FROM shun)"),
            (
                "transaction_events",
                "SELECT EXISTS(SELECT 1 FROM transaction_events)",
            ),
            ("transactions", "SELECT EXISTS(SELECT 1 FROM transactions)"),
            ("unsent", "SELECT EXISTS(SELECT 1 FROM unsent)"),
            ("unversioned", "SELECT EXISTS(SELECT 1 FROM unversioned)"),
        ];
        for &(table, sql) in EMPTY_TABLES {
            // SQL literals enumerate the fixed reviewed schema inventory above.
            if self.query_bound(sql, vec![]).await? != vec![vec![Value::Integer(0)]] {
                return Err(StorageError::state(format!(
                    "unsupported nonempty catalog6 table: {table}"
                )));
            }
        }
        self.recording_preflight(limits, work).await?;
        let issues = self.admit_issues(principal, limits, work, parse).await?;
        let (_, tags) = issue_ticket::assign_tags(None, "", &issues);
        self.expect_shared_tables(
            recorded_history::SharedTables {
                shared_rid: 1,
                shared_receipt: 1,
                plink: vec![],
                mlink: vec![],
                events: vec![],
                tagxref: vec![],
                tags,
                filenames: vec![],
                leaves: vec![vec![Value::Integer(1)]],
                unsent: std::collections::BTreeSet::new(),
                referenced: [(1, (root.to_owned(), 1))].into(),
                owners: vec![],
            },
            &issues,
            limits.parser.entries,
            limits.parser.input_bytes,
            work,
        )
        .await?;
        let identities = self
            .query_bound("SELECT value FROM config WHERE name='server-code'", vec![])
            .await?;
        if !matches!(identities.as_slice(), [row] if matches!(row.as_slice(), [Value::Text(value)] if value.len()==40 && value.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))))
        {
            return Err(StorageError::state("native server identity unavailable"));
        }
        self.validate_initial_checkout(principal, 1, root, limits)
            .await?;
        Ok(())
    }
    /// Validate every supported public receipt and checkout root from exact bytes.
    /// # Arguments
    /// * `principal` - Sealed local repository and owner binding.
    /// * `limits` - Explicit finite bounds for this operation.
    /// # Returns
    /// Success when the complete empty repository passes admission.
    /// # Errors
    /// Rejects unsupported schema or versions, invalid policy, bindings, root content or crosslinks.
    async fn validate_initial_repository(
        &mut self,
        principal: &Principal,
        limits: AdmissionLimits,
    ) -> Result<(), StorageError> {
        // Catalog1 retains its original validator and all original conflict guards.
        if self.supported_catalog_version().await? == NEWEST_CATALOG {
            let mut budget = super::content::Budget::new(limits.content)?;
            let mut parse = ParseWork::new(limits.parser)?;
            return self
                .validate_recording_repository(principal, limits, &mut budget, &mut parse)
                .await;
        }
        let result = async {
            self.validate_principal(principal).await?;
            if !self.query_bound("PRAGMA foreign_key_check",vec![]).await?.is_empty() { return Err(StorageError::state("repository foreign key integrity failure")); }
            let roots = self.query_bound("SELECT DISTINCT supporting_root FROM g_public_receipt ORDER BY supporting_root LIMIT 2",vec![]).await?;
            if roots.len() != 1 { return Err(StorageError::state("supported public root closure unavailable")); }
            let [Value::Text(root)] = roots[0].as_slice() else { return Err(StorageError::state("corrupt public root binding")); };
            let receipts = self.query_bound("SELECT native_hash,supporting_root,validation_class FROM g_public_receipt ORDER BY native_hash,supporting_root LIMIT 2",vec![]).await?;
            if receipts != vec![vec![Value::Text(root.clone()),Value::Text(root.clone()),Value::Text("local-explicit-publication".into())]] { return Err(StorageError::state("unsupported required public graph closure")); }
            let mut budget = super::content::Budget::new(limits.content)?;
            let bytes = self.read_owner_content_budget(principal,root,&mut budget).await?;
            let version = self.query_bound("SELECT storage_version FROM g_storage WHERE singleton=1",vec![]).await?;
            let parsed = if version == vec![vec![Value::Integer(NEWEST_CATALOG)]] {
                let mut parse_work = ParseWork::new(limits.parser)?;
                parse_owned_budgeted(self,&bytes,&mut parse_work).await?
            } else { parse_owned(self,&bytes,limits.parser).await? };
            validate_empty_root(&parsed)?;
            let rows = self.query_bound("SELECT b.rid,e.type,e.mtime,e.user,e.comment,e.omtime,e.tagid,e.uid,e.bgcolor,e.euser,e.ecomment,e.brief FROM blob b JOIN event e ON e.objid=b.rid JOIN leaf l ON l.rid=b.rid WHERE b.uuid=?1 AND NOT EXISTS(SELECT 1 FROM private p WHERE p.rid=b.rid) AND NOT EXISTS(SELECT 1 FROM delta d WHERE d.rid=b.rid)",vec![Value::Text(root.clone())]).await?;
            let [row] = rows.as_slice() else { return Err(StorageError::state("native public root crosslink unavailable")); };
            let Some(Value::Integer(rid)) = row.first() else { return Err(StorageError::state("corrupt root RID")); };
            let user = parsed.user().unwrap();
            let user = std::str::from_utf8(&user.bytes[..user.c_string_len]).map_err(|e| StorageError::caused("root user UTF-8",e))?;
            let date = parsed.date().unwrap();
            let stored_date = native_stored_date(date);
            if row != &vec![Value::Integer(*rid),Value::Text("ci".into()),stored_date.clone(),Value::Text(user.into()),Value::Text("initial empty check-in".into()),stored_date.clone(),Value::Null,Value::Null,Value::Null,Value::Null,Value::Null,Value::Null] { return Err(StorageError::state("root event differs from canonical bytes")); }
            if self.query_bound(ROOT_PROJECTION_CONFLICTS,vec![Value::Integer(*rid),Value::Text(root.clone())]).await? != vec![vec![Value::Integer(0);ROOT_PROJECTION_FLAGS]] {
                return Err(StorageError::state("root projection contradicts canonical bytes"));
            }
            let tags = self.query_bound("SELECT t.tagname,x.tagtype,x.srcid,x.origid,x.value,x.mtime FROM tagxref x LEFT JOIN tag t ON t.tagid=x.tagid WHERE x.rid=?1 ORDER BY t.tagname LIMIT 3",vec![Value::Integer(*rid)]).await?;
            let expected = vec![vec![Value::Text("branch".into()),Value::Integer(2),Value::Integer(*rid),Value::Integer(*rid),Value::Text("trunk".into()),stored_date.clone()],vec![Value::Text("sym-trunk".into()),Value::Integer(2),Value::Integer(*rid),Value::Integer(*rid),Value::Null,stored_date.clone()]];
            if tags != expected { return Err(StorageError::state("root branch crosslinks differ from canonical bytes")); }
            let checkouts = self.query_bound("SELECT c.shared_head,c.combined_head,c.generation,c.pending_version,c.availability,o.owner_id FROM g_checkout c LEFT JOIN g_checkout_owner o ON o.checkout_id=c.checkout_id",vec![]).await?;
            if checkouts.is_empty() || checkouts.iter().any(|row| row != &vec![Value::Text(root.clone()),Value::Null,Value::Integer(0),Value::Integer(0),Value::Text("ready".into()),Value::Text(principal.owner.clone())]) { return Err(StorageError::state("unsupported or unavailable checkout root closure")); }
            use std::os::unix::fs::MetadataExt;
            let bindings = self.query_bound("SELECT c.root,o.root_device,o.root_inode FROM g_checkout c LEFT JOIN g_checkout_owner o ON o.checkout_id=c.checkout_id ORDER BY c.checkout_id",vec![]).await?;
            for row in bindings {
                let [Value::Text(path),Value::Text(device),Value::Text(inode)] = row.as_slice() else { return Err(StorageError::state("checkout root binding unavailable")); };
                let metadata = std::fs::symlink_metadata(path).map_err(|e|StorageError::caused("registered checkout root unavailable",e))?;
                if !metadata.is_dir() || metadata.dev().to_string()!=*device || metadata.ino().to_string()!=*inode { return Err(StorageError::state("registered checkout root identity differs")); }
            }
            Ok(())
        }.await;
        self.record(result)
    }
}
#[cfg(test)]
mod boundary_injection {
    use std::cell::RefCell;
    thread_local! {
        pub(super) static DATES: RefCell<Option<Vec<libsql::Value>>> = const { RefCell::new(None) };
        pub(super) static METADATA: RefCell<Option<i32>> = const { RefCell::new(None) };
    }
    pub(super) struct Reset;
    impl Reset {
        pub(super) fn consumed(&self) {
            DATES.with(|slot| assert!(slot.borrow().is_none()));
            METADATA.with(|slot| assert!(slot.borrow().is_none()));
        }
    }
    impl Drop for Reset {
        fn drop(&mut self) {
            DATES.with(|slot| *slot.borrow_mut() = None);
            METADATA.with(|slot| *slot.borrow_mut() = None);
        }
    }
}
/// Validate the text encoding required by a canonical checkout root's stored binding.
/// # Arguments
/// * `root` - Canonical checkout directory path obtained by the caller.
/// # Returns
/// The exact UTF-8 path text, without lossy conversion.
/// # Errors
/// Rejects native path bytes that cannot be stored as the checkout's text root.
fn checkout_root_text(root: &Path) -> Result<&str, StorageError> {
    root.to_str()
        .ok_or_else(|| StorageError::state("checkout root is not UTF-8"))
}
/// Preserve metadata IO causes before repository admission can acquire authority.
/// # Arguments
/// * `result` - Operating-system metadata result whose cause must be retained.
/// # Returns
/// Owned file metadata, retaining any original operating-system error.
/// # Errors
/// Returns a storage error retaining the original operating-system metadata cause.
fn repository_metadata(
    result: std::io::Result<std::fs::Metadata>,
) -> Result<std::fs::Metadata, StorageError> {
    #[cfg(test)]
    let result = boundary_injection::METADATA
        .with(|slot| slot.borrow_mut().take())
        .map_or(result, |code| Err(std::io::Error::from_raw_os_error(code)));
    result.map_err(|error| StorageError::caused("local repository metadata unavailable", error))
}
/// Match NUMERIC affinity for an already validated native Julian day.
/// # Arguments
/// * `date` - Finite native day in SQLite's supported range, 0 through 5373484.5.
/// # Returns
/// An integer for an integral day, otherwise its original floating-point value.
/// # Errors
/// This conversion cannot fail for a validated native day.
fn native_stored_date(date: f64) -> Value {
    if date.fract() == 0.0 {
        Value::Integer(date as i64)
    } else {
        Value::Real(date)
    }
}
/// Decode the fully stepped single native date result without retaining engine rows.
/// # Arguments
/// * `values` - Owned SQL values bound to that statement.
/// # Returns
/// The finite native Julian day or native NULL rejection.
/// # Errors
/// Rejects unexpected row shape, nonnumeric values and nonfinite Julian days.
fn decode_native_date(values: Vec<Value>) -> Result<Option<f64>, StorageError> {
    match values.as_slice() {
        [] => Err(StorageError::state("native date provider returned no row")),
        [Value::Null] => Ok(None),
        [Value::Real(day)] if day.is_finite() => Ok(Some(*day)),
        [_] => Err(StorageError::state("invalid native date provider row")),
        _ => Err(StorageError::state(
            "native date provider returned extra rows",
        )),
    }
}
/// Borrow the same owner for native SQLite date conversion; no separate engine or shim.
struct OwnerDates<'a, 'b>(&'a super::WriteTransaction<'b>);
impl crate::artifact::DateProvider for OwnerDates<'_, '_> {
    async fn julianday(
        &self,
        token: Option<&[u8]>,
    ) -> Result<Option<f64>, crate::artifact::DateProviderError> {
        let result = async {
            if let Some(error) = &self.0.failure {
                return Err(error.clone());
            }
            let value = match token {
                None => Value::Null,
                Some(bytes) => Value::Blob(bytes.iter().copied().take_while(|b| *b != 0).collect()),
            };
            let statement = self
                .0
                .connection()
                .prepare("SELECT julianday(CAST(?1 AS TEXT))")
                .await?;
            let mut rows = statement.query(vec![value]).await?;
            let mut owned = Vec::new();
            while let Some(row) = rows.next().await? {
                owned.push(row.get_value(0)?);
            }
            #[cfg(test)]
            let owned = boundary_injection::DATES
                .with(|slot| slot.borrow_mut().take())
                .unwrap_or(owned);
            decode_native_date(owned)
        }
        .await;
        result.map_err(|e| crate::artifact::DateProviderError(Box::new(e)))
    }
}
/// One complete operation's remaining logical parser counters; no defaults or hidden replenishment.
pub(super) struct ParseWork {
    /// Unconsumed logical work available to this parser.
    remaining: crate::artifact::ParseLimits,
    /// True when the limits bound each artifact alone, so a charge subtracts nothing.
    per_artifact: bool,
}
impl ParseWork {
    /// Start one validated finite operation budget without inherited charges.
    /// # Arguments
    /// * `limits` - Positive aggregate input-byte, card, owned-byte and entry ceilings.
    /// # Returns
    /// A parser work ledger whose remaining allowance equals the declared ceilings.
    /// # Errors
    /// Rejects aggregate parser limits when any input, card, owned-byte or entry ceiling is zero.
    pub(super) fn new(limits: crate::artifact::ParseLimits) -> Result<Self, StorageError> {
        if [
            limits.input_bytes,
            limits.cards,
            limits.owned_bytes,
            limits.entries,
        ]
        .contains(&0)
        {
            return Err(StorageError::state("invalid aggregate parser limits"));
        }
        Ok(Self {
            remaining: limits,
            per_artifact: false,
        })
    }
    /// Start a parser ledger whose limits bound each artifact alone.
    ///
    /// `gos v verify` uses it, so that the proof has no ceiling on the whole repository.
    /// # Arguments
    /// * `limits` - Positive input-byte, card, owned-byte and entry ceilings of one artifact.
    /// # Returns
    /// A ledger whose remaining allowance stays equal to the ceilings.
    /// # Errors
    /// Rejects limits when any ceiling is zero.
    pub(super) fn per_artifact(limits: crate::artifact::ParseLimits) -> Result<Self, StorageError> {
        let mut work = Self::new(limits)?;
        work.per_artifact = true;
        Ok(work)
    }
    /// Subtract the parser's exact existing logical charges from the shared remaining work.
    /// # Arguments
    /// * `usage` - Exact input-byte, card, owned-byte and entry charges from one parser call.
    /// # Returns
    /// Unit after subtracting every charge from the remaining parser allowance.
    /// # Errors
    /// Rejects usage exceeding any remaining input-byte, card, owned-byte or entry allowance.
    pub(super) fn charge(
        &mut self,
        usage: crate::artifact::ParseUsage,
    ) -> Result<(), StorageError> {
        let exhausted =
            || StorageError::state("aggregate parser usage exceeds remaining caller limits");
        let before = self.remaining;
        let next = crate::artifact::ParseLimits {
            input_bytes: before
                .input_bytes
                .checked_sub(usage.input_bytes)
                .ok_or_else(exhausted)?,
            cards: before
                .cards
                .checked_sub(usage.cards)
                .ok_or_else(exhausted)?,
            owned_bytes: before
                .owned_bytes
                .checked_sub(usage.owned_bytes)
                .ok_or_else(exhausted)?,
            entries: before
                .entries
                .checked_sub(usage.entries)
                .ok_or_else(exhausted)?,
        };
        if !self.per_artifact {
            self.remaining = next;
        }
        Ok(())
    }
}
/// Parse on the actual owner with remaining aggregate ceilings checked before parser allocation.
/// # Arguments
/// * `tx` - Existing reserved owner transaction used for native date validation.
/// * `bytes` - Canonical artifact bytes to parse.
/// * `work` - Existing cumulative parser allowance to debit.
/// # Returns
/// The owned parsed artifact after native validation and exact usage charging.
/// # Errors
/// Returns an existing latched writer error without parsing. Otherwise propagates structural
/// artifact/date-provider failures and parser usage exceeding the caller's remaining work.
pub(super) async fn parse_owned_budgeted(
    tx: &mut super::WriteTransaction<'_>,
    bytes: &[u8],
    work: &mut ParseWork,
) -> Result<crate::artifact::ParsedArtifact, StorageError> {
    if let Some(error) = &tx.failure {
        return Err(error.clone());
    }
    let result = async {
        let (parsed, usage) =
            crate::artifact::parse_artifact_with_usage(bytes, work.remaining, &OwnerDates(tx))
                .await
                .map_err(|error| {
                    StorageError::caused("native artifact validation failed", error)
                })?;
        work.charge(usage)?;
        Ok(parsed)
    }
    .await;
    tx.record(result)
}

/// Parse bounded artifact bytes with native dates on the existing transaction owner.
/// # Arguments
/// * `tx` - Existing sole-connection transaction owner.
/// * `bytes` - Input bytes for this operation.
/// * `limits` - Explicit finite bounds for this operation.
/// # Returns
/// The bounded structural artifact parsed through the owned date provider.
/// # Errors
/// Returns bounded structural parsing or selected-provider date errors with their cause retained.
async fn parse_owned(
    tx: &super::WriteTransaction<'_>,
    bytes: &[u8],
    limits: crate::artifact::ParseLimits,
) -> Result<crate::artifact::ParsedArtifact, StorageError> {
    crate::artifact::parse_artifact(bytes, limits, &OwnerDates(tx))
        .await
        .map_err(|e| StorageError::caused("native artifact validation failed", e))
}
/// The initial root has no missing/native/typed dependencies; other closures stay unsupported.
/// # Arguments
/// * `parsed` - Bounded parsed artifact to validate.
/// # Returns
/// Success when the artifact has only supported empty-root semantics.
/// # Errors
/// Rejects any artifact type or card semantics outside the supported canonical empty root.
fn validate_empty_root(parsed: &crate::artifact::ParsedArtifact) -> Result<(), StorageError> {
    use crate::artifact::ArtifactType;
    fn token(value: Option<&crate::token_codec::DecodedToken>) -> Option<&[u8]> {
        value.map(|t| t.bytes.as_slice())
    }
    if parsed.artifact_type() != ArtifactType::Manifest
        || parsed.baseline().is_some()
        || !parsed.files().is_empty()
        || !parsed.parents().is_empty()
        || !parsed.cherrypicks().is_empty()
        || token(parsed.comment()) != Some(b"initial empty check-in".as_slice())
        || parsed.repository_checksum() != Some(b"d41d8cd98f00b204e9800998ecf8427e".as_slice())
        || parsed.user().is_none()
        || parsed.date().is_none()
        || parsed.tags().len() != 2
    {
        return Err(StorageError::state(
            "unsupported complete public root closure",
        ));
    }
    let user = parsed.user().unwrap();
    let date_token = parsed
        .date_token()
        .ok_or_else(|| StorageError::state("root D token unavailable"))?;
    for (tag, name, value) in [
        (
            &parsed.tags()[0],
            b"*branch".as_slice(),
            Some(b"trunk".as_slice()),
        ),
        (&parsed.tags()[1], b"*sym-trunk".as_slice(), None),
    ] {
        if tag.name().bytes != name || tag.target().is_some() || token(tag.value()) != value {
            return Err(StorageError::state("unsupported initial branch tags"));
        }
    }
    if empty_root_bytes(date_token, &user.bytes, parsed.raw().len())? != parsed.raw() {
        return Err(StorageError::state(
            "unsupported noncanonical initial root bytes",
        ));
    }
    Ok(())
}

/// Construct the exact native db_initial_setup empty-root artifact; parsing follows before admission.
/// # Arguments
/// * `date` - Validated native D-card token.
/// * `user` - Native U-card bytes to escape.
/// * `max_output` - Maximum complete canonical root length.
/// # Returns
/// Complete canonical empty-root bytes within the output ceiling.
/// # Errors
/// Rejects invalid date tokens, checked-length overflow, output limits and allocation failures.
fn empty_root_bytes(date: &[u8], user: &[u8], max_output: usize) -> Result<Vec<u8>, StorageError> {
    if date.is_empty()
        || date.iter().any(|b| b.is_ascii_whitespace() || *b == 0)
        || user.contains(&0)
    {
        return Err(StorageError::state("invalid empty-root date or user input"));
    }
    const PREFIX: &[u8] = b"C initial\\sempty\\scheck-in\nD ";
    const AFTER_DATE: &[u8] =
        b"\nR d41d8cd98f00b204e9800998ecf8427e\nT *branch * trunk\nT *sym-trunk *\nU ";
    const BEFORE_CHECKSUM: &[u8] = b"\nZ ";
    const CHECKSUM_BYTES: usize = 32;
    const END: &[u8] = b"\n";
    let user_size = crate::token_codec::fossilized_size(user, max_output).map_err(|error| {
        StorageError::caused("empty-root token size exceeds construction limit", error)
    })?;
    let total_size = [
        PREFIX.len(),
        date.len(),
        AFTER_DATE.len(),
        user_size,
        BEFORE_CHECKSUM.len(),
        CHECKSUM_BYTES,
        END.len(),
    ]
    .into_iter()
    .try_fold(0_usize, usize::checked_add)
    .filter(|size| *size <= max_output.min(isize::MAX as usize))
    .ok_or_else(|| StorageError::state("initial root construction limit"))?;
    // The complete canonical size passes before either variable-sized output allocation.
    let user = crate::token_codec::fossilize(user)
        .map_err(|error| StorageError::caused("empty-root user token encoding failed", error))?;
    fn append(output: &mut Vec<u8>, bytes: &[u8], max: usize) -> Result<(), StorageError> {
        output
            .len()
            .checked_add(bytes.len())
            .filter(|n| *n <= max)
            .ok_or_else(|| StorageError::state("initial root construction limit"))?;
        output
            .try_reserve_exact(bytes.len())
            .map_err(|e| StorageError::caused("initial root allocation failed", e))?;
        output.extend_from_slice(bytes);
        Ok(())
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(total_size)
        .map_err(|error| StorageError::caused("initial root allocation failed", error))?;
    append(&mut bytes, PREFIX, max_output)?;
    append(&mut bytes, date, max_output)?;
    append(&mut bytes, AFTER_DATE, max_output)?;
    append(&mut bytes, &user, max_output)?;
    append(&mut bytes, &BEFORE_CHECKSUM[..1], max_output)?;
    let checksum = crate::content_hash::hex(&crate::artifact_checksum::md5(&bytes));
    append(&mut bytes, &BEFORE_CHECKSUM[1..], max_output)?;
    append(&mut bytes, checksum.as_bytes(), max_output)?;
    append(&mut bytes, END, max_output)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    /// The root guard has one result flag for each of its conflict checks.
    #[test]
    fn root_guard_has_one_flag_per_conflict_check() {
        assert_eq!(
            ROOT_PROJECTION_CONFLICTS.matches("EXISTS(").count(),
            ROOT_PROJECTION_FLAGS
        );
    }

    use crate::{compression, content_hash};
    /// Allocation observations belong only to the current test thread and never alter production allocation.
    mod allocation_probe {
        use std::{
            alloc::{GlobalAlloc, Layout, System},
            cell::Cell,
        };
        thread_local! {
            /// Enable observations only on the current test thread.
            static CAPTURE: Cell<bool> = const { Cell::new(false) };
            /// Largest Rust allocation request observed during the measured operation.
            static LARGEST: Cell<usize> = const { Cell::new(0) };
        }
        /// Forward every allocation unchanged while observing test-thread request sizes.
        struct ObservedSystem;
        /// Retain a bounded scalar measurement without allocating in the allocator.
        /// # Arguments
        /// * `size` - Requested allocation size.
        fn observe(size: usize) {
            let _ = CAPTURE.try_with(|capture| {
                if capture.get() {
                    let _ = LARGEST.try_with(|largest| largest.set(largest.get().max(size)));
                }
            });
        }
        // SAFETY: all requests and their original layouts are forwarded unchanged to System.
        unsafe impl GlobalAlloc for ObservedSystem {
            /// Forward an allocation request and observe its requested size.
            /// # Safety
            /// The caller must supply a nonzero layout satisfying [`GlobalAlloc::alloc`].
            unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
                observe(layout.size());
                // SAFETY: the caller supplies GlobalAlloc's valid layout contract.
                unsafe { System.alloc(layout) }
            }
            /// Forward a zeroed allocation request and observe its requested size.
            /// # Safety
            /// The caller must supply a nonzero layout satisfying [`GlobalAlloc::alloc_zeroed`].
            unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
                observe(layout.size());
                // SAFETY: the caller supplies GlobalAlloc's valid layout contract.
                unsafe { System.alloc_zeroed(layout) }
            }
            /// Release the original pointer using its unchanged allocation layout.
            /// # Safety
            /// The pointer must be a live allocation from this allocator with the original layout.
            unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
                // SAFETY: this is the original pointer/layout from the forwarding allocator.
                unsafe { System.dealloc(pointer, layout) }
            }
            /// Forward a resize and observe the new requested size.
            /// # Safety
            /// The pointer and layout must identify a live allocation from this allocator.
            /// The new size must satisfy [`GlobalAlloc::realloc`], including its nonzero size bound.
            unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
                observe(new_size);
                // SAFETY: the caller supplies the original allocation and valid new size.
                unsafe { System.realloc(pointer, layout, new_size) }
            }
        }
        /// Test-only forwarding allocator; production binaries retain their default allocator.
        #[global_allocator]
        static ALLOCATOR: ObservedSystem = ObservedSystem;
        /// Measure the largest Rust allocation made by a synchronous operation.
        /// # Arguments
        /// * `operation` - Operation executed on the current test thread.
        /// # Returns
        /// Operation result and largest observed allocation request.
        pub(super) fn measure<T>(operation: impl FnOnce() -> T) -> (T, usize) {
            LARGEST.with(|largest| largest.set(0));
            CAPTURE.with(|capture| capture.set(true));
            let result = operation();
            CAPTURE.with(|capture| capture.set(false));
            (result, LARGEST.with(Cell::get))
        }
    }
    /// Catalog dispatch accepts exact integers only, never coercing raw markers.
    #[tokio::test]
    async fn catalog_dispatch_refuses_malformed_types_and_cardinality() {
        let cases = [
            (vec![], None),
            (vec![(Value::Integer(1), Value::Integer(1))], Some(1)),
            (vec![(Value::Integer(1), Value::Integer(6))], Some(6)),
            (vec![(Value::Integer(1), Value::Text("6".into()))], None),
            (vec![(Value::Integer(1), Value::Blob(b"6".to_vec()))], None),
            (vec![(Value::Integer(1), Value::Real(6.0))], None),
            (vec![(Value::Integer(1), Value::Null)], None),
            (vec![(Value::Integer(1), Value::Integer(3))], None),
            (vec![(Value::Integer(1), Value::Integer(7))], None),
            (vec![(Value::Integer(1), Value::Text("4".into()))], None),
            (vec![(Value::Integer(0), Value::Integer(6))], None),
            (vec![(Value::Integer(0), Value::Integer(4))], None),
            (vec![(Value::Text("1".into()), Value::Integer(6))], None),
            (
                vec![
                    (Value::Integer(1), Value::Integer(6)),
                    (Value::Integer(2), Value::Integer(6)),
                ],
                None,
            ),
            (
                vec![
                    (Value::Integer(1), Value::Integer(4)),
                    (Value::Integer(2), Value::Integer(4)),
                ],
                None,
            ),
        ];
        for (markers, expected) in cases {
            // Minimal private schema deliberately permits every hostile storage class.
            let mut store = LocalStore::open_inner(Path::new(":memory:"), false)
                .await
                .unwrap();
            let connection = store.connection.as_ref().unwrap();
            connection
                .execute("CREATE TABLE g_storage(singleton,storage_version)", ())
                .await
                .unwrap();
            for (singleton, version) in markers {
                assert_eq!(
                    connection
                        .execute(
                            "INSERT INTO g_storage VALUES(?1,?2)",
                            vec![singleton, version]
                        )
                        .await
                        .unwrap(),
                    1
                );
            }
            let mut tx = store.begin_write().await.unwrap();
            match expected {
                Some(expected) => {
                    assert_eq!(tx.supported_catalog_version().await.unwrap(), expected);
                    tx.rollback().await.unwrap();
                }
                None => {
                    assert_eq!(
                        tx.supported_catalog_version().await.unwrap_err().operation,
                        "unsupported storage version"
                    );
                    assert_eq!(
                        tx.rollback().await.unwrap_err().operation,
                        "unsupported storage version"
                    );
                }
            }
        }
    }

    /// Fresh open rejects huge raw markers without provider-sized Rust ownership.
    #[test]
    fn catalog_dispatch_precedes_raw_marker_allocation_at_tiny_limits() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let tiny = AdmissionLimits {
            content: Limits {
                max_stored_bytes: 1,
                max_output_bytes: 1,
                max_chain_depth: 1,
                max_total_stored_bytes: 1,
                max_total_decoded_bytes: 1,
            },
            parser: crate::artifact::ParseLimits {
                input_bytes: 1,
                cards: 1,
                owned_bytes: 1,
                entries: 1,
            },
        };
        let payload = vec![0xff; 1_048_576];
        for text in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("bad-marker");
            let raw=runtime.block_on(async {
                let database=libsql::Builder::new_local(&path).build().await.unwrap();
                let connection=database.connect().unwrap();
                connection.execute("CREATE TABLE g_storage(singleton INTEGER,storage_version)",()).await.unwrap();
                let sql=if text {"INSERT INTO g_storage VALUES(1,CAST(?1 AS TEXT))"} else {"INSERT INTO g_storage VALUES(1,?1)"};
                assert_eq!(connection.execute(sql,vec![Value::Blob(payload.clone())]).await.unwrap(),1);
                let mut rows=connection.query("SELECT typeof(storage_version),hex(CAST(storage_version AS BLOB)) FROM g_storage",()).await.unwrap();
                let row=rows.next().await.unwrap().unwrap();
                let result=(row.get::<String>(0).unwrap(),row.get::<String>(1).unwrap());
                assert_eq!(result.0,if text {"text"} else {"blob"});
                assert_eq!(result.1,"FF".repeat(payload.len()));
                result
            });
            let (result, largest) = allocation_probe::measure(|| {
                runtime.block_on(LocalOwnerRepository::open(&path, tiny))
            });
            assert_eq!(
                result.err().unwrap().operation,
                "unsupported storage version"
            );
            assert!(
                largest < payload.len() / 2,
                "owned a raw-marker-sized Rust allocation: {largest}"
            );
            let after=runtime.block_on(async {
                let database=libsql::Builder::new_local(&path).build().await.unwrap();
                let connection=database.connect().unwrap();
                let mut rows=connection.query("SELECT typeof(storage_version),hex(CAST(storage_version AS BLOB)) FROM g_storage",()).await.unwrap();
                let row=rows.next().await.unwrap().unwrap();
                (row.get::<String>(0).unwrap(),row.get::<String>(1).unwrap())
            });
            assert_eq!(after, raw);
        }
    }

    #[tokio::test]
    async fn root_numeric_dates_preserve_affinity_and_reject_tampering() {
        for (date, expected) in [
            (b"2026-11-18T00:00:00".as_slice(), Value::Real(2461362.5)),
            (b"2026-11-18T12:00:00".as_slice(), Value::Integer(2461363)),
            (
                b"2026-11-18T12:00:00.001".as_slice(),
                Value::Real(2461363.0 + 1.0 / 86400000.0),
            ),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("numeric-date");
            let owner = LocalOwnerRepository::create(
                &path,
                CreateOptions {
                    repository_id: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    owner_id: "owner",
                    checkout_id: "checkout",
                    checkout_root: temp.path(),
                    date,
                    user: b"actor",
                    content_limits: root_content_limits(),
                    parse_limits: root_parse_limits(),
                },
            )
            .await
            .unwrap();
            drop(owner);
            let mut store = LocalStore::open(&path).await.unwrap();
            let mut tx = store.begin_write().await.unwrap();
            let rows = tx
                .query("SELECT mtime,omtime FROM event UNION ALL SELECT mtime,mtime FROM tagxref")
                .await
                .unwrap();
            assert_eq!(rows, vec![vec![expected.clone(), expected.clone()]; 3]);
            tx.rollback().await.unwrap();
            drop(store);
            drop(
                LocalOwnerRepository::open(&path, admission_limits())
                    .await
                    .unwrap(),
            );
            for (table, column) in [
                ("event", "mtime"),
                ("event", "omtime"),
                ("tagxref", "mtime"),
            ] {
                for invalid in [
                    "NULL",
                    "'invalid'",
                    "X'32343631333633'",
                    "1e999",
                    "2461364",
                    "2461363.25",
                    "9223372036854775807",
                ] {
                    let mut store = LocalStore::open(&path).await.unwrap();
                    let mut tx = store.begin_write().await.unwrap();
                    tx.execute(&format!("UPDATE {table} SET {column}={invalid}"))
                        .await
                        .unwrap();
                    tx.commit().await.unwrap();
                    drop(store);
                    assert!(
                        LocalOwnerRepository::open(&path, admission_limits())
                            .await
                            .is_err(),
                        "{date:?}: {table}.{column}={invalid}"
                    );
                    let mut store = LocalStore::open(&path).await.unwrap();
                    let mut tx = store.begin_write().await.unwrap();
                    let restore = match (table, column) {
                        ("event", "mtime") => "UPDATE event SET mtime=?1",
                        ("event", "omtime") => "UPDATE event SET omtime=?1",
                        _ => "UPDATE tagxref SET mtime=?1",
                    };
                    tx.execute_bound(restore, vec![expected.clone()])
                        .await
                        .unwrap();
                    tx.commit().await.unwrap();
                    drop(store);
                    drop(
                        LocalOwnerRepository::open(&path, admission_limits())
                            .await
                            .unwrap(),
                    );
                }
            }
        }
    }
    #[tokio::test]
    async fn root_projection_mutations_deny_disk_reopen() {
        // Every SQL statement changes one independently forbidden projection surface.
        let mutations = [
            "INSERT INTO g_contributor(scope_key,ticket_k,native_hash,comparison_sha3,status) SELECT 'shared','fabricated-K',uuid,uuid,'valid' FROM blob WHERE rid=1",
            "INSERT INTO g_vfile(checkout_id,id,vid,rid,mrid,pathname) VALUES('default',1,1,1,1,'fabricated')",
            "INSERT INTO g_vmerge(checkout_id,id,merge,mhash) VALUES('default',1,1,'opaque')",
            "INSERT INTO g_path_scope(checkout_id,path,scope) VALUES('default','fabricated','shared')",
            "INSERT INTO g_pending_path(checkout_id,path,action,selected_scope) VALUES('default','fabricated','remove','shared')",
            "INSERT INTO g_descriptor SELECT 'shared',uuid,'other','target','descriptor',1,'valid' FROM blob WHERE rid=1",
            "INSERT INTO g_descriptor SELECT 'shared','other',uuid,'target','descriptor',1,'valid' FROM blob WHERE rid=1",
            "INSERT INTO g_discussion_message SELECT 'shared','message',uuid,'thread',NULL,NULL,'author','date','text',X'00' FROM blob WHERE rid=1",
            "INSERT INTO g_code_link SELECT 'shared','link',uuid,'kind','key','ticket','v1','target','relation' FROM blob WHERE rid=1",
            "INSERT INTO g_discussion_target(scope_key,target_id,thread_id,attachment_artifact_hash,repository_id,target_kind) SELECT 'shared','target','thread',uuid,'repo','repository' FROM blob WHERE rid=1",
            "INSERT INTO g_message_context SELECT 'shared',uuid,'message','other',NULL,NULL FROM blob WHERE rid=1",
            "INSERT INTO g_message_context SELECT 'shared','other','message',uuid,NULL,NULL FROM blob WHERE rid=1",
            "INSERT INTO g_message_context_target SELECT 'shared',uuid,'other','target' FROM blob WHERE rid=1",
            "INSERT INTO g_message_context_target SELECT 'shared','other',uuid,'target' FROM blob WHERE rid=1",
            "INSERT INTO tagxref(tagid,rid) VALUES(999,1)",
            "INSERT INTO g_journal SELECT checkout_id,'pending',0,0,1,1,shared_head,NULL,shared_head,NULL,'pending' FROM g_checkout",
            "INSERT INTO g_journal SELECT checkout_id,'blocked',0,0,1,1,shared_head,NULL,shared_head,NULL,'blocked' FROM g_checkout",
            "UPDATE event SET objid=99 WHERE objid=1",
            "INSERT INTO g_admission SELECT supporting_root,'legacy_shared_incomplete','pending' FROM g_public_receipt",
            "INSERT INTO g_admission SELECT supporting_root,'owner_unclassified','pending' FROM g_public_receipt",
            "INSERT INTO attachment(attachid) VALUES(1)",
            "INSERT INTO ticketchng(tkt_rid) VALUES(1)",
            "INSERT INTO modreq(objid) VALUES(1)",
            "INSERT INTO tagxref(tagid,rid,srcid) VALUES(8,99,1)",
            "INSERT INTO tagxref(tagid,rid,origid) VALUES(8,99,1)",
            "INSERT INTO g_admission SELECT supporting_root,'shared_pending','pending' FROM g_public_receipt",
            "INSERT INTO plink(pid,cid) VALUES(1,1)",
            "INSERT INTO plink(pid,cid) VALUES(1,99)",
            "INSERT INTO plink(pid,cid) VALUES(99,1)",
            "INSERT INTO plink(pid,cid,baseid) VALUES(98,99,1)",
            "INSERT INTO mlink(mid) VALUES(1)",
            "INSERT INTO mlink(fid) VALUES(1)",
            "INSERT INTO mlink(pmid) VALUES(1)",
            "INSERT INTO mlink(pid) VALUES(1)",
            "INSERT INTO cherrypick(parentid,childid) VALUES(1,99)",
            "INSERT INTO cherrypick(parentid,childid) VALUES(99,1)",
            "INSERT INTO phantom VALUES(1)",
            "INSERT INTO orphan(rid,baseline) VALUES(1,99)",
            "INSERT INTO orphan(rid,baseline) VALUES(99,1)",
            "INSERT INTO backlink(target,srctype,srcid) VALUES('target',0,1)",
            "INSERT INTO forumpost(fpid) VALUES(1)",
            "INSERT INTO forumpost(fpid,froot) VALUES(99,1)",
            "INSERT INTO forumpost(fpid,fprev) VALUES(99,1)",
            "INSERT INTO forumpost(fpid,firt) VALUES(99,1)",
            "UPDATE event SET tagid=1 WHERE objid=1",
            "UPDATE event SET uid=1 WHERE objid=1",
            "UPDATE event SET bgcolor='red' WHERE objid=1",
            "UPDATE event SET euser='other' WHERE objid=1",
            "UPDATE event SET ecomment='unbacked' WHERE objid=1",
            "UPDATE event SET brief='unbacked' WHERE objid=1",
            "UPDATE event SET type='w' WHERE objid=1",
            "UPDATE event SET mtime=0 WHERE objid=1",
            "UPDATE event SET user='other' WHERE objid=1",
            "UPDATE event SET comment='other' WHERE objid=1",
            "UPDATE event SET omtime=0 WHERE objid=1",
            "UPDATE g_public_receipt SET validation_class='independent-public-receipt'",
            "UPDATE g_public_receipt SET validation_class='legacy-public-import'",
            "INSERT INTO g_dependency SELECT 'shared',supporting_root,'parent','missing',1 FROM g_public_receipt",
            "INSERT INTO g_dependency SELECT 'shared',supporting_root,'optional','missing',0 FROM g_public_receipt",
            "INSERT INTO g_dependency SELECT 'shared','other','parent',supporting_root,1 FROM g_public_receipt",
        ];
        let mut outcomes = Vec::new();
        for sql in mutations {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("projection");
            fixture(&path).await;
            let mut store = LocalStore::open(&path).await.unwrap();
            let mut tx = store.begin_write().await.unwrap();
            tx.execute(sql).await.unwrap();
            tx.commit().await.unwrap();
            drop(store);
            outcomes.push((
                sql,
                LocalOwnerRepository::open(&path, admission_limits())
                    .await
                    .is_err(),
            ));
        }
        assert!(
            outcomes.iter().all(|(_, rejected)| *rejected),
            "projection rejection outcomes: {outcomes:#?}"
        );
    }

    #[tokio::test]
    async fn root_projection_preserves_operational_and_unrelated_content_rows() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("operational");
        let name = fixture(&path).await;
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        for sql in [
            "INSERT INTO g_contributor SELECT 'shared','invalid',uuid,uuid,NULL,NULL,'invalid' FROM blob WHERE rid=1",
            "INSERT INTO g_contributor SELECT 'shared','unsupported',uuid,uuid,NULL,NULL,'unsupported' FROM blob WHERE rid=1",
            "INSERT INTO g_contributor SELECT 'shared','unresolved',uuid,uuid,NULL,NULL,'unresolved' FROM blob WHERE rid=1",
            "INSERT INTO g_diagnostic SELECT 'shared','K',uuid,'field','invalid','retained' FROM blob WHERE rid=1",
            "INSERT INTO g_descriptor SELECT 'shared',uuid,uuid,uuid,'unknown',1,'invalid' FROM blob WHERE rid=1",
            "INSERT INTO g_record_diagnostic SELECT 'shared',uuid,uuid,'field','invalid','retained' FROM blob WHERE rid=1",
            "INSERT INTO g_discussion_target(scope_key,target_id,thread_id,attachment_artifact_hash,repository_id,target_kind,checkin_hash,content_hash,captured_content_hash) SELECT 'shared','target','thread','other','repo','snapshot',uuid,uuid,uuid FROM blob WHERE rid=1",
            "INSERT INTO g_stash(checkout_id,stashid,vid) VALUES('default',1,1)",
            "INSERT INTO g_undo_vfile(checkout_id,id,vid,rid,mrid,pathname) VALUES('default',1,1,1,1,'history')",
            "INSERT INTO g_selector_revision VALUES('test-owner','kind','0000000000000000000000000000000000000000',0,'follow_shared',NULL)",
            "INSERT INTO g_selector VALUES('test-owner','kind','0000000000000000000000000000000000000000','follow_shared',0)",
            "INSERT INTO g_selector_member SELECT 'test-owner','kind','0000000000000000000000000000000000000000',0,uuid,'frozen_shared_base' FROM blob WHERE rid=1",
            "INSERT INTO g_target_entity_member SELECT 'shared','target',uuid FROM blob WHERE rid=1",
            "INSERT INTO g_suggestion SELECT 'shared','suggestion','message','target',uuid,uuid,'proposed','valid' FROM blob WHERE rid=1",
            "INSERT INTO g_suggestion_application SELECT 'shared','application','suggestion',uuid,uuid,uuid,'actor','date' FROM blob WHERE rid=1",
            "INSERT INTO g_local_command VALUES('retained','test-owner','project',0,1,X'00',0)",
            "INSERT INTO g_inverse_chain VALUES('retained',X'00')",
            "INSERT INTO modreq(objid,attachRid) VALUES(99,1)",
            "INSERT INTO unsent VALUES(1)",
            "INSERT INTO rcvfrom(rcvid,uid,mtime,ipaddr) VALUES(99,0,0,'local')",
            "UPDATE blob SET rcvid=99 WHERE rid=1",
            "INSERT INTO blob(rid,size,uuid) VALUES(99,-1,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa')",
            "INSERT INTO phantom VALUES(99)",
            "INSERT INTO unclustered VALUES(99)",
            "INSERT INTO g_dependency SELECT 'private','opaque','optional',supporting_root,0 FROM g_public_receipt",
        ] {
            tx.execute(sql).await.unwrap();
        }
        let delta_target = b"private physical root delta";
        let delta_name = content_hash::hex(&content_hash::sha3(delta_target, 256).unwrap());
        let root_bytes =
            empty_root_bytes(b"2026-11-18T00:00:00.000", b"Local Owner", 65536).unwrap();
        let delta = crate::delta::create(&root_bytes, delta_target).unwrap();
        tx.execute_bound(
            "INSERT INTO blob(rid,rcvid,size,uuid,content) VALUES(100,99,?1,?2,?3)",
            vec![
                Value::Integer(delta_target.len() as i64),
                Value::Text(delta_name.clone()),
                Value::Blob(compression::pack(&delta, -1).unwrap()),
            ],
        )
        .await
        .unwrap();
        tx.execute("INSERT INTO private VALUES(100)").await.unwrap();
        tx.execute("INSERT INTO delta VALUES(100,1)").await.unwrap();
        tx.commit().await.unwrap();
        drop(store);
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        assert_eq!(
            owner
                .read_content(&delta_name, root_content_limits())
                .await
                .unwrap(),
            delta_target
        );
        assert_eq!(
            owner.read_content(&name, limits()).await.unwrap(),
            b"owner bytes"
        );
        owner
            .put_private_file_content(&name, b"owner bytes", limits())
            .await
            .unwrap();
        drop(owner);
        let mut reopened = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        let mut tx = reopened.store.begin_write().await.unwrap();
        assert_eq!(
            tx.query("SELECT objid,attachRid FROM modreq")
                .await
                .unwrap(),
            vec![vec![Value::Integer(99), Value::Integer(1)]]
        );
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    async fn root_projection_counter_refresh_cannot_bless_conflicts() {
        for sql in [
            "INSERT INTO plink(pid,cid) VALUES(1,99)",
            "INSERT INTO mlink(mid) VALUES(1)",
            "INSERT INTO cherrypick(parentid,childid) VALUES(1,99)",
            "INSERT INTO phantom VALUES(1)",
            "INSERT INTO orphan(rid) VALUES(1)",
            "INSERT INTO backlink(srcid) VALUES(1)",
            "INSERT INTO forumpost(fpid) VALUES(1)",
            "INSERT INTO g_dependency SELECT 'shared',supporting_root,'parent','missing',1 FROM g_public_receipt",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("counter");
            let name = fixture(&path).await;
            let mut owner = LocalOwnerRepository::open(&path, admission_limits())
                .await
                .unwrap();
            let mut tx = owner.store.begin_write().await.unwrap();
            tx.execute(sql).await.unwrap();
            tx.commit().await.unwrap();
            // Model a future controlled-write counter refresh without changing the authority seal.
            owner.admission.total_changes =
                owner.store.connection.as_ref().unwrap().total_changes();
            assert!(
                owner
                    .put_private_file_content(&name, b"owner bytes", limits())
                    .await
                    .unwrap_err()
                    .operation
                    .contains("authority changed"),
                "{sql}"
            );
        }
    }

    #[tokio::test]
    async fn root_projection_validation_latches_and_rolls_back_prior_write() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("projection-latch");
        fixture(&path).await;
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        tx.execute("INSERT INTO config(name,value) VALUES('projection-prior-write','pending')")
            .await
            .unwrap();
        tx.execute("UPDATE event SET ecomment='unbacked' WHERE objid=1")
            .await
            .unwrap();
        let first = tx
            .validate_initial_repository(&owner.principal, admission_limits())
            .await
            .unwrap_err();
        assert_eq!(tx.commit().await.unwrap_err().operation, first.operation);
        drop(owner);
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        assert!(
            tx.query("SELECT 1 FROM config WHERE name='projection-prior-write'")
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            tx.query("SELECT ecomment FROM event WHERE objid=1")
                .await
                .unwrap(),
            vec![vec![Value::Null]]
        );
        tx.rollback().await.unwrap();
    }
    #[tokio::test(flavor = "current_thread")]
    async fn owned_date_results_preserve_engine_values_and_abort_prior_writes() {
        use crate::artifact::DateProvider;
        use std::error::Error;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("date-results");
        fixture(&path).await;
        for values in [
            vec![],
            vec![Value::Integer(1)],
            vec![Value::Real(f64::INFINITY)],
            vec![Value::Null, Value::Null],
        ] {
            let mut owner = LocalOwnerRepository::open(&path, admission_limits())
                .await
                .unwrap();
            let mut tx = owner.store.begin_write().await.unwrap();
            assert_eq!(
                OwnerDates(&tx)
                    .julianday(Some(b"2000-01-01T12:00:00"))
                    .await
                    .unwrap(),
                Some(2451545.0)
            );
            assert_eq!(
                OwnerDates(&tx).julianday(Some(b"invalid")).await.unwrap(),
                None
            );
            tx.execute("INSERT INTO config(name,value) VALUES('date-boundary','pending')")
                .await
                .unwrap();
            let reset = boundary_injection::Reset;
            boundary_injection::DATES.with(|slot| *slot.borrow_mut() = Some(values));
            let failure = tx
                .validate_initial_repository(&owner.principal, admission_limits())
                .await
                .unwrap_err();
            reset.consumed();
            assert_eq!(failure.operation, "native artifact validation failed");
            let parser_error = failure
                .source()
                .unwrap()
                .downcast_ref::<crate::artifact::ArtifactError>()
                .unwrap();
            assert!(matches!(
                parser_error,
                crate::artifact::ArtifactError::Provider(_)
            ));
            let provider_error = parser_error
                .source()
                .unwrap()
                .downcast_ref::<crate::artifact::DateProviderError>()
                .unwrap();
            let native_error = provider_error.0.downcast_ref::<StorageError>().unwrap();
            assert!(matches!(
                native_error.operation.as_str(),
                "native date provider returned no row"
                    | "invalid native date provider row"
                    | "native date provider returned extra rows"
            ));
            assert_eq!(tx.commit().await.unwrap_err().operation, failure.operation);
            drop(owner);
            let mut reopened = LocalOwnerRepository::open(&path, admission_limits())
                .await
                .unwrap();
            let mut tx = reopened.store.begin_write().await.unwrap();
            assert!(
                tx.query("SELECT value FROM config WHERE name='date-boundary'")
                    .await
                    .unwrap()
                    .is_empty()
            );
            tx.rollback().await.unwrap();
        }
    }
    #[tokio::test(flavor = "current_thread")]
    async fn metadata_open_failure_preserves_os_cause_without_owner_handle() {
        use std::error::Error;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("metadata");
        fixture(&path).await;
        let reset = boundary_injection::Reset;
        boundary_injection::METADATA.with(|slot| *slot.borrow_mut() = Some(5));
        let error = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .err()
            .expect("metadata failure must deny owner handle");
        reset.consumed();
        assert_eq!(error.operation, "local repository metadata unavailable");
        assert_eq!(
            error
                .source()
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .raw_os_error(),
            Some(5)
        );
        LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
    }
    #[tokio::test(flavor = "current_thread")]
    async fn malformed_catalog_query_denies_real_owner_admission() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("catalog-result");
        fixture(&path).await;
        let reset = super::super::schema::catalog_injection::Reset;
        super::super::schema::catalog_injection::ROWS.with(|slot| {
            *slot.borrow_mut() = Some(vec![vec![
                Value::Text("table".into()),
                Value::Text("g_storage".into()),
                Value::Text("g_storage".into()),
                Value::Integer(42),
            ]])
        });
        let error = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .err()
            .expect("typed catalog failure must deny owner handle");
        reset.consumed();
        assert_eq!(error.operation, "corrupt schema catalog value");
        LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
    }
    #[test]
    fn rejected_root_expansion_cannot_allocate_an_oversized_user_token() {
        let user = [b' '; 256];
        let (result, largest) =
            allocation_probe::measure(|| empty_root_bytes(b"2026-11-18T00:00:00.000", &user, 256));
        assert!(result.is_err());
        println!(
            "root ceiling256, observed largest allocation request {largest}, rejected before variable output"
        );
        assert!(
            largest <= 256,
            "root ceiling256, observed allocation request {largest}"
        );
    }
    #[test]
    fn whole_root_size_is_checked_before_any_variable_output_allocation() {
        let user = [b'a'; 128];
        let date = b"2026-11-18T00:00:00.000";
        let expected = empty_root_bytes(date, &user, 4096).unwrap();
        let (rejected, largest) =
            allocation_probe::measure(|| empty_root_bytes(date, &user, expected.len() - 1));
        assert!(rejected.is_err());
        println!("whole-root one-under rejected, largest fixed diagnostic request {largest}");
        // Only the fixed diagnostic is allocated: neither 128 user bytes nor the full root.
        assert!(
            largest <= 64,
            "whole-root rejection allocated {largest} bytes"
        );
        let (exact, largest) =
            allocation_probe::measure(|| empty_root_bytes(date, &user, expected.len()));
        assert_eq!(exact.unwrap(), expected);
        assert!(largest <= expected.len());
        let escapes = b" \t\n\r\x0b\x0c\\";
        let expected = empty_root_bytes(date, escapes, 4096).unwrap();
        assert_eq!(
            empty_root_bytes(date, escapes, expected.len()).unwrap(),
            expected
        );
        assert!(empty_root_bytes(date, escapes, expected.len() - 1).is_err());
    }
    #[tokio::test]
    async fn complete_native_root_creation_reopens_and_refuses_existing_destination() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let make_options = || CreateOptions {
            repository_id: "0123456789abcdef0123456789abcdef01234567",
            owner_id: "local-owner",
            checkout_id: "default",
            checkout_root: temp.path(),
            date: b"2026-11-18T00:00:00.000",
            user: b"Local Owner",
            content_limits: Limits {
                max_stored_bytes: 4096,
                max_output_bytes: 4096,
                max_chain_depth: 4,
                max_total_stored_bytes: 8192,
                max_total_decoded_bytes: 16384,
            },
            parse_limits: crate::artifact::ParseLimits {
                input_bytes: 4096,
                cards: 16,
                owned_bytes: 16384,
                entries: 16,
            },
        };
        let owner = LocalOwnerRepository::create(&path, make_options())
            .await
            .unwrap();
        drop(owner);
        let owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        drop(owner);
        assert!(
            LocalOwnerRepository::create(&path, make_options())
                .await
                .is_err()
        );
    }
    #[test]
    fn native_empty_root_constructor_has_exact_records_and_legacy_checksum() {
        let bytes = empty_root_bytes(b"2026-11-18T00:00:00.000", b"Local Owner", 65536).unwrap();
        let prefix = b"C initial\\sempty\\scheck-in\nD 2026-11-18T00:00:00.000\nR d41d8cd98f00b204e9800998ecf8427e\nT *branch * trunk\nT *sym-trunk *\nU Local\\sOwner\n";
        assert!(bytes.starts_with(prefix));
        assert_eq!(
            &bytes[prefix.len()..],
            format!(
                "Z {}\n",
                content_hash::hex(&crate::artifact_checksum::md5(prefix))
            )
            .as_bytes()
        );
        assert!(empty_root_bytes(b"bad\nD injected", b"owner", 65536).is_err());
    }
    fn root_content_limits() -> Limits {
        Limits {
            max_stored_bytes: 65536,
            max_output_bytes: 65536,
            max_chain_depth: 8,
            max_total_stored_bytes: 262144,
            max_total_decoded_bytes: 1048576,
        }
    }
    fn root_parse_limits() -> crate::artifact::ParseLimits {
        crate::artifact::ParseLimits {
            input_bytes: 65536,
            cards: 16,
            owned_bytes: 262144,
            entries: 16,
        }
    }
    fn admission_limits() -> AdmissionLimits {
        AdmissionLimits {
            content: root_content_limits(),
            parser: root_parse_limits(),
        }
    }
    /// Build fixtures through the real complete local initializer, then add private file bytes.
    async fn fixture(path: &Path) -> String {
        let mut owner = LocalOwnerRepository::create(
            path,
            CreateOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "test-owner",
                checkout_id: "default",
                checkout_root: path.parent().unwrap(),
                date: b"2026-11-18T00:00:00.000",
                user: b"Local Owner",
                content_limits: root_content_limits(),
                parse_limits: root_parse_limits(),
            },
        )
        .await
        .unwrap();
        let name = content_hash::hex(&content_hash::sha3(b"owner bytes", 256).unwrap());
        owner
            .put_private_file_content(&name, b"owner bytes", limits())
            .await
            .unwrap();
        name
    }
    fn limits() -> Limits {
        Limits {
            max_stored_bytes: 1024,
            max_output_bytes: 64,
            max_chain_depth: 4,
            max_total_stored_bytes: 4096,
            max_total_decoded_bytes: 256,
        }
    }
    #[tokio::test]
    async fn local_owner_opens_only_complete_matching_catalog_and_revalidates() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let name = fixture(&path).await;
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        assert_eq!(
            owner.read_content(&name, limits()).await.unwrap(),
            b"owner bytes"
        );
        let mut tx = owner.store.begin_write().await.unwrap();
        tx.execute("UPDATE g_storage SET policy_complete=0")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert!(owner.read_content(&name, limits()).await.is_err());
        assert!(
            LocalOwnerRepository::open(&path, admission_limits())
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn private_file_roundtrip_dedup_and_phantom_stale_delta_removal() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        fixture(&path).await;
        let bytes = b"new private file";
        let name = content_hash::hex(&content_hash::sha3(bytes, 256).unwrap());
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        owner
            .put_private_file_content(&name, bytes, limits())
            .await
            .unwrap();
        owner
            .put_private_file_content(&name, bytes, limits())
            .await
            .unwrap();
        drop(owner);
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        assert_eq!(owner.read_content(&name, limits()).await.unwrap(), bytes);
        let empty_name = content_hash::hex(&content_hash::sha3(b"", 256).unwrap());
        let mut tx = owner.store.begin_write().await.unwrap();
        tx.execute_bound(
            "INSERT INTO blob(rid,size,uuid) VALUES(100,-1,?1)",
            vec![Value::Text(empty_name.clone())],
        )
        .await
        .unwrap();
        tx.execute("INSERT INTO phantom VALUES(100)").await.unwrap();
        tx.execute("INSERT INTO delta VALUES(100,999)")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        drop(owner);
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        owner
            .put_private_file_content(&empty_name, b"", limits())
            .await
            .unwrap();
        assert_eq!(
            owner.read_content(&empty_name, limits()).await.unwrap(),
            b""
        );
        let mut tx = owner.store.begin_write().await.unwrap();
        assert_eq!(
            tx.query("SELECT count(*) FROM delta WHERE rid=100")
                .await
                .unwrap(),
            vec![vec![Value::Integer(0)]]
        );
        assert_eq!(
            tx.query("SELECT count(*) FROM phantom WHERE rid=100")
                .await
                .unwrap(),
            vec![vec![Value::Integer(0)]]
        );
        tx.commit().await.unwrap();
    }
    #[tokio::test]
    async fn verified_full_bytes_replace_unavailable_nonphantom_delta_without_leaving_delta_row() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        fixture(&path).await;
        let bytes = b"recovered full bytes";
        let name = content_hash::hex(&content_hash::sha3(bytes, 256).unwrap());
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute_bound(
            "INSERT INTO blob(rid,size,uuid,content) VALUES(200,?1,?2,?3)",
            vec![
                Value::Integer(bytes.len() as i64),
                Value::Text(name.clone()),
                Value::Blob(compression::pack(b"not available delta", -1).unwrap()),
            ],
        )
        .await
        .unwrap();
        tx.execute("INSERT INTO delta VALUES(200,999)")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        owner
            .put_private_file_content(&name, bytes, limits())
            .await
            .unwrap();
        assert_eq!(owner.read_content(&name, limits()).await.unwrap(), bytes);
        let mut tx = owner.store.begin_write().await.unwrap();
        assert_eq!(
            tx.query("SELECT count(*) FROM delta WHERE rid=200")
                .await
                .unwrap(),
            vec![vec![Value::Integer(0)]]
        );
        tx.commit().await.unwrap();
    }
    #[tokio::test]
    async fn native_delta_chain_reconstructs_and_shunned_or_cycle_baselines_fail() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let base_name = fixture(&path).await;
        let bytes = b"owner bytes plus delta";
        let name = content_hash::hex(&content_hash::sha3(bytes, 256).unwrap());
        let instructions = crate::delta::create(b"owner bytes", bytes).unwrap();
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        tx.execute_bound(
            "INSERT INTO blob(rid,size,uuid,content) VALUES(10,?1,?2,?3)",
            vec![
                Value::Integer(bytes.len() as i64),
                Value::Text(name.clone()),
                Value::Blob(compression::pack(&instructions, -1).unwrap()),
            ],
        )
        .await
        .unwrap();
        tx.execute("INSERT INTO delta VALUES(10,2)").await.unwrap();
        tx.commit().await.unwrap();
        drop(owner);
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        assert_eq!(owner.read_content(&name, limits()).await.unwrap(), bytes);
        let mut tx = owner.store.begin_write().await.unwrap();
        tx.execute_bound(
            "INSERT INTO shun(uuid) VALUES(?1)",
            vec![Value::Text(base_name)],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        drop(owner);
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        assert!(owner.read_content(&name, limits()).await.is_err());
        let mut tx = owner.store.begin_write().await.unwrap();
        tx.execute("DELETE FROM shun").await.unwrap();
        tx.execute("UPDATE delta SET srcid=10 WHERE rid=10")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        drop(owner);
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        assert!(
            owner
                .read_content(&name, limits())
                .await
                .unwrap_err()
                .operation
                .contains("cycle")
        );
    }
    #[tokio::test]
    async fn caught_private_write_hash_error_rolls_back_entire_operation() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        fixture(&path).await;
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        let bytes = b"valid new bytes";
        let name = content_hash::hex(&content_hash::sha3(bytes, 256).unwrap());
        tx.put_private_file_content(&owner.principal, &name, bytes, limits())
            .await
            .unwrap();
        assert!(
            tx.put_private_file_content(&owner.principal, &"0".repeat(64), b"mismatch", limits())
                .await
                .is_err()
        );
        assert!(tx.commit().await.is_err());
        drop(owner);
        let mut reopened = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        assert!(reopened.read_content(&name, limits()).await.is_err());
        assert!(
            reopened
                .read_content(
                    &content_hash::hex(&content_hash::sha3(b"owner bytes", 256).unwrap()),
                    limits()
                )
                .await
                .is_ok()
        );
    }
    #[tokio::test]
    async fn all_post_acquisition_policy_codec_and_limit_errors_latch_prior_writes() {
        for variant in [
            "wrong-owner",
            "wrong-repository",
            "missing-policy",
            "catalog",
            "limits",
            "write-limit",
            "codec",
            "size",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("repo");
            let original = fixture(&path).await;
            let mut owner = LocalOwnerRepository::open(&path, admission_limits())
                .await
                .unwrap();
            let mut tx = owner.store.begin_write().await.unwrap();
            let bytes = b"prior valid write";
            let name = content_hash::hex(&content_hash::sha3(bytes, 256).unwrap());
            tx.put_private_file_content(&owner.principal, &name, bytes, limits())
                .await
                .unwrap();
            let failure = match variant {
                "wrong-owner" | "wrong-repository" => {
                    let principal = Principal {
                        repository: if variant == "wrong-repository" {
                            "wrong".into()
                        } else {
                            owner.principal.repository.clone()
                        },
                        owner: if variant == "wrong-owner" {
                            "wrong".into()
                        } else {
                            owner.principal.owner.clone()
                        },
                    };
                    tx.validate_principal(&principal).await
                }
                "missing-policy" => {
                    tx.execute("DELETE FROM g_storage").await.unwrap();
                    tx.validate_principal(&owner.principal).await
                }
                "catalog" => {
                    tx.execute("DROP INDEX delta_i1").await.unwrap();
                    tx.validate_principal(&owner.principal).await
                }
                "limits" => tx
                    .read_owner_content(
                        &owner.principal,
                        &name,
                        Limits {
                            max_chain_depth: 0,
                            ..limits()
                        },
                    )
                    .await
                    .map(|_| ()),
                "write-limit" => {
                    let empty = content_hash::hex(&content_hash::sha3(b"", 256).unwrap());
                    tx.put_private_file_content(
                        &owner.principal,
                        &empty,
                        b"",
                        Limits {
                            max_stored_bytes: 3,
                            ..limits()
                        },
                    )
                    .await
                    .map(|_| ())
                }
                "codec" => {
                    tx.execute("UPDATE blob SET content=x'00000000' WHERE rid=2")
                        .await
                        .unwrap();
                    tx.read_owner_content(&owner.principal, &original, limits())
                        .await
                        .map(|_| ())
                }
                "size" => {
                    tx.execute("UPDATE blob SET size=12 WHERE rid=2")
                        .await
                        .unwrap();
                    tx.read_owner_content(&owner.principal, &original, limits())
                        .await
                        .map(|_| ())
                }
                _ => unreachable!(),
            };
            let error = failure.unwrap_err();
            assert_eq!(
                tx.execute("DELETE FROM blob").await.unwrap_err().operation,
                error.operation,
                "{variant}"
            );
            assert_eq!(
                tx.commit().await.unwrap_err().operation,
                error.operation,
                "{variant}"
            );
            drop(owner);
            let mut reopened = LocalOwnerRepository::open(&path, admission_limits())
                .await
                .unwrap();
            assert_eq!(
                reopened.read_content(&original, limits()).await.unwrap(),
                b"owner bytes",
                "{variant}"
            );
            assert!(
                reopened.read_content(&name, limits()).await.is_err(),
                "{variant}"
            );
        }
    }
    #[tokio::test]
    async fn restricted_shared_view_hides_private_names_and_keeps_public_identity_public() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let private = fixture(&path).await;
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        let mut view = owner.shared_view();
        let absent = view
            .read_content(&"0".repeat(64), root_content_limits())
            .await
            .unwrap_err();
        let hidden = view
            .read_content(&private, root_content_limits())
            .await
            .unwrap_err();
        assert_eq!(absent.operation, hidden.operation);
        let mut tx = owner.store.begin_write().await.unwrap();
        let rows = tx
            .query("SELECT shared_head FROM g_checkout")
            .await
            .unwrap();
        let root_bytes =
            empty_root_bytes(b"2026-11-18T00:00:00.000", b"Local Owner", 65536).unwrap();
        let root = content_hash::hex(&content_hash::sha3(&root_bytes, 256).unwrap());
        assert_eq!(rows, vec![vec![Value::Text(root.clone())]]);
        tx.commit().await.unwrap();
        let bytes = owner
            .shared_view()
            .read_content(&root, root_content_limits())
            .await
            .unwrap();
        owner
            .put_private_file_content(&root, &bytes, root_content_limits())
            .await
            .unwrap();
        assert_eq!(
            owner
                .shared_view()
                .read_content(&root, root_content_limits())
                .await
                .unwrap(),
            bytes
        );
    }
    #[tokio::test]
    async fn unsupported_optional_schema_and_unregistered_trigger_are_distinct() {
        for (sql, reason) in [
            (
                "CREATE TABLE foreign_custom(value TEXT)",
                "unregistered persistent schema object",
            ),
            (
                "CREATE TABLE fx_custom(value TEXT)",
                "unsupported optional native schema family",
            ),
            (
                "CREATE TRIGGER imported AFTER INSERT ON blob BEGIN DELETE FROM private; END",
                "unregistered persistent trigger",
            ),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("repo");
            fixture(&path).await;
            let mut store = LocalStore::open(&path).await.unwrap();
            let mut tx = store.begin_write().await.unwrap();
            tx.execute(sql).await.unwrap();
            tx.commit().await.unwrap();
            let error = LocalOwnerRepository::open(&path, admission_limits())
                .await
                .err()
                .unwrap();
            assert!(error.operation.contains(reason), "{}", error.operation);
        }
    }
    #[tokio::test]
    async fn every_initialization_phase_failure_rolls_back_all_schema_and_state() {
        for phase in [
            "schema",
            "content",
            "crosslink",
            "receipt",
            "bindings",
            "marker",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("repo");
            let options = CreateOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                checkout_id: "default",
                checkout_root: temp.path(),
                date: b"2026-11-18T00:00:00.000",
                user: b"Owner",
                content_limits: root_content_limits(),
                parse_limits: root_parse_limits(),
            };
            let error = LocalOwnerRepository::create_internal(&path, options, Some(phase))
                .await
                .err()
                .unwrap();
            assert_eq!(
                error.operation, "injected initialization phase failure",
                "{phase}"
            );
            assert!(
                LocalOwnerRepository::open(&path, admission_limits())
                    .await
                    .is_err(),
                "{phase}"
            );
            let mut store = LocalStore::open(&path).await.unwrap();
            let mut tx = store.begin_write().await.unwrap();
            assert_eq!(
                tx.query("SELECT count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'")
                    .await
                    .unwrap(),
                vec![vec![Value::Integer(0)]],
                "{phase}"
            );
            tx.commit().await.unwrap();
        }
    }
    #[tokio::test]
    async fn admission_detects_external_commit_and_unexpected_same_connection_writes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let name = fixture(&path).await;
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        let small = Limits {
            max_output_bytes: 16,
            max_stored_bytes: 1024,
            max_chain_depth: 4,
            max_total_stored_bytes: 4096,
            max_total_decoded_bytes: 64,
        };
        assert_eq!(
            owner.read_content(&name, small).await.unwrap(),
            b"owner bytes"
        );
        let mut other = LocalStore::open(&path).await.unwrap();
        let mut tx = other.begin_write().await.unwrap();
        tx.execute("INSERT INTO config(name,value,mtime) VALUES('unrelated','value',0)")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let error = owner.read_content(&name, small).await.unwrap_err();
        assert!(error.operation.contains("admission stale"));
        assert!(
            owner
                .put_private_file_content(&name, b"owner bytes", small)
                .await
                .unwrap_err()
                .operation
                .contains("admission stale")
        );
        // A caught rejection cannot silently advance the stale data_version.
        assert!(
            owner
                .read_content(&name, small)
                .await
                .unwrap_err()
                .operation
                .contains("admission stale")
        );
        drop(owner);
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        assert_eq!(
            owner.read_content(&name, small).await.unwrap(),
            b"owner bytes"
        );
        let mut tx = owner.store.begin_write().await.unwrap();
        tx.execute("INSERT INTO config(name,value,mtime) VALUES('same-connection','value',0)")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        assert!(
            owner
                .read_content(&name, small)
                .await
                .unwrap_err()
                .operation
                .contains("admission stale")
        );
    }
    #[tokio::test]
    async fn date_provider_and_invalid_complete_root_fail_without_partial_initialization() {
        use crate::artifact::DateProvider;
        let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
        let tx = store.begin_write().await.unwrap();
        assert_eq!(OwnerDates(&tx).julianday(None).await.unwrap(), None);
        assert_eq!(
            OwnerDates(&tx)
                .julianday(Some(b"not-a-date"))
                .await
                .unwrap(),
            None
        );
        tx.rollback().await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let first = tx.verify_content_name("invalid", b"bytes").unwrap_err();
        let provider_error = OwnerDates(&tx)
            .julianday(Some(b"2026-11-18T00:00:00.000"))
            .await
            .unwrap_err();
        assert_eq!(
            provider_error
                .0
                .downcast_ref::<StorageError>()
                .unwrap()
                .operation,
            first.operation
        );
        assert!(tx.commit().await.is_err());
        let temp = tempfile::tempdir().unwrap();
        let error = LocalOwnerRepository::create(
            temp.path().join("repo"),
            CreateOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                checkout_id: "default",
                checkout_root: temp.path(),
                date: b"not-a-date",
                user: b"Owner",
                content_limits: root_content_limits(),
                parse_limits: root_parse_limits(),
            },
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error.operation, "native artifact validation failed");
    }
    #[tokio::test]
    async fn valid_native_artifact_with_unsupported_root_semantics_cannot_crosslink() {
        let original = empty_root_bytes(b"2026-11-18T00:00:00.000", b"Owner", 65536).unwrap();
        let prefix = std::str::from_utf8(&original[..original.len() - 35]).unwrap();
        for (from, to, reason) in [
            (
                "initial\\sempty\\scheck-in",
                "other",
                "unsupported complete public root closure",
            ),
            (
                "*branch * trunk",
                "*branch * other",
                "unsupported initial branch tags",
            ),
            (
                "initial\\sempty\\scheck-in",
                "\\initial\\sempty\\scheck-in",
                "unsupported noncanonical initial root bytes",
            ),
        ] {
            let changed = prefix.replace(from, to);
            let digest = content_hash::hex(&crate::artifact_checksum::md5(changed.as_bytes()));
            let bytes = format!("{changed}Z {digest}\n").into_bytes();
            let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
            let mut tx = store.begin_write().await.unwrap();
            tx.install_schema().await.unwrap();
            let parsed = parse_owned(&tx, &bytes, root_parse_limits()).await.unwrap();
            assert!(
                tx.crosslink_supported_checkin(1, &parsed)
                    .await
                    .unwrap_err()
                    .operation
                    .contains(reason),
                "{reason}"
            );
            assert!(tx.commit().await.is_err());
        }
    }
    #[tokio::test]
    async fn admission_rejects_wrong_root_inode_and_persisted_foreign_key_damage() {
        for variant in ["inode", "foreign-key"] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("repo");
            fixture(&path).await;
            let mut store = LocalStore::open(&path).await.unwrap();
            if variant == "foreign-key" {
                store
                    .connection
                    .as_ref()
                    .unwrap()
                    .execute("PRAGMA foreign_keys=OFF", ())
                    .await
                    .unwrap();
            }
            let mut tx = store.begin_write().await.unwrap();
            tx.execute(if variant == "inode" {
                "UPDATE g_checkout_owner SET root_inode='0'"
            } else {
                "INSERT INTO g_path_scope VALUES('missing-checkout','path','shared')"
            })
            .await
            .unwrap();
            tx.commit().await.unwrap();
            let error = LocalOwnerRepository::open(&path, admission_limits())
                .await
                .err()
                .unwrap();
            assert!(
                error.operation.contains(if variant == "inode" {
                    "root identity differs"
                } else {
                    "foreign key integrity failure"
                }),
                "{}",
                error.operation
            );
        }
    }
    #[tokio::test]
    async fn malformed_old_delta_metadata_and_limits_roll_back_prior_private_puts() {
        for variant in [
            "cycle",
            "depth",
            "size-type",
            "shun",
            "source-type",
            "read-source-type",
            "output-limit",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("repo");
            fixture(&path).await;
            let bytes = b"restored target";
            let name = content_hash::hex(&content_hash::sha3(bytes, 256).unwrap());
            let base = b"baseline";
            let base_name = content_hash::hex(&content_hash::sha3(base, 256).unwrap());
            let instructions = crate::delta::create(base, bytes).unwrap();
            let mut store = LocalStore::open(&path).await.unwrap();
            let mut tx = store.begin_write().await.unwrap();
            tx.execute_bound(
                "INSERT INTO blob(rid,size,uuid,content) VALUES(200,?1,?2,?3)",
                vec![
                    Value::Integer(bytes.len() as i64),
                    Value::Text(name.clone()),
                    Value::Blob(compression::pack(&instructions, -1).unwrap()),
                ],
            )
            .await
            .unwrap();
            tx.execute_bound(
                "INSERT INTO blob(rid,size,uuid,content) VALUES(201,?1,?2,?3)",
                vec![
                    Value::Integer(base.len() as i64),
                    Value::Text(base_name.clone()),
                    Value::Blob(compression::pack(base, -1).unwrap()),
                ],
            )
            .await
            .unwrap();
            tx.execute("INSERT INTO delta VALUES(200,201)")
                .await
                .unwrap();
            match variant {
                "cycle" => {
                    tx.execute("INSERT INTO delta VALUES(201,200)")
                        .await
                        .unwrap();
                }
                "size-type" => {
                    tx.execute("UPDATE blob SET size='bad' WHERE rid=201")
                        .await
                        .unwrap();
                }
                "shun" => {
                    tx.execute_bound(
                        "INSERT INTO shun(uuid) VALUES(?1)",
                        vec![Value::Text(base_name)],
                    )
                    .await
                    .unwrap();
                }
                "source-type" | "read-source-type" => {
                    tx.execute("UPDATE delta SET srcid='bad' WHERE rid=200")
                        .await
                        .unwrap();
                }
                _ => {}
            }
            tx.commit().await.unwrap();
            let mut owner = LocalOwnerRepository::open(&path, admission_limits())
                .await
                .unwrap();
            let mut tx = owner.store.begin_write().await.unwrap();
            let prior = b"prior valid file";
            let prior_name = content_hash::hex(&content_hash::sha3(prior, 256).unwrap());
            tx.put_private_file_content(&owner.principal, &prior_name, prior, limits())
                .await
                .unwrap();
            let bounds = if variant == "depth" {
                Limits {
                    max_chain_depth: 1,
                    ..limits()
                }
            } else if variant == "output-limit" {
                Limits {
                    max_output_bytes: 1,
                    ..limits()
                }
            } else {
                limits()
            };
            let error = if variant == "read-source-type" {
                tx.read_owner_content(&owner.principal, &name, bounds)
                    .await
                    .map(|_| ())
            } else {
                tx.put_private_file_content(&owner.principal, &name, bytes, bounds)
                    .await
                    .map(|_| ())
            }
            .unwrap_err();
            assert_eq!(
                tx.commit().await.unwrap_err().operation,
                error.operation,
                "{variant}"
            );
            drop(owner);
            let mut reopened = LocalOwnerRepository::open(&path, admission_limits())
                .await
                .unwrap();
            assert!(
                reopened.read_content(&prior_name, limits()).await.is_err(),
                "{variant}"
            );
        }
    }
    #[tokio::test]
    async fn full_target_fulfills_with_phantom_old_baseline_without_modifying_that_baseline() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        fixture(&path).await;
        let bytes = b"restored target";
        let name = content_hash::hex(&content_hash::sha3(bytes, 256).unwrap());
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute_bound(
            "INSERT INTO blob(rid,size,uuid,content) VALUES(200,?1,?2,?3)",
            vec![
                Value::Integer(bytes.len() as i64),
                Value::Text(name.clone()),
                Value::Blob(compression::pack(b"old delta", -1).unwrap()),
            ],
        )
        .await
        .unwrap();
        tx.execute("INSERT INTO blob(rid,size,uuid) VALUES(201,-1,'0000000000000000000000000000000000000000')").await.unwrap();
        tx.execute("INSERT INTO phantom VALUES(201)").await.unwrap();
        tx.execute("INSERT INTO delta VALUES(200,201)")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        owner
            .put_private_file_content(&name, bytes, limits())
            .await
            .unwrap();
        assert_eq!(owner.read_content(&name, limits()).await.unwrap(), bytes);
        let mut tx = owner.store.begin_write().await.unwrap();
        assert_eq!(
            tx.query("SELECT size FROM blob WHERE rid=201")
                .await
                .unwrap(),
            vec![vec![Value::Integer(-1)]]
        );
        tx.commit().await.unwrap();
    }
    #[tokio::test]
    async fn creation_rejects_invalid_bindings_and_open_rejects_nonregular_files() {
        let temp = tempfile::tempdir().unwrap();
        let root_file = temp.path().join("root-file");
        std::fs::write(&root_file, b"file").unwrap();
        for variant in ["identity", "root-kind"] {
            let options = CreateOptions {
                repository_id: if variant == "identity" {
                    "bad"
                } else {
                    "0123456789abcdef0123456789abcdef01234567"
                },
                owner_id: "owner",
                checkout_id: "default",
                checkout_root: if variant == "root-kind" {
                    &root_file
                } else {
                    temp.path()
                },
                date: b"2026-11-18T00:00:00.000",
                user: b"Owner",
                content_limits: root_content_limits(),
                parse_limits: root_parse_limits(),
            };
            let path = temp.path().join(variant);
            assert!(LocalOwnerRepository::create(&path, options).await.is_err());
            assert!(!path.exists());
        }
        let error = LocalOwnerRepository::open(Path::new("/dev/null"), admission_limits())
            .await
            .err()
            .unwrap();
        assert!(error.operation.contains("not a regular file"));
    }
    #[tokio::test]
    async fn admitted_authority_metadata_cannot_be_blessed_by_a_private_operation_counter_refresh()
    {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let name = fixture(&path).await;
        let mut owner = LocalOwnerRepository::open(&path, admission_limits())
            .await
            .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        tx.execute("UPDATE g_checkout SET availability='blocked'")
            .await
            .unwrap();
        tx.commit().await.unwrap();
        // Simulate a future controlled-operation counter refresh: the exact authority proof must still reject the altered checkout.
        owner.admission.total_changes = owner.store.connection.as_ref().unwrap().total_changes();
        assert!(
            owner
                .read_content(&name, limits())
                .await
                .unwrap_err()
                .operation
                .contains("authority changed")
        );
    }
    #[tokio::test]
    async fn missing_or_changed_catalog_denies_local_owner_admission() {
        let temp = tempfile::tempdir().unwrap();
        let empty = temp.path().join("empty");
        assert!(
            LocalOwnerRepository::open(&empty, admission_limits())
                .await
                .is_err()
        );
        let path = temp.path().join("repo");
        fixture(&path).await;
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("DROP INDEX delta_i1").await.unwrap();
        tx.commit().await.unwrap();
        assert!(
            LocalOwnerRepository::open(&path, admission_limits())
                .await
                .is_err()
        );
    }
    /// Every parser compartment has a finite positive ceiling and independent exhaustion check.
    #[test]
    fn aggregate_parser_limits_and_each_usage_compartment_fail_closed() {
        for index in 0..4 {
            let mut limits = root_parse_limits();
            match index {
                0 => limits.input_bytes = 0,
                1 => limits.cards = 0,
                2 => limits.owned_bytes = 0,
                _ => limits.entries = 0,
            }
            assert_eq!(
                ParseWork::new(limits).err().unwrap().operation,
                "invalid aggregate parser limits"
            );
            let mut work = ParseWork::new(root_parse_limits()).unwrap();
            let mut usage = crate::artifact::ParseUsage {
                input_bytes: 0,
                cards: 0,
                owned_bytes: 0,
                entries: 0,
            };
            match index {
                0 => usage.input_bytes = root_parse_limits().input_bytes + 1,
                1 => usage.cards = root_parse_limits().cards + 1,
                2 => usage.owned_bytes = root_parse_limits().owned_bytes + 1,
                _ => usage.entries = root_parse_limits().entries + 1,
            }
            assert_eq!(
                work.charge(usage).unwrap_err().operation,
                "aggregate parser usage exceeds remaining caller limits"
            );
        }
    }
    /// A malformed parse latches its cause; later parsing preserves that first error and allowance.
    #[tokio::test]
    async fn budgeted_parser_latches_malformed_artifact_and_preserves_prior_failure() {
        let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let mut work = ParseWork::new(root_parse_limits()).unwrap();
        let before = work.remaining;
        let first = parse_owned_budgeted(&mut tx, b"not an artifact", &mut work)
            .await
            .unwrap_err();
        assert_eq!(first.operation, "native artifact validation failed");
        let bytes = empty_root_bytes(b"2026-11-18T00:00:00.000", b"Owner", 4096).unwrap();
        let error = parse_owned_budgeted(&mut tx, &bytes, &mut work)
            .await
            .unwrap_err();
        assert_eq!(error.operation, first.operation);
        assert_eq!(
            (
                work.remaining.input_bytes,
                work.remaining.cards,
                work.remaining.owned_bytes,
                work.remaining.entries
            ),
            (
                before.input_bytes,
                before.cards,
                before.owned_bytes,
                before.entries
            )
        );
        assert_eq!(tx.commit().await.unwrap_err().operation, first.operation);
    }
    /// Check the consumed encoding boundary without requiring filesystem support for raw names.
    #[test]
    fn checkout_root_text_preserves_utf8_and_refuses_raw_non_utf8_bytes() {
        use std::os::unix::ffi::OsStringExt;
        let valid = Path::new("/checkout/valid");
        assert_eq!(checkout_root_text(valid).unwrap(), "/checkout/valid");
        let raw = std::path::PathBuf::from(std::ffi::OsString::from_vec(
            b"/checkout/invalid-\xff".to_vec(),
        ));
        assert_eq!(
            checkout_root_text(&raw).unwrap_err().operation,
            "checkout root is not UTF-8"
        );
    }
    /// Successful parses debit one shared ledger exactly; the next exhausted parse latches refusal.
    #[tokio::test]
    async fn cumulative_parser_ledger_retains_exact_charges_across_successful_parses() {
        let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let bytes = empty_root_bytes(b"2026-11-18T00:00:00.000", b"Owner", 4096).unwrap();
        let (_, usage) = crate::artifact::parse_artifact_with_usage(
            &bytes,
            root_parse_limits(),
            &OwnerDates(&tx),
        )
        .await
        .unwrap();
        let mut work = ParseWork::new(crate::artifact::ParseLimits {
            input_bytes: usage.input_bytes * 2,
            cards: usage.cards * 2,
            owned_bytes: usage.owned_bytes * 2,
            entries: usage.entries * 2,
        })
        .unwrap();
        for left in [1, 0] {
            let parsed = parse_owned_budgeted(&mut tx, &bytes, &mut work)
                .await
                .unwrap();
            assert_eq!(parsed.raw(), bytes);
            assert_eq!(
                (
                    work.remaining.input_bytes,
                    work.remaining.cards,
                    work.remaining.owned_bytes,
                    work.remaining.entries
                ),
                (
                    usage.input_bytes * left,
                    usage.cards * left,
                    usage.owned_bytes * left,
                    usage.entries * left
                )
            );
        }
        let first = parse_owned_budgeted(&mut tx, &bytes, &mut work)
            .await
            .unwrap_err();
        assert_eq!(first.operation, "native artifact validation failed");
        assert!(std::error::Error::source(&first).is_some());
        assert_eq!(tx.commit().await.unwrap_err().operation, first.operation);
    }
    /// A per-artifact ledger keeps its whole allowance after each parse, and still
    /// refuses one artifact over a ceiling.
    #[tokio::test]
    async fn a_per_artifact_parser_ledger_does_not_accumulate() {
        let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let bytes = empty_root_bytes(b"2026-11-18T00:00:00.000", b"Owner", 4096).unwrap();
        let (_, usage) = crate::artifact::parse_artifact_with_usage(
            &bytes,
            root_parse_limits(),
            &OwnerDates(&tx),
        )
        .await
        .unwrap();
        let exact = crate::artifact::ParseLimits {
            input_bytes: usage.input_bytes,
            cards: usage.cards,
            owned_bytes: usage.owned_bytes,
            entries: usage.entries,
        };
        let mut work = ParseWork::per_artifact(exact).unwrap();
        for _ in 0..3 {
            parse_owned_budgeted(&mut tx, &bytes, &mut work)
                .await
                .unwrap();
            assert_eq!(
                (
                    work.remaining.input_bytes,
                    work.remaining.cards,
                    work.remaining.owned_bytes,
                    work.remaining.entries
                ),
                (
                    usage.input_bytes,
                    usage.cards,
                    usage.owned_bytes,
                    usage.entries
                )
            );
        }
        let mut over = usage;
        over.entries += 1;
        let refused = work.charge(over).unwrap_err();
        assert_eq!(
            refused.operation,
            "aggregate parser usage exceeds remaining caller limits"
        );
        assert_eq!(work.remaining.entries, usage.entries);
        tx.rollback().await.unwrap();
    }
    /// Installing the `ticket-table` setting needs both ticket tables and their index, and puts their SQL in the setting in order.
    #[tokio::test]
    async fn ticket_table_setting_requires_the_complete_ticket_schema() {
        let mut store = LocalStore::open_inner(Path::new(":memory:"), false)
            .await
            .unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("CREATE TABLE config(name,value,mtime)")
            .await
            .unwrap();
        // Create the tables out of name order, so that only the ORDER BY gives the order.
        tx.execute("CREATE TABLE ticketchng(b)").await.unwrap();
        tx.execute("CREATE TABLE ticket(a)").await.unwrap();
        assert_eq!(
            tx.install_ticket_table_config()
                .await
                .unwrap_err()
                .operation,
            "native ticket schema unavailable"
        );
        tx.execute("CREATE INDEX ticketchng_idx1 ON ticketchng(b)")
            .await
            .unwrap();
        tx.install_ticket_table_config().await.unwrap();
        assert_eq!(
            tx.query("SELECT value FROM config WHERE name='ticket-table'")
                .await
                .unwrap(),
            vec![vec![Value::Text(
                "CREATE TABLE ticket(a);\nCREATE TABLE ticketchng(b);\nCREATE INDEX ticketchng_idx1 ON ticketchng(b);\n"
                    .into()
            )]]
        );
        tx.rollback().await.unwrap();
    }
    /// Catalog dispatch refuses malformed aggregate shapes without inspecting raw identities.
    #[test]
    fn catalog_scalar_decoder_accepts_only_one_supported_marker() {
        for version in [1, 6] {
            assert_eq!(
                supported_catalog_from_rows(&[vec![
                    Value::Integer(1),
                    Value::Integer(1),
                    Value::Integer(version)
                ]])
                .unwrap(),
                version
            );
        }
        // One marker of an older Gossil catalog is an older-catalog refusal (CAT-07).
        for version in [2, 4, 5] {
            let error = supported_catalog_from_rows(&[vec![
                Value::Integer(1),
                Value::Integer(1),
                Value::Integer(version),
            ]])
            .unwrap_err();
            assert_eq!(
                error.older(),
                Some(super::super::OlderCatalog { found: version })
            );
            assert_eq!(
                error.operation,
                format!("repository is at catalog {version}, and this build needs catalog 6")
            );
            let named = error.naming_repository(Path::new("/r/old.gossil"));
            assert_eq!(
                named.operation,
                format!(
                    "gossil: repository /r/old.gossil is at catalog {version}, and this build needs catalog 6.\n\
                     Gossil has no upgrade path before its first real release.\n\
                     Create a new repository with gos v init or gos i init."
                )
            );
        }
        // Naming leaves every other refusal unchanged.
        let other = StorageError::state("unsupported storage version");
        assert_eq!(
            other.naming_repository(Path::new("/r")).operation,
            "unsupported storage version"
        );
        for rows in [
            vec![],
            vec![vec![], vec![]],
            vec![vec![
                Value::Integer(1),
                Value::Integer(1),
                Value::Integer(3),
            ]],
            vec![vec![
                Value::Integer(1),
                Value::Integer(1),
                Value::Integer(7),
            ]],
            vec![vec![
                Value::Integer(2),
                Value::Integer(2),
                Value::Integer(4),
            ]],
        ] {
            assert_eq!(
                supported_catalog_from_rows(&rows).unwrap_err().operation,
                "unsupported storage version"
            );
        }
    }

    /// The empty-root guard of catalog 6 refuses a row in each of the three audit tables, and a catalog with no owner.
    #[tokio::test]
    async fn empty_catalog6_guard_refuses_each_audit_table_row() {
        for (table, sql) in [
            (
                "transactions",
                "INSERT INTO transactions VALUES('t','p',NULL,'c',NULL,'d')",
            ),
            (
                "transaction_events",
                "INSERT INTO transactions VALUES('t','p',NULL,'c',NULL,'d');INSERT INTO transaction_events VALUES('e','t',NULL,'insert','r','x',NULL,NULL,NULL,'d')",
            ),
            (
                "g_command_audit",
                "INSERT INTO transactions VALUES('t','p',NULL,'c',NULL,'d');INSERT INTO g_local_command VALUES('c','owner','p',0,1,x'00',0,'gest_entity',NULL);INSERT INTO g_command_audit VALUES('c','t','owner')",
            ),
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("repository");
            let mut limits = admission_limits();
            limits.content.max_total_decoded_bytes = 16_777_216;
            let mut owner = LocalOwnerRepository::initialize(
                &path,
                InitializeOptions {
                    repository_id: "0123456789abcdef0123456789abcdef01234567",
                    owner_id: "owner",
                    date: "2026-11-18 12:34:56.125",
                    user: b"Owner",
                    password: "secret",
                    sha1: false,
                    content_limits: limits.content,
                    parse_limits: limits.parser,
                },
            )
            .await
            .unwrap();
            let mut tx = owner.store.begin_write().await.unwrap();
            for statement in sql.split(';') {
                tx.execute(statement).await.unwrap();
            }
            assert_eq!(
                tx.validate_empty_catalog6(
                    "root",
                    &owner.principal,
                    limits,
                    &mut crate::storage::content::Budget::new(limits.content).unwrap(),
                    &mut ParseWork::new(limits.parser).unwrap()
                )
                .await
                .unwrap_err()
                .operation,
                format!("unsupported nonempty catalog6 table: {table}")
            );
            // The guard checks the owner first, so with no owner it fails before any table check runs.
            if table == "transactions" {
                tx.execute("DELETE FROM g_local_owner").await.unwrap();
                assert_eq!(
                    tx.validate_empty_catalog6(
                        "root",
                        &owner.principal,
                        limits,
                        &mut crate::storage::content::Budget::new(limits.content).unwrap(),
                        &mut ParseWork::new(limits.parser).unwrap()
                    )
                    .await
                    .unwrap_err()
                    .operation,
                    "unsupported catalog6 owner cardinality"
                );
            }
            tx.rollback().await.unwrap();
        }
    }
    /// The plain policy route checks the checkout count of catalog 6, as the metered route does.
    #[tokio::test]
    async fn plain_policy_refuses_a_catalog6_checkout_count_without_registrations() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repository");
        let mut limits = admission_limits();
        limits.content.max_total_decoded_bytes = 16_777_216;
        let mut owner = LocalOwnerRepository::initialize(
            &path,
            InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                date: "2026-11-18 12:34:56.125",
                user: b"Owner",
                password: "secret",
                sha1: false,
                content_limits: limits.content,
                parse_limits: limits.parser,
            },
        )
        .await
        .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        tx.validate_principal(&owner.principal).await.unwrap();
        tx.execute("UPDATE g_storage SET checkout_count=1")
            .await
            .unwrap();
        assert_eq!(
            tx.validate_principal(&owner.principal)
                .await
                .unwrap_err()
                .operation,
            "unsupported populated catalog6 checkout scope"
        );
        tx.rollback().await.unwrap_err();
    }
    /// Legacy internal entries route catalog6 through bounded admission and preserve proof equality.
    #[tokio::test]
    async fn catalog6_internal_admission_entries_use_recording_validation_and_exact_proof() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repository");
        let mut limits = admission_limits();
        limits.content.max_total_decoded_bytes = 16_777_216;
        let mut owner = LocalOwnerRepository::initialize(
            &path,
            InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                date: "2026-11-18 12:34:56.125",
                user: b"Owner",
                password: "secret",
                sha1: false,
                content_limits: limits.content,
                parse_limits: limits.parser,
            },
        )
        .await
        .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        let mut invalid = limits;
        invalid.content.max_total_decoded_bytes = 0;
        assert_eq!(
            tx.capture_admission(invalid).await.err().unwrap().operation,
            "invalid finite content limits"
        );
        assert_eq!(
            tx.validate_initial_repository(&owner.principal, invalid)
                .await
                .unwrap_err()
                .operation,
            "invalid finite content limits"
        );
        invalid = limits;
        invalid.parser.entries = 0;
        assert_eq!(
            tx.validate_initial_repository(&owner.principal, invalid)
                .await
                .unwrap_err()
                .operation,
            "invalid aggregate parser limits"
        );
        tx.validate_initial_repository(&owner.principal, limits)
            .await
            .unwrap();
        let mut proof = tx.capture_admission(limits).await.unwrap();
        assert!(proof.content_limits.is_some());
        tx.check_admission_budgeted(
            &owner.principal,
            &proof,
            &mut super::super::content::Budget::new(limits.content).unwrap(),
        )
        .await
        .unwrap();
        // A proof with missing authority rows is stale even when connection counters still agree.
        proof.authority.clear();
        let first = tx
            .check_admission_budgeted(
                &owner.principal,
                &proof,
                &mut super::super::content::Budget::new(limits.content).unwrap(),
            )
            .await
            .unwrap_err();
        assert_eq!(
            first.operation,
            "repository authority changed; explicit readmission required"
        );
        assert_eq!(tx.commit().await.unwrap_err().operation, first.operation);
    }

    /// Real malformed blob metadata is refused before payload ownership and the failure rolls back.
    #[tokio::test]
    async fn recording_content_reader_refuses_persisted_wrong_type_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repository");
        let mut limits = admission_limits();
        limits.content.max_total_decoded_bytes = 16_777_216;
        let mut owner = LocalOwnerRepository::initialize(
            &path,
            InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                date: "2026-11-18 12:34:56.125",
                user: b"Owner",
                password: "secret",
                sha1: false,
                content_limits: limits.content,
                parse_limits: limits.parser,
            },
        )
        .await
        .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        let names = tx
            .query_bound("SELECT uuid FROM blob", vec![])
            .await
            .unwrap();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].len(), 1);
        let name = names[0][0].as_text().expect("native hash expected");
        let before = tx
            .read_recording_content_budget(
                &owner.principal,
                name,
                &mut super::super::content::Budget::new(limits.content).unwrap(),
            )
            .await
            .unwrap();
        tx.execute("UPDATE blob SET size='invalid-size'")
            .await
            .unwrap();
        assert_eq!(
            tx.query_bound("SELECT typeof(size) FROM blob", vec![])
                .await
                .unwrap(),
            vec![vec![Value::Text("text".into())]]
        );
        let error = tx
            .read_recording_content_budget(
                &owner.principal,
                name,
                &mut super::super::content::Budget::new(limits.content).unwrap(),
            )
            .await
            .unwrap_err();
        assert_eq!(error.operation, "corrupt recording content metadata");
        assert_eq!(tx.commit().await.unwrap_err().operation, error.operation);
        drop(owner);
        let mut owner = LocalOwnerRepository::open(&path, limits).await.unwrap();
        assert_eq!(
            owner.read_content(name, limits.content).await.unwrap(),
            before
        );
    }
}
