//! Register one empty native checkout without selecting files or publishing a locator.
//!
//! ## Recommendations
//! Register an explicitly chosen existing directory, then reopen with fresh admission limits.
use super::{AdmissionLimits, LocalOwnerRepository, Principal, StorageError};
use crate::storage::WriteTransaction;
use libsql::Value;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Finite retained registration identity ceiling, further bounded by parser input bytes.
const ID_BYTES: usize = 1024;
/// Finite retained canonical root ceiling, further bounded by parser input bytes.
const ROOT_BYTES: usize = 4096;

/// Describe an authenticated empty-root registration from an admitted [`LocalOwnerRepository`].
///
/// Returned by [`LocalOwnerRepository::register_checkout`],
/// [`LocalOwnerRepository::checkout_view`] and [`LocalOwnerRepository::checkout_view_for_root`].
/// Fields describe the admitted snapshot; callers can mutate their owned copy without changing storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckoutView {
    /// Native project-code identity authenticated against this database's sealed owner.
    pub repository_id: String,
    /// Exact persisted registration identity.
    pub checkout_id: String,
    /// Canonical directory verified against its retained device and inode.
    pub registered_root: PathBuf,
    /// Actual authenticated public initial check-in name.
    pub shared_head: String,
    /// No private combined head is supported by this API.
    pub combined_head: Option<String>,
    /// Persisted generation, zero in the supported initial-root scope.
    pub generation: u64,
    /// Persisted pending version; advances once per batch enrolling new native paths.
    pub pending_version: u64,
}

/// Bound a registration ID before retaining it or passing it to the provider.
/// # Arguments
/// * `id` - Caller supplied or persisted registration identity.
/// * `limits` - Parser input bound, also applied to registration text.
/// # Returns
/// Success for a nonempty, NUL-free identity within both ceilings.
/// # Errors
/// Rejects empty, NUL-containing and oversized identities.
fn check_id(id: &str, limits: AdmissionLimits) -> Result<(), StorageError> {
    if id.is_empty() || id.contains('\0') || id.len() > ID_BYTES.min(limits.parser.input_bytes) {
        return Err(StorageError::state("checkout identity limit or spelling"));
    }
    Ok(())
}

/// Resolve caller aliases, then observe the canonical directory without following links.
/// # Arguments
/// * `root` - Explicit existing checkout directory, absolute or relative.
/// * `limits` - Parser input ceiling applied alongside the fixed root ceiling.
/// # Returns
/// Canonical UTF-8 directory and its observed metadata.
/// # Errors
/// Rejects missing or non-directory roots, invalid text, excessive paths and changed canonical components.
fn real_directory(
    root: &Path,
    limits: AdmissionLimits,
) -> Result<(PathBuf, std::fs::Metadata), StorageError> {
    let text = root
        .to_str()
        .ok_or_else(|| StorageError::state("checkout root is not UTF-8"))?;
    if text.is_empty()
        || text.contains('\0')
        || text.len() > ROOT_BYTES.min(limits.parser.input_bytes)
    {
        return Err(StorageError::state("checkout root path limit or spelling"));
    }
    let path = std::fs::canonicalize(root)
        .map_err(|e| StorageError::caused("checkout root unavailable", e))?;
    observe_canonical_directory(path, limits)
}

/// Observe an already resolved directory without following replacement links.
/// # Arguments
/// * `path` - Resolved namespace to observe; caller aliases must be canonicalized first.
/// * `limits` - Finite ceiling for the retained canonical spelling.
/// # Returns
/// The observed path and directory identity.
/// # Errors
/// Rejects symbolic-link components, invalid text, excessive spelling and unavailable directories.
fn observe_canonical_directory(
    path: PathBuf,
    limits: AdmissionLimits,
) -> Result<(PathBuf, std::fs::Metadata), StorageError> {
    for ancestor in path.ancestors() {
        let metadata = std::fs::symlink_metadata(ancestor)
            .map_err(|e| StorageError::caused("checkout root component unavailable", e))?;
        if metadata.file_type().is_symlink() {
            return Err(StorageError::state("checkout root symbolic link"));
        }
    }
    let text = path
        .to_str()
        .ok_or_else(|| StorageError::state("checkout root is not UTF-8"))?;
    if text.len() > ROOT_BYTES.min(limits.parser.input_bytes) {
        return Err(StorageError::state("checkout canonical root path limit"));
    }
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|e| StorageError::caused("checkout root metadata unavailable", e))?;
    if !metadata.is_dir() {
        return Err(StorageError::state("checkout root is not a directory"));
    }
    Ok((path, metadata))
}

impl LocalOwnerRepository {
    /// Atomically register exactly one existing directory at the native public initial root.
    /// Ordinary files within the directory are neither read nor tracked. This changes only the database.
    /// # Arguments
    /// * `checkout_id` - Explicit nonempty NUL-free identity, at most 1024 bytes and parser input bytes.
    /// * `root` - Existing UTF-8 directory, at most 4096 bytes and parser input bytes before and after canonicalization; caller aliases resolve to the persisted canonical root.
    /// * `limits` - Explicit finite native admission and registration text ceilings.
    /// # Returns
    /// Authenticated persisted registration after a checked commit and admission refresh.
    /// # Errors
    /// Refuses catalog1, occupied catalog4, stale authority, invalid roots, unsupported state,
    /// resource limits and provider failures. A failed operation never refreshes authority.
    /// # Examples
    /// ```no_run
    /// async fn register(owner: &mut gossil::storage::LocalOwnerRepository, root: &std::path::Path, limits: gossil::storage::AdmissionLimits) {
    ///     let view = owner.register_checkout("work", root, limits).await.unwrap();
    ///     assert_eq!(view.checkout_id, "work");
    /// }
    /// ```
    pub async fn register_checkout(
        &mut self,
        checkout_id: &str,
        root: &Path,
        limits: AdmissionLimits,
    ) -> Result<CheckoutView, StorageError> {
        check_id(checkout_id, limits)?;
        let (root, metadata) = real_directory(root, limits)?;
        let mut tx = self.store.begin_write().await?;
        let result =
            async {
                tx.check_admission(&self.principal, &self.admission).await?;
                tx.require_native_checkout().await?;
                tx.validate_initial_repository(&self.principal, limits)
                    .await?;
                if tx
                    .query_bound("SELECT count(*) FROM g_checkout", vec![])
                    .await?
                    != vec![vec![Value::Integer(0)]]
                {
                    return Err(StorageError::state(
                        "checkout already registered; only one registration supported",
                    ));
                }
                let heads = tx
                    .query_bound(
                        "SELECT supporting_root FROM g_public_receipt LIMIT 2",
                        vec![],
                    )
                    .await?;
                let head = initial_head(&heads)?;
                registration_boundary(8)?;
                tx.execute_bound(
                    "INSERT INTO g_checkout VALUES(?1,?2,?3,NULL,0,0,'ready')",
                    vec![
                        Value::Text(checkout_id.into()),
                        Value::Text(root.to_str().unwrap().into()),
                        Value::Text(head.clone()),
                    ],
                )
                .await?;
                registration_boundary(1)?;
                tx.execute_bound(
                    "INSERT INTO g_checkout_owner VALUES(?1,?2,?3,?4)",
                    vec![
                        Value::Text(checkout_id.into()),
                        Value::Text(self.principal.owner.clone()),
                        Value::Text(metadata.dev().to_string()),
                        Value::Text(metadata.ino().to_string()),
                    ],
                )
                .await?;
                registration_boundary(2)?;
                require_checkout_count_update(tx.execute_bound(
                "UPDATE g_storage SET checkout_count=1 WHERE singleton=1 AND checkout_count=0",
                vec![],
            ).await?)?;
                registration_boundary(3)?;
                let fingerprint = tx
                    .initial_checkout_fingerprint(limits.parser.input_bytes)
                    .await?;
                let ids = tx
                    .query_bound(
                        "SELECT rid FROM blob WHERE uuid=?1",
                        vec![Value::Text(head.clone())],
                    )
                    .await?;
                let rid = initial_rid(&ids)?;
                for (index, (name, value)) in [
                    ("checkout", rid.to_string()),
                    ("checkout-hash", head.clone()),
                    ("fingerprint", fingerprint),
                ]
                .into_iter()
                .enumerate()
                {
                    tx.execute_bound(
                        "INSERT INTO g_vvar VALUES(?1,?2,?3)",
                        vec![
                            Value::Text(checkout_id.into()),
                            Value::Text(name.into()),
                            Value::Text(value),
                        ],
                    )
                    .await?;
                    registration_boundary(4 + index)?;
                }
                tx.validate_initial_repository(&self.principal, limits)
                    .await?;
                let view = tx.read_checkout_view(&self.principal, checkout_id).await?;
                let admission = tx.capture_admission().await?;
                registration_boundary(7)?;
                Ok((view, admission))
            }
            .await;
        let result = tx.record(result);
        tx.commit().await?;
        let (view, admission) = result?;
        self.admission = admission;
        Ok(view)
    }

    /// Authenticate and read one exact existing native registration, without creating or repairing it.
    /// # Arguments
    /// * `checkout_id` - Exact existing identity within the registration text ceilings.
    /// * `limits` - Explicit finite complete root admission ceilings.
    /// # Returns
    /// An owned registration whose directory, owner, head and native keys passed admission.
    /// # Errors
    /// Rejects missing identities, catalog1, stale authority, changed directory identity,
    /// unsupported native/local state, resource limits and provider failures.
    /// # Examples
    /// ```no_run
    /// async fn view(owner: &mut gossil::storage::LocalOwnerRepository, limits: gossil::storage::AdmissionLimits) {
    ///     let view = owner.checkout_view("work", limits).await.unwrap();
    ///     assert_eq!(view.generation, 0);
    /// }
    /// ```
    pub async fn checkout_view(
        &mut self,
        checkout_id: &str,
        limits: AdmissionLimits,
    ) -> Result<CheckoutView, StorageError> {
        check_id(checkout_id, limits)?;
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission(&self.principal, &self.admission).await?;
            tx.require_native_checkout().await?;
            tx.validate_initial_repository(&self.principal, limits)
                .await?;
            tx.read_checkout_view(&self.principal, checkout_id).await
        }
        .await;
        let result = tx.record(result);
        tx.commit().await?;
        result
    }

    /// Authenticate the sole existing native registration for an explicitly chosen root.
    /// This supplies authenticated facts for future locator reconstruction; it publishes no locator.
    /// It allocates no identities and changes no durable application state.
    /// # Arguments
    /// * `root` - Existing directory with the same canonical path and identity checks as [`Self::register_checkout`].
    /// * `limits` - Explicit finite complete native admission and registration text ceilings.
    /// # Returns
    /// The sole authenticated registration only when its persisted canonical root matches exactly.
    /// # Errors
    /// Rejects zero or multiple registrations, another root, unauthorized owner, stale admission,
    /// root replacement, unsupported state, invalid input, resource limits and provider failures.
    /// # Examples
    /// ```no_run
    /// async fn recover(owner: &mut gossil::storage::LocalOwnerRepository, root: &std::path::Path, limits: gossil::storage::AdmissionLimits) {
    ///     let view = owner.checkout_view_for_root(root, limits).await.unwrap();
    ///     assert_eq!(view.registered_root, std::fs::canonicalize(root).unwrap());
    /// }
    /// ```
    pub async fn checkout_view_for_root(
        &mut self,
        root: &Path,
        limits: AdmissionLimits,
    ) -> Result<CheckoutView, StorageError> {
        let (root, metadata) = real_directory(root, limits)?;
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission(&self.principal, &self.admission).await?;
            tx.require_native_checkout().await?;
            tx.validate_initial_repository(&self.principal, limits).await?;
            let rows = tx.query_bound("SELECT c.checkout_id FROM g_checkout c JOIN g_checkout_owner o ON o.checkout_id=c.checkout_id WHERE c.root=?1 AND o.root_device=?2 AND o.root_inode=?3", vec![Value::Text(root.to_str().unwrap().into()), Value::Text(metadata.dev().to_string()), Value::Text(metadata.ino().to_string())]).await?;
            let [row] = rows.as_slice() else { return Err(StorageError::state("checkout for root unavailable")); };
            let [Value::Text(id)] = row.as_slice() else { return Err(StorageError::state("invalid checkout for root")); };
            tx.read_checkout_view(&self.principal, id).await
        }.await;
        let result = tx.record(result);
        tx.commit().await?;
        result
    }
}

impl WriteTransaction<'_> {
    /// Require the sole catalog supported by the registration API.
    /// # Returns
    /// Success only for exact catalog4.
    /// # Errors
    /// Refuses every other catalog and provider failure.
    pub(super) async fn require_native_checkout(&mut self) -> Result<(), StorageError> {
        if self
            .query_bound(
                "SELECT storage_version FROM g_storage WHERE singleton=1",
                vec![],
            )
            .await?
            != vec![vec![Value::Integer(4)]]
        {
            return Err(StorageError::state("native checkout requires catalog4"));
        }
        Ok(())
    }

    /// Derive Fossil `db_fingerprint(0,1)` from this reservation's actual receive record.
    /// # Arguments
    /// * `max_bytes` - Bounds raw receipt fields and conservative SQL conversion output.
    /// # Returns
    /// Positive receive ID followed by the native MD5 spelling.
    /// # Errors
    /// Rejects excessive, missing or malformed receipts and unavailable conversions.
    async fn initial_checkout_fingerprint(
        &mut self,
        max_bytes: usize,
    ) -> Result<String, StorageError> {
        // Narrow extraction of migration::initial_fingerprint and fingerprint_receipt from the recorded donor.
        let sizes = self.query_bound("SELECT coalesce(length(CAST(uid AS BLOB)),0)+coalesce(length(CAST(mtime AS BLOB)),0)+coalesce(length(CAST(nonce AS BLOB)),0)+coalesce(length(CAST(ipaddr AS BLOB)),0) FROM rcvfrom ORDER BY rcvid DESC LIMIT 1", vec![]).await?;
        if !matches!(sizes.as_slice(), [row] if matches!(row.as_slice(), [Value::Integer(n)] if *n >= 0 && (*n as u64).checked_mul(2).and_then(|n| n.checked_add(128)).is_some_and(|n| n <= max_bytes as u64)))
        {
            return Err(StorageError::state("native fingerprint input limit"));
        }
        let rows = self.query_bound("SELECT rcvid,CAST(quote(uid) AS BLOB),CAST(datetime(mtime) AS BLOB),CAST(quote(nonce) AS BLOB),CAST(quote(ipaddr) AS BLOB) FROM rcvfrom ORDER BY rcvid DESC LIMIT 1", vec![]).await?;
        fingerprint_receipt(&rows)
    }

    /// Validate the complete zero-or-one initial registration inventory independently on reopen.
    /// # Arguments
    /// * `principal` - Sealed repository and owner identity.
    /// * `rid` - Authenticated public initial RID.
    /// * `head` - Authenticated public initial name.
    /// * `limits` - Root/parser bounds and registration text ceilings.
    /// # Returns
    /// Success for exact empty or single-registration native state.
    /// # Errors
    /// Refuses extra/missing rows, invalid text, root replacement, and noncanonical native keys.
    pub(super) async fn validate_initial_checkout(
        &mut self,
        principal: &Principal,
        rid: i64,
        head: &str,
        limits: AdmissionLimits,
    ) -> Result<(), StorageError> {
        let counts = self.query_bound("SELECT (SELECT count(*) FROM g_checkout),(SELECT count(*) FROM g_checkout_owner),(SELECT count(*) FROM g_vvar),checkout_count FROM g_storage WHERE singleton=1", vec![]).await?;
        if counts == vec![vec![Value::Integer(0); 4]] {
            if self
                .query_bound("SELECT EXISTS(SELECT 1 FROM g_vfile)", vec![])
                .await?
                != vec![vec![Value::Integer(0)]]
            {
                return Err(StorageError::state("unregistered native tracked inventory"));
            }
            return Ok(());
        }
        if counts
            != vec![vec![
                Value::Integer(1),
                Value::Integer(1),
                Value::Integer(3),
                Value::Integer(1),
            ]]
        {
            return Err(StorageError::state("unsupported native checkout inventory"));
        }
        // Refuse unsupported scalar types and values before retaining their raw provider values.
        let scalars = self.query_bound("SELECT typeof(combined_head)='null' AND typeof(generation)='integer' AND generation IS 0 AND typeof(pending_version)='integer' AND pending_version >= 0 FROM g_checkout", vec![]).await?;
        if scalars != vec![vec![Value::Integer(1)]] {
            return Err(StorageError::state(
                "unsupported initial checkout scalar state",
            ));
        }
        // Bound provider-retained text before selecting any persisted registration strings.
        let sizes = self.query_bound("SELECT length(CAST(c.checkout_id AS BLOB)),length(CAST(c.root AS BLOB)),length(CAST(c.shared_head AS BLOB)),length(CAST(c.availability AS BLOB)),length(CAST(o.owner_id AS BLOB)),length(CAST(o.root_device AS BLOB)),length(CAST(o.root_inode AS BLOB)) FROM g_checkout c JOIN g_checkout_owner o ON o.checkout_id=c.checkout_id", vec![]).await?;
        if !matches!(sizes.as_slice(), [row] if matches!(row.as_slice(), [Value::Integer(id),Value::Integer(root),Value::Integer(head),Value::Integer(availability),Value::Integer(owner),Value::Integer(device),Value::Integer(inode)] if *id>0 && *root>0 && *id as u64 <= ID_BYTES.min(limits.parser.input_bytes) as u64 && *root as u64 <= ROOT_BYTES.min(limits.parser.input_bytes) as u64 && (0..=64).contains(head) && (0..=5).contains(availability) && *owner>=0 && *owner as u64 <= principal.owner.len() as u64 && (1..=20).contains(device) && (1..=20).contains(inode)))
        {
            return Err(StorageError::state("persisted checkout text limit"));
        }
        let rows = self.query_bound("SELECT c.checkout_id,c.root,c.shared_head,c.combined_head,c.generation,c.pending_version,c.availability,o.owner_id,o.root_device,o.root_inode FROM g_checkout c JOIN g_checkout_owner o ON o.checkout_id=c.checkout_id", vec![]).await?;
        let InitialCheckoutRow {
            id,
            path,
            shared,
            availability,
            owner,
            device,
            inode,
            pending,
        } = initial_checkout_row(&rows)?;
        check_id(id, limits)?;
        let (canonical, metadata) = real_directory(Path::new(path), limits)?;
        if canonical.as_os_str() != Path::new(path).as_os_str()
            || shared != head
            || availability != "ready"
            || owner != &principal.owner
            || device != &metadata.dev().to_string()
            || inode != &metadata.ino().to_string()
        {
            return Err(StorageError::state(
                "registered checkout root or authority differs",
            ));
        }
        let fingerprint = self
            .initial_checkout_fingerprint(limits.parser.input_bytes)
            .await?;
        let exact = self.query_bound("SELECT count(*) FROM g_vvar WHERE checkout_id=?1 AND typeof(value)='text' AND ((name='checkout' AND value=?2) OR (name='checkout-hash' AND value=?3) OR (name='fingerprint' AND value=?4))", vec![Value::Text(id.to_owned()), Value::Text(rid.to_string()), Value::Text(head.into()), Value::Text(fingerprint)]).await?;
        if exact != vec![vec![Value::Integer(3)]] {
            return Err(StorageError::state("initial native checkout keys differ"));
        }
        self.validate_added_files(principal, id, rid, pending, limits)
            .await?;
        Ok(())
    }

    /// Decode a registration only after complete initial-root admission in this transaction.
    /// # Arguments
    /// * `principal` - Sealed repository identity for the returned view.
    /// * `id` - Exact registration to read, already text-bounded.
    /// # Returns
    /// Owned initial-root view for the exact ID.
    /// # Errors
    /// Rejects absent or malformed registrations and provider failures.
    pub(super) async fn read_checkout_view(
        &mut self,
        principal: &Principal,
        id: &str,
    ) -> Result<CheckoutView, StorageError> {
        let rows = self
            .query_bound(
                "SELECT root,shared_head,pending_version FROM g_checkout WHERE checkout_id=?1",
                vec![Value::Text(id.into())],
            )
            .await?;
        checkout_view_from_rows(principal, id, &rows)
    }
}

/// Decode the sole admitted initial head without cloning provider-owned text.
/// # Arguments
/// * `rows` - Initial receipt head query result.
/// # Returns
/// The borrowed exact initial head.
/// # Errors
/// Rejects missing or ambiguous results and non-text heads.
fn initial_head(rows: &[Vec<Value>]) -> Result<&String, StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("initial root unavailable"));
    };
    let [Value::Text(head)] = row.as_slice() else {
        return Err(StorageError::state("invalid initial root"));
    };
    Ok(head)
}

/// Decode the sole native RID without numeric coercion.
/// # Arguments
/// * `rows` - Native identity query result.
/// # Returns
/// The borrowed integer native RID.
/// # Errors
/// Rejects missing or ambiguous results and noninteger RIDs.
fn initial_rid(rows: &[Vec<Value>]) -> Result<&i64, StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("initial root RID unavailable"));
    };
    let [Value::Integer(rid)] = row.as_slice() else {
        return Err(StorageError::state("invalid initial root RID"));
    };
    Ok(rid)
}

/// Require the exact one-row durable checkout count transition.
/// # Arguments
/// * `changed` - Provider-reported rows affected by the guarded update.
/// # Returns
/// Success only for one changed row.
/// # Errors
/// Rejects absent or ambiguous updates.
fn require_checkout_count_update(changed: u64) -> Result<(), StorageError> {
    if changed != 1 {
        return Err(StorageError::state(
            "checkout registration count unavailable",
        ));
    }
    Ok(())
}

/// Hash the bounded native SQL receipt conversions without retaining copies.
/// # Arguments
/// * `rows` - Preflighted receive ID and four converted BLOB fields.
/// # Returns
/// The positive native witness and checksum spelling.
/// # Errors
/// Rejects malformed cardinality, witness, field count and conversions.
fn fingerprint_receipt(rows: &[Vec<Value>]) -> Result<String, StorageError> {
    let [row] = rows else {
        return Err(StorageError::state(
            "native fingerprint receipt unavailable",
        ));
    };
    let [Value::Integer(id), parts @ ..] = row.as_slice() else {
        return Err(StorageError::state("native fingerprint receipt invalid"));
    };
    if *id <= 0 || i32::try_from(*id).is_err() || parts.len() != 4 {
        return Err(StorageError::state("native fingerprint witness invalid"));
    }
    let mut checksum = crate::artifact_checksum::Md5Checksum::new();
    for value in parts {
        let Value::Blob(text) = value else {
            return Err(StorageError::state(
                "native fingerprint conversion unavailable",
            ));
        };
        checksum.update(text.split(|b| *b == 0).next().unwrap());
    }
    Ok(format!(
        "{id}/{}",
        crate::content_hash::hex(&checksum.finish())
    ))
}

/// Borrow the exact typed registration fields after scalar and size preflights.
struct InitialCheckoutRow<'a> {
    /// Nonnegative admitted pending additions generation.
    pending: i64,
    /// Persisted checkout identity.
    id: &'a String,
    /// Canonical registered path.
    path: &'a String,
    /// Native initial shared head.
    shared: &'a String,
    /// Current availability marker.
    availability: &'a String,
    /// Sealed local owner binding.
    owner: &'a String,
    /// Retained directory device identity.
    device: &'a String,
    /// Retained directory inode identity.
    inode: &'a String,
}

/// Decode a provider row only after the caller's scalar and retained-text preflights.
/// # Arguments
/// * `rows` - Bounded registration and owner join query result.
/// # Returns
/// Borrowed typed fields without cloning or allocating their contents.
/// # Errors
/// Rejects missing or ambiguous owners and unsupported field types or scalar values.
fn initial_checkout_row(rows: &[Vec<Value>]) -> Result<InitialCheckoutRow<'_>, StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("checkout owner binding unavailable"));
    };
    let [
        Value::Text(id),
        Value::Text(path),
        Value::Text(shared),
        Value::Null,
        Value::Integer(0),
        Value::Integer(pending),
        Value::Text(availability),
        Value::Text(owner),
        Value::Text(device),
        Value::Text(inode),
    ] = row.as_slice()
    else {
        return Err(StorageError::state("unsupported initial checkout state"));
    };
    if *pending < 0 {
        return Err(StorageError::state("unsupported initial checkout state"));
    }
    Ok(InitialCheckoutRow {
        pending: *pending,
        id,
        path,
        shared,
        availability,
        owner,
        device,
        inode,
    })
}

/// Construct the lean public snapshot from an already admitted bounded result.
/// # Arguments
/// * `principal` - Sealed repository identity.
/// * `id` - Explicit bounded checkout identity.
/// * `rows` - Admitted root/head query result.
/// # Returns
/// Owned initial-root view.
/// # Errors
/// Rejects missing or ambiguous registrations and malformed root/head fields.
fn checkout_view_from_rows(
    principal: &Principal,
    id: &str,
    rows: &[Vec<Value>],
) -> Result<CheckoutView, StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("checkout unavailable"));
    };
    let [
        Value::Text(root),
        Value::Text(head),
        Value::Integer(pending),
    ] = row.as_slice()
    else {
        return Err(StorageError::state("invalid checkout view"));
    };
    let pending_version =
        u64::try_from(*pending).map_err(|_| StorageError::state("invalid checkout view"))?;
    Ok(CheckoutView {
        repository_id: principal.repository.clone(),
        checkout_id: id.into(),
        registered_root: PathBuf::from(root),
        shared_head: head.clone(),
        combined_head: None,
        generation: 0,
        pending_version,
    })
}

/// Inject a failure at a registration write boundary in private source controls.
/// # Arguments
/// * `phase` - Before the first insert, after checkout/owner/count/each native key, or before commit.
/// # Returns
/// Success unless the private test selects this boundary.
/// # Errors
/// A selected test boundary returns a latched precommit failure.
fn registration_boundary(phase: usize) -> Result<(), StorageError> {
    #[cfg(test)]
    if injection::PHASE.with(|slot| slot.get() == phase) {
        return Err(StorageError::state(
            "injected registration boundary failure",
        ));
    }
    let _ = phase;
    Ok(())
}

#[cfg(test)]
/// Exercise registration rollback at every private precommit boundary.
mod injection {
    use super::*;
    thread_local! {
        /// Select one failure boundary for the current test thread; zero disables injection.
        pub(super) static PHASE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }
    /// Reset failpoints even if an assertion unwinds.
    struct Reset;
    impl Drop for Reset {
        /// Clear the thread-local failpoint when the test guard is destroyed.
        fn drop(&mut self) {
            PHASE.with(|slot| slot.set(0));
        }
    }
    #[tokio::test]
    /// Each registration failure rolls back; prior partial writes invalidate the old owner proof.
    /// # Panics
    /// Panics if fixture setup fails or an asserted rollback, admission or retry result differs.
    async fn every_registration_write_boundary_rolls_back() {
        use crate::artifact::ParseLimits;
        use crate::storage::{InitializeOptions, Limits};
        let _reset = Reset;
        let limits = AdmissionLimits {
            content: Limits {
                max_stored_bytes: 4096,
                max_output_bytes: 4096,
                max_chain_depth: 4,
                max_total_stored_bytes: 8192,
                max_total_decoded_bytes: 16384,
            },
            parser: ParseLimits {
                input_bytes: 4096,
                cards: 16,
                owned_bytes: 16384,
                entries: 64,
            },
        };
        for phase in 1..=8 {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("repo");
            let directory = temp.path().join("root");
            std::fs::create_dir(&directory).unwrap();
            let directory = std::fs::canonicalize(directory).unwrap();
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
            PHASE.with(|slot| slot.set(phase));
            let error = owner
                .register_checkout("work", &directory, limits)
                .await
                .unwrap_err();
            assert_eq!(error.operation, "injected registration boundary failure");
            assert!(error.cleanup.is_none());
            PHASE.with(|slot| slot.set(0));
            let stale = owner.checkout_view("work", limits).await.unwrap_err();
            if phase == 8 {
                assert_eq!(stale.operation, "checkout unavailable");
            } else {
                assert!(stale.operation.contains("admission stale"));
            }
            drop(owner);
            let mut owner = LocalOwnerRepository::open(&path, limits).await.unwrap();
            let mut tx = owner.store.begin_write().await.unwrap();
            assert_eq!(tx.query_bound("SELECT checkout_count,(SELECT count(*) FROM g_checkout),(SELECT count(*) FROM g_checkout_owner),(SELECT count(*) FROM g_vvar) FROM g_storage", vec![]).await.unwrap(), vec![vec![Value::Integer(0); 4]]);
            tx.commit().await.unwrap();
            drop(owner);
            let mut owner = LocalOwnerRepository::open(&path, limits).await.unwrap();
            owner
                .register_checkout("work", &directory, limits)
                .await
                .unwrap();
        }
    }
}

#[cfg(test)]
/// Test defensive provider decoders independently of the public admission gates before them.
mod defensive_controls {
    use super::*;
    use crate::artifact::ParseLimits;
    use crate::storage::{InitializeOptions, Limits};

    /// Supply finite bounds for direct private controls.
    fn limits() -> AdmissionLimits {
        AdmissionLimits {
            content: Limits {
                max_stored_bytes: 4096,
                max_output_bytes: 4096,
                max_chain_depth: 4,
                max_total_stored_bytes: 8192,
                max_total_decoded_bytes: 16384,
            },
            parser: ParseLimits {
                input_bytes: 4096,
                cards: 16,
                owned_bytes: 16384,
                entries: 64,
            },
        }
    }

    #[test]
    /// Decode valid head/RID results and reject malformed provider cardinalities and types.
    /// # Panics
    /// Panics if a decoder accepts malformed rows or changes its exact error class.
    fn initial_identity_decoders_preserve_exact_shapes() {
        let heads = vec![vec![Value::Text("head".into())]];
        assert_eq!(initial_head(&heads).unwrap(), "head");
        assert!(matches!(
            &heads[0][0],
            Value::Text(head) if std::ptr::eq(initial_head(&heads).unwrap(), head)
        ));
        for rows in [vec![], vec![vec![], vec![]]] {
            assert_eq!(
                initial_head(&rows).unwrap_err().operation,
                "initial root unavailable"
            );
            assert_eq!(
                initial_rid(&rows).unwrap_err().operation,
                "initial root RID unavailable"
            );
        }
        for row in [
            vec![],
            vec![Value::Null],
            vec![Value::Integer(1)],
            vec![Value::Text("head".into()), Value::Null],
        ] {
            assert_eq!(
                initial_head(&[row]).unwrap_err().operation,
                "invalid initial root"
            );
        }
        let ids = vec![vec![Value::Integer(1)]];
        assert_eq!(*initial_rid(&ids).unwrap(), 1);
        for row in [
            vec![],
            vec![Value::Null],
            vec![Value::Text("1".into())],
            vec![Value::Integer(1), Value::Null],
        ] {
            assert_eq!(
                initial_rid(&[row]).unwrap_err().operation,
                "invalid initial root RID"
            );
        }
    }

    #[test]
    /// Enforce the provider's exact single-row count transition without pretending public reachability.
    /// # Panics
    /// Panics if zero or multiple affected rows are accepted or one row is rejected.
    fn checkout_count_update_requires_exactly_one_row() {
        require_checkout_count_update(1).unwrap();
        for count in [0, 2, u64::MAX] {
            assert_eq!(
                require_checkout_count_update(count).unwrap_err().operation,
                "checkout registration count unavailable"
            );
        }
    }

    #[test]
    /// Validate native converted receipts, including positive witnesses and C-string checksum bytes.
    /// # Panics
    /// Panics if malformed provider facts are accepted or native checksum/error semantics change.
    fn fingerprint_decoder_rejects_malformed_provider_facts() {
        for rows in [vec![], vec![vec![], vec![]]] {
            assert_eq!(
                fingerprint_receipt(&rows).unwrap_err().operation,
                "native fingerprint receipt unavailable"
            );
        }
        for row in [vec![], vec![Value::Null], vec![Value::Text("1".into())]] {
            assert_eq!(
                fingerprint_receipt(&[row]).unwrap_err().operation,
                "native fingerprint receipt invalid"
            );
        }
        for id in [0, -1, i64::from(i32::MAX) + 1] {
            let mut row = vec![Value::Integer(id)];
            row.extend((0..4).map(|_| Value::Blob(vec![])));
            assert_eq!(
                fingerprint_receipt(&[row]).unwrap_err().operation,
                "native fingerprint witness invalid"
            );
        }
        assert_eq!(
            fingerprint_receipt(&[vec![Value::Integer(1)]])
                .unwrap_err()
                .operation,
            "native fingerprint witness invalid"
        );
        for invalid in [Value::Null, Value::Text("NULL".into()), Value::Integer(0)] {
            let row = vec![
                Value::Integer(1),
                Value::Blob(vec![]),
                invalid,
                Value::Blob(vec![]),
                Value::Blob(vec![]),
            ];
            assert_eq!(
                fingerprint_receipt(&[row]).unwrap_err().operation,
                "native fingerprint conversion unavailable"
            );
        }
        let mut row = vec![Value::Integer(1)];
        row.extend((0..4).map(|_| Value::Blob(vec![])));
        assert_eq!(
            fingerprint_receipt(&[row.clone()]).unwrap(),
            "1/d41d8cd98f00b204e9800998ecf8427e"
        );
        row[1] = Value::Blob(b"a\0ignored".to_vec());
        assert_eq!(
            fingerprint_receipt(&[row]).unwrap(),
            "1/0cc175b9c0f1b6a831c399e269772661"
        );
    }

    #[test]
    /// Observe canonical namespaces directly while preserving supplied-alias acceptance and expansion bounds.
    /// # Panics
    /// Panics if fixture setup fails or canonical identity, nofollow or path-ceiling behavior changes.
    fn canonical_observation_and_alias_expansion_are_distinct() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(temp.path()).unwrap();
        let directory = base.join("long-directory-".repeat(10));
        std::fs::create_dir(&directory).unwrap();
        let alias = base.join("alias");
        symlink(&directory, &alias).unwrap();
        assert_eq!(
            observe_canonical_directory(alias.clone(), limits())
                .unwrap_err()
                .operation,
            "checkout root symbolic link"
        );
        let (observed, metadata) =
            observe_canonical_directory(directory.clone(), limits()).unwrap();
        assert_eq!(observed, directory);
        assert_eq!(metadata.ino(), std::fs::metadata(&directory).unwrap().ino());
        assert_eq!(real_directory(&alias, limits()).unwrap().0, directory);
        let mut bounded = limits();
        bounded.parser.input_bytes = alias.to_str().unwrap().len() + 1;
        assert!(bounded.parser.input_bytes < directory.to_str().unwrap().len());
        assert_eq!(
            real_directory(&alias, bounded).unwrap_err().operation,
            "checkout canonical root path limit"
        );
        let missing = base.join("missing");
        assert_eq!(
            observe_canonical_directory(missing, limits())
                .unwrap_err()
                .operation,
            "checkout root component unavailable"
        );
    }

    #[tokio::test]
    /// Exercise real provider budget and invalid-datetime failures at the private fingerprint boundary.
    /// # Panics
    /// Panics if fixture setup, exact failure classes or rollback preservation differ.
    async fn fingerprint_query_bounds_and_conversions_are_checked() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let bounds = limits();
        let mut owner = LocalOwnerRepository::initialize(
            &path,
            InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                date: "2026-11-18 12:34:56.125",
                user: b"Owner",
                password: "secret",
                sha1: false,
                content_limits: bounds.content,
                parse_limits: bounds.parser,
            },
        )
        .await
        .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        assert_eq!(
            tx.initial_checkout_fingerprint(0)
                .await
                .unwrap_err()
                .operation,
            "native fingerprint input limit"
        );
        tx.rollback().await.unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        tx.execute_bound("UPDATE rcvfrom SET mtime='not-a-date'", vec![])
            .await
            .unwrap();
        assert_eq!(
            tx.initial_checkout_fingerprint(4096)
                .await
                .unwrap_err()
                .operation,
            "native fingerprint conversion unavailable"
        );
        tx.rollback().await.unwrap();
        drop(owner);
        assert!(LocalOwnerRepository::open(&path, bounds).await.is_ok());
    }

    #[test]
    /// Decode bounded owner rows by borrowing fields; reject malformed join cardinality or shape.
    /// # Panics
    /// Panics if a typed owner row changes or malformed provider values are accepted.
    fn checkout_owner_decoder_borrows_exact_initial_fields() {
        let valid = vec![
            Value::Text("work".into()),
            Value::Text("/root".into()),
            Value::Text("head".into()),
            Value::Null,
            Value::Integer(0),
            Value::Integer(0),
            Value::Text("ready".into()),
            Value::Text("owner".into()),
            Value::Text("1".into()),
            Value::Text("2".into()),
        ];
        let rows = vec![valid.clone()];
        let decoded = initial_checkout_row(&rows).unwrap();
        assert_eq!(
            (
                decoded.id.as_str(),
                decoded.path.as_str(),
                decoded.shared.as_str(),
                decoded.availability.as_str(),
                decoded.owner.as_str(),
                decoded.device.as_str(),
                decoded.inode.as_str()
            ),
            ("work", "/root", "head", "ready", "owner", "1", "2")
        );
        assert!(matches!(
            &rows[0][0],
            Value::Text(id) if std::ptr::eq(decoded.id, id)
        ));
        for malformed in [vec![], vec![valid.clone(), valid.clone()]] {
            assert_eq!(
                initial_checkout_row(&malformed).err().unwrap().operation,
                "checkout owner binding unavailable"
            );
        }
        let mut negative = valid.clone();
        negative[5] = Value::Integer(-1);
        assert_eq!(
            initial_checkout_row(&[negative]).err().unwrap().operation,
            "unsupported initial checkout state"
        );
        for index in 0..valid.len() {
            let mut row = valid.clone();
            row[index] = if index == 3 {
                Value::Text("unsupported".into())
            } else {
                Value::Null
            };
            assert_eq!(
                initial_checkout_row(&[row]).err().unwrap().operation,
                "unsupported initial checkout state"
            );
        }
    }

    #[test]
    /// Build a lean admitted snapshot and reject malformed view rows without SQL authority bypass.
    /// # Panics
    /// Panics if a valid view differs or malformed provider results are accepted.
    fn checkout_view_decoder_preserves_admitted_snapshot() {
        let principal = Principal {
            repository: "repository".into(),
            owner: "owner".into(),
        };
        let valid = vec![
            Value::Text("/root".into()),
            Value::Text("head".into()),
            Value::Integer(0),
        ];
        let view =
            checkout_view_from_rows(&principal, "work", std::slice::from_ref(&valid)).unwrap();
        assert_eq!(
            view,
            CheckoutView {
                repository_id: "repository".into(),
                checkout_id: "work".into(),
                registered_root: PathBuf::from("/root"),
                shared_head: "head".into(),
                combined_head: None,
                generation: 0,
                pending_version: 0
            }
        );
        for rows in [vec![], vec![valid.clone(), valid]] {
            assert_eq!(
                checkout_view_from_rows(&principal, "work", &rows)
                    .unwrap_err()
                    .operation,
                "checkout unavailable"
            );
        }
        let changed = checkout_view_from_rows(
            &principal,
            "work",
            &[vec![
                Value::Text("/root".into()),
                Value::Text("head".into()),
                Value::Integer(7),
            ]],
        )
        .unwrap();
        assert_eq!(changed.pending_version, 7);
        for row in [
            vec![
                Value::Text("/root".into()),
                Value::Text("head".into()),
                Value::Integer(-1),
            ],
            vec![
                Value::Text("/root".into()),
                Value::Text("head".into()),
                Value::Null,
            ],
            vec![
                Value::Text("/root".into()),
                Value::Text("head".into()),
                Value::Text("1".into()),
            ],
            vec![],
            vec![Value::Null, Value::Null],
            vec![Value::Text("/root".into()), Value::Integer(1)],
        ] {
            assert_eq!(
                checkout_view_from_rows(&principal, "work", &[row])
                    .unwrap_err()
                    .operation,
                "invalid checkout view"
            );
        }
    }
    /// Refuse tracking rows when no native checkout is registered.
    /// # Panics
    /// Panics if fixture SQL fails or orphan tracking rows pass checkout admission.
    #[tokio::test]
    async fn private_checkout_admission_refuses_orphan_native_inventory() {
        let mut store = crate::storage::LocalStore::open_inner(Path::new(":memory:"), false)
            .await
            .unwrap();
        let mut tx = store.begin_write().await.unwrap();
        for sql in [
            "CREATE TABLE g_checkout(checkout_id TEXT)",
            "CREATE TABLE g_checkout_owner(checkout_id TEXT)",
            "CREATE TABLE g_vvar(checkout_id TEXT)",
            "CREATE TABLE g_storage(singleton INTEGER,checkout_count INTEGER)",
            "INSERT INTO g_storage VALUES(1,0)",
            "CREATE TABLE g_vfile(checkout_id TEXT)",
            "INSERT INTO g_vfile VALUES('orphan')",
        ] {
            tx.execute(sql).await.unwrap();
        }
        let principal = Principal {
            repository: "repo".into(),
            owner: "owner".into(),
        };
        assert_eq!(
            tx.validate_initial_checkout(&principal, 1, "head", limits())
                .await
                .unwrap_err()
                .operation,
            "unregistered native tracked inventory"
        );
        tx.rollback().await.unwrap();
    }
}
