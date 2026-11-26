//! Native Fossil-style pending additions on the existing checkout substrate.
//!
//! Additions use the currently selected native check-in, including private history.
//! Recording rereads every tracked pathname, including edits made without another add.
//! Newly enrolled paths remain explicitly unselected. Absent `g_path_scope` rows convey
//! no shared/private classification. Recording and sync must require an explicit
//! persisted scope decision before treating these additions as commit-ready.
//!
//! ## Recommendations
//! Keep additions as native `g_vfile` rows with zero content RIDs. Next, qualify
//! reopen and atomic batch failure through the shared checkout admission path.
use super::{AdmissionLimits, CheckoutView, LocalOwnerRepository, Principal, StorageError, Value};
use crate::storage::WriteTransaction;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};
/// Capture bounded regular files through stable descriptor observations.
pub(super) mod capture;

/// Finite ceilings for a complete explicit tracking operation.
#[derive(Clone, Copy, Debug)]
pub struct TrackingLimits {
    /// Complete tracked inventory and requested batch ceiling.
    pub paths: usize,
    /// Maximum bytes per exact relative pathname.
    pub path_bytes: usize,
    /// Maximum number of explicit components per path.
    pub components: usize,
    /// Largest working file that one capture reads. A command captures one file at a
    /// time, and the final recheck reads no byte again.
    pub retained_bytes: u64,
    /// Maximum bytes in one descriptor read buffer.
    pub io_buffer_bytes: usize,
}

/// A native addition awaiting its first recording commit.
/// Its bytes remain live on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingAdd {
    /// Exact tracked relative pathname.
    pub path: String,
    /// Native executable flag observed when the path was first enrolled.
    pub executable: bool,
}

impl LocalOwnerRepository {
    /// Persist an explicit batch without changing working bytes or creating a check-in.
    ///
    /// Repeated calls retain earlier paths. A batch containing only tracked paths
    /// is a checked no-op, preserving native rows and pending version even after
    /// working edits. A batch introducing paths advances pending version once.
    /// No frozen staging snapshot or shared/private scope selection is created.
    /// # Examples
    /// ```no_run
    /// use gossil::storage::{AdmissionLimits, CheckoutView, LocalOwnerRepository, StorageError, TrackingLimits};
    /// async fn enroll(owner: &mut LocalOwnerRepository, view: &CheckoutView,
    ///     admission: AdmissionLimits, tracking: TrackingLimits) -> Result<CheckoutView, StorageError> {
    ///     owner.add_files(&view.checkout_id, &["notes/plan.md"], view, admission, tracking).await
    /// }
    /// ```
    /// # Arguments
    /// * `checkout` - Exact registered native checkout identity.
    /// * `paths` - Existing regular files, relative to its registered root.
    /// * `wanted` - Expected complete checkout snapshot from this owner.
    /// * `limits` - Complete native admission bounds.
    /// * `tracking` - Complete inventory, filesystem and capture bounds.
    /// # Returns
    /// The updated checkout view after the single transaction commits.
    /// # Errors
    /// Refuses stale views, unsupported state, invalid paths, aliases, nonregular
    /// files, changed descriptors, exhausted bounds and SQL failures atomically.
    pub async fn add_files(
        &mut self,
        checkout: &str,
        paths: &[&str],
        wanted: &CheckoutView,
        limits: AdmissionLimits,
        tracking: TrackingLimits,
    ) -> Result<CheckoutView, StorageError> {
        if paths.is_empty() || paths.len() > tracking.paths {
            return Err(StorageError::state("tracking batch count limit"));
        }
        // Ceilings bound each file and row, not the whole command.
        let (_, mut budget, _) = super::verification::proof_ledgers(limits)?;
        let mut unique = BTreeSet::new();
        let mut folded = BTreeSet::new();
        for path in paths {
            budget.charge_decoded(
                path.len()
                    .checked_add(128)
                    .ok_or_else(|| StorageError::state("tracking input work overflow"))?,
            )?;
            capture::checked_path(path, tracking)?;
            // A repeated path and two spellings that differ only in ASCII case are refused.
            if !unique.insert(*path) || !folded.insert(path.to_ascii_lowercase()) {
                return Err(StorageError::state("duplicate or aliased tracking path"));
            }
        }
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &mut budget).await?;
            tx.require_native_checkout().await?;
            tx.check_recording_state(&self.principal, limits, &mut budget).await?;
            // A table that must stay empty, such as a pending remove, changes what this write means.
            tx.require_recording_empty_tables().await?;
            let before = tx.read_checkout_view_budgeted(&self.principal,checkout,&mut budget).await?;
            if &before != wanted { return Err(StorageError::state("tracking checkout changed")); }
            let bindings = tx.query_bound("SELECT root_device,root_inode FROM g_checkout_owner WHERE checkout_id=?1",vec![Value::Text(checkout.into())]).await?;
            let [row] = bindings.as_slice() else {return Err(StorageError::state("tracking owner binding unavailable"));};
            let [Value::Text(device),Value::Text(inode)] = row.as_slice() else {return Err(StorageError::state("tracking owner binding invalid"));};
            let number = |value: &str| value.parse::<u64>().map_err(|e|StorageError::caused("tracking root identity invalid",e));
            let root = capture::open_root_budget(&before.registered_root,number(device)?,number(inode)?,&mut budget)?;
            // This private hook must use the store's retained native provider identity.
            let protected = tx.tracking_protected_paths()?;
            let protected: Vec<&Path> = protected.iter().map(PathBuf::as_path).collect();
            tx.charge_tracking_inventory(checkout, &mut budget).await?;
            let existing = tx.read_tracking_inventory(checkout,tracking,false).await?;
            let known: BTreeSet<&str> = existing.iter().map(|entry|entry.path.as_str()).collect();
            let folded: BTreeSet<String> = existing.iter().map(|entry|entry.path.to_ascii_lowercase()).collect();
            for path in paths {
                if !known.contains(path) && folded_alias(&folded, &path.to_ascii_lowercase()) {
                    return Err(StorageError::state("aliased tracked path"));
                }
            }
            let total = existing.len().checked_add(paths.iter().filter(|path|!known.contains(**path)).count()).ok_or_else(||StorageError::state("tracking inventory overflow"))?;
            if total>tracking.paths { return Err(StorageError::state("tracking inventory count limit")); }
            budget.charge_decoded(paths.len().checked_mul(std::mem::size_of::<(&str,capture::Captured)>()).ok_or_else(||StorageError::state("tracking capture work overflow"))?)?;
            let mut captures = Vec::new();
            captures.try_reserve_exact(paths.len()).map_err(|e|StorageError::caused("tracking capture allocation",e))?;
            // Each file is read once and its bytes dropped, so memory follows the largest file.
            for path in paths {
                let captured = capture::capture_budget(&root,path,&protected,tracking,tracking.retained_bytes,&mut budget)?;
                captures.push((*path,captured.mode,captured.mtime,captured.observed()));
            }
            let rid = tx.query_bound("SELECT rid FROM blob WHERE uuid=?1",vec![Value::Text(before.combined_head.as_ref().unwrap_or(&before.shared_head).clone())]).await?;
            let [row] = rid.as_slice() else {return Err(StorageError::state("tracking head unavailable"));};
            let [Value::Integer(rid)] = row.as_slice() else {return Err(StorageError::state("tracking head invalid"));};
            for (path,mode,mtime,_) in &captures {
                if known.contains(path) { continue; }
                budget.charge_decoded(path.len().checked_add(checkout.len()).and_then(|n|n.checked_add(512)).ok_or_else(||StorageError::state("tracking SQL work overflow"))?)?;
                // Fossil add.c add_one_file: vid=current checkout, rid=mrid=0,
                // chnged=deleted=islink=0. No content or event row is produced.
                tx.execute_bound("INSERT INTO g_vfile(checkout_id,id,vid,chnged,deleted,isexe,islink,rid,mrid,mtime,pathname,origname,mhash) SELECT ?1,coalesce(max(id),0)+1,?2,0,0,?3,0,0,0,?4,?5,NULL,NULL FROM g_vfile WHERE checkout_id=?1",vec![Value::Text(checkout.into()),Value::Integer(*rid),Value::Integer(*mode),Value::Integer(*mtime),Value::Text((*path).into())]).await?;
            }
            if total > existing.len() {
            let pending = before.pending_version.checked_add(1).ok_or_else(||StorageError::state("tracking pending counter exhausted"))?;
            require_pending_update(tx.execute_bound("UPDATE g_checkout SET pending_version=?1 WHERE checkout_id=?2 AND generation=?3 AND pending_version=?4 AND availability='ready'",vec![Value::Integer(i64::try_from(pending).map_err(|_|StorageError::state("tracking pending counter exhausted"))?),Value::Text(checkout.into()),Value::Integer(i64::try_from(before.generation).map_err(|_|StorageError::state("tracking generation exhausted"))?),Value::Integer(i64::try_from(before.pending_version).map_err(|_|StorageError::state("tracking pending counter exhausted"))?)]).await?)?;
            }
            for (path,_,_,seen) in &captures { capture::recheck_observed(&root,path,seen,&protected)?; }
            // Observe the retained provider again after all descriptor rechecks.
            // Its namespace/suffix vector is immutable; observe refuses any drift.
            tx.tracking_protected_paths()?;
            tx.check_recording_state(&self.principal, limits, &mut budget).await?;
            tx.read_checkout_view_budgeted(&self.principal,checkout,&mut budget).await
        }.await;
        let result = tx.record(result);
        tx.commit().await?;
        result
    }

    /// Read explicit durable scope choices without assigning absent paths a scope.
    /// # Arguments
    /// * `checkout` - Exact admitted checkout identity.
    /// * `limits` - Native admission ceilings of one artifact and one row.
    /// * `tracking` - Complete inventory and pathname ceilings.
    /// # Returns
    /// Explicit choices only; absent rows remain unselected.
    /// # Errors
    /// Refuses a changed owner, malformed state and exhausted work.
    /// # Examples
    /// ```no_run
    /// async fn inspect(owner: &mut gossil::storage::LocalOwnerRepository,
    ///     limits: gossil::storage::AdmissionLimits, tracking: gossil::storage::TrackingLimits) {
    ///     let explicit = owner.path_scopes("work", limits, tracking).await.unwrap();
    ///     for choice in explicit { println!("{}: {:?}", choice.path, choice.scope); }
    /// }
    /// ```
    pub async fn path_scopes(
        &mut self,
        checkout: &str,
        limits: AdmissionLimits,
        tracking: TrackingLimits,
    ) -> Result<Vec<super::path_scope::PathScopeSelection>, StorageError> {
        // Ceilings bound each row, not the whole command.
        let (_, mut budget, _) = super::verification::proof_ledgers(limits)?;
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &mut budget)
                .await?;
            tx.require_native_checkout().await?;
            tx.check_recording_state(&self.principal, limits, &mut budget)
                .await?;
            tx.read_checkout_view_budgeted(&self.principal, checkout, &mut budget)
                .await?;
            tx.read_path_scopes_budgeted(checkout, tracking, &mut budget)
                .await
        }
        .await;
        let result = tx.record(result);
        tx.commit().await?;
        result
    }

    /// List every admitted tracked path, including recorded files and pending additions.
    /// # Arguments
    /// * `checkout` - Exact admitted checkout identity.
    /// * `limits` - Native admission ceilings of one artifact and one row.
    /// * `tracking` - Complete inventory and pathname ceilings.
    /// # Returns
    /// Canonical tracked paths in exact pathname order, without inferred scopes.
    /// # Errors
    /// Refuses a changed owner, malformed history, invalid inventory and exhausted work.
    /// # Examples
    /// ```no_run
    /// async fn inspect(owner: &mut gossil::storage::LocalOwnerRepository,
    ///     limits: gossil::storage::AdmissionLimits, tracking: gossil::storage::TrackingLimits) {
    ///     let all = owner.tracked_paths("work", limits, tracking).await.unwrap();
    ///     for path in all { println!("{path}"); }
    /// }
    /// ```
    pub async fn tracked_paths(
        &mut self,
        checkout: &str,
        limits: AdmissionLimits,
        tracking: TrackingLimits,
    ) -> Result<Vec<String>, StorageError> {
        // Ceilings bound each row, not the whole command.
        let (_, mut budget, _) = super::verification::proof_ledgers(limits)?;
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &mut budget)
                .await?;
            tx.require_native_checkout().await?;
            tx.check_recording_state(&self.principal, limits, &mut budget)
                .await?;
            tx.read_checkout_view_budgeted(&self.principal, checkout, &mut budget)
                .await?;
            tx.read_scope_tracked_paths(checkout, tracking, &mut budget)
                .await
        }
        .await;
        let result = tx.record(result);
        tx.commit().await?;
        result
    }

    /// List the complete bounded native pending-add inventory in pathname order.
    /// # Examples
    /// ```no_run
    /// use gossil::storage::{AdmissionLimits, LocalOwnerRepository, PendingAdd, StorageError, TrackingLimits};
    /// async fn inventory(owner: &mut LocalOwnerRepository, checkout: &str,
    ///     admission: AdmissionLimits, tracking: TrackingLimits) -> Result<Vec<PendingAdd>, StorageError> {
    ///     owner.pending_adds(checkout, admission, tracking).await
    /// }
    /// ```
    /// # Arguments
    /// * `checkout` - Exact admitted registration identity.
    /// * `limits` - Complete native repository admission bounds.
    /// * `tracking` - Inventory and pathname ceilings.
    /// # Returns
    /// Native additions; the executable flag describes initial enrollment.
    /// # Errors
    /// Refuses a changed owner, unsupported native rows and exhausted bounds.
    pub async fn pending_adds(
        &mut self,
        checkout: &str,
        limits: AdmissionLimits,
        tracking: TrackingLimits,
    ) -> Result<Vec<PendingAdd>, StorageError> {
        // Ceilings bound each row, not the whole command.
        let (_, mut budget, _) = super::verification::proof_ledgers(limits)?;
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &mut budget)
                .await?;
            tx.require_native_checkout().await?;
            tx.check_recording_state(&self.principal, limits, &mut budget)
                .await?;
            tx.read_checkout_view_budgeted(&self.principal, checkout, &mut budget)
                .await?;
            tx.charge_tracking_inventory(checkout, &mut budget).await?;
            tx.read_pending_adds(checkout, tracking).await
        }
        .await;
        let result = tx.record(result);
        tx.commit().await?;
        result
    }
}

impl WriteTransaction<'_> {
    /// Charge complete inventory ownership before the all-tracked or pending reader.
    /// # Arguments
    /// * `checkout` - Exact admitted identity.
    /// * `budget` - Shared operation ledger.
    /// # Returns
    /// Success after charging both provider rows and decoded entries.
    /// # Errors
    /// Refuses invalid scalar counts, overflow and exhausted work.
    async fn charge_tracking_inventory(
        &mut self,
        checkout: &str,
        budget: &mut crate::storage::content::Budget,
    ) -> Result<(), StorageError> {
        budget.charge_decoded(
            checkout
                .len()
                .checked_add(256)
                .ok_or_else(|| StorageError::state("tracking work overflow"))?,
        )?;
        let rows = self.query_bound("SELECT count(*),coalesce(sum(length(CAST(pathname AS BLOB))),0) FROM g_vfile WHERE checkout_id=?1", vec![Value::Text(checkout.into())]).await?;
        charge_tracking_rows(&rows, budget)
    }

    /// Obtain protected names from the retained resolved store namespace only.
    /// The private accessor rechecks both selected identity and observed namespace;
    /// these are cooperative observations, not atomic SQLite-handle binding.
    /// # Returns
    /// Retained resolved database and SQLite sidecar pathnames after namespace observation.
    /// # Errors
    /// Refuses an absent disk identity or a changed or unreadable provider namespace.
    pub(super) fn tracking_protected_paths(&self) -> Result<Vec<PathBuf>, StorageError> {
        self.store
            .identity
            .as_ref()
            .ok_or_else(|| StorageError::state("tracking requires disk provider"))?
            .protected_paths()
    }

    /// Decode a complete native inventory only after scalar and retained-text preflight.
    /// # Arguments
    /// * `checkout` - Exact registered checkout whose complete inventory is decoded.
    /// * `limits` - Finite inventory count, pathname and component bounds.
    /// # Returns
    /// Validated native pending additions ordered by exact pathname.
    /// # Errors
    /// Refuses invalid limits, SQL failures, malformed row types, invalid UTF-8, invalid paths and allocation or inventory bounds.
    async fn read_pending_adds(
        &mut self,
        checkout: &str,
        limits: TrackingLimits,
    ) -> Result<Vec<PendingAdd>, StorageError> {
        self.read_tracking_inventory(checkout, limits, true).await
    }

    /// Read all tracked paths for add membership or only native pending additions.
    /// # Arguments
    /// * `checkout` - Exact admitted checkout identity.
    /// * `limits` - Finite path and inventory bounds.
    /// * `pending_only` - Select zero-RID additions when true.
    /// # Returns
    /// Validated selected paths in canonical order.
    /// # Errors
    /// Refuses malformed rows, invalid paths, provider and allocation failures.
    async fn read_tracking_inventory(
        &mut self,
        checkout: &str,
        limits: TrackingLimits,
        pending_only: bool,
    ) -> Result<Vec<PendingAdd>, StorageError> {
        if limits.paths == 0
            || limits.path_bytes == 0
            || limits.components == 0
            || limits.io_buffer_bytes == 0
        {
            return Err(StorageError::state("tracking limits must be nonzero"));
        }
        // Bound and type-check persisted bytes before provider-owned materialization.
        // Native TEXT conversion can stop at NUL; BLOB conversion retains exact byte length.
        if self.query_bound("SELECT EXISTS(SELECT 1 FROM g_vfile WHERE checkout_id=?1 AND (typeof(pathname)<>'text' OR typeof(isexe)<>'integer' OR isexe NOT IN(0,1)))", vec![Value::Text(checkout.into())]).await? != vec![vec![Value::Integer(0)]] {
            return Err(StorageError::state("tracking inventory invalid"));
        }
        let counts=self.query_bound("SELECT count(*),coalesce(max(length(CAST(pathname AS BLOB))),0),coalesce(sum(length(CAST(pathname AS BLOB))),0) FROM g_vfile WHERE checkout_id=?1",vec![Value::Text(checkout.into())]).await?;
        if !matches!(counts.as_slice(),[row] if matches!(row.as_slice(),[Value::Integer(n),Value::Integer(max),Value::Integer(sum)] if *n>=0 && *n as u64<=limits.paths as u64 && *max>=0 && *max as u64<=limits.path_bytes as u64 && *sum>=0 && *sum as u64 <= (limits.paths as u64).saturating_mul(limits.path_bytes as u64)))
        {
            return Err(StorageError::state("tracking inventory text limit"));
        }
        let rows = self
            .query_bound(
                "SELECT CAST(pathname AS BLOB),isexe FROM g_vfile WHERE checkout_id=?1 AND (?2=0 OR (rid=0 AND mrid=0)) ORDER BY pathname",
                vec![Value::Text(checkout.into()), Value::Integer(i64::from(pending_only))],
            )
            .await?;
        let mut result = Vec::new();
        result
            .try_reserve_exact(rows.len())
            .map_err(|e| StorageError::caused("tracking inventory allocation", e))?;
        for row in rows {
            result.push(pending_add_from_row(&row, limits)?);
        }
        Ok(result)
    }

    /// Admit exactly this producer's initial-root native additions on every reopen.
    /// Existing complete checkout admission retains root, owner and native key checks.
    /// # Arguments
    /// * `_principal` - Admitted owner context; unused here because the caller validates authority.
    /// * `checkout` - Exact registered identity required in every native tracking row.
    /// * `rid` - Actual initial root RID required as each row's vid.
    /// * `pending` - Persisted pending version checked against inventory emptiness.
    /// * `limits` - Admission bounds used to constrain the complete stored inventory.
    /// # Returns
    /// Success when all rows, counters, contiguous IDs and nonaliasing paths match this producer.
    /// # Errors
    /// Refuses SQL failures, unsupported native row shapes, invalid inventory, counters or row identities, aliases and exhausted bounds.
    pub(super) async fn validate_added_files(
        &mut self,
        _principal: &Principal,
        checkout: &str,
        rid: i64,
        pending: i64,
        limits: AdmissionLimits,
    ) -> Result<(), StorageError> {
        let bounds = TrackingLimits {
            paths: limits.parser.entries,
            path_bytes: limits.parser.input_bytes,
            components: limits.parser.entries,
            retained_bytes: limits.content.max_total_decoded_bytes,
            io_buffer_bytes: 4096,
        };
        let invalid=self.query_bound("SELECT EXISTS(SELECT 1 FROM g_vfile WHERE typeof(checkout_id)<>'text' OR checkout_id IS NOT ?1 OR typeof(id)<>'integer' OR id<=0 OR typeof(vid)<>'integer' OR vid<>?2 OR typeof(chnged)<>'integer' OR chnged<>0 OR typeof(deleted)<>'integer' OR deleted<>0 OR typeof(isexe)<>'integer' OR isexe NOT IN(0,1) OR typeof(islink)<>'integer' OR islink<>0 OR typeof(rid)<>'integer' OR rid<>0 OR typeof(mrid)<>'integer' OR mrid<>0 OR typeof(mtime)<>'integer' OR typeof(pathname)<>'text' OR origname IS NOT NULL OR mhash IS NOT NULL)",vec![Value::Text(checkout.into()),Value::Integer(rid)]).await?;
        if invalid != vec![vec![Value::Integer(0)]] {
            return Err(StorageError::state("unsupported native tracked state"));
        }
        let entries = self.read_pending_adds(checkout, bounds).await?;
        if pending < 0
            || (entries.is_empty() && pending != 0)
            || (!entries.is_empty() && pending == 0)
        {
            return Err(StorageError::state("tracking pending version invalid"));
        }
        let ids=self.query_bound("SELECT count(*),coalesce(min(id),1),coalesce(max(id),0) FROM g_vfile WHERE checkout_id=?1",vec![Value::Text(checkout.into())]).await?;
        if !matches!(ids.as_slice(),[row] if matches!(row.as_slice(),[Value::Integer(count),Value::Integer(min),Value::Integer(max)] if *min==1 && *max==*count))
        {
            return Err(StorageError::state(
                "unsupported native tracking row identities",
            ));
        }
        let mut names = BTreeSet::new();
        for entry in entries {
            let name = entry.path.to_ascii_lowercase();
            if folded_alias(&names, &name) {
                return Err(StorageError::state("aliased native tracked path"));
            }
            names.insert(name);
        }
        Ok(())
    }
}

/// Find a path that aliases one already seen, in time that grows with its components.
///
/// Two paths alias when their ASCII-folded spellings are equal, or when one is a
/// directory of the other.
/// # Arguments
/// * `names` - The folded paths seen so far.
/// * `folded` - The folded path to check.
/// # Returns
/// Whether `folded` equals a seen path, is a directory of one, or lies below one.
pub(super) fn folded_alias(names: &BTreeSet<String>, folded: &str) -> bool {
    let below = format!("{folded}/");
    names.contains(folded)
        || names
            .range(below.clone()..)
            .next()
            .is_some_and(|next| next.starts_with(&below))
        || folded
            .match_indices('/')
            .any(|(end, _)| names.contains(&folded[..end]))
}

/// Decode and charge the complete tracking inventory before retaining its paths.
/// # Arguments
/// * `rows` - Fixed aggregate query result for the selected checkout.
/// * `budget` - Original borrowed operation ledger.
/// # Returns
/// Success after charging provider and decoded inventory ownership.
/// # Errors
/// Refuses missing/malformed scalars, negative values, overflow and exhausted work.
fn charge_tracking_rows(
    rows: &[Vec<Value>],
    budget: &mut crate::storage::content::Budget,
) -> Result<(), StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("tracking work sizing unavailable"));
    };
    let [Value::Integer(count), Value::Integer(bytes)] = row.as_slice() else {
        return Err(StorageError::state("tracking work sizing invalid"));
    };
    let count =
        usize::try_from(*count).map_err(|_| StorageError::state("tracking work size overflow"))?;
    let bytes =
        usize::try_from(*bytes).map_err(|_| StorageError::state("tracking work size overflow"))?;
    let charge = bytes
        .checked_mul(2)
        .and_then(|n| n.checked_add(count.checked_mul(512)?))
        .and_then(|n| n.checked_add(8192))
        .ok_or_else(|| StorageError::state("tracking work overflow"))?;
    budget.charge_decoded(charge)
}

/// Require one guarded checkout counter update.
/// # Arguments
/// * `changed` - Provider-reported affected row count.
/// # Returns
/// Success for exactly one updated checkout.
/// # Errors
/// Refuses zero or ambiguous updates without refreshing authority.
fn require_pending_update(changed: u64) -> Result<(), StorageError> {
    if changed != 1 {
        return Err(StorageError::state("tracking checkout changed"));
    }
    Ok(())
}

/// Decode one bounded exact raw pathname/provider mode result.
/// # Arguments
/// * `row` - Provider-owned row after complete scalar/text preflight.
/// * `limits` - Complete pathname and component ceilings.
/// # Returns
/// Owned validated pending-add facts.
/// # Errors
/// Refuses malformed types/shapes, invalid UTF-8, invalid paths and unsupported modes.
fn pending_add_from_row(row: &[Value], limits: TrackingLimits) -> Result<PendingAdd, StorageError> {
    let [Value::Blob(bytes), Value::Integer(mode)] = row else {
        return Err(StorageError::state("tracking inventory invalid"));
    };
    let path = std::str::from_utf8(bytes)
        .map_err(|error| StorageError::caused("tracking pathname is not UTF-8", error))?;
    capture::checked_path(path, limits)?;
    if !matches!(mode, 0 | 1) {
        return Err(StorageError::state("tracking mode invalid"));
    }
    Ok(PendingAdd {
        path: path.to_owned(),
        executable: *mode == 1,
    })
}

/// Apply the exact capture path policy to persisted scope membership.
/// # Arguments
/// * `path` - Canonical relative pathname.
/// * `limits` - Finite path and component bounds.
/// # Returns
/// Success for a supported path spelling.
/// # Errors
/// Refuses invalid limits, reserved paths and unsupported aliases.
pub(super) fn checked_scope_path(path: &str, limits: TrackingLimits) -> Result<(), StorageError> {
    capture::checked_path(path, limits)
}

#[cfg(test)]
mod boundary_controls {
    //! Check exact pending-row decoding and finite inventory admission.
    use super::*;
    /// Return finite descriptor and pathname bounds for direct provider controls.
    fn limits() -> TrackingLimits {
        TrackingLimits {
            paths: 4,
            path_bytes: 64,
            components: 4,
            retained_bytes: 64,
            io_buffer_bytes: 8,
        }
    }
    /// The alias check finds an equal path, a directory of a seen path and a path
    /// below one, and passes neighbours that only share a prefix.
    #[test]
    fn folded_alias_finds_equal_directory_and_below() {
        let names: BTreeSet<String> = ["a/b", "c"].map(String::from).into();
        for alias in ["a/b", "a", "c/d", "c/d/e"] {
            assert!(folded_alias(&names, alias), "{alias}");
        }
        for other in ["a.b", "a/bc", "a/c", "ab", "cd", "b"] {
            assert!(!folded_alias(&names, other), "{other}");
        }
    }
    /// The admission of pending additions refuses a row that aliases another by case
    /// or as a directory of it.
    /// # Panics
    /// Panics if fixture setup fails or an aliased row passes.
    #[tokio::test]
    async fn added_files_admission_refuses_aliased_rows() {
        use crate::storage::{InitializeOptions, Limits};
        for alias in ["FIRST", "first/child"] {
            let temp = tempfile::tempdir().unwrap();
            let base = std::fs::canonicalize(temp.path()).unwrap();
            let root = base.join("root");
            std::fs::create_dir(&root).unwrap();
            std::fs::write(root.join("first"), b"1").unwrap();
            let bounds = AdmissionLimits {
                content: Limits {
                    max_stored_bytes: 1 << 20,
                    max_output_bytes: 1 << 20,
                    max_chain_depth: 128,
                    max_total_stored_bytes: 64 << 20,
                    max_total_decoded_bytes: 64 << 20,
                },
                parser: crate::artifact::ParseLimits {
                    input_bytes: 1 << 20,
                    cards: 4096,
                    owned_bytes: 8 << 20,
                    entries: 4096,
                },
            };
            let mut owner = LocalOwnerRepository::initialize(
                &base.join("repo"),
                InitializeOptions {
                    repository_id: "0123456789abcdef0123456789abcdef01234567",
                    owner_id: "owner",
                    date: "2026-11-20T00:00:00",
                    user: b"Owner",
                    password: "secret",
                    sha1: false,
                    content_limits: bounds.content,
                    parse_limits: bounds.parser,
                },
            )
            .await
            .unwrap();
            let view = owner
                .register_checkout("work", &root, bounds)
                .await
                .unwrap();
            let view = owner
                .add_files("work", &["first"], &view, bounds, limits())
                .await
                .unwrap();
            let principal = &owner.principal;
            let mut tx = owner.store.begin_write().await.unwrap();
            tx.execute(&format!("INSERT INTO g_vfile SELECT checkout_id,2,vid,chnged,deleted,isexe,islink,rid,mrid,mtime,'{alias}',origname,mhash FROM g_vfile"))
                .await
                .unwrap();
            let pending = i64::try_from(view.pending_version).unwrap();
            assert_eq!(
                tx.validate_added_files(principal, "work", 1, pending, bounds)
                    .await
                    .unwrap_err()
                    .operation,
                "aliased native tracked path"
            );
            tx.rollback().await.unwrap();
        }
    }
    /// Refuse ambiguous update counts and malformed pending-row shapes or modes.
    /// # Panics
    /// Panics if a decoder accepts malformed input or returns the wrong error.
    #[test]
    fn pending_provider_decoders_are_exact() {
        assert!(require_pending_update(1).is_ok());
        for count in [0, 2] {
            assert_eq!(
                require_pending_update(count).unwrap_err().operation,
                "tracking checkout changed"
            );
        }
        for row in [vec![], vec![Value::Text("path".into()), Value::Integer(0)]] {
            assert_eq!(
                pending_add_from_row(&row, limits()).unwrap_err().operation,
                "tracking inventory invalid"
            );
        }
        assert_eq!(
            pending_add_from_row(
                &[Value::Blob(b"path".to_vec()), Value::Integer(2)],
                limits()
            )
            .unwrap_err()
            .operation,
            "tracking mode invalid"
        );
    }
    /// Refuse zero inventory bounds and invalid stored scalar types before decoding.
    /// # Panics
    /// Panics if fixture SQL fails or the reader accepts invalid limits or scalars.
    #[tokio::test]
    async fn private_reader_refuses_zero_limits_and_invalid_scalars() {
        let mut store = crate::storage::LocalStore::open_inner(Path::new(":memory:"), false)
            .await
            .unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let mut zero = limits();
        zero.paths = 0;
        assert_eq!(
            tx.read_pending_adds("work", zero)
                .await
                .unwrap_err()
                .operation,
            "tracking limits must be nonzero"
        );
        tx.execute("CREATE TABLE g_vfile(checkout_id TEXT,pathname TEXT,isexe INTEGER)")
            .await
            .unwrap();
        tx.execute("INSERT INTO g_vfile VALUES('work',NULL,0)")
            .await
            .unwrap();
        assert_eq!(
            tx.read_pending_adds("work", limits())
                .await
                .unwrap_err()
                .operation,
            "tracking inventory invalid"
        );
        tx.rollback().await.unwrap();
    }
    /// Inventory work remains cumulative, and malformed aggregates never become allocations.
    #[tokio::test]
    async fn tracking_inventory_charge_refuses_bad_shape_and_early_exhaustion() {
        use crate::storage::content::Budget;
        use crate::storage::{Limits, LocalStore};
        let limits = Limits {
            max_stored_bytes: 16,
            max_output_bytes: 16,
            max_chain_depth: 1,
            max_total_stored_bytes: 16,
            max_total_decoded_bytes: 8192,
        };
        for rows in [vec![], vec![vec![], vec![]]] {
            assert_eq!(
                charge_tracking_rows(&rows, &mut Budget::new(limits).unwrap())
                    .unwrap_err()
                    .operation,
                "tracking work sizing unavailable"
            );
        }
        assert_eq!(
            charge_tracking_rows(
                &[vec![Value::Null, Value::Integer(0)]],
                &mut Budget::new(limits).unwrap()
            )
            .unwrap_err()
            .operation,
            "tracking work sizing invalid"
        );
        for (count, bytes, expected) in [
            (-1, 0, "tracking work size overflow"),
            (0, -1, "tracking work size overflow"),
            (i64::MAX, i64::MAX, "tracking work overflow"),
        ] {
            assert_eq!(
                charge_tracking_rows(
                    &[vec![Value::Integer(count), Value::Integer(bytes)]],
                    &mut Budget::new(limits).unwrap()
                )
                .unwrap_err()
                .operation,
                expected
            );
        }
        let empty = vec![vec![Value::Integer(0), Value::Integer(0)]];
        let mut exact = Budget::new(limits).unwrap();
        charge_tracking_rows(&empty, &mut exact).unwrap();
        assert_eq!(
            charge_tracking_rows(&empty, &mut exact)
                .unwrap_err()
                .operation,
            "total decoded content limit"
        );
        let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let mut work = Budget::new(Limits {
            max_total_decoded_bytes: 259,
            ..limits
        })
        .unwrap();
        assert_eq!(
            tx.charge_tracking_inventory("work", &mut work)
                .await
                .unwrap_err()
                .operation,
            "total decoded content limit"
        );
        tx.rollback().await.unwrap();
    }
}
