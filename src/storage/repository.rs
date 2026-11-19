//! Local OS access establishes owner trust separately from remote request principals.
//!
//! ## Recommendations
//! Open this handle only for local CLI/library work; never derive HTTP request authority from it.
use super::{LocalStore, StorageError, content::Limits, policy::Principal};
use libsql::Value;
use std::path::Path;

/// Fixed scalar predicates bound returned data, not total SQLite scan work.
/// The admitted root cannot produce other artifact types or a current file/merge selection.
/// Historical rows, diagnostics, opaque content and target-only references retain no root authority.
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

/// Explicit finite bounds for repository root admission.
#[derive(Clone, Copy)]
pub struct AdmissionLimits {
    /// Envelope, reconstruction and work bounds for root content.
    pub content: Limits,
    /// Structural parser bounds for root validation.
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
}

/// Explicit local identities and bounds for a newly created native repository.
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
        let result = async {
            tx.check_admission(&self.repository.principal,&self.repository.admission).await?;
            let rows = tx.query_bound("SELECT b.uuid FROM blob b JOIN g_public_receipt p ON p.native_hash=b.uuid AND p.supporting_root=b.uuid WHERE b.uuid=?1 AND NOT EXISTS(SELECT 1 FROM private x WHERE x.rid=b.rid) AND NOT EXISTS(SELECT 1 FROM delta d WHERE d.rid=b.rid)",vec![Value::Text(native_name.into())]).await?;
            if rows.is_empty() { return Err(StorageError::state("shared content unavailable")); }
            tx.read_owner_content(&self.repository.principal,native_name,limits).await
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
    /// Create a complete repository in an exclusively created destination file.
    /// # Arguments
    /// * `path` - New database path; existing files are rejected.
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
    /// Rejects an existing destination, invalid root/bindings, or any initialization failure.
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
    /// Keep deterministic phase failures private to the module's tests.
    /// # Arguments
    /// * `path` - New local database destination.
    /// * `options` - Local identities, root fields and finite admission bounds.
    /// # Returns
    /// A sealed owner after complete initialization and admission.
    /// # Errors
    /// Rejects existing destinations, invalid identities or roots, resource limits and filesystem or SQL failures.
    async fn create_internal(
        path: impl AsRef<Path>,
        options: CreateOptions<'_>,
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
            || options.checkout_id.is_empty()
        {
            return Err(StorageError::state(
                "invalid local repository identity binding",
            ));
        }
        let root = std::fs::canonicalize(options.checkout_root)
            .map_err(|e| StorageError::caused("checkout root unavailable", e))?;
        let metadata = std::fs::metadata(&root)
            .map_err(|e| StorageError::caused("checkout root metadata unavailable", e))?;
        if !metadata.is_dir() {
            return Err(StorageError::state("checkout root is not a directory"));
        }
        let root_text = root
            .to_str()
            .ok_or_else(|| StorageError::state("checkout root is not UTF-8"))?;
        let root_ceiling = usize::try_from(
            u64::from(options.content_limits.max_output_bytes)
                .min(options.content_limits.max_total_decoded_bytes),
        )
        .unwrap_or(usize::MAX)
        .min(options.parse_limits.input_bytes);
        let bytes = empty_root_bytes(options.date, options.user, root_ceiling)?;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| StorageError::caused("exclusive repository creation failed", e))?;
        drop(file);
        let mut store = LocalStore::open(path).await?;
        let mut tx = store.begin_write().await?;
        let result = async {
            tx.install_schema().await?;
            #[cfg(test)]
            if failure == Some("schema") { return Err(StorageError::state("injected initialization phase failure")); }
            let parsed = parse_owned(&tx, &bytes, options.parse_limits).await?;
            validate_empty_root(&parsed)?;
            let date = parsed.date().ok_or_else(|| StorageError::state("empty root has no date"))?;
            std::str::from_utf8(options.user).map_err(|e| StorageError::caused("local native user is not UTF-8", e))?;
            let hash = crate::content_hash::hex(&crate::content_hash::sha3(&bytes, 256).unwrap());
            tx.verify_content_name(&hash, &bytes)?;
            let packed = crate::compression::pack_bounded(&bytes, -1, usize::try_from(options.content_limits.max_stored_bytes.min(options.content_limits.max_total_stored_bytes)).unwrap_or(usize::MAX))
                .map_err(|e| StorageError::caused("empty root compression failed", e))?;
            tx.decode_full_content(&hash, bytes.len() as i64, &packed, options.content_limits)?;
            tx.execute_bound("INSERT INTO config(name,value,mtime) VALUES('project-code',?1,unixepoch()),('content-schema','2',unixepoch()),('aux-schema','2015-01-24',unixepoch()),('server-code',lower(hex(randomblob(20))),unixepoch())", vec![Value::Text(options.repository_id.into())]).await?;
            tx.execute_bound("INSERT INTO g_storage VALUES(1,?1,1,'gest-ticket-v1',0)", vec![Value::Text(options.repository_id.into())]).await?;
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
            tx.execute_bound("INSERT INTO g_checkout VALUES(?1,?2,?3,NULL,0,0,'ready')", vec![Value::Text(options.checkout_id.into()), Value::Text(root_text.into()), Value::Text(hash)]).await?;
            tx.execute_bound("INSERT INTO g_checkout_owner VALUES(?1,?2,?3,?4)", vec![Value::Text(options.checkout_id.into()), Value::Text(options.owner_id.into()), Value::Text(metadata.dev().to_string()), Value::Text(metadata.ino().to_string())]).await?;
            tx.validate_catalog().await?;
            #[cfg(test)]
            if failure == Some("bindings") { return Err(StorageError::state("injected initialization phase failure")); }
            tx.execute_bound("UPDATE g_storage SET policy_complete=1 WHERE singleton=1", vec![]).await?;
            #[cfg(test)]
            if failure == Some("marker") { return Err(StorageError::state("injected initialization phase failure")); }
            tx.validate_initial_repository(&Principal { repository: options.repository_id.into(), owner: options.owner_id.into() },AdmissionLimits {content:options.content_limits,parser:options.parse_limits}).await?;
            Ok(())
        }.await;
        let result = tx.record(result);
        if result.is_err() {
            return Err(tx.commit().await.unwrap_err());
        }
        let admission = tx.capture_admission().await;
        tx.commit().await?;
        let admission = admission?;
        Ok(Self {
            store,
            principal: Principal {
                repository: options.repository_id.into(),
                owner: options.owner_id.into(),
            },
            admission,
        })
    }
    /// Open an existing complete repository under local operating-system authority.
    ///
    /// The durable repository and owner identities are read from this file rather
    /// than accepted as caller-selected capability strings.
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
    /// Rejects missing policy, unsupported markers, corrupt bindings and engine errors.
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
        let mut store = LocalStore::open(path).await?;
        let mut tx = store.begin_write().await?;
        let result = async {
            let rows = tx.query_bound("SELECT s.repository_id,o.owner_id FROM g_storage s CROSS JOIN g_local_owner o WHERE s.singleton=1 AND o.singleton=1", vec![]).await?;
            let [row] = rows.as_slice() else { return Err(StorageError::state("local repository binding unavailable")); };
            let [Value::Text(repository), Value::Text(owner)] = row.as_slice() else { return Err(StorageError::state("corrupt local repository binding")); };
            let principal = Principal { repository: repository.clone(), owner: owner.clone() };
            tx.validate_principal(&principal).await?;
            tx.validate_initial_repository(&principal,admission_limits).await?;
            Ok(principal)
        }.await;
        let principal = tx.record(result);
        if principal.is_err() {
            return Err(tx.commit().await.unwrap_err());
        }
        let admission = tx.capture_admission().await;
        tx.commit().await?;
        let admission = admission?;
        let principal = principal?;
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

impl super::WriteTransaction<'_> {
    /// Capture durable authority metadata without reading any content envelope.
    /// # Returns
    /// Owned exact durable authority rows.
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
        let roots = self.query_bound("SELECT rid,uuid FROM blob WHERE uuid IN(SELECT supporting_root FROM g_public_receipt) ORDER BY rid LIMIT 2",vec![]).await?;
        for root in roots {
            snapshot.push(self.query_bound(ROOT_PROJECTION_CONFLICTS, root).await?);
        }
        Ok(snapshot)
    }
    /// Capture authority and connection counters after successful validation.
    /// # Returns
    /// The sealed authority snapshot and current connection counters.
    /// # Errors
    /// Returns SQL execution or owned value-decoding errors while capturing authority.
    async fn capture_admission(&mut self) -> Result<AdmissionProof, StorageError> {
        let authority = self.authority_snapshot().await?;
        let data_version = self.query_bound("PRAGMA data_version", vec![]).await?;
        Ok(AdmissionProof {
            authority,
            data_version,
            total_changes: self.connection().total_changes(),
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
        let result = async {
            self.validate_principal(principal).await?;
            if !self.query_bound("PRAGMA foreign_key_check",vec![]).await?.is_empty() { return Err(StorageError::state("repository foreign key integrity failure")); }
            let roots = self.query_bound("SELECT DISTINCT supporting_root FROM g_public_receipt ORDER BY supporting_root LIMIT 2",vec![]).await?;
            if roots.len() != 1 { return Err(StorageError::state("supported public root closure unavailable")); }
            let [Value::Text(root)] = roots[0].as_slice() else { return Err(StorageError::state("corrupt public root binding")); };
            let receipts = self.query_bound("SELECT native_hash,supporting_root,validation_class FROM g_public_receipt ORDER BY native_hash,supporting_root LIMIT 2",vec![]).await?;
            if receipts != vec![vec![Value::Text(root.clone()),Value::Text(root.clone()),Value::Text("local-explicit-publication".into())]] { return Err(StorageError::state("unsupported required public graph closure")); }
            let bytes = self.read_owner_content(principal,root,limits.content).await?;
            let parsed = parse_owned(self,&bytes,limits.parser).await?;
            validate_empty_root(&parsed)?;
            let rows = self.query_bound("SELECT b.rid,e.type,e.mtime,e.user,e.comment,e.omtime,e.tagid,e.uid,e.bgcolor,e.euser,e.ecomment,e.brief FROM blob b JOIN event e ON e.objid=b.rid JOIN leaf l ON l.rid=b.rid WHERE b.uuid=?1 AND NOT EXISTS(SELECT 1 FROM private p WHERE p.rid=b.rid) AND NOT EXISTS(SELECT 1 FROM delta d WHERE d.rid=b.rid)",vec![Value::Text(root.clone())]).await?;
            let [row] = rows.as_slice() else { return Err(StorageError::state("native public root crosslink unavailable")); };
            let Some(Value::Integer(rid)) = row.first() else { return Err(StorageError::state("corrupt root RID")); };
            let user = parsed.user().unwrap();
            let user = std::str::from_utf8(&user.bytes[..user.c_string_len]).map_err(|e| StorageError::caused("root user UTF-8",e))?;
            let date = parsed.date().unwrap();
            // Native julianday is bounded to 0..5373484.5. NUMERIC affinity stores
            // an integral day exactly as INTEGER in event and tagxref columns.
            let stored_date = if date.fract() == 0.0 {
                Value::Integer(date as i64)
            } else {
                Value::Real(date)
            };
            if row != &vec![Value::Integer(*rid),Value::Text("ci".into()),stored_date.clone(),Value::Text(user.into()),Value::Text("initial empty check-in".into()),stored_date.clone(),Value::Null,Value::Null,Value::Null,Value::Null,Value::Null,Value::Null] { return Err(StorageError::state("root event differs from canonical bytes")); }
            if self.query_bound(ROOT_PROJECTION_CONFLICTS,vec![Value::Integer(*rid),Value::Text(root.clone())]).await? != vec![vec![Value::Integer(0);ROOT_PROJECTION_FLAGS]] {
                return Err(StorageError::state("root projection contradicts canonical bytes"));
            }
            let tags = self.query_bound("SELECT t.tagname,x.tagtype,x.srcid,x.origid,x.value,x.mtime FROM tagxref x LEFT JOIN tag t ON t.tagid=x.tagid WHERE x.rid=?1 ORDER BY t.tagname LIMIT 3",vec![Value::Integer(*rid)]).await?;
            let expected = vec![vec![Value::Text("branch".into()),Value::Integer(2),Value::Integer(*rid),Value::Integer(*rid),Value::Text("trunk".into()),stored_date.clone()],vec![Value::Text("sym-trunk".into()),Value::Integer(2),Value::Integer(*rid),Value::Integer(*rid),Value::Null,stored_date]];
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
    use crate::{compression, content_hash};
    /// Allocation observations belong only to the current test thread and never alter production allocation.
    mod allocation_probe {
        use std::{
            alloc::{GlobalAlloc, Layout, System},
            cell::Cell,
        };
        thread_local! {
            static CAPTURE: Cell<bool> = const { Cell::new(false) };
            static LARGEST: Cell<usize> = const { Cell::new(0) };
        }
        struct ObservedSystem;
        fn observe(size: usize) {
            let _ = CAPTURE.try_with(|capture| {
                if capture.get() {
                    let _ = LARGEST.try_with(|largest| largest.set(largest.get().max(size)));
                }
            });
        }
        // SAFETY: all requests and their original layouts are forwarded unchanged to System.
        unsafe impl GlobalAlloc for ObservedSystem {
            unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
                observe(layout.size());
                // SAFETY: the caller supplies GlobalAlloc's valid layout contract.
                unsafe { System.alloc(layout) }
            }
            unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
                observe(layout.size());
                // SAFETY: the caller supplies GlobalAlloc's valid layout contract.
                unsafe { System.alloc_zeroed(layout) }
            }
            unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
                // SAFETY: this is the original pointer/layout from the forwarding allocator.
                unsafe { System.dealloc(pointer, layout) }
            }
            unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
                observe(new_size);
                // SAFETY: the caller supplies the original allocation and valid new size.
                unsafe { System.realloc(pointer, layout, new_size) }
            }
        }
        #[global_allocator]
        static ALLOCATOR: ObservedSystem = ObservedSystem;
        pub(super) fn measure<T>(operation: impl FnOnce() -> T) -> (T, usize) {
            LARGEST.with(|largest| largest.set(0));
            CAPTURE.with(|capture| capture.set(true));
            let result = operation();
            CAPTURE.with(|capture| capture.set(false));
            (result, LARGEST.with(Cell::get))
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
}
