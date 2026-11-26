//! Persist explicit immutable scope choices for already tracked canonical paths.
//!
//! Missing rows mean unselected. Scope confers no recording or sync authority.
//!
//! ## Recommendations
//! Persist explicit choices only after tracking. Next, require complete private
//! coverage in the recording coordinator before it captures working content.
use super::tracking::{TrackingLimits, checked_scope_path};
use super::{AdmissionLimits, CheckoutView, LocalOwnerRepository, StorageError, Value};
use crate::storage::{WriteTransaction, content::Budget};
use std::collections::BTreeSet;

/// Choose an immutable scope for a path, independently of content classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathScope {
    /// Explicit shared selection; does not publish or authorize synchronization.
    Shared,
    /// Explicit private selection; does not by itself authorize recording.
    Private,
}
impl PathScope {
    /// Encode the exact persisted scope spelling.
    /// # Returns
    /// The schema's canonical lowercase token.
    fn token(self) -> &'static str {
        match self {
            Self::Shared => "shared",
            Self::Private => "private",
        }
    }
}

/// Select an explicit scope for one exact tracked relative pathname.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathScopeSelection {
    /// Exact canonical relative pathname already present in the tracked inventory.
    pub path: String,
    /// Immutable choice; absence of a selection never supplies this value.
    pub scope: PathScope,
}

impl LocalOwnerRepository {
    /// Atomically select immutable scopes without capturing or changing working files.
    /// Equal existing choices are checked no-ops. New choices advance the pending
    /// counter once; a conflicting choice refuses the entire batch.
    /// # Arguments
    /// * `checkout` - Exact admitted checkout identity.
    /// * `wanted` - Expected complete seven-field checkout view.
    /// * `selections` - Nonempty bounded explicit batch for already tracked paths.
    /// * `limits` - Complete native admission ceilings.
    /// * `tracking` - Complete inventory and canonical pathname ceilings.
    /// # Returns
    /// The committed view, unchanged for an entirely equal batch.
    /// # Errors
    /// Refuses a changed owner, stale expectations, invalid limits, aliases, unknown
    /// paths, conflicting choices, corrupt inventories, counter overflow and SQL failures.
    /// # Examples
    /// ```no_run
    /// use gossil::storage::{AdmissionLimits, CheckoutView, LocalOwnerRepository,
    ///     PathScope, PathScopeSelection, StorageError, TrackingLimits};
    /// async fn select(owner: &mut LocalOwnerRepository, view: &CheckoutView,
    ///     admission: AdmissionLimits, tracking: TrackingLimits)
    ///     -> Result<CheckoutView, StorageError> {
    ///     owner.select_path_scopes(&view.checkout_id, view,
    ///         &[PathScopeSelection { path: "notes.txt".into(), scope: PathScope::Private }],
    ///         admission, tracking).await
    /// }
    /// ```
    pub async fn select_path_scopes(
        &mut self,
        checkout: &str,
        wanted: &CheckoutView,
        selections: &[PathScopeSelection],
        limits: AdmissionLimits,
        tracking: TrackingLimits,
    ) -> Result<CheckoutView, StorageError> {
        if selections.is_empty() || selections.len() > tracking.paths {
            return Err(StorageError::state("scope batch count limit"));
        }
        // Ceilings bound each row, not the whole command.
        let (_, mut budget, _) = super::verification::proof_ledgers(limits)?;
        let mut names = BTreeSet::new();
        for selection in selections {
            checked_scope_path(&selection.path, tracking)?;
            budget.charge_decoded(
                selection
                    .path
                    .len()
                    .checked_add(128)
                    .ok_or_else(|| StorageError::state("scope input work overflow"))?,
            )?;
            if !names.insert(selection.path.to_ascii_lowercase()) {
                return Err(StorageError::state("duplicate or aliased scope path"));
            }
        }
        let mut tx = self.store.begin_write().await?;
        let result = async {
            tx.check_admission_budgeted(&self.principal, &mut budget).await?;
            tx.require_native_checkout().await?;
            tx.check_recording_state(&self.principal, limits, &mut budget).await?;
            // A table that must stay empty, such as a pending remove, changes what this write means.
            tx.require_recording_empty_tables().await?;
            let before = tx.read_checkout_view_budgeted(&self.principal, checkout, &mut budget).await?;
            if &before != wanted { return Err(StorageError::state("scope checkout changed")); }
            tx.tracking_protected_paths()?;
            let tracked = tx.read_scope_tracked_paths(checkout, tracking, &mut budget).await?;
            let existing = tx.read_path_scopes_budgeted(checkout, tracking, &mut budget).await?;
            budget.charge_decoded(selections.len().checked_mul(std::mem::size_of::<&PathScopeSelection>()).ok_or_else(||StorageError::state("scope batch work overflow"))?)?;
            let mut new = Vec::new();
            new.try_reserve_exact(selections.len()).map_err(|e| StorageError::caused("scope batch allocation", e))?;
            let tracked: BTreeSet<&str> = tracked.iter().map(String::as_str).collect();
            let existing: std::collections::BTreeMap<&str, &PathScopeSelection> = existing.iter().map(|entry| (entry.path.as_str(), entry)).collect();
            // Validate the entire batch and counter before the first write.
            for selection in selections {
                if !tracked.contains(selection.path.as_str()) {
                    return Err(StorageError::state("scope path is not tracked"));
                }
                match existing.get(selection.path.as_str()) {
                    Some(entry) if entry.scope != selection.scope => return Err(StorageError::state("scope selection is immutable")),
                    Some(_) => {},
                    None => new.push(selection),
                }
            }
            if !new.is_empty() {
                let pending = before.pending_version.checked_add(1).and_then(|value| i64::try_from(value).ok()).ok_or_else(|| StorageError::state("scope pending counter exhausted"))?;
                let generation = i64::try_from(before.generation).map_err(|_| StorageError::state("scope generation exhausted"))?;
                let old_pending = i64::try_from(before.pending_version).map_err(|_| StorageError::state("scope pending counter exhausted"))?;
                for selection in new {
                    budget.charge_decoded(selection.path.len().checked_add(checkout.len()).and_then(|n|n.checked_add(256)).ok_or_else(||StorageError::state("scope SQL work overflow"))?)?;
                    let changed = tx.execute_bound("INSERT INTO g_path_scope(checkout_id,path,scope) VALUES(?1,?2,?3)", vec![Value::Text(checkout.into()), Value::Text(selection.path.clone()), Value::Text(selection.scope.token().into())]).await?;
                    if changed != 1 { return Err(StorageError::state("scope insert count differs")); }
                }
                require_scope_update(tx.execute_bound("UPDATE g_checkout SET pending_version=?1 WHERE checkout_id=?2 AND generation=?3 AND pending_version=?4 AND availability='ready'", vec![Value::Integer(pending), Value::Text(checkout.into()), Value::Integer(generation), Value::Integer(old_pending)]).await?)?;
            }
            tx.tracking_protected_paths()?;
            tx.check_recording_state(&self.principal, limits, &mut budget).await?;
            tx.read_path_scopes_budgeted(checkout, tracking, &mut budget).await?;
            tx.tracking_protected_paths()?;
            tx.read_checkout_view_budgeted(&self.principal, checkout, &mut budget).await
        }.await;
        let result = tx.record(result);
        tx.commit().await?;
        result
    }
}

impl WriteTransaction<'_> {
    /// Read the complete bounded scope inventory without inferring absent choices.
    /// Callers must authenticate the complete checkout/history family separately.
    /// # Arguments
    /// * `checkout` - Exact sole admitted checkout identity.
    /// * `limits` - Complete tracked inventory and canonical pathname bounds.
    /// * `budget` - Shared operation work accounting, charged before text materialization.
    /// # Returns
    /// Explicit selections in exact pathname order, possibly empty or mixed.
    /// # Errors
    /// Refuses foreign checkout rows, unknown paths, malformed types or tokens,
    /// nulls, invalid UTF-8, aliases, allocation failure and exhausted bounds.
    pub(super) async fn read_path_scopes_budgeted(
        &mut self,
        checkout: &str,
        limits: TrackingLimits,
        budget: &mut Budget,
    ) -> Result<Vec<PathScopeSelection>, StorageError> {
        let tracked = self
            .read_scope_tracked_paths(checkout, limits, budget)
            .await?;
        let invalid = self.query_bound("SELECT EXISTS(SELECT 1 FROM g_path_scope s WHERE typeof(s.checkout_id)<>'text' OR s.checkout_id IS NOT ?1 OR typeof(s.path)<>'text' OR typeof(s.scope)<>'text' OR s.scope NOT IN('shared','private') OR NOT EXISTS(SELECT 1 FROM g_vfile v WHERE v.checkout_id=s.checkout_id AND v.pathname=s.path))", vec![Value::Text(checkout.into())]).await?;
        if invalid != vec![vec![Value::Integer(0)]] {
            return Err(StorageError::state("scope inventory invalid"));
        }
        let counts = self.query_bound("SELECT count(*),coalesce(max(length(CAST(path AS BLOB))),0),coalesce(sum(length(CAST(path AS BLOB))),0) FROM g_path_scope", vec![]).await?;
        charge_scope_rows(&counts, tracked.len(), limits, budget)?;
        let rows = self
            .query_bound(
                "SELECT CAST(path AS BLOB),scope FROM g_path_scope ORDER BY path",
                vec![],
            )
            .await?;
        scopes_from_rows(rows, &tracked, limits)
    }

    /// Read all tracked path identities, including recorded and pending entries.
    /// # Arguments
    /// * `checkout` - Exact sole admitted checkout identity.
    /// * `limits` - Finite inventory and canonical path ceilings.
    /// * `budget` - Borrowed operation budget charged before provider text retention.
    /// # Returns
    /// Exact canonical paths in pathname order, independent of content RIDs.
    /// # Errors
    /// Refuses foreign identities, invalid types, paths, aliases, allocation and bounds.
    pub(super) async fn read_scope_tracked_paths(
        &mut self,
        checkout: &str,
        limits: TrackingLimits,
        budget: &mut Budget,
    ) -> Result<Vec<String>, StorageError> {
        // Validate zero limits even when the inventory is empty.
        if limits.paths == 0
            || limits.path_bytes == 0
            || limits.components == 0
            || limits.io_buffer_bytes == 0
        {
            return Err(StorageError::state("scope limits must be nonzero"));
        }
        let invalid = self.query_bound("SELECT EXISTS(SELECT 1 FROM g_vfile WHERE typeof(checkout_id)<>'text' OR checkout_id IS NOT ?1 OR typeof(pathname)<>'text')", vec![Value::Text(checkout.into())]).await?;
        if invalid != vec![vec![Value::Integer(0)]] {
            return Err(StorageError::state("scope tracked inventory invalid"));
        }
        let counts = self.query_bound("SELECT count(*),coalesce(max(length(CAST(pathname AS BLOB))),0),coalesce(sum(length(CAST(pathname AS BLOB))),0) FROM g_vfile", vec![]).await?;
        let total = scope_tracked_bytes(&counts, limits)?;
        budget.charge_decoded(total)?;
        let rows = self
            .query_bound(
                "SELECT CAST(pathname AS BLOB) FROM g_vfile ORDER BY pathname",
                vec![],
            )
            .await?;
        scope_tracked_from_rows(rows, limits)
    }
}

/// Require one guarded scope counter update before refreshing the proof.
/// # Arguments
/// * `changed` - Provider-reported affected row count.
/// # Returns
/// Success for exactly one checkout update.
/// # Errors
/// Refuses a missing or ambiguous checkout update.
fn require_scope_update(changed: u64) -> Result<(), StorageError> {
    if changed != 1 {
        return Err(StorageError::state("scope checkout changed"));
    }
    Ok(())
}

/// Validate scope counts and charge all scope text before fetching rows.
/// # Arguments
/// * `counts` - Fixed aggregate result for the complete scope inventory.
/// * `tracked` - Already validated complete tracked path count.
/// * `limits` - Finite path and inventory ceilings.
/// * `budget` - Borrowed operation work ledger.
/// # Returns
/// Success after charging the complete path and token text.
/// # Errors
/// Refuses malformed or excessive counts, overflow and exhausted work.
fn charge_scope_rows(
    counts: &[Vec<Value>],
    tracked: usize,
    limits: TrackingLimits,
    budget: &mut Budget,
) -> Result<(), StorageError> {
    let [row] = counts else {
        return Err(StorageError::state("scope inventory text limit"));
    };
    let [Value::Integer(n), Value::Integer(max), Value::Integer(sum)] = row.as_slice() else {
        return Err(StorageError::state("scope inventory text limit"));
    };
    if *n < 0
        || *n as u64 > limits.paths as u64
        || *n as u64 > tracked as u64
        || *max < 0
        || *max as u64 > limits.path_bytes as u64
        || *sum < 0
        || *sum as u64 > (limits.paths as u64).saturating_mul(limits.path_bytes as u64)
    {
        return Err(StorageError::state("scope inventory text limit"));
    }
    let bytes = (*n as u64)
        .checked_mul(7)
        .and_then(|tokens| tokens.checked_add(*sum as u64))
        .and_then(|total| usize::try_from(total).ok())
        .ok_or_else(|| StorageError::state("scope inventory accounting overflow"))?;
    budget.charge_decoded(bytes)
}

/// Decode the bounded complete scope result after ownership was charged.
/// # Arguments
/// * `rows` - Owned raw path and token rows from the fixed query.
/// * `tracked` - Complete validated tracked identities.
/// * `limits` - Canonical pathname limits.
/// # Returns
/// Exact validated scope choices in query order.
/// # Errors
/// Refuses malformed rows/text, unknown or duplicate paths and unsupported tokens.
fn scopes_from_rows(
    rows: Vec<Vec<Value>>,
    tracked: &[String],
    limits: TrackingLimits,
) -> Result<Vec<PathScopeSelection>, StorageError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(rows.len())
        .map_err(|e| StorageError::caused("scope inventory allocation", e))?;
    let mut names = BTreeSet::new();
    let tracked: BTreeSet<&str> = tracked.iter().map(String::as_str).collect();
    for row in rows {
        let [Value::Blob(bytes), Value::Text(token)] = row.as_slice() else {
            return Err(StorageError::state("scope inventory invalid"));
        };
        let path = std::str::from_utf8(bytes)
            .map_err(|e| StorageError::caused("scope pathname is not UTF-8", e))?;
        checked_scope_path(path, limits)?;
        if !tracked.contains(path) || !names.insert(path.to_ascii_lowercase()) {
            return Err(StorageError::state("scope inventory path differs"));
        }
        let scope = match token.as_str() {
            "shared" => PathScope::Shared,
            "private" => PathScope::Private,
            _ => return Err(StorageError::state("scope inventory invalid")),
        };
        result.push(PathScopeSelection {
            path: path.into(),
            scope,
        });
    }
    Ok(result)
}

/// Decode the scalar tracked inventory bounds before retaining its paths.
/// # Arguments
/// * `counts` - Complete fixed aggregate query result.
/// * `limits` - Finite inventory and pathname ceilings.
/// # Returns
/// Total retained path bytes to charge to the caller's ledger.
/// # Errors
/// Refuses malformed or excessive counts and byte conversion overflow.
fn scope_tracked_bytes(
    counts: &[Vec<Value>],
    limits: TrackingLimits,
) -> Result<usize, StorageError> {
    match counts {
        [row] => match row.as_slice() {
            [Value::Integer(n), Value::Integer(max), Value::Integer(sum)]
                if *n >= 0
                    && *n as u64 <= limits.paths as u64
                    && *max >= 0
                    && *max as u64 <= limits.path_bytes as u64
                    && *sum >= 0
                    && *sum as u64
                        <= (limits.paths as u64).saturating_mul(limits.path_bytes as u64) =>
            {
                usize::try_from(*sum)
                    .map_err(|e| StorageError::caused("scope tracked accounting overflow", e))
            }
            _ => Err(StorageError::state("scope tracked text limit")),
        },
        _ => Err(StorageError::state("scope tracked inventory invalid")),
    }
}

/// Decode all bounded tracked paths independently of their content RIDs.
/// # Arguments
/// * `rows` - Precharged raw pathname rows in query order.
/// * `limits` - Canonical path and component ceilings.
/// # Returns
/// Exact validated path identities.
/// # Errors
/// Refuses malformed rows/text, invalid paths and case or ancestor aliases.
fn scope_tracked_from_rows(
    rows: Vec<Vec<Value>>,
    limits: TrackingLimits,
) -> Result<Vec<String>, StorageError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(rows.len())
        .map_err(|e| StorageError::caused("scope tracked allocation", e))?;
    let mut names = BTreeSet::new();
    for row in rows {
        let [Value::Blob(bytes)] = row.as_slice() else {
            return Err(StorageError::state("scope tracked inventory invalid"));
        };
        let path = std::str::from_utf8(bytes)
            .map_err(|e| StorageError::caused("scope tracked pathname is not UTF-8", e))?;
        checked_scope_path(path, limits)?;
        let folded = path.to_ascii_lowercase();
        if super::tracking::folded_alias(&names, &folded) {
            return Err(StorageError::state("scope tracked aliases"));
        }
        names.insert(folded);
        result.push(path.to_owned());
    }
    Ok(result)
}

#[cfg(test)]
/// Check cumulative scope accounting and exact batch bounds.
mod budget_controls {
    use super::*;
    use crate::storage::{Limits, LocalStore};
    use std::path::Path;

    /// Supply finite content ceilings with a precise aggregate decoded boundary.
    /// # Arguments
    /// * `decoded` - Shared retained-work allowance in bytes.
    /// # Returns
    /// Complete nonzero finite content limits.
    fn budget_limits(decoded: u64) -> Limits {
        Limits {
            max_stored_bytes: 128,
            max_output_bytes: 128,
            max_chain_depth: 4,
            max_total_stored_bytes: 256,
            max_total_decoded_bytes: decoded,
        }
    }

    #[tokio::test]
    /// Verify exact reader charges and refusal with a partially consumed budget.
    /// # Panics
    /// Panics if either reader resets borrowed work or omits retained-byte charges.
    async fn readers_consume_the_same_borrowed_budget() {
        let mut store = LocalStore::open_inner(Path::new(":memory:"), false)
            .await
            .unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("CREATE TABLE g_vfile(checkout_id TEXT,pathname TEXT)")
            .await
            .unwrap();
        tx.execute("CREATE TABLE g_path_scope(checkout_id TEXT,path TEXT,scope TEXT)")
            .await
            .unwrap();
        tx.execute("INSERT INTO g_vfile VALUES('work','alpha'),('work','beta')")
            .await
            .unwrap();
        tx.execute("INSERT INTO g_path_scope VALUES('work','alpha','private')")
            .await
            .unwrap();
        let bounds = TrackingLimits {
            paths: 2,
            path_bytes: 16,
            components: 2,
            retained_bytes: 128,
            io_buffer_bytes: 16,
        };
        // All-tracked retention is 9 bytes; scope retention adds 5+7 bytes.
        let mut exact = Budget::new(budget_limits(21)).unwrap();
        assert_eq!(
            tx.read_path_scopes_budgeted("work", bounds, &mut exact)
                .await
                .unwrap(),
            vec![PathScopeSelection {
                path: "alpha".into(),
                scope: PathScope::Private
            }]
        );
        assert!(exact.charge_decoded(1).is_err());
        let mut consumed = Budget::new(budget_limits(21)).unwrap();
        consumed.charge_decoded(1).unwrap();
        assert!(
            tx.read_path_scopes_budgeted("work", bounds, &mut consumed)
                .await
                .is_err()
        );
        let mut cumulative = Budget::new(budget_limits(17)).unwrap();
        assert_eq!(
            tx.read_scope_tracked_paths("work", bounds, &mut cumulative)
                .await
                .unwrap(),
            vec!["alpha", "beta"]
        );
        assert!(
            tx.read_scope_tracked_paths("work", bounds, &mut cumulative)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
    }
    /// Scalar and raw decoders retain exact scope membership and alias checks.
    #[test]
    fn scope_decoders_reject_malformed_counts_rows_and_aliases() {
        let bounds = TrackingLimits {
            paths: 2,
            path_bytes: 16,
            components: 2,
            retained_bytes: 128,
            io_buffer_bytes: 16,
        };
        assert!(require_scope_update(1).is_ok());
        for changed in [0, 2] {
            assert_eq!(
                require_scope_update(changed).unwrap_err().operation,
                "scope checkout changed"
            );
        }
        for rows in [
            vec![],
            vec![vec![]],
            vec![vec![
                Value::Integer(3),
                Value::Integer(1),
                Value::Integer(3),
            ]],
            vec![vec![
                Value::Integer(1),
                Value::Integer(17),
                Value::Integer(17),
            ]],
        ] {
            assert_eq!(
                charge_scope_rows(
                    &rows,
                    2,
                    bounds,
                    &mut Budget::new(budget_limits(128)).unwrap()
                )
                .unwrap_err()
                .operation,
                "scope inventory text limit"
            );
        }
        for (n, max, sum, tracked) in [
            (-1, 0, 0, 2),
            (1, -1, 0, 2),
            (1, 1, -1, 2),
            (1, 1, 33, 2),
            (2, 1, 2, 1),
        ] {
            let rows = vec![vec![
                Value::Integer(n),
                Value::Integer(max),
                Value::Integer(sum),
            ]];
            assert_eq!(
                charge_scope_rows(
                    &rows,
                    tracked,
                    bounds,
                    &mut Budget::new(budget_limits(128)).unwrap()
                )
                .unwrap_err()
                .operation,
                "scope inventory text limit"
            );
        }
        let mut exact = Budget::new(budget_limits(12)).unwrap();
        charge_scope_rows(
            &[vec![
                Value::Integer(1),
                Value::Integer(5),
                Value::Integer(5),
            ]],
            1,
            bounds,
            &mut exact,
        )
        .unwrap();
        assert_eq!(
            exact.charge_decoded(1).unwrap_err().operation,
            "total decoded content limit"
        );
        assert_eq!(
            scope_tracked_bytes(&[], bounds).unwrap_err().operation,
            "scope tracked inventory invalid"
        );
        assert_eq!(
            scope_tracked_bytes(&[vec![Value::Null]], bounds)
                .unwrap_err()
                .operation,
            "scope tracked text limit"
        );
        let tracked = vec!["alpha".into(), "beta".into()];
        let row = |path: &str, scope: &str| {
            vec![
                Value::Blob(path.as_bytes().to_vec()),
                Value::Text(scope.into()),
            ]
        };
        assert_eq!(
            scopes_from_rows(
                vec![row("alpha", "private"), row("beta", "shared")],
                &tracked,
                bounds
            )
            .unwrap()
            .len(),
            2
        );
        for rows in [vec![vec![Value::Null]], vec![row("alpha", "unknown")]] {
            assert_eq!(
                scopes_from_rows(rows, &tracked, bounds)
                    .unwrap_err()
                    .operation,
                "scope inventory invalid"
            );
        }
        for rows in [
            vec![row("other", "private")],
            vec![row("alpha", "private"), row("alpha", "private")],
        ] {
            assert_eq!(
                scopes_from_rows(rows, &tracked, bounds)
                    .unwrap_err()
                    .operation,
                "scope inventory path differs"
            );
        }
        assert_eq!(
            scope_tracked_from_rows(vec![vec![Value::Null]], bounds)
                .unwrap_err()
                .operation,
            "scope tracked inventory invalid"
        );
        for paths in [
            ["alpha", "ALPHA"],
            ["alpha", "alpha/child"],
            ["alpha/child", "alpha"],
        ] {
            let rows = paths
                .into_iter()
                .map(|path| vec![Value::Blob(path.as_bytes().to_vec())])
                .collect();
            assert_eq!(
                scope_tracked_from_rows(rows, bounds).unwrap_err().operation,
                "scope tracked aliases"
            );
        }
    }

    /// The tracked reader refuses zero limits before SQL and foreign ownership before decoding.
    #[tokio::test]
    async fn scope_tracked_reader_checks_limits_and_complete_ownership() {
        let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let bounds = TrackingLimits {
            paths: 2,
            path_bytes: 16,
            components: 2,
            retained_bytes: 128,
            io_buffer_bytes: 16,
        };
        for index in 0..4 {
            let mut invalid = bounds;
            match index {
                0 => invalid.paths = 0,
                1 => invalid.path_bytes = 0,
                2 => invalid.components = 0,
                _ => invalid.io_buffer_bytes = 0,
            }
            assert_eq!(
                tx.read_scope_tracked_paths(
                    "work",
                    invalid,
                    &mut Budget::new(budget_limits(128)).unwrap()
                )
                .await
                .unwrap_err()
                .operation,
                "scope limits must be nonzero"
            );
        }
        tx.execute("CREATE TABLE g_vfile(checkout_id TEXT,pathname TEXT)")
            .await
            .unwrap();
        tx.execute("INSERT INTO g_vfile VALUES('foreign','alpha')")
            .await
            .unwrap();
        assert_eq!(
            tx.read_scope_tracked_paths(
                "work",
                bounds,
                &mut Budget::new(budget_limits(128)).unwrap()
            )
            .await
            .unwrap_err()
            .operation,
            "scope tracked inventory invalid"
        );
        tx.rollback().await.unwrap();
    }
}
