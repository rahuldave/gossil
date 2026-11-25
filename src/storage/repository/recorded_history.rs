//! Validate exact private history and native projections after admission of the immutable root.
//!
//! This validator grants no schema, owner, checkout-root or provider authority.
//! Its caller must authenticate those sealed facts and the unchanged public S.
//!
//! ## Recommendations
//! Admit this family only together with the immutable catalog6 boundary.
use super::issue_ticket::{AdmittedIssue, assign_tags};
use super::recording_manifest::{
    ManifestFile, ManifestParent, PrivateManifestSpec, PrivateParents, build_private_manifest,
};
use super::{AdmissionLimits, ParseWork, StorageError};
use crate::storage::{WriteTransaction, content::Budget, policy::Principal};
use libsql::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Describe an authenticated file in a complete tree, without retaining staged payload bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RecordedFile {
    /// Exact canonical relative path.
    pub(super) path: String,
    /// Verified native name.
    pub(super) hash: String,
    /// Actual existing full-content native identity.
    pub(super) rid: i64,
    /// Native regular/executable mode.
    pub(super) mode: i64,
}
/// Retain one verified full manifest in first-parent order.
pub(super) struct RecordedCommit {
    /// Actual native identity.
    pub(super) rid: i64,
    /// Verified native name.
    pub(super) hash: String,
    /// Original validated native date token.
    pub(super) date: Vec<u8>,
    /// Native Julian day from the selected SQL provider.
    pub(super) julian: f64,
    /// Manifest's unchanged operation receipt.
    pub(super) receipt: i64,
    /// Complete immutable file tree.
    pub(super) files: Vec<RecordedFile>,
}
/// Retain the authenticated history used by the private recorder.
pub(super) struct RecordedHistory {
    /// Authenticated empty public S, independently admitted by the caller.
    pub(super) shared: RecordedCommit,
    /// Private linear history, oldest first.
    pub(super) commits: Vec<RecordedCommit>,
    /// Exact branch derived from the authenticated replica server-code.
    pub(super) branch: String,
}

/// Describe a newly inserted private manifest and its complete captured file tree.
pub(super) struct ProjectionInput<'a> {
    /// Native identity of the newly inserted manifest.
    pub(super) rid: i64,
    /// Canonical full manifest bytes.
    pub(super) bytes: &'a [u8],
    /// Complete captured tree with actual native content identities.
    pub(super) files: &'a [RecordedFile],
}

impl RecordedHistory {
    /// Borrow the effective recorded complete tree.
    /// # Arguments
    /// No arguments; reads this privately authenticated history only.
    /// # Returns
    /// The selected L tree, or empty S tree before first recording.
    /// # Errors
    /// This immutable accessor cannot fail.
    pub(super) fn current_files(&self) -> &[RecordedFile] {
        self.commits.last().unwrap_or(&self.shared).files.as_slice()
    }
}

/// Decode one exact signed integer scalar without coercion.
/// # Arguments
/// * `rows` - Provider result expected to contain one integer cell.
/// # Returns
/// The exact integer value; callers check its required domain separately.
/// # Errors
/// Refuses absent, ambiguous, NULL, real, text and malformed rows.
pub(super) fn scalar_i64(rows: &[Vec<Value>]) -> Result<i64, StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("recording scalar cardinality"));
    };
    let [Value::Integer(value)] = row.as_slice() else {
        return Err(StorageError::state("recording scalar type"));
    };
    Ok(*value)
}

/// Charge exact retained SQL values after bounded scalar preflight.
/// # Arguments
/// * `values` - Complete finite values to retain or bind.
/// * `work` - Borrowed whole-operation ledger.
/// # Returns
/// Success after debiting scalar slots and exact string/blob lengths.
/// # Errors
/// Refuses arithmetic overflow and aggregate work exhaustion.
fn charge_values(values: &[Value], work: &mut Budget) -> Result<(), StorageError> {
    let mut size = values
        .len()
        .checked_mul(32)
        .ok_or_else(|| StorageError::state("recording SQL size overflow"))?;
    for value in values {
        size = size
            .checked_add(match value {
                Value::Text(s) => s.len(),
                Value::Blob(b) => b.len(),
                _ => 0,
            })
            .ok_or_else(|| StorageError::state("recording SQL size overflow"))?;
    }
    work.charge_decoded(size)
}

/// Read a finite SQL set after complete family text preflight.
/// # Arguments
/// * `tx` - Reserved transaction with the complete text preflight already performed.
/// * `sql` - Private fixed SELECT statement; no caller SQL is accepted.
/// * `args` - Exact bound values, charged before provider binding.
/// * `count` - Maximum complete result cardinality.
/// * `row_bytes` - Maximum sum of retained native values in one row.
/// * `work` - Borrowed cumulative operation ledger.
/// # Returns
/// Complete exact rows, never a truncated accepted set.
/// # Errors
/// Rejects invalid ceilings, excessive count or row width, budget and SQL failures.
/// Count and conservative result capacity are charged before provider materialization;
/// family preflight bounds every TEXT/BLOB field before this helper is called.
pub(super) async fn bounded_rows(
    tx: &mut WriteTransaction<'_>,
    sql: &str,
    args: Vec<Value>,
    count: usize,
    row_bytes: usize,
    work: &mut Budget,
) -> Result<Vec<Vec<Value>>, StorageError> {
    if count == 0 || row_bytes == 0 {
        return Err(StorageError::state("recording SQL limits"));
    }
    charge_values(&args, work)?;
    let sizing = format!("SELECT count(*) FROM ({sql})");
    work.charge_decoded(
        sizing
            .len()
            .checked_add(64)
            .ok_or_else(|| StorageError::state("recording SQL overflow"))?,
    )?;
    let n = scalar_i64(&tx.query_bound(&sizing, args.clone()).await?)?;
    if n < 0 || n as u64 > count as u64 {
        return Err(StorageError::state("recording SQL count limit"));
    }
    // Bound repeated provider retention BEFORE materialization using the complete
    // raw text inventory, rather than charging max_manifest_bytes for every tiny
    // scalar cell. BLOB payload reads belong to the content boundary. Each listed
    // variable-width field is also covered by enclosing family/root preflight.
    let text_sql = concat!(
        "SELECT coalesce(sum(n),0) FROM (",
        "SELECT coalesce(sum(length(CAST(uuid AS BLOB))),0) n FROM blob UNION ALL ",
        "SELECT coalesce(sum(length(CAST(type AS BLOB))+coalesce(length(CAST(user AS BLOB)),0)+coalesce(length(CAST(comment AS BLOB)),0)+coalesce(length(CAST(euser AS BLOB)),0)+coalesce(length(CAST(ecomment AS BLOB)),0)+coalesce(length(CAST(bgcolor AS BLOB)),0)+coalesce(length(CAST(brief AS BLOB)),0)),0) FROM event UNION ALL ",
        "SELECT coalesce(sum(length(CAST(name AS BLOB))),0) FROM filename UNION ALL ",
        "SELECT coalesce(sum(length(CAST(tagname AS BLOB))),0) FROM tag UNION ALL ",
        "SELECT coalesce(sum(length(CAST(value AS BLOB))),0) FROM tagxref UNION ALL ",
        "SELECT coalesce(sum(length(CAST(checkout_id AS BLOB))+length(CAST(pathname AS BLOB))),0) FROM g_vfile UNION ALL ",
        "SELECT coalesce(sum(length(CAST(checkout_id AS BLOB))+length(CAST(path AS BLOB))+length(CAST(scope AS BLOB))),0) FROM g_path_scope UNION ALL ",
        "SELECT coalesce(sum(coalesce(length(CAST(uid AS BLOB)),0)+coalesce(length(CAST(mtime AS BLOB)),0)+coalesce(length(CAST(nonce AS BLOB)),0)+coalesce(length(CAST(ipaddr AS BLOB)),0)),0) FROM rcvfrom UNION ALL ",
        "SELECT coalesce(sum(length(CAST(checkout_id AS BLOB))+length(CAST(name AS BLOB))+coalesce(length(CAST(value AS BLOB)),0)),0) FROM g_vvar UNION ALL ",
        "SELECT coalesce(sum(length(CAST(name AS BLOB))+coalesce(length(CAST(value AS BLOB)),0)),0) FROM config UNION ALL ",
        "SELECT coalesce(sum(length(CAST(checkout_id AS BLOB))+length(CAST(root AS BLOB))+length(CAST(shared_head AS BLOB))+coalesce(length(CAST(combined_head AS BLOB)),0)+length(CAST(availability AS BLOB))),0) FROM g_checkout UNION ALL ",
        "SELECT coalesce(sum(length(CAST(checkout_id AS BLOB))+length(CAST(owner_id AS BLOB))+length(CAST(root_device AS BLOB))+length(CAST(root_inode AS BLOB))),0) FROM g_checkout_owner)"
    );
    work.charge_decoded(text_sql.len() + 64)?;
    let text = scalar_i64(&tx.query_bound(text_sql, vec![]).await?)?;
    let text =
        usize::try_from(text).map_err(|_| StorageError::state("recording SQL text capacity"))?;
    let capacity = text
        .checked_mul(2)
        .and_then(|v| v.checked_add((n as usize).checked_mul(512)?))
        .and_then(|v| v.checked_add(128))
        .ok_or_else(|| StorageError::state("recording SQL allocation overflow"))?;
    work.charge_decoded(capacity)?;
    charge_values(&args, work)?;
    let rows = tx.query_bound(sql, args).await?;
    validate_retained_rows(&rows, n as usize, row_bytes)?;
    Ok(rows)
}

/// Read exactly one native integer through the same bounded SQL preflight.
/// # Arguments
/// * `tx` - Reserved transaction after complete family text preflight.
/// * `sql` - Private fixed SELECT requiring exactly one integer result.
/// * `args` - Exact bound values, charged by the bounded row reader.
/// * `work` - Borrowed whole-operation ledger.
/// # Returns
/// The exact integer from one row and one cell, without coercion.
/// # Errors
/// Refuses SQL, budget, row-count, width, cardinality and native type errors.
pub(super) async fn bounded_scalar(
    tx: &mut WriteTransaction<'_>,
    sql: &str,
    args: Vec<Value>,
    work: &mut Budget,
) -> Result<i64, StorageError> {
    let rows = bounded_rows(tx, sql, args, 1, 64, work).await?;
    scalar_i64(&rows)
}

/// Reserve the two retained manifest copies before parsing a root or private commit.
/// # Arguments
/// * `bytes` - Complete manifest bytes already bounded by content admission.
/// * `work` - Original operation ledger, including earlier content and SQL work.
/// # Returns
/// Success after debiting retained copies; parser work remains a separate debit.
/// # Errors
/// Refuses size overflow or exhausted cumulative decoded work.
fn charge_manifest_retention(bytes: &[u8], work: &mut Budget) -> Result<(), StorageError> {
    work.charge_decoded(
        bytes
            .len()
            .checked_mul(2)
            .ok_or_else(|| StorageError::state("recording parser charge overflow"))?,
    )
}

/// Require the immutable shared root to have neither files nor parents.
/// # Arguments
/// * `root` - Native parsed artifact; outer admission owns its identity and authority.
/// # Returns
/// Success for the empty-root shape used by this recording family.
/// # Errors
/// Refuses any file or parent; this predicate grants no repository authority.
fn require_empty_shared_root(root: &crate::artifact::ParsedArtifact) -> Result<(), StorageError> {
    if !root.files().is_empty() || !root.parents().is_empty() {
        return Err(StorageError::state("recording requires empty immutable S"));
    }
    Ok(())
}

/// Retain ordered borrowed names of newly added files after complete row validation.
/// # Arguments
/// * `rows` - Exact four-column tracked facts already checked by `tracked_facts`.
/// * `work` - Borrowed ledger charged for every row before retaining references.
/// # Returns
/// Names whose native content identity is zero, in the original row order.
/// # Errors
/// Refuses capacity overflow or exhausted work before building the collection.
fn collect_pending_references<'a>(
    rows: &'a [Vec<Value>],
    work: &mut Budget,
) -> Result<Vec<&'a String>, StorageError> {
    work.charge_decoded(
        rows.len()
            .checked_mul(16)
            .ok_or_else(|| StorageError::state("recording pending reference allocation"))?,
    )?;
    Ok(rows
        .iter()
        .filter_map(|row| match row.as_slice() {
            [_, Value::Text(path), Value::Integer(0), _] => Some(path),
            _ => None,
        })
        .collect())
}

/// Validate the complete retained result against its preflight count and width.
/// # Arguments
/// * `rows` - Already bounded provider result; this helper performs no allocation.
/// * `count` - Exact count observed before the caller charged retention capacity.
/// * `row_bytes` - Maximum total bytes in each retained row.
/// # Returns
/// Success for exact cardinality and bounded native values.
/// # Errors
/// Rejects changed cardinality, width overflow and over-wide rows.
fn validate_retained_rows(
    rows: &[Vec<Value>],
    count: usize,
    row_bytes: usize,
) -> Result<(), StorageError> {
    if rows.len() != count {
        return Err(StorageError::state("recording SQL changed"));
    }
    for row in rows {
        let mut width = 0usize;
        for v in row {
            width = width
                .checked_add(match v {
                    Value::Text(s) => s.len(),
                    Value::Blob(b) => b.len(),
                    _ => 8,
                })
                .ok_or_else(|| StorageError::state("recording SQL row overflow"))?;
        }
        if width > row_bytes {
            return Err(StorageError::state("recording SQL width limit"));
        }
    }
    Ok(())
}

/// Decode complete count, maximum-width and total-byte preflight facts.
/// # Arguments
/// * `rows` - Fixed aggregate query output, decoded without coercion.
/// * `limits` - The operation's existing parser and content ceilings.
/// # Returns
/// Exact nonnegative retained text size, before the caller debits its ledger.
/// # Errors
/// Rejects ambiguous rows, wrong types, negative facts and exceeded ceilings.
fn preflight_text_total(rows: &[Vec<Value>], limits: AdmissionLimits) -> Result<i64, StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("recording text preflight cardinality"));
    };
    let [
        Value::Integer(n),
        Value::Integer(width),
        Value::Integer(total),
    ] = row.as_slice()
    else {
        return Err(StorageError::state("recording text preflight type"));
    };
    if *n < 0
        || *width < 0
        || *total < 0
        || *n as u64 > limits.parser.entries as u64
        || *width as u64 > limits.parser.input_bytes as u64
        || *total as u64 > limits.content.max_total_decoded_bytes
    {
        return Err(StorageError::state("recording text preflight limit"));
    }
    Ok(*total)
}

/// Decode one complete full-content identity after the fixed filtered query.
/// # Arguments
/// * `rows` - Exact RID and receive-ID result; SQL filters remain the caller's duty.
/// # Returns
/// Positive native content and provenance identities.
/// # Errors
/// Rejects missing/ambiguous content, malformed row shape and nonpositive IDs.
fn content_identity(rows: &[Vec<Value>]) -> Result<(i64, i64), StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("recording full content unavailable"));
    };
    let [Value::Integer(rid), Value::Integer(receipt)] = row.as_slice() else {
        return Err(StorageError::state("recording content identity"));
    };
    if *rid <= 0 || *receipt <= 0 {
        return Err(StorageError::state("recording content identity"));
    }
    Ok((*rid, *receipt))
}

/// Borrow the exact canonical replica identity from one config result.
/// # Arguments
/// * `rows` - Complete server-code query result.
/// # Returns
/// Borrowed forty-byte lowercase hexadecimal identity without allocation.
/// # Errors
/// Rejects missing/ambiguous rows, nontext values and noncanonical grammar.
fn replica_identity(rows: &[Vec<Value>]) -> Result<&str, StorageError> {
    let [identity] = rows else {
        return Err(StorageError::state("recording replica missing"));
    };
    let [Value::Text(server)] = identity.as_slice() else {
        return Err(StorageError::state("recording replica type"));
    };
    if server.len() != 40
        || !server
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(StorageError::state("recording replica grammar"));
    }
    Ok(server)
}

/// Borrow decoded checkout facts from the same bounded provider result.
struct CheckoutFacts<'a> {
    /// Exact sole checkout identity.
    checkout_id: &'a String,
    /// Owned cursor matching the replay's existing ownership contract.
    cursor: Option<String>,
    /// Nonnegative private generation.
    generation: &'a i64,
    /// Nonnegative pending mutation counter.
    pending: &'a i64,
}

/// Decode the sole checkout's selected head and monotone counters.
/// # Arguments
/// * `rows` - Complete fixed four-column checkout query.
/// # Returns
/// Borrowed checkout/counters and the same owned optional head used by replay.
/// # Errors
/// Rejects absent/multiple rows, malformed types and negative counters.
fn checkout_facts(rows: &[Vec<Value>]) -> Result<CheckoutFacts<'_>, StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("recording sole checkout required"));
    };
    let [
        Value::Text(checkout_id),
        head,
        Value::Integer(generation),
        Value::Integer(pending),
    ] = row.as_slice()
    else {
        return Err(StorageError::state("recording checkout shape"));
    };
    if *generation < 0 || *pending < 0 {
        return Err(StorageError::state("recording counters"));
    }
    let cursor = match head {
        Value::Null => None,
        Value::Text(s) => Some(s.clone()),
        _ => return Err(StorageError::state("recording head type")),
    };
    Ok(CheckoutFacts {
        checkout_id,
        cursor,
        generation,
        pending,
    })
}

/// Check a payload's immutable receipt against its first or later reference.
/// # Arguments
/// * `seen` - Whether this native RID already belongs to the replayed prefix.
/// * `receipt` - Immutable receive ID retained on the payload.
/// * `commit_receipt` - Current manifest's independently checked receive ID.
/// # Returns
/// Success for first use in this receipt or reuse of an earlier receipt.
/// # Errors
/// Rejects first-use mismatch before the separate future-receipt defense.
fn validate_file_receipt(
    seen: bool,
    receipt: i64,
    commit_receipt: i64,
) -> Result<(), StorageError> {
    if !seen && receipt != commit_receipt {
        return Err(StorageError::state("recording first-use receipt mismatch"));
    }
    if receipt > commit_receipt {
        return Err(StorageError::state("recording file future receipt"));
    }
    Ok(())
}

/// Check and hash exact native receipt conversions without replacing the witness.
/// # Arguments
/// * `rows` - Complete four-column quote/date conversion query output.
/// * `receipt` - Previously checked exact witness used for this query.
/// * `work` - The caller's existing ledger, debited before hashing each value.
/// # Returns
/// Canonical witness and checksum with native first-NUL termination semantics.
/// # Errors
/// Rejects missing/ambiguous rows, wrong width, unavailable conversions and budget exhaustion.
fn fingerprint_from_conversion(
    rows: &[Vec<Value>],
    receipt: i64,
    work: &mut Budget,
) -> Result<String, StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("recording fingerprint receipt missing"));
    };
    if row.len() != 4 {
        return Err(StorageError::state("recording fingerprint receipt shape"));
    }
    let mut checksum = crate::artifact_checksum::Md5Checksum::new();
    for value in row {
        let Value::Blob(bytes) = value else {
            return Err(StorageError::state(
                "recording fingerprint conversion unavailable",
            ));
        };
        work.charge_decoded(
            bytes
                .len()
                .checked_mul(2)
                .ok_or_else(|| StorageError::state("recording parser charge overflow"))?,
        )?;
        checksum.update(bytes.split(|b| *b == 0).next().unwrap());
    }
    work.charge_decoded(64)?;
    Ok(format!(
        "{receipt}/{}",
        crate::content_hash::hex(&checksum.finish())
    ))
}
/// Validate the complete ordered receipt inventory after bounded retention.
/// # Arguments
/// * `receipts` - Full ordered receive rows, including native conversion validity.
/// * `root_receipt` - Independently authenticated immutable root receipt.
/// * `owners` - The receipt that each check-in and each issue owns, in any order.
/// # Returns
/// Success for the root witness and exactly one local-origin receipt per check-in or
/// issue, which follow the root receipt with no gaps.
/// # Errors
/// Rejects extra or missing receipts, a wrong root witness and invalid local origins.
fn validate_receipts(
    receipts: &[Vec<Value>],
    root_receipt: i64,
    owners: &[i64],
) -> Result<(), StorageError> {
    let mut owned = owners.to_vec();
    owned.sort_unstable();
    if receipts.len() != owned.len() + 1
        || owned
            .iter()
            .zip(root_receipt + 1..)
            .any(|(owner, expected)| *owner != expected)
    {
        return Err(StorageError::state("recording unexplained receipt"));
    }
    for (index, row) in receipts.iter().enumerate() {
        if index == 0 {
            if !matches!(row.first(),Some(Value::Integer(id)) if *id==root_receipt) {
                return Err(StorageError::state("recording root receipt"));
            }
            continue;
        }
        if !matches!(row.as_slice(),[Value::Integer(id),Value::Integer(0),Value::Text(kind),Value::Null,Value::Null,Value::Integer(1)] if *id==root_receipt+index as i64 && (kind=="real" || kind=="integer"))
        {
            return Err(StorageError::state("recording local receipt policy"));
        }
    }
    Ok(())
}

/// The complete replay rows of the native tables that check-ins and issues share.
///
/// [`WriteTransaction::expect_shared_tables`] adds the rows of each admitted issue,
/// so the check-in rows here are the only other rows that these tables may hold.
pub(super) struct SharedTables {
    /// The RID of the shared root, whose `event` and `tagxref` rows admission checks first.
    pub(super) shared_rid: i64,
    /// The receive record of the shared root.
    pub(super) shared_receipt: i64,
    /// Every `plink` row, in `cid,isprim DESC` order.
    pub(super) plink: Vec<Vec<Value>>,
    /// Every `mlink` row, in `mid,fnid,isaux` order.
    pub(super) mlink: Vec<Vec<Value>>,
    /// Each check-in `event` row with its `objid`.
    pub(super) events: Vec<(i64, Vec<Value>)>,
    /// Each check-in `tagxref` row with its `(rid, tagid)` key.
    pub(super) tagxref: Vec<((i64, i64), Vec<Value>)>,
    /// Every `tag` row above the built-in 12, in ID order.
    pub(super) tags: Vec<Vec<Value>>,
    /// Every `filename` row, in ID order.
    pub(super) filenames: Vec<Vec<Value>>,
    /// Every `leaf` row, in RID order.
    pub(super) leaves: Vec<Vec<Value>>,
    /// Each RID that `unsent` holds.
    pub(super) unsent: BTreeSet<i64>,
    /// The native name and receipt of each check-in blob, the shared root included.
    pub(super) referenced: BTreeMap<i64, (String, i64)>,
    /// The receipt that each check-in owns.
    pub(super) owners: Vec<i64>,
}

/// Borrow exact tracked scalar fields after the caller's SQL preflight.
struct TrackedFacts<'a> {
    /// Contiguous tracked inventory identity.
    id: &'a i64,
    /// Exact retained pathname.
    path: &'a String,
    /// Existing native content identity, or zero for a new path.
    rid: &'a i64,
    /// Native executable mode.
    mode: &'a i64,
}

/// Decode one fixed tracked row without coercion or allocation.
/// # Arguments
/// * `row` - ID, pathname, content identity and executable-mode values.
/// # Returns
/// Borrowed typed facts for the unchanged namespace and manifest checks.
/// # Errors
/// Rejects missing/extra columns and any wrong native type.
fn tracked_facts(row: &[Value]) -> Result<TrackedFacts<'_>, StorageError> {
    let [
        Value::Integer(id),
        Value::Text(path),
        Value::Integer(rid),
        Value::Integer(mode),
    ] = row
    else {
        return Err(StorageError::state("recording tracked row type"));
    };
    Ok(TrackedFacts {
        id,
        path,
        rid,
        mode,
    })
}

/// Hash opaque captured bytes with the initialized native SHA3 policy.
/// # Arguments
/// * `bytes` - Complete file or canonical manifest bytes.
/// * `work` - Borrowed cumulative ledger.
/// # Returns
/// Lowercase 64-byte native SHA3-256 name.
/// # Errors
/// Refuses work exhaustion or unavailable hashing.
pub(super) fn native_name(bytes: &[u8], work: &mut Budget) -> Result<String, StorageError> {
    work.charge_decoded(
        bytes
            .len()
            .checked_add(64)
            .ok_or_else(|| StorageError::state("recording hash overflow"))?,
    )?;
    let digest = crate::content_hash::sha3(bytes, 256)
        .ok_or_else(|| StorageError::state("recording native hash unavailable"))?;
    Ok(crate::content_hash::hex(&digest))
}

/// Parse native bytes while debiting both cumulative ledgers before allocation.
/// # Arguments
/// * `tx` - Existing reserved transaction, also the selected native date provider.
/// * `bytes` - Complete bounded manifest bytes.
/// * `parse` - Caller-owned remaining parser compartments, never reset here.
/// * `work` - Caller-owned operation budget covering parser retention and SQL dates.
/// # Returns
/// A validated owned artifact with actual usage subtracted from parser allowance.
/// # Errors
/// Refuses malformed content, exhausted aggregate compartments or provider failures.
/// The parser's per-call owned ceiling is additionally capped to a finite
/// conservative input-derived capacity, charged before it allocates any output.
pub(super) async fn parse_recording_budgeted(
    tx: &mut WriteTransaction<'_>,
    bytes: &[u8],
    parse: &mut ParseWork,
    work: &mut Budget,
) -> Result<crate::artifact::ParsedArtifact, StorageError> {
    let capacity = bytes
        .len()
        .checked_mul(64)
        .and_then(|n| n.checked_add(4096))
        .ok_or_else(|| StorageError::state("recording parser capacity overflow"))?;
    let mut remaining = parse.remaining;
    remaining.owned_bytes = remaining.owned_bytes.min(capacity);
    work.charge_decoded(
        bytes
            .len()
            .checked_add(remaining.owned_bytes)
            .and_then(|n| n.checked_add(256))
            .ok_or_else(|| StorageError::state("recording parser charge overflow"))?,
    )?;
    let result = async {
        let (artifact, usage) =
            crate::artifact::parse_artifact_with_usage(bytes, remaining, &super::OwnerDates(tx))
                .await
                .map_err(|e| StorageError::caused("recording native artifact validation", e))?;
        parse.charge(usage)?;
        Ok(artifact)
    }
    .await;
    tx.record(result)
}

impl WriteTransaction<'_> {
    /// Charge one mutation and its bound material before invoking the provider.
    /// # Arguments
    /// * `sql` - Private fixed native mutation.
    /// * `args` - Complete bound inputs.
    /// * `work` - Borrowed whole-operation ledger.
    /// # Returns
    /// Exact provider affected-row count.
    /// # Errors
    /// Refuses budget, SQL and latched transaction failures.
    pub(super) async fn recording_execute(
        &mut self,
        sql: &'static str,
        args: Vec<Value>,
        work: &mut Budget,
    ) -> Result<u64, StorageError> {
        work.charge_decoded(sql.len())?;
        charge_values(&args, work)?;
        self.execute_bound(sql, args).await
    }

    /// Bound every family-owned variable-width field before retaining SQL rows.
    /// # Arguments
    /// * `limits` - Finite parser and native content ceilings.
    /// * `work` - Borrowed cumulative operation allowance.
    /// # Returns
    /// Success when all supported native/local text is bounded and scalar shapes fit.
    /// # Errors
    /// Rejects excess rows/text, malformed native types and SQL failures.
    pub(super) async fn recording_preflight(
        &mut self,
        limits: AdmissionLimits,
        work: &mut Budget,
    ) -> Result<(), StorageError> {
        let max = limits.parser.input_bytes;
        if max == 0 || limits.parser.entries == 0 {
            return Err(StorageError::state("recording admission limits"));
        }
        // Aggregate raw-byte sums use BLOB casts, preserving embedded NUL bytes.
        // Each table is enumerated, so a small sampled manifest cannot hide a
        // giant foreign row behind a later rejection.
        for (table, fields) in [
            ("blob", "coalesce(length(CAST(uuid AS BLOB)),0)"),
            (
                "event",
                "coalesce(length(CAST(type AS BLOB)),0)+coalesce(length(CAST(user AS BLOB)),0)+coalesce(length(CAST(comment AS BLOB)),0)+coalesce(length(CAST(euser AS BLOB)),0)+coalesce(length(CAST(ecomment AS BLOB)),0)+coalesce(length(CAST(bgcolor AS BLOB)),0)+coalesce(length(CAST(brief AS BLOB)),0)",
            ),
            ("filename", "coalesce(length(CAST(name AS BLOB)),0)"),
            ("tag", "coalesce(length(CAST(tagname AS BLOB)),0)"),
            ("tagxref", "coalesce(length(CAST(value AS BLOB)),0)"),
            (
                "g_vfile",
                "coalesce(length(CAST(pathname AS BLOB)),0)+coalesce(length(CAST(checkout_id AS BLOB)),0)+coalesce(length(CAST(origname AS BLOB)),0)+coalesce(length(CAST(mhash AS BLOB)),0)",
            ),
            (
                "g_path_scope",
                "coalesce(length(CAST(path AS BLOB)),0)+coalesce(length(CAST(scope AS BLOB)),0)+coalesce(length(CAST(checkout_id AS BLOB)),0)",
            ),
            (
                "rcvfrom",
                "coalesce(length(CAST(uid AS BLOB)),0)+coalesce(length(CAST(mtime AS BLOB)),0)+coalesce(length(CAST(nonce AS BLOB)),0)+coalesce(length(CAST(ipaddr AS BLOB)),0)",
            ),
            (
                "g_vvar",
                "coalesce(length(CAST(name AS BLOB)),0)+coalesce(length(CAST(value AS BLOB)),0)+coalesce(length(CAST(checkout_id AS BLOB)),0)",
            ),
        ] {
            let sql = format!(
                "SELECT count(*),coalesce(max({fields}),0),coalesce(sum({fields}),0) FROM {table}"
            );
            work.charge_decoded(sql.len() + 96)?;
            let rows = self.query_bound(&sql, vec![]).await?;
            let total = preflight_text_total(&rows, limits)?;
            work.charge_decoded(
                usize::try_from(total)
                    .map_err(|_| StorageError::state("recording text overflow"))?,
            )?;
        }
        Ok(())
    }

    /// Resolve a complete existing full-content native identity under owner policy.
    /// # Arguments
    /// * `principal` - Authenticated local owner.
    /// * `name` - Exact full canonical native name.
    /// * `work` - The same operation ledger used by capture and projections.
    /// # Returns
    /// Exact RID, receipt and verified bytes.
    /// # Errors
    /// Refuses missing/phantom/delta/shunned content, malformed identities and bounds.
    pub(super) async fn recorded_content(
        &mut self,
        principal: &Principal,
        name: &str,
        work: &mut Budget,
    ) -> Result<(i64, i64, Vec<u8>), StorageError> {
        if !matches!(name.len(), 40 | 64)
            || !name
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(StorageError::state("recording native name grammar"));
        }
        let rows=bounded_rows(self,"SELECT rid,rcvid FROM blob WHERE uuid=?1 AND typeof(rid)='integer' AND typeof(rcvid)='integer' AND size>=0 AND NOT EXISTS(SELECT 1 FROM delta WHERE delta.rid=blob.rid)",vec![Value::Text(name.into())],1,64,work).await?;
        let (rid, receipt) = content_identity(&rows)?;
        let bytes = self
            .read_recording_content_budget(principal, name, work)
            .await?;
        Ok((rid, receipt, bytes))
    }

    /// Validate exactly the initialized-plus-private-recording family.
    /// # Arguments
    /// * `principal` - Owner authenticated by the enclosing catalog6 admission.
    /// * `shared_hash` - Immutable empty public S authenticated by that admission.
    /// * `limits` - Whole-operation content/parser ceilings.
    /// * `work` - Caller-owned cumulative content/SQL/hash allowance.
    /// * `parse` - Caller-owned cumulative parser compartments.
    /// # Returns
    /// Private immutable chain facts after complete projection/provenance validation.
    /// # Errors
    /// Rejects every unsupported history, foreign row, stale receipt, malformed
    /// projection, tracking/scope mismatch, exhausted bound and native SQL failure.
    pub(super) async fn validate_recorded_history(
        &mut self,
        principal: &Principal,
        shared_hash: &str,
        limits: AdmissionLimits,
        work: &mut Budget,
        parse: &mut ParseWork,
    ) -> Result<RecordedHistory, StorageError> {
        self.recording_preflight(limits, work).await?;
        let n = limits.parser.entries;
        let width = limits.parser.input_bytes;
        let identity = bounded_rows(
            self,
            "SELECT value FROM config WHERE name='server-code'",
            vec![],
            1,
            width,
            work,
        )
        .await?;
        let server = replica_identity(&identity)?;
        work.charge_decoded(server.len() + 15)?;
        let branch = format!("gossil-private/{server}");
        let (srid, sreceipt, sbytes) = self.recorded_content(principal, shared_hash, work).await?;
        charge_manifest_retention(&sbytes, work)?;
        let root = parse_recording_budgeted(self, &sbytes, parse, work).await?;
        require_empty_shared_root(&root)?;
        let shared = RecordedCommit {
            rid: srid,
            hash: shared_hash.into(),
            date: root
                .date_token()
                .ok_or_else(|| StorageError::state("recording root date"))?
                .to_vec(),
            julian: root
                .date()
                .ok_or_else(|| StorageError::state("recording root Julian date"))?,
            receipt: sreceipt,
            files: vec![],
        };
        let checkout = bounded_rows(
            self,
            "SELECT checkout_id,combined_head,generation,pending_version FROM g_checkout",
            vec![],
            1,
            width,
            work,
        )
        .await?;
        let CheckoutFacts {
            checkout_id,
            mut cursor,
            generation,
            pending,
        } = checkout_facts(&checkout)?;
        let mut reversed = Vec::new();
        let mut seen = BTreeSet::new();
        while let Some(hash) = cursor {
            if reversed.len() >= n || !seen.insert(hash.clone()) || hash == shared_hash {
                return Err(StorageError::state("recording history cycle or limit"));
            }
            let (rid, receipt, bytes) = self.recorded_content(principal, &hash, work).await?;
            if hash.len() != 64 {
                return Err(StorageError::state("recording manifest hash policy"));
            }
            charge_manifest_retention(&bytes, work)?;
            let artifact = parse_recording_budgeted(self, &bytes, parse, work).await?;
            let parents = artifact.parents();
            if parents.len() == 1 && parents[0].as_slice() == shared_hash.as_bytes() {
                cursor = None;
            } else if parents.len() == 2 && parents[1].as_slice() == shared_hash.as_bytes() {
                cursor = Some(
                    std::str::from_utf8(&parents[0])
                        .map_err(|e| StorageError::caused("recording parent UTF-8", e))?
                        .to_owned(),
                );
            } else {
                return Err(StorageError::state("recording exact paired parents"));
            }
            let mut files = Vec::new();
            for file in artifact.files() {
                let path = std::str::from_utf8(&file.name().bytes)
                    .map_err(|e| StorageError::caused("recording file path UTF-8", e))?
                    .to_owned();
                let bounds = super::tracking::TrackingLimits {
                    paths: n,
                    path_bytes: width,
                    components: n,
                    retained_bytes: limits.content.max_total_decoded_bytes,
                    io_buffer_bytes: 4096,
                };
                super::tracking::capture::checked_path(&path, bounds)?;
                let mode = match file.permissions() {
                    None => 0,
                    Some(b"x") => 1,
                    _ => return Err(StorageError::state("recording file mode")),
                };
                if file.prior_name().is_some() {
                    return Err(StorageError::state("recording rename unsupported"));
                }
                let name = std::str::from_utf8(
                    file.hash()
                        .ok_or_else(|| StorageError::state("recording deleted file"))?,
                )
                .map_err(|e| StorageError::caused("recording file hash UTF-8", e))?
                .to_owned();
                let (fid, _, _) = self.recorded_content(principal, &name, work).await?;
                files.push(RecordedFile {
                    path,
                    hash: name,
                    rid: fid,
                    mode,
                });
            }
            let date = artifact
                .date_token()
                .ok_or_else(|| StorageError::state("recording date missing"))?
                .to_vec();
            let julian = artifact
                .date()
                .ok_or_else(|| StorageError::state("recording Julian date missing"))?;
            reversed.push((
                RecordedCommit {
                    rid,
                    hash,
                    date,
                    julian,
                    receipt,
                    files,
                },
                bytes,
                artifact,
            ));
        }
        reversed.reverse();
        if *generation as u64 != reversed.len() as u64
            || (!reversed.is_empty() && *pending < *generation)
        {
            return Err(StorageError::state("recording generation/history mismatch"));
        }
        let issues = self.admit_issues(principal, limits, work, parse).await?;
        let (branch_tag, tags) = assign_tags(
            reversed.first().map(|(commit, _, _)| commit.receipt),
            &format!("sym-{branch}"),
            &issues,
        );
        let mut history = RecordedHistory {
            shared,
            commits: Vec::new(),
            branch,
        };
        let mut expected_plink = Vec::new();
        let mut expected_mlink = Vec::new();
        let mut expected_events = Vec::new();
        let mut expected_tags = Vec::new();
        let mut names = BTreeMap::new();
        let mut referenced = BTreeMap::new();
        referenced.insert(
            history.shared.rid,
            (history.shared.hash.clone(), history.shared.receipt),
        );
        let mut unsent = BTreeSet::new();
        for (index, (commit, bytes, artifact)) in reversed.into_iter().enumerate() {
            // Issue receipts may fall between check-ins, so the chain only increases.
            let previous_receipt = history.commits.last().unwrap_or(&history.shared).receipt;
            if commit.receipt <= previous_receipt {
                return Err(StorageError::state("recording exact receipt sequence"));
            }
            work.charge_decoded(
                bytes
                    .len()
                    .checked_mul(4)
                    .and_then(|n| n.checked_add(commit.files.len().checked_mul(512)?))
                    .ok_or_else(|| StorageError::state("recording projection capacity overflow"))?,
            )?;
            let previous = history.commits.last().unwrap_or(&history.shared);
            if !commit.julian.is_finite()
                || commit.julian <= previous.julian
                || commit.julian <= history.shared.julian
            {
                return Err(StorageError::state("recording SQL parent date ordering"));
            }
            if commit.files.len() == previous.files.len()
                && commit
                    .files
                    .iter()
                    .zip(&previous.files)
                    .all(|(a, b)| a.path == b.path && a.hash == b.hash && a.mode == b.mode)
            {
                return Err(StorageError::state(
                    "recording unchanged historical commit unsupported",
                ));
            }
            // No deletion or rename authority exists in this family.
            if previous
                .files
                .iter()
                .any(|old| !commit.files.iter().any(|new| new.path == old.path))
            {
                return Err(StorageError::state("recording deletion unsupported"));
            }
            let shared = ManifestParent {
                hash: &history.shared.hash,
                date: &history.shared.date,
            };
            let parents = if index == 0 {
                PrivateParents::First { shared }
            } else {
                PrivateParents::Later {
                    previous: ManifestParent {
                        hash: &previous.hash,
                        date: &previous.date,
                    },
                    shared,
                }
            };
            let files: Vec<_> = commit
                .files
                .iter()
                .map(|f| ManifestFile {
                    path: &f.path,
                    hash: &f.hash,
                    mode: f.mode,
                })
                .collect();
            let actor = artifact
                .user()
                .ok_or_else(|| StorageError::state("recording actor missing"))?;
            let comment = artifact
                .comment()
                .ok_or_else(|| StorageError::state("recording comment missing"))?;
            let canonical = build_private_manifest(
                &PrivateManifestSpec {
                    parents,
                    files: &files,
                    actor: &actor.bytes,
                    comment: &comment.bytes,
                    date: &commit.date,
                    branch: &history.branch,
                },
                work,
                width,
            )?;
            if canonical != bytes {
                return Err(StorageError::state("recording noncanonical full manifest"));
            }
            expected_plink.push(vec![
                Value::Integer(previous.rid),
                Value::Integer(commit.rid),
                Value::Integer(1),
                super::native_stored_date(commit.julian),
                Value::Null,
            ]);
            if index > 0 {
                expected_plink.push(vec![
                    Value::Integer(history.shared.rid),
                    Value::Integer(commit.rid),
                    Value::Integer(0),
                    super::native_stored_date(commit.julian),
                    Value::Null,
                ]);
            }
            for file in &commit.files {
                if !names.contains_key(&file.path) {
                    let id = names.len() as i64 + 1;
                    names.insert(file.path.clone(), id);
                }
                let old = previous.files.iter().find(|old| old.path == file.path);
                if old.is_none_or(|old| old.rid != file.rid || old.mode != file.mode) {
                    expected_mlink.push([
                        commit.rid,
                        file.rid,
                        previous.rid,
                        old.map_or(0, |f| f.rid),
                        names[&file.path],
                        0,
                        file.mode,
                        0,
                    ]);
                    if index > 0 {
                        expected_mlink.push([
                            commit.rid,
                            file.rid,
                            history.shared.rid,
                            0,
                            names[&file.path],
                            0,
                            file.mode,
                            1,
                        ]);
                    }
                    unsent.insert(file.rid);
                }
                let (rid, receipt, _) = self.recorded_content(principal, &file.hash, work).await?;
                validate_file_receipt(referenced.contains_key(&rid), receipt, commit.receipt)?;
                referenced
                    .entry(rid)
                    .or_insert((file.hash.clone(), receipt));
            }
            unsent.insert(commit.rid);
            referenced.insert(commit.rid, (commit.hash.clone(), commit.receipt));
            expected_events.push(vec![
                Value::Text("ci".into()),
                super::native_stored_date(commit.julian),
                Value::Integer(commit.rid),
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Null,
                Value::Text(
                    String::from_utf8(actor.bytes.clone())
                        .map_err(|e| StorageError::caused("recording actor UTF-8", e))?,
                ),
                Value::Null,
                Value::Text(
                    String::from_utf8(comment.bytes.clone())
                        .map_err(|e| StorageError::caused("recording comment UTF-8", e))?,
                ),
                Value::Null,
                super::native_stored_date(commit.julian),
            ]);
            for (tag, type_, value) in [
                (8, 2, Value::Text(history.branch.clone())),
                (12, 0, Value::Null),
                (branch_tag, 2, Value::Null),
            ] {
                expected_tags.push((
                    (commit.rid, tag),
                    vec![
                        Value::Integer(tag),
                        Value::Integer(type_),
                        Value::Integer(commit.rid),
                        Value::Integer(commit.rid),
                        value,
                        super::native_stored_date(commit.julian),
                        Value::Integer(commit.rid),
                    ],
                ));
            }
            history.commits.push(commit);
        }
        let mut leaves = vec![vec![Value::Integer(history.shared.rid)]];
        if let Some(last) = history.commits.last() {
            leaves.push(vec![Value::Integer(last.rid)]);
        }
        let mut filenames: Vec<_> = names
            .iter()
            .map(|(name, id)| (*id, vec![Value::Integer(*id), Value::Text(name.clone())]))
            .collect();
        filenames.sort_by_key(|(id, _)| *id);
        // fnid is assigned on first appearance and need not follow pathname
        // order after a later lexically earlier addition. Replay compares the
        // same persistent native key order used by the complete SQL projection.
        expected_mlink.sort_by_key(|row| (row[0], row[4], row[7]));
        self.expect_shared_tables(
            SharedTables {
                shared_rid: history.shared.rid,
                shared_receipt: history.shared.receipt,
                plink: expected_plink,
                mlink: expected_mlink
                    .into_iter()
                    .map(|row| row.into_iter().map(Value::Integer).collect())
                    .collect(),
                events: history
                    .commits
                    .iter()
                    .map(|commit| commit.rid)
                    .zip(expected_events)
                    .collect(),
                tagxref: expected_tags,
                tags,
                filenames: filenames.into_iter().map(|(_, row)| row).collect(),
                leaves,
                unsent,
                referenced,
                owners: history
                    .commits
                    .iter()
                    .map(|commit| commit.receipt)
                    .collect(),
            },
            &issues,
            n,
            width,
            work,
        )
        .await?;
        let selected = history.commits.last().unwrap_or(&history.shared);
        let vars = bounded_rows(
            self,
            "SELECT name,value FROM g_vvar WHERE checkout_id=?1 ORDER BY name",
            vec![Value::Text(checkout_id.clone())],
            3,
            width,
            work,
        )
        .await?;
        if vars.len() != 3 {
            return Err(StorageError::state("recording native checkout keys"));
        }
        let current = vars
            .iter()
            .find(|r| r.first() == Some(&Value::Text("fingerprint".into())))
            .ok_or_else(|| StorageError::state("recording fingerprint missing"))?;
        let [_, Value::Text(fingerprint)] = current.as_slice() else {
            return Err(StorageError::state("recording fingerprint type"));
        };
        let witness = self
            .verify_recording_fingerprint(fingerprint, width, work)
            .await?;
        // Registration takes the newest receipt, which an earlier issue may own, and
        // each check-in takes its own receipt.
        let witnessed = if history.commits.is_empty() {
            witness == selected.receipt || issues.iter().any(|issue| issue.receipt == witness)
        } else {
            witness == selected.receipt
        };
        if !witnessed {
            return Err(StorageError::state("recording current receipt relation"));
        }
        let mut expected_vars = vec![
            vec![Value::Text("checkout".into()), Value::Integer(selected.rid)],
            vec![
                Value::Text("checkout-hash".into()),
                Value::Text(selected.hash.clone()),
            ],
            vec![
                Value::Text("fingerprint".into()),
                Value::Text(fingerprint.clone()),
            ],
        ];
        // Native CLOB affinity stores the checkout integer as decimal TEXT.
        expected_vars[0][1] = Value::Text(selected.rid.to_string());
        if vars != expected_vars {
            return Err(StorageError::state("recording native checkout selection"));
        }
        self.validate_recorded_tracking(checkout_id, selected, *generation, *pending, limits, work)
            .await?;
        Ok(history)
    }

    /// Compare a complete exact native projection against canonical replay.
    /// # Arguments
    /// * `sql` - Fixed ordered projection query.
    /// * `args` - Exact query bindings.
    /// * `expected` - Complete replay output in the same native order.
    /// * `count` - Finite maximum rows.
    /// * `width` - Finite maximum retained row bytes.
    /// * `work` - Shared operation allowance.
    /// # Returns
    /// Success only for exact values, NULLs, types, ordering and cardinality.
    /// # Errors
    /// Rejects foreign, missing, malformed or divergent rows and exhausted limits.
    async fn recording_expect(
        &mut self,
        sql: &str,
        args: Vec<Value>,
        expected: Vec<Vec<Value>>,
        count: usize,
        width: usize,
        work: &mut Budget,
    ) -> Result<(), StorageError> {
        for row in &expected {
            charge_values(row, work)?;
        }
        if bounded_rows(self, sql, args, count.max(1), width, work).await? != expected {
            return Err(StorageError::state("recording native projection differs"));
        }
        Ok(())
    }

    /// Compare every shared native table with replay of the check-ins and the issues.
    ///
    /// Each table must hold exactly the replay rows, in native order. Every receive
    /// record after the shared root's must belong to one check-in or one issue.
    /// # Arguments
    /// * `tables` - The replay rows of the check-ins.
    /// * `issues` - Every admitted issue, in receipt order.
    /// * `n` - The parser entry ceiling, which bounds the check-ins.
    /// * `width` - The parser input ceiling, which bounds each row.
    /// * `work` - Shared operation allowance.
    /// # Returns
    /// Success only when every table equals replay.
    /// # Errors
    /// Rejects foreign, missing or divergent rows, unexplained receipts and exhausted limits.
    pub(super) async fn expect_shared_tables(
        &mut self,
        mut tables: SharedTables,
        issues: &[AdmittedIssue],
        n: usize,
        width: usize,
        work: &mut Budget,
    ) -> Result<(), StorageError> {
        let extra = issues.len();
        for issue in issues {
            tables.events.push((issue.rid, issue.event.clone()));
            tables
                .tagxref
                .push(((issue.rid, issue.tagid), issue.tagxref.clone()));
            tables
                .referenced
                .insert(issue.rid, (issue.hash.clone(), issue.receipt));
            tables.owners.push(issue.receipt);
        }
        tables.events.sort_by_key(|(rid, _)| *rid);
        tables.tagxref.sort_by_key(|(key, _)| *key);
        /// Drop the sort key of each row.
        fn unkeyed<K>(rows: Vec<(K, Vec<Value>)>) -> Vec<Vec<Value>> {
            rows.into_iter().map(|(_, row)| row).collect()
        }
        let shared = Value::Integer(tables.shared_rid);
        let private = tables
            .referenced
            .keys()
            .filter(|rid| **rid != tables.shared_rid)
            .map(|rid| vec![Value::Integer(*rid)])
            .collect();
        let blobs = tables
            .referenced
            .iter()
            .map(|(rid, (hash, receipt))| {
                vec![
                    Value::Integer(*rid),
                    Value::Text(hash.clone()),
                    Value::Integer(*receipt),
                ]
            })
            .collect();
        let unsent = tables
            .unsent
            .iter()
            .map(|rid| vec![Value::Integer(*rid)])
            .collect();
        let many = n.saturating_mul(n).saturating_add(extra);
        for (sql, args, expected, count, row_bytes) in [
            (
                "SELECT pid,cid,isprim,mtime,baseid FROM plink ORDER BY cid,isprim DESC",
                vec![],
                tables.plink,
                n.saturating_mul(2),
                width,
            ),
            (
                "SELECT mid,fid,pmid,pid,fnid,pfnid,mperm,isaux FROM mlink ORDER BY mid,fnid,isaux",
                vec![],
                tables.mlink,
                n.saturating_mul(n).saturating_mul(2),
                width,
            ),
            (
                "SELECT type,mtime,objid,tagid,uid,bgcolor,euser,user,ecomment,comment,brief,omtime FROM event WHERE objid<>?1 ORDER BY objid",
                vec![shared.clone()],
                unkeyed(tables.events),
                n.saturating_add(extra),
                width,
            ),
            // Tag rows 1..12 and the root's two xrefs are authenticated before this call.
            (
                "SELECT tagid,tagtype,srcid,origid,value,mtime,rid FROM tagxref WHERE rid IS NOT ?1 ORDER BY rid,tagid",
                vec![shared.clone()],
                unkeyed(tables.tagxref),
                n.saturating_mul(3).saturating_add(extra),
                width,
            ),
            (
                "SELECT tagid,tagname FROM tag WHERE tagid>12 ORDER BY tagid",
                vec![],
                tables.tags,
                extra.saturating_add(1),
                width,
            ),
            (
                "SELECT fnid,name FROM filename ORDER BY fnid",
                vec![],
                tables.filenames,
                n,
                width,
            ),
            (
                "SELECT rid FROM leaf ORDER BY rid",
                vec![],
                tables.leaves,
                2,
                64,
            ),
            (
                "SELECT rid FROM unclustered ORDER BY rid",
                vec![],
                vec![vec![shared]],
                1,
                64,
            ),
            (
                "SELECT rid FROM unsent ORDER BY rid",
                vec![],
                unsent,
                many,
                64,
            ),
            (
                "SELECT rid FROM private ORDER BY rid",
                vec![],
                private,
                many,
                64,
            ),
            (
                "SELECT rid,uuid,rcvid FROM blob ORDER BY rid",
                vec![],
                blobs,
                many,
                width,
            ),
        ] {
            self.recording_expect(sql, args, expected, count, row_bytes, work)
                .await?;
        }
        let receipts=bounded_rows(self,"SELECT rcvid,uid,typeof(mtime),nonce,ipaddr,datetime(mtime) IS NOT NULL FROM rcvfrom ORDER BY rcvid",vec![],n.saturating_add(extra).saturating_add(1),128,work).await?;
        validate_receipts(&receipts, tables.shared_receipt, &tables.owners)
    }

    /// Verify recorded and newly added tracking rows against the selected full tree.
    /// # Arguments
    /// * `checkout` - Sole independently authenticated checkout identity.
    /// * `selected` - Exact S or current L manifest.
    /// * `generation` - Exact private manifest count.
    /// * `pending` - Persisted mutation counter, bounded by reachable minimum steps.
    /// * `limits` - Complete finite inventory/path ceilings.
    /// * `work` - Shared operation allowance.
    /// # Returns
    /// Success for all recorded rows plus zero-RID new additions and partial scopes.
    /// # Errors
    /// Rejects unsupported scalar/null fields, missing tree entries, orphan scopes,
    /// aliases, foreign rows, duplicate IDs, pending paths and exhausted bounds.
    async fn validate_recorded_tracking(
        &mut self,
        checkout: &str,
        selected: &RecordedCommit,
        generation: i64,
        pending: i64,
        limits: AdmissionLimits,
        work: &mut Budget,
    ) -> Result<(), StorageError> {
        let bad=bounded_scalar(self,"SELECT EXISTS(SELECT 1 FROM g_vfile WHERE typeof(checkout_id)<>'text' OR checkout_id IS NOT ?1 OR typeof(id)<>'integer' OR id<=0 OR typeof(vid)<>'integer' OR vid<>?2 OR typeof(chnged)<>'integer' OR chnged<>0 OR typeof(deleted)<>'integer' OR deleted<>0 OR typeof(isexe)<>'integer' OR isexe NOT IN(0,1) OR typeof(islink)<>'integer' OR islink<>0 OR typeof(rid)<>'integer' OR rid<0 OR typeof(mrid)<>'integer' OR mrid<>rid OR typeof(mtime)<>'integer' OR typeof(pathname)<>'text' OR origname IS NOT NULL OR mhash IS NOT NULL)",vec![Value::Text(checkout.into()),Value::Integer(selected.rid)],work).await?;
        if bad != 0 {
            return Err(StorageError::state("recording unsupported tracked state"));
        }
        let rows = bounded_rows(
            self,
            "SELECT id,pathname,rid,isexe FROM g_vfile ORDER BY id",
            vec![],
            limits.parser.entries,
            limits.parser.input_bytes,
            work,
        )
        .await?;
        let bounds = super::tracking::TrackingLimits {
            paths: limits.parser.entries,
            path_bytes: limits.parser.input_bytes,
            components: limits.parser.entries,
            retained_bytes: limits.content.max_total_decoded_bytes,
            io_buffer_bytes: 4096,
        };
        let mut names = BTreeSet::new();
        let mut recorded = BTreeSet::new();
        for (index, row) in rows.iter().enumerate() {
            let TrackedFacts {
                id,
                path,
                rid,
                mode,
            } = tracked_facts(row)?;
            if *id != index as i64 + 1 {
                return Err(StorageError::state("recording tracked row identity"));
            }
            super::tracking::capture::checked_path(path, bounds)?;
            let folded = path.to_ascii_lowercase();
            if names.iter().any(|prior: &String| {
                prior == &folded
                    || prior
                        .strip_prefix(&folded)
                        .is_some_and(|tail| tail.starts_with('/'))
                    || folded
                        .strip_prefix(prior)
                        .is_some_and(|tail| tail.starts_with('/'))
            }) {
                return Err(StorageError::state("recording tracked alias"));
            }
            names.insert(folded);
            let prior = selected.files.iter().find(|file| file.path == *path);
            match prior {
                Some(file) if *rid == file.rid && *mode == file.mode => {
                    recorded.insert(path.clone());
                }
                None if *rid == 0 => (),
                _ => return Err(StorageError::state("recording tracked/manifest mismatch")),
            }
        }
        if recorded.len() != selected.files.len() {
            return Err(StorageError::state("recording selected file missing"));
        }
        let scopes = self
            .read_path_scopes_budgeted(checkout, bounds, work)
            .await?;
        let new_paths = collect_pending_references(&rows, work)?;
        let scoped_new = new_paths.iter().any(|path| {
            scopes
                .iter()
                .any(|scope| scope.path.as_str() == path.as_str())
        });
        let minimum = if generation == 0 {
            i64::from(!rows.is_empty()) + i64::from(!scopes.is_empty())
        } else {
            generation
                .checked_add(2)
                .and_then(|n| n.checked_add(i64::from(!new_paths.is_empty())))
                .and_then(|n| n.checked_add(i64::from(scoped_new)))
                .ok_or_else(|| StorageError::state("recording minimum counter overflow"))?
        };
        if pending < minimum || (rows.is_empty() && pending != 0) {
            return Err(StorageError::state("recording pending counter unreachable"));
        }
        if selected.files.iter().any(|file| {
            !scopes.iter().any(|scope| {
                scope.path == file.path && scope.scope == super::path_scope::PathScope::Private
            })
        }) {
            return Err(StorageError::state(
                "recording historical private scope missing",
            ));
        }
        if bounded_scalar(self, "SELECT count(*) FROM g_pending_path", vec![], work).await? != 0 {
            return Err(StorageError::state(
                "recording pending-path authority unsupported",
            ));
        }
        Ok(())
    }

    /// Compute the version-one native checksum using one exact retained witness.
    /// # Arguments
    /// * `receipt` - Positive exact receive row ID, never silently replaced by latest.
    /// * `max` - Finite raw-field and conservative SQL conversion bound.
    /// * `work` - Borrowed cumulative ledger.
    /// # Returns
    /// Canonical decimal witness, slash and lowercase native MD5 checksum.
    /// # Errors
    /// Rejects absent/malformed receipt, oversized conversion, NULL date or limits.
    pub(super) async fn recording_fingerprint(
        &mut self,
        receipt: i64,
        max: usize,
        work: &mut Budget,
    ) -> Result<String, StorageError> {
        if receipt <= 0 || i32::try_from(receipt).is_err() {
            return Err(StorageError::state("recording fingerprint witness"));
        }
        let size=bounded_scalar(self,"SELECT coalesce(length(CAST(uid AS BLOB)),0)+coalesce(length(CAST(mtime AS BLOB)),0)+coalesce(length(CAST(nonce AS BLOB)),0)+coalesce(length(CAST(ipaddr AS BLOB)),0) FROM rcvfrom WHERE rcvid=?1",vec![Value::Integer(receipt)],work).await?;
        if size < 0
            || (size as u64)
                .checked_mul(2)
                .and_then(|n| n.checked_add(128))
                .is_none_or(|n| n > max as u64)
        {
            return Err(StorageError::state(
                "recording fingerprint conversion bound",
            ));
        }
        let rows=bounded_rows(self,"SELECT CAST(quote(uid) AS BLOB),CAST(datetime(mtime) AS BLOB),CAST(quote(nonce) AS BLOB),CAST(quote(ipaddr) AS BLOB) FROM rcvfrom WHERE rcvid=?1",vec![Value::Integer(receipt)],1,max,work).await?;
        fingerprint_from_conversion(&rows, receipt, work)
    }

    /// Verify an embedded witness independently of current-head receipt policy.
    /// # Arguments
    /// * `fingerprint` - Exact persisted witness/checksum spelling.
    /// * `max` - Finite SQL conversion ceiling.
    /// * `work` - Borrowed whole-operation ledger.
    /// # Returns
    /// The exact embedded witness after native checksum validation.
    /// # Errors
    /// Rejects noncanonical spelling, missing receipt, mismatch or exhausted limits.
    pub(super) async fn verify_recording_fingerprint(
        &mut self,
        fingerprint: &str,
        max: usize,
        work: &mut Budget,
    ) -> Result<i64, StorageError> {
        work.charge_decoded(fingerprint.len())?;
        let (prefix, digest) = fingerprint
            .split_once('/')
            .ok_or_else(|| StorageError::state("recording fingerprint grammar"))?;
        if prefix.is_empty()
            || prefix.starts_with('0')
            || !prefix.bytes().all(|b| b.is_ascii_digit())
            || digest.len() != 32
            || !digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(StorageError::state("recording fingerprint grammar"));
        }
        let receipt = prefix
            .parse::<i64>()
            .map_err(|e| StorageError::caused("recording fingerprint witness", e))?;
        if self.recording_fingerprint(receipt, max, work).await? != fingerprint {
            return Err(StorageError::state("recording fingerprint checksum"));
        }
        Ok(receipt)
    }

    /// Install the exact native crosslinks for a newly inserted private full manifest.
    /// # Arguments
    /// * `history` - Privately authenticated complete previous history.
    /// * `projection` - Newly inserted manifest identity, bytes and complete file tree.
    /// * `limits` - Finite native/parser bounds.
    /// * `work` - Borrowed cumulative operation budget.
    /// * `parse` - Borrowed cumulative parser compartments.
    /// # Returns
    /// Success after parent/file links, three tag cards, event and leaf effects.
    /// # Errors
    /// Rejects an oversized tree before retention, parsing or writes, then rejects
    /// malformed canonical input, impossible prior state, other bounds or SQL errors.
    pub(super) async fn project_recording(
        &mut self,
        history: &RecordedHistory,
        projection: ProjectionInput<'_>,
        limits: AdmissionLimits,
        work: &mut Budget,
        parse: &mut ParseWork,
    ) -> Result<(), StorageError> {
        let ProjectionInput { rid, bytes, files } = projection;
        // Reject an oversized complete tree before parser retention or projection writes.
        if files.len() > limits.parser.entries {
            return Err(StorageError::state("recording projection entry limit"));
        }
        charge_manifest_retention(bytes, work)?;
        let artifact = parse_recording_budgeted(self, bytes, parse, work).await?;
        let julian = artifact
            .date()
            .ok_or_else(|| StorageError::state("recording projection date"))?;
        let previous = history.commits.last().unwrap_or(&history.shared);
        self.recording_execute(
            "INSERT INTO plink(pid,cid,isprim,mtime,baseid) VALUES(?1,?2,1,?3,NULL)",
            vec![
                Value::Integer(previous.rid),
                Value::Integer(rid),
                Value::Real(julian),
            ],
            work,
        )
        .await?;
        if !history.commits.is_empty() {
            self.recording_execute(
                "INSERT INTO plink(pid,cid,isprim,mtime,baseid) VALUES(?1,?2,0,?3,NULL)",
                vec![
                    Value::Integer(history.shared.rid),
                    Value::Integer(rid),
                    Value::Real(julian),
                ],
                work,
            )
            .await?;
        }
        for file in files {
            self.recording_execute(
                "INSERT OR IGNORE INTO filename(name) VALUES(?1)",
                vec![Value::Text(file.path.clone())],
                work,
            )
            .await?;
            let fnid = bounded_scalar(
                self,
                "SELECT fnid FROM filename WHERE name=?1",
                vec![Value::Text(file.path.clone())],
                work,
            )
            .await?;
            let old = previous.files.iter().find(|old| old.path == file.path);
            if old.is_none_or(|old| old.rid != file.rid || old.mode != file.mode) {
                self.recording_execute("INSERT INTO mlink(mid,fid,pmid,pid,fnid,pfnid,mperm,isaux) VALUES(?1,?2,?3,?4,?5,0,?6,0)",vec![Value::Integer(rid),Value::Integer(file.rid),Value::Integer(previous.rid),Value::Integer(old.map_or(0,|old|old.rid)),Value::Integer(fnid),Value::Integer(file.mode)],work).await?;
                if !history.commits.is_empty() {
                    self.recording_execute("INSERT INTO mlink(mid,fid,pmid,pid,fnid,pfnid,mperm,isaux) VALUES(?1,?2,?3,0,?4,0,?5,1)",vec![Value::Integer(rid),Value::Integer(file.rid),Value::Integer(history.shared.rid),Value::Integer(fnid),Value::Integer(file.mode)],work).await?;
                }
                // Native commit queues changed file identities even when a complete
                // existing blob is reused; classification and rcvid remain untouched.
                self.recording_execute(
                    "INSERT OR IGNORE INTO unsent(rid) VALUES(?1)",
                    vec![Value::Integer(file.rid)],
                    work,
                )
                .await?;
            }
        }
        let actor = artifact
            .user()
            .ok_or_else(|| StorageError::state("recording projection actor"))?;
        let comment = artifact
            .comment()
            .ok_or_else(|| StorageError::state("recording projection comment"))?;
        let actor = String::from_utf8(actor.bytes.clone())
            .map_err(|e| StorageError::caused("recording actor UTF-8", e))?;
        let comment = String::from_utf8(comment.bytes.clone())
            .map_err(|e| StorageError::caused("recording comment UTF-8", e))?;
        self.recording_execute(
            "INSERT INTO event(type,mtime,objid,user,comment,omtime) VALUES('ci',?1,?2,?3,?4,?1)",
            vec![
                Value::Real(julian),
                Value::Integer(rid),
                Value::Text(actor),
                Value::Text(comment),
            ],
            work,
        )
        .await?;
        self.recording_execute(
            "INSERT OR IGNORE INTO tag(tagname) VALUES(?1)",
            vec![Value::Text(format!("sym-{}", history.branch))],
            work,
        )
        .await?;
        let tag = bounded_scalar(
            self,
            "SELECT tagid FROM tag WHERE tagname=?1",
            vec![Value::Text(format!("sym-{}", history.branch))],
            work,
        )
        .await?;
        // Issues may hold lower tag IDs. The post-state admission checks this ID.
        for (id, type_, value) in [
            (8, 2, Value::Text(history.branch.clone())),
            (12, 0, Value::Null),
            (tag, 2, Value::Null),
        ] {
            self.recording_execute("INSERT INTO tagxref(tagid,tagtype,srcid,origid,value,mtime,rid) VALUES(?1,?2,?3,?3,?4,?5,?3)",vec![Value::Integer(id),Value::Integer(type_),Value::Integer(rid),value,Value::Real(julian)],work).await?;
        }
        if previous.rid != history.shared.rid {
            self.recording_execute(
                "DELETE FROM leaf WHERE rid=?1",
                vec![Value::Integer(previous.rid)],
                work,
            )
            .await?;
        }
        self.recording_execute(
            "INSERT INTO leaf(rid) VALUES(?1)",
            vec![Value::Integer(rid)],
            work,
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
/// Check exact provider scalars and the historical fingerprint algorithm.
mod controls {
    use super::*;
    use crate::storage::{InitializeOptions, Limits, LocalOwnerRepository};

    /// Supply finite native and parser bounds for decoder and real-provider controls.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// The same generous limits used by the recorded-history fixture.
    /// # Errors
    /// Cannot fail.
    fn bounds() -> AdmissionLimits {
        AdmissionLimits {
            content: Limits {
                max_stored_bytes: 1 << 20,
                max_output_bytes: 1 << 20,
                max_chain_depth: 128,
                max_total_stored_bytes: 64 << 20,
                max_total_decoded_bytes: 256 << 20,
            },
            parser: crate::artifact::ParseLimits {
                input_bytes: 4 << 20,
                cards: 65536,
                owned_bytes: 16 << 20,
                entries: 8192,
            },
        }
    }

    /// Initialize an actual native repository for private-boundary controls.
    /// # Arguments
    /// * `path` - Disposable fixture database retained by the test.
    /// # Returns
    /// Initialized authenticated owner with no checkout or private history.
    /// # Panics
    /// Panics if fixture initialization fails.
    async fn initialized(path: &std::path::Path) -> LocalOwnerRepository {
        let b = bounds();
        LocalOwnerRepository::initialize(
            path,
            InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                date: "2026-11-20T00:00:00",
                user: b"Owner",
                password: "secret",
                sha1: false,
                content_limits: b.content,
                parse_limits: b.parser,
            },
        )
        .await
        .unwrap()
    }

    /// Assert a precise refusal without requiring successful facts to implement Debug.
    /// # Arguments
    /// * `result` - Actual private validation outcome.
    /// * `operation` - Independently specified failure cause.
    /// # Returns
    /// Unit after exact error comparison.
    /// # Panics
    /// Panics on success or a different failure cause.
    fn refuses<T>(result: Result<T, StorageError>, operation: &str) {
        assert_eq!(result.err().expect("required refusal").operation, operation);
    }

    /// Read every table inside the existing fixture transaction without changing it.
    /// # Arguments
    /// * `tx` - Reserved real native fixture transaction.
    /// # Returns
    /// Complete deterministically sorted table values.
    /// # Panics
    /// Panics on an invalid fixture identifier or provider failure.
    async fn snapshot_tables(tx: &mut WriteTransaction<'_>) -> Vec<(String, Vec<Vec<Value>>)> {
        let names = tx
            .query("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")
            .await
            .unwrap();
        let mut snapshot = Vec::new();
        for row in names {
            assert_eq!(row.len(), 1);
            let name = row[0].as_text().expect("native table name");
            assert!(name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
            let mut values = tx.query(&format!("SELECT * FROM {name}")).await.unwrap();
            values.sort_by_key(|row| format!("{row:?}"));
            snapshot.push((name.clone(), values));
        }
        snapshot
    }

    #[tokio::test]
    /// Exact scalar queries reject actual native shape/type errors and retain prior debit.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after literal SQL controls, failure latching and all-table preservation.
    /// # Panics
    /// Panics on coercion, ignored cardinality, reset work or persistent mutation.
    async fn bounded_scalar_requires_one_native_integer() {
        let temp = tempfile::tempdir().unwrap();
        let mut owner = initialized(&temp.path().join("repo")).await;
        let b = bounds();
        let mut tx = owner.store.begin_write().await.unwrap();
        let before = snapshot_tables(&mut tx).await;
        let mut work = Budget::new(b.content).unwrap();
        assert_eq!(
            bounded_scalar(&mut tx, "SELECT 7", vec![], &mut work)
                .await
                .unwrap(),
            7
        );
        for (sql, expected) in [
            ("SELECT NULL", "recording scalar type"),
            ("SELECT '7'", "recording scalar type"),
            ("SELECT 7,8", "recording scalar type"),
            ("SELECT 7 WHERE 0", "recording scalar cardinality"),
            ("SELECT 7 UNION ALL SELECT 8", "recording SQL count limit"),
            ("SELECT zeroblob(65)", "recording SQL width limit"),
        ] {
            refuses(
                bounded_scalar(&mut tx, sql, vec![], &mut work).await,
                expected,
            );
        }
        assert_eq!(snapshot_tables(&mut tx).await, before);
        let mut work = Budget::new(Limits {
            max_total_decoded_bytes: 128,
            ..b.content
        })
        .unwrap();
        work.charge_decoded(127).unwrap();
        let refusal = bounded_scalar(&mut tx, "SELECT 7", vec![], &mut work).await;
        refuses(tx.record(refusal), "total decoded content limit");
        refuses(tx.commit().await, "total decoded content limit");
        let mut tx = owner.store.begin_write().await.unwrap();
        assert_eq!(snapshot_tables(&mut tx).await, before);
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    /// Parsed file/parent facts enforce the empty-root policy without assigning authority.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after the four-case policy and cumulative manifest-retention controls.
    /// # Panics
    /// Panics on invalid authored native syntax, policy acceptance or restored allowance.
    async fn empty_root_policy_and_manifest_retention_use_real_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        let mut owner = initialized(&temp.path().join("repo")).await;
        let b = bounds();
        let mut tx = owner.store.begin_write().await.unwrap();
        let before = snapshot_tables(&mut tx).await;
        let hash = crate::content_hash::hex(&crate::content_hash::sha3(b"payload", 256).unwrap());
        let parent = "1".repeat(64);
        let mut empty = Vec::new();
        for (has_file, has_parent) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut body = "C root\nD 2026-11-20T00:00:00\n".to_owned();
            if has_file {
                body.push_str(&format!("F a {hash}\n"));
            }
            if has_parent {
                body.push_str(&format!("P {parent}\n"));
            }
            body.push_str("U Owner\n");
            let checksum =
                crate::content_hash::hex(&crate::artifact_checksum::md5(body.as_bytes()));
            let bytes = format!("{body}Z {checksum}\n").into_bytes();
            let artifact = parse_recording_budgeted(
                &mut tx,
                &bytes,
                &mut ParseWork::new(b.parser).unwrap(),
                &mut Budget::new(b.content).unwrap(),
            )
            .await
            .unwrap();
            assert_eq!(artifact.files().len(), usize::from(has_file));
            assert_eq!(artifact.parents().len(), usize::from(has_parent));
            if has_file || has_parent {
                refuses(
                    require_empty_shared_root(&artifact),
                    "recording requires empty immutable S",
                );
            } else {
                require_empty_shared_root(&artifact).unwrap();
                empty = bytes;
            }
        }
        let empty_hash = crate::content_hash::hex(&crate::content_hash::sha3(&empty, 256).unwrap());
        let branch = format!("gossil-private/{}", "2".repeat(40));
        let private = build_private_manifest(
            &PrivateManifestSpec {
                parents: PrivateParents::First {
                    shared: ManifestParent {
                        hash: &empty_hash,
                        date: b"2026-11-20T00:00:00",
                    },
                },
                files: &[ManifestFile {
                    path: "a",
                    hash: &hash,
                    mode: 0,
                }],
                actor: b"Owner",
                comment: b"private",
                date: b"2026-11-20T00:00:01",
                branch: &branch,
            },
            &mut Budget::new(b.content).unwrap(),
            b.parser.input_bytes,
        )
        .unwrap();
        let artifact = parse_recording_budgeted(
            &mut tx,
            &private,
            &mut ParseWork::new(b.parser).unwrap(),
            &mut Budget::new(b.content).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(artifact.files().len(), 1);
        let budget = |total| {
            Budget::new(Limits {
                max_total_decoded_bytes: total,
                ..b.content
            })
            .unwrap()
        };
        // Discover the observed two-artifact boundary instead of duplicating its debit formula.
        let threshold = (1..=4096)
            .find(|total| {
                let mut work = budget(*total);
                charge_manifest_retention(&empty, &mut work).is_ok()
                    && charge_manifest_retention(&private, &mut work).is_ok()
            })
            .expect("finite small manifest boundary");
        let mut work = budget(threshold);
        charge_manifest_retention(&empty, &mut work).unwrap();
        charge_manifest_retention(&private, &mut work).unwrap();
        refuses(work.charge_decoded(1), "total decoded content limit");
        let mut work = budget(threshold);
        work.charge_decoded(1).unwrap();
        charge_manifest_retention(&empty, &mut work).unwrap();
        refuses(
            charge_manifest_retention(&private, &mut work),
            "total decoded content limit",
        );
        let mut work = budget(threshold);
        work.charge_decoded((threshold - 1) as usize).unwrap();
        refuses(
            charge_manifest_retention(&empty, &mut work),
            "total decoded content limit",
        );
        // Prior operation work must refuse parsing before consuming parser compartments.
        // The same real private artifact parsed successfully above with sufficient work.
        let mut work = Budget::new(b.content).unwrap();
        work.charge_decoded((b.content.max_total_decoded_bytes - 1) as usize)
            .unwrap();
        let mut parse = ParseWork::new(b.parser).unwrap();
        let before_parse = parse.remaining;
        refuses(
            parse_recording_budgeted(&mut tx, &private, &mut parse, &mut work).await,
            "total decoded content limit",
        );
        assert_eq!(parse.remaining.input_bytes, before_parse.input_bytes);
        assert_eq!(parse.remaining.cards, before_parse.cards);
        assert_eq!(parse.remaining.owned_bytes, before_parse.owned_bytes);
        assert_eq!(parse.remaining.entries, before_parse.entries);
        assert_eq!(snapshot_tables(&mut tx).await, before);
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    /// Unsupported pending authority is refused at the private selected-history boundary.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after real selected history, native remove-row types and complete rollback.
    /// # Panics
    /// Panics on an admitted pending row, wrong guard, mutation or failed restored reopen.
    async fn selected_tracking_refuses_pending_authority_without_repair() {
        use crate::storage::{
            PathScope, PathScopeSelection, PrivateRecordingRequest, TrackingLimits,
        };
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        std::fs::write(root.join("a"), b"payload").unwrap();
        let mode = std::fs::metadata(root.join("a"))
            .unwrap()
            .permissions()
            .mode();
        let mut owner = initialized(&repo).await;
        let b = bounds();
        let tracking = TrackingLimits {
            paths: 8,
            path_bytes: 4096,
            components: 32,
            retained_bytes: 1 << 20,
            io_buffer_bytes: 1024,
        };
        let view = owner.register_checkout("work", &root, b).await.unwrap();
        let view = owner
            .add_files("work", &["a"], &view, b, tracking)
            .await
            .unwrap();
        let view = owner
            .select_path_scopes(
                "work",
                &view,
                &[PathScopeSelection {
                    path: "a".into(),
                    scope: PathScope::Private,
                }],
                b,
                tracking,
            )
            .await
            .unwrap();
        owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: &view,
                    paths: &["a"],
                    scope: PathScope::Private,
                    actor: b"Owner",
                    comment: b"selected",
                    date: b"2026-11-20T00:00:01",
                },
                b,
                tracking,
            )
            .await
            .unwrap();
        let view = owner.checkout_view("work", b).await.unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        let original = snapshot_tables(&mut tx).await;
        let mut work = Budget::new(b.content).unwrap();
        let mut parse = ParseWork::new(b.parser).unwrap();
        let history = tx
            .validate_recorded_history(
                &owner.principal,
                &view.shared_head,
                b,
                &mut work,
                &mut parse,
            )
            .await
            .unwrap();
        let selected = history
            .commits
            .last()
            .expect("real selected private commit");
        assert_eq!(selected.files.len(), 1);
        let generation = i64::try_from(view.generation).unwrap();
        let pending = i64::try_from(view.pending_version).unwrap();
        tx.validate_recorded_tracking("work", selected, generation, pending, b, &mut work)
            .await
            .unwrap();
        assert_eq!(
            tx.query("SELECT count(*) FROM g_pending_path")
                .await
                .unwrap(),
            vec![vec![Value::Integer(0)]]
        );
        // This schema-valid row is deliberately outside the admitted producer family.
        tx.execute("INSERT INTO g_pending_path(checkout_id,path,action,selected_hash,selected_mode,selected_scope,old_path) VALUES('work','a','remove',NULL,NULL,'private',NULL)").await.unwrap();
        assert_eq!(tx.query("SELECT checkout_id,path,action,typeof(selected_hash),typeof(selected_mode),selected_scope,typeof(old_path) FROM g_pending_path").await.unwrap(), vec![vec![
            Value::Text("work".into()), Value::Text("a".into()), Value::Text("remove".into()),
            Value::Text("null".into()), Value::Text("null".into()), Value::Text("private".into()), Value::Text("null".into()),
        ]]);
        let unsupported = snapshot_tables(&mut tx).await;
        refuses(
            tx.validate_recorded_tracking("work", selected, generation, pending, b, &mut work)
                .await,
            "recording pending-path authority unsupported",
        );
        assert_eq!(snapshot_tables(&mut tx).await, unsupported);
        tx.rollback().await.unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        assert_eq!(snapshot_tables(&mut tx).await, original);
        tx.rollback().await.unwrap();
        drop(owner);
        let mut reopened = LocalOwnerRepository::open(&repo, b).await.unwrap();
        assert_eq!(reopened.checkout_view("work", b).await.unwrap(), view);
        assert_eq!(std::fs::read(root.join("a")).unwrap(), b"payload");
        assert_eq!(
            std::fs::metadata(root.join("a"))
                .unwrap()
                .permissions()
                .mode(),
            mode
        );
    }

    /// Replace a selected fixture manifest while keeping its native projections consistent.
    /// # Arguments
    /// * `tx` - Private fixture mutation transaction, never a public producer operation.
    /// * `rid` - Actual existing selected manifest row identity.
    /// * `prior` - Exact currently selected manifest hash.
    /// * `name` - Independently computed replacement native hash.
    /// * `size` - Actual replacement decoded length.
    /// * `packed` - Replacement full native envelope, or the exact original envelope on restore.
    /// * `day` - Native Julian day matching the replacement manifest's D card.
    /// # Returns
    /// Unit after all manifest selection and time projections change exactly once.
    /// # Panics
    /// Panics if any expected fixture row is missing, ambiguous or rejected by native SQL.
    async fn rewrite_replay_fixture(
        tx: &mut WriteTransaction<'_>,
        rid: i64,
        prior: &str,
        name: &str,
        size: i64,
        packed: &[u8],
        day: f64,
    ) {
        for (sql, args, count) in [
            (
                "UPDATE blob SET uuid=?1,size=?2,content=?3 WHERE rid=?4 AND uuid=?5",
                vec![
                    Value::Text(name.into()),
                    Value::Integer(size),
                    Value::Blob(packed.to_vec()),
                    Value::Integer(rid),
                    Value::Text(prior.into()),
                ],
                1,
            ),
            (
                "UPDATE g_checkout SET combined_head=?1 WHERE checkout_id='work' AND combined_head=?2",
                vec![Value::Text(name.into()), Value::Text(prior.into())],
                1,
            ),
            (
                "UPDATE g_vvar SET value=?1 WHERE checkout_id='work' AND name='checkout-hash' AND value=?2",
                vec![Value::Text(name.into()), Value::Text(prior.into())],
                1,
            ),
            (
                "UPDATE event SET mtime=?1,omtime=?1 WHERE objid=?2",
                vec![Value::Real(day), Value::Integer(rid)],
                1,
            ),
            (
                "UPDATE plink SET mtime=?1 WHERE cid=?2",
                vec![Value::Real(day), Value::Integer(rid)],
                1,
            ),
            (
                "UPDATE tagxref SET mtime=?1 WHERE srcid=?2",
                vec![Value::Real(day), Value::Integer(rid)],
                3,
            ),
        ] {
            assert_eq!(tx.execute_bound(sql, args).await.unwrap(), count);
        }
    }

    /// Run one independent replay trial and verify its complete fixture remains unchanged.
    /// # Arguments
    /// * `owner` - Retained fixture owner; this private control does not reuse its admission proof.
    /// * `shared` - Original unchanged immutable-root identity from the real fixture.
    /// * `decoded` - Fresh trial's finite cumulative decoded allowance.
    /// * `snapshot` - Complete intentionally chronology-invalid fixture table values.
    /// # Returns
    /// True only when replay reaches the exact chronology refusal after its reservation.
    /// # Panics
    /// Panics on any other failure, accepted history or persistent fixture mutation.
    async fn replay_reaches_date_guard(
        owner: &mut LocalOwnerRepository,
        shared: &str,
        decoded: u64,
        snapshot: &[(String, Vec<Vec<Value>>)],
    ) -> bool {
        let b = bounds();
        let limits = AdmissionLimits {
            content: Limits {
                max_total_decoded_bytes: decoded,
                ..b.content
            },
            ..b
        };
        let mut tx = owner.store.begin_write().await.unwrap();
        let mut work = Budget::new(limits.content).unwrap();
        let mut parse = ParseWork::new(limits.parser).unwrap();
        let result = tx
            .validate_recorded_history(&owner.principal, shared, limits, &mut work, &mut parse)
            .await;
        let error = tx
            .record(result)
            .err()
            .expect("chronology-invalid replay must refuse");
        assert!(
            matches!(
                error.operation.as_str(),
                "total decoded content limit" | "recording SQL parent date ordering"
            ),
            "unexpected replay guard: {}",
            error.operation
        );
        let rollback = tx.rollback().await.unwrap_err();
        assert_eq!(rollback.operation, error.operation);
        assert!(rollback.cleanup.is_none());
        // The failed operation is latched; inspect only after successful cleanup.
        let mut tx = owner.store.begin_write().await.unwrap();
        assert_eq!(snapshot_tables(&mut tx).await.as_slice(), snapshot);
        tx.rollback().await.unwrap();
        error.operation == "recording SQL parent date ordering"
    }

    #[tokio::test]
    /// Real replay reaches its reservation boundary without copying its debit expression.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after the adjacent budget/date outcomes and exact original fixture restoration.
    /// # Panics
    /// Panics on invalid native fixture syntax, a different guard, mutation or failed reopen.
    async fn replay_reservation_precedes_chronology_without_resetting_work() {
        use crate::storage::{
            PathScope, PathScopeSelection, PrivateRecordingRequest, TrackingLimits,
        };
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        std::fs::write(root.join("a"), b"payload").unwrap();
        let mode = std::fs::metadata(root.join("a"))
            .unwrap()
            .permissions()
            .mode();
        let b = bounds();
        let tracking = TrackingLimits {
            paths: 8,
            path_bytes: 4096,
            components: 32,
            retained_bytes: 1 << 20,
            io_buffer_bytes: 1024,
        };
        let mut owner = initialized(&repo).await;
        let view = owner.register_checkout("work", &root, b).await.unwrap();
        let view = owner
            .add_files("work", &["a"], &view, b, tracking)
            .await
            .unwrap();
        let view = owner
            .select_path_scopes(
                "work",
                &view,
                &[PathScopeSelection {
                    path: "a".into(),
                    scope: PathScope::Private,
                }],
                b,
                tracking,
            )
            .await
            .unwrap();
        owner
            .commit_private_tracked_files(
                "work",
                PrivateRecordingRequest {
                    expected: &view,
                    paths: &["a"],
                    scope: PathScope::Private,
                    actor: b"Owner",
                    comment: b"replay",
                    date: b"2026-11-20T00:00:01",
                },
                b,
                tracking,
            )
            .await
            .unwrap();
        let view = owner.checkout_view("work", b).await.unwrap();
        let original_name = view
            .combined_head
            .as_ref()
            .expect("real private selection")
            .clone();
        let mut tx = owner.store.begin_write().await.unwrap();
        let original = snapshot_tables(&mut tx).await;
        let history = tx
            .validate_recorded_history(
                &owner.principal,
                &view.shared_head,
                b,
                &mut Budget::new(b.content).unwrap(),
                &mut ParseWork::new(b.parser).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(history.commits.len(), 1);
        let selected = &history.commits[0];
        let rid = selected.rid;
        let original_day = selected.julian;
        let selected_rows = tx
            .query_bound(
                "SELECT size,content FROM blob WHERE rid=?1",
                vec![Value::Integer(rid)],
            )
            .await
            .unwrap();
        assert_eq!(selected_rows.len(), 1);
        assert_eq!(selected_rows[0].len(), 2);
        let original_size = *selected_rows[0][0]
            .as_integer()
            .expect("native manifest size");
        let original_packed = selected_rows[0][1]
            .as_blob()
            .expect("native manifest envelope")
            .clone();
        let (_, _, bytes) = tx
            .recorded_content(
                &owner.principal,
                &original_name,
                &mut Budget::new(b.content).unwrap(),
            )
            .await
            .unwrap();
        let text = String::from_utf8(bytes).unwrap();
        assert_eq!(
            text.lines().filter(|line| line.starts_with("D ")).count(),
            1
        );
        let root_date = std::str::from_utf8(&history.shared.date).unwrap();
        let cards: Vec<_> = text
            .lines()
            .filter(|line| !line.starts_with("Z "))
            .map(|line| {
                if line.starts_with("D ") {
                    format!("D {root_date}")
                } else {
                    line.to_owned()
                }
            })
            .collect();
        let body = format!("{}\n", cards.join("\n"));
        let checksum = crate::content_hash::hex(&crate::artifact_checksum::md5(body.as_bytes()));
        let changed = format!("{body}Z {checksum}\n").into_bytes();
        let changed_name =
            crate::content_hash::hex(&crate::content_hash::sha3(&changed, 256).unwrap());
        let artifact =
            crate::artifact::parse_artifact(&changed, b.parser, &super::super::OwnerDates(&tx))
                .await
                .unwrap();
        assert_eq!(artifact.date(), Some(history.shared.julian));
        assert_eq!(artifact.files().len(), 1);
        let packed = crate::compression::pack(&changed, -1).unwrap();
        // Only the parent-date relationship is intentionally invalid; identities,
        // receive provenance, selection and time projections still match the artifact.
        rewrite_replay_fixture(
            &mut tx,
            rid,
            &original_name,
            &changed_name,
            changed.len() as i64,
            &packed,
            history.shared.julian,
        )
        .await;
        let unsupported = snapshot_tables(&mut tx).await;
        tx.commit().await.unwrap();
        let mut low = 1;
        let mut high = b.content.max_total_decoded_bytes;
        assert!(!replay_reaches_date_guard(&mut owner, &view.shared_head, low, &unsupported).await);
        assert!(replay_reaches_date_guard(&mut owner, &view.shared_head, high, &unsupported).await);
        // Each midpoint is a separate transaction and operation, never a reset ledger.
        while high - low > 1 {
            let middle = low + (high - low) / 2;
            if replay_reaches_date_guard(&mut owner, &view.shared_head, middle, &unsupported).await
            {
                high = middle;
            } else {
                low = middle;
            }
        }
        assert_eq!(low + 1, high);
        assert!(
            !replay_reaches_date_guard(&mut owner, &view.shared_head, high - 1, &unsupported).await
        );
        assert!(replay_reaches_date_guard(&mut owner, &view.shared_head, high, &unsupported).await);
        assert!(
            replay_reaches_date_guard(&mut owner, &view.shared_head, high + 1, &unsupported).await
        );
        let mut tx = owner.store.begin_write().await.unwrap();
        rewrite_replay_fixture(
            &mut tx,
            rid,
            &changed_name,
            &original_name,
            original_size,
            &original_packed,
            original_day,
        )
        .await;
        assert_eq!(snapshot_tables(&mut tx).await, original);
        tx.commit().await.unwrap();
        drop(owner);
        let mut reopened = LocalOwnerRepository::open(&repo, b).await.unwrap();
        assert_eq!(reopened.checkout_view("work", b).await.unwrap(), view);
        let mut tx = reopened.store.begin_write().await.unwrap();
        assert_eq!(snapshot_tables(&mut tx).await, original);
        tx.rollback().await.unwrap();
        assert_eq!(std::fs::read(root.join("a")).unwrap(), b"payload");
        assert_eq!(
            std::fs::metadata(root.join("a"))
                .unwrap()
                .permissions()
                .mode(),
            mode
        );
    }

    #[test]
    /// Pending reference collection preserves order, borrowing and every-row cumulative charges.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after mixed, recorded-only, empty and exhausted-work controls.
    /// # Panics
    /// Panics on changed membership, cloned names or reset/omitted accounting.
    fn pending_references_keep_order_and_share_the_ledger() {
        let rows = vec![
            vec![
                Value::Integer(1),
                Value::Text("recorded".into()),
                Value::Integer(4),
                Value::Integer(0),
            ],
            vec![
                Value::Integer(2),
                Value::Text("z-new".into()),
                Value::Integer(0),
                Value::Integer(0),
            ],
            vec![
                Value::Integer(3),
                Value::Text("a-new".into()),
                Value::Integer(0),
                Value::Integer(1),
            ],
        ];
        for row in &rows {
            tracked_facts(row).unwrap();
        }
        let budget = |total| {
            Budget::new(Limits {
                max_total_decoded_bytes: total,
                ..bounds().content
            })
            .unwrap()
        };
        let references = collect_pending_references(&rows, &mut budget(1024)).unwrap();
        assert_eq!(
            references.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            ["z-new", "a-new"]
        );
        assert!(std::ptr::eq(references[0], rows[1][1].as_text().unwrap()));
        assert!(std::ptr::eq(references[1], rows[2][1].as_text().unwrap()));
        let threshold = (1..=1024)
            .find(|total| collect_pending_references(&rows, &mut budget(*total)).is_ok())
            .unwrap();
        let mut recorded = rows.clone();
        for row in &mut recorded {
            row[2] = Value::Integer(4);
        }
        let mut work = budget(threshold * 2 - 1);
        assert!(
            collect_pending_references(&recorded, &mut work)
                .unwrap()
                .is_empty()
        );
        refuses(
            collect_pending_references(&rows, &mut work),
            "total decoded content limit",
        );
        let mut work = budget(threshold);
        assert!(
            collect_pending_references(&[], &mut work)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            collect_pending_references(&rows, &mut work).unwrap(),
            references
        );
        let mut work = budget(threshold);
        work.charge_decoded((threshold - 1) as usize).unwrap();
        refuses(
            collect_pending_references(&rows, &mut work),
            "total decoded content limit",
        );
    }

    #[test]
    /// Complete result and aggregate decoders reject coercions and preserve exact bounds.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after cardinality, types, signed values and all three ceilings are checked.
    /// # Panics
    /// Panics if a malformed aggregate/result is accepted or the boundary is exclusive.
    fn retained_rows_and_preflight_facts_are_exact() {
        let rows = vec![vec![Value::Text("a\0b".into()), Value::Integer(9)]];
        validate_retained_rows(&rows, 1, 11).unwrap();
        refuses(
            validate_retained_rows(&rows, 0, 11),
            "recording SQL changed",
        );
        refuses(
            validate_retained_rows(&rows, 1, 10),
            "recording SQL width limit",
        );
        validate_retained_rows(&[], 0, 1).unwrap();
        let mut b = bounds();
        b.parser.entries = 2;
        b.parser.input_bytes = 3;
        b.content.max_total_decoded_bytes = 6;
        assert_eq!(
            preflight_text_total(
                &[vec![
                    Value::Integer(2),
                    Value::Integer(3),
                    Value::Integer(6)
                ]],
                b
            )
            .unwrap(),
            6
        );
        for facts in [
            [-1, 0, 0],
            [0, -1, 0],
            [0, 0, -1],
            [3, 0, 0],
            [0, 4, 0],
            [0, 0, 7],
        ] {
            refuses(
                preflight_text_total(&[facts.into_iter().map(Value::Integer).collect()], b),
                "recording text preflight limit",
            );
        }
        for rows in [vec![], vec![vec![], vec![]]] {
            refuses(
                preflight_text_total(&rows, b),
                "recording text preflight cardinality",
            );
        }
        for row in [
            vec![],
            vec![Value::Integer(0)],
            vec![Value::Integer(0), Value::Real(0.0), Value::Integer(0)],
            vec![
                Value::Integer(0),
                Value::Integer(0),
                Value::Text("0".into()),
            ],
        ] {
            refuses(
                preflight_text_total(&[row], b),
                "recording text preflight type",
            );
        }
    }

    #[test]
    /// Positive native IDs and canonical replicas never accept SQL affinity coercion.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after exact identity and grammar controls.
    /// # Panics
    /// Panics on an accepted invalid identity or a rejected valid boundary.
    fn content_and_replica_decoders_are_exact() {
        assert_eq!(
            content_identity(&[vec![Value::Integer(1), Value::Integer(1)]]).unwrap(),
            (1, 1)
        );
        for rows in [vec![], vec![vec![], vec![]]] {
            refuses(
                content_identity(&rows),
                "recording full content unavailable",
            );
            refuses(replica_identity(&rows), "recording replica missing");
        }
        for row in [
            vec![],
            vec![Value::Integer(1)],
            vec![Value::Integer(1), Value::Text("1".into())],
            vec![Value::Integer(0), Value::Integer(1)],
            vec![Value::Integer(1), Value::Integer(-1)],
        ] {
            refuses(content_identity(&[row]), "recording content identity");
        }
        for row in [vec![], vec![Value::Null], vec![Value::Blob(vec![b'a'; 40])]] {
            refuses(replica_identity(&[row]), "recording replica type");
        }
        let valid = vec![vec![Value::Text(
            "0123456789abcdef0123456789abcdef01234567".into(),
        )]];
        assert_eq!(replica_identity(&valid).unwrap().len(), 40);
        for name in [
            "a".repeat(39),
            "a".repeat(41),
            "A".repeat(40),
            "g".repeat(40),
            "\0".repeat(40),
        ] {
            refuses(
                replica_identity(&[vec![Value::Text(name)]]),
                "recording replica grammar",
            );
        }
    }

    #[test]
    /// Checkout and tracked row decoders preserve native shapes and counter signs.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after null/text heads, counters and all tracked scalar types are checked.
    /// # Panics
    /// Panics on a coerced fact or invalid accepted counter.
    fn checkout_and_tracked_facts_are_exact() {
        let valid = vec![
            Value::Text("work".into()),
            Value::Null,
            Value::Integer(0),
            Value::Integer(0),
        ];
        let rows = vec![valid.clone()];
        let facts = checkout_facts(&rows).unwrap();
        assert_eq!(facts.checkout_id, "work");
        assert_eq!(facts.cursor, None);
        assert_eq!((*facts.generation, *facts.pending), (0, 0));
        let mut head = valid.clone();
        head[1] = Value::Text("selected".into());
        assert_eq!(
            checkout_facts(&[head]).unwrap().cursor.as_deref(),
            Some("selected")
        );
        for rows in [vec![], vec![valid.clone(), valid.clone()]] {
            refuses(checkout_facts(&rows), "recording sole checkout required");
        }
        refuses(checkout_facts(&[vec![]]), "recording checkout shape");
        for index in [0, 2, 3] {
            let mut row = valid.clone();
            row[index] = Value::Null;
            refuses(checkout_facts(&[row]), "recording checkout shape");
        }
        for index in [2, 3] {
            let mut row = valid.clone();
            row[index] = Value::Integer(-1);
            refuses(checkout_facts(&[row]), "recording counters");
        }
        let mut row = valid;
        row[1] = Value::Integer(1);
        refuses(checkout_facts(&[row]), "recording head type");
        let tracked = vec![
            Value::Integer(1),
            Value::Text("a".into()),
            Value::Integer(2),
            Value::Integer(1),
        ];
        let facts = tracked_facts(&tracked).unwrap();
        assert_eq!(
            (*facts.id, facts.path.as_str(), *facts.rid, *facts.mode),
            (1, "a", 2, 1)
        );
        for index in 0..4 {
            let mut row = tracked.clone();
            row[index] = Value::Null;
            refuses(tracked_facts(&row), "recording tracked row type");
        }
        refuses(tracked_facts(&[]), "recording tracked row type");
    }

    #[test]
    /// Immutable payload provenance and complete receipt sequence are independently checked.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after first/reused/future payload and ordered local-origin receipt controls.
    /// # Panics
    /// Panics if future references, missing receipts or changed origin policy are admitted.
    fn receipt_policies_are_exact() {
        for (seen, receipt, commit) in [(false, 2, 2), (true, 1, 2), (true, 2, 2)] {
            validate_file_receipt(seen, receipt, commit).unwrap();
        }
        for receipt in [1, 3] {
            refuses(
                validate_file_receipt(false, receipt, 2),
                "recording first-use receipt mismatch",
            );
        }
        refuses(
            validate_file_receipt(true, 3, 2),
            "recording file future receipt",
        );
        let root = vec![Value::Integer(1)];
        let local = vec![
            Value::Integer(2),
            Value::Integer(0),
            Value::Text("real".into()),
            Value::Null,
            Value::Null,
            Value::Integer(1),
        ];
        validate_receipts(&[root.clone(), local.clone()], 1, &[2]).unwrap();
        validate_receipts(std::slice::from_ref(&root), 1, &[]).unwrap();
        // Two owners of one receipt, or an owner past the last receipt, leave one unexplained.
        let mut third = local.clone();
        third[0] = Value::Integer(3);
        for owners in [&[2, 2][..], &[2, 4], &[3]] {
            let rows = [root.clone(), local.clone(), third.clone()];
            refuses(
                validate_receipts(&rows[..owners.len() + 1], 1, owners),
                "recording unexplained receipt",
            );
        }
        refuses(
            validate_receipts(&[], 1, &[]),
            "recording unexplained receipt",
        );
        refuses(
            validate_receipts(&[vec![Value::Integer(2)]], 1, &[]),
            "recording root receipt",
        );
        for (index, value) in [
            (0, Value::Integer(3)),
            (1, Value::Integer(1)),
            (2, Value::Text("text".into())),
            (3, Value::Text("nonce".into())),
            (4, Value::Text("ip".into())),
            (5, Value::Integer(0)),
        ] {
            let mut row = local.clone();
            row[index] = value;
            refuses(
                validate_receipts(&[root.clone(), row], 1, &[2]),
                "recording local receipt policy",
            );
        }
        let mut integer_time = local;
        integer_time[2] = Value::Text("integer".into());
        validate_receipts(&[root, integer_time], 1, &[2]).unwrap();
    }

    #[test]
    /// Fingerprint conversions retain native NUL behavior and debit the borrowed ledger.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after exact checksum, conversion-shape and remaining-capacity controls.
    /// # Panics
    /// Panics on witness replacement, accepted NULL conversions or a reset budget.
    fn fingerprint_conversion_is_exact_and_cumulative() {
        let content = bounds().content;
        let row = vec![
            Value::Blob(b"0".to_vec()),
            Value::Blob(b"date\0ignored".to_vec()),
            Value::Blob(b"NULL".to_vec()),
            Value::Blob(b"NULL".to_vec()),
        ];
        let mut work = Budget::new(content).unwrap();
        assert_eq!(
            fingerprint_from_conversion(std::slice::from_ref(&row), 7, &mut work).unwrap(),
            format!(
                "7/{}",
                crate::content_hash::hex(&crate::artifact_checksum::md5(b"0dateNULLNULL"))
            )
        );
        for rows in [vec![], vec![row.clone(), row.clone()]] {
            refuses(
                fingerprint_from_conversion(&rows, 7, &mut work),
                "recording fingerprint receipt missing",
            );
        }
        refuses(
            fingerprint_from_conversion(&[vec![]], 7, &mut work),
            "recording fingerprint receipt shape",
        );
        for index in 0..4 {
            let mut bad = row.clone();
            bad[index] = Value::Null;
            refuses(
                fingerprint_from_conversion(&[bad], 7, &mut work),
                "recording fingerprint conversion unavailable",
            );
        }
        let debit = 2 * (1 + 12 + 4 + 4) + 64;
        let limits = Limits {
            max_total_decoded_bytes: debit,
            ..content
        };
        let mut exact = Budget::new(limits).unwrap();
        fingerprint_from_conversion(std::slice::from_ref(&row), 7, &mut exact).unwrap();
        refuses(exact.charge_decoded(1), "total decoded content limit");
        let mut short = Budget::new(limits).unwrap();
        short.charge_decoded((debit - 1) as usize).unwrap();
        refuses(
            fingerprint_from_conversion(&[row], 7, &mut short),
            "total decoded content limit",
        );
    }

    #[tokio::test]
    /// Real bounded SQL, hash and preflight helpers honor finite and predebited limits.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after exact boundaries and unchanged persistent blob inventory are checked.
    /// # Panics
    /// Panics on an accepted invalid bound, reset allowance or fixture mutation.
    async fn real_helper_bounds_preserve_inventory() {
        let temp = tempfile::tempdir().unwrap();
        let mut owner = initialized(&temp.path().join("repo")).await;
        let b = bounds();
        let mut tx = owner.store.begin_write().await.unwrap();
        let before = tx.query("SELECT * FROM blob ORDER BY rid").await.unwrap();
        let mut work = Budget::new(b.content).unwrap();
        for (count, width) in [(0, 8), (1, 0)] {
            refuses(
                bounded_rows(&mut tx, "SELECT 1", vec![], count, width, &mut work).await,
                "recording SQL limits",
            );
        }
        assert_eq!(
            bounded_rows(&mut tx, "SELECT 1", vec![], 1, 8, &mut work)
                .await
                .unwrap(),
            vec![vec![Value::Integer(1)]]
        );
        refuses(
            bounded_rows(&mut tx, "SELECT 1", vec![], 1, 7, &mut work).await,
            "recording SQL width limit",
        );
        let tiny = Limits {
            max_total_decoded_bytes: 1,
            ..b.content
        };
        refuses(
            bounded_rows(
                &mut tx,
                "SELECT 1",
                vec![],
                1,
                8,
                &mut Budget::new(tiny).unwrap(),
            )
            .await,
            "total decoded content limit",
        );
        let exact = Limits {
            max_total_decoded_bytes: 67,
            ..b.content
        };
        let mut hash_work = Budget::new(exact).unwrap();
        assert_eq!(
            native_name(b"abc", &mut hash_work).unwrap(),
            "3a985da74fe225b2045c172d6bd390bd855f086e3e9d525b46bfe24511431532"
        );
        refuses(hash_work.charge_decoded(1), "total decoded content limit");
        let mut short = Budget::new(exact).unwrap();
        short.charge_decoded(1).unwrap();
        refuses(
            native_name(b"abc", &mut short),
            "total decoded content limit",
        );
        tx.recording_preflight(b, &mut work).await.unwrap();
        for (input_bytes, entries) in [(0, b.parser.entries), (b.parser.input_bytes, 0)] {
            let limits = AdmissionLimits {
                parser: crate::artifact::ParseLimits {
                    input_bytes,
                    entries,
                    ..b.parser
                },
                ..b
            };
            refuses(
                tx.recording_preflight(limits, &mut work).await,
                "recording admission limits",
            );
        }
        let low = AdmissionLimits {
            parser: crate::artifact::ParseLimits {
                input_bytes: 1,
                ..b.parser
            },
            ..b
        };
        refuses(
            tx.recording_preflight(low, &mut work).await,
            "recording text preflight limit",
        );
        // This exact first-table SQL debit leaves no room for its nonempty UUID sum.
        let sql = "SELECT count(*),coalesce(max(coalesce(length(CAST(uuid AS BLOB)),0)),0),coalesce(sum(coalesce(length(CAST(uuid AS BLOB)),0)),0) FROM blob";
        let first = Limits {
            max_total_decoded_bytes: (sql.len() + 96) as u64,
            ..b.content
        };
        refuses(
            tx.recording_preflight(b, &mut Budget::new(first).unwrap())
                .await,
            "total decoded content limit",
        );
        assert_eq!(
            tx.query("SELECT * FROM blob ORDER BY rid").await.unwrap(),
            before
        );
        tx.rollback().await.unwrap();
    }

    #[tokio::test]
    /// Full-content lookup refuses unavailable native identities without rewriting rows.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after canonical, malformed, missing and delta-backed identity controls.
    /// # Panics
    /// Panics on admitted unavailable content, the wrong cause or a persistent repair.
    async fn full_content_lookup_preserves_native_identity() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let mut owner = initialized(&repo).await;
        let mut tx = owner.store.begin_write().await.unwrap();
        let before = tx.query("SELECT * FROM blob ORDER BY rid").await.unwrap();
        let mut work = Budget::new(bounds().content).unwrap();
        let mut names = tx
            .connection()
            .query("SELECT uuid FROM blob WHERE rid=1", ())
            .await
            .unwrap();
        let name = names
            .next()
            .await
            .unwrap()
            .unwrap()
            .get::<String>(0)
            .unwrap();
        assert!(names.next().await.unwrap().is_none());
        drop(names);
        let (rid, receipt, bytes) = tx
            .recorded_content(&owner.principal, &name, &mut work)
            .await
            .unwrap();
        assert_eq!((rid, receipt), (1, 1));
        assert!(!bytes.is_empty());
        for invalid in ["".into(), "A".repeat(64), "g".repeat(40), "a".repeat(63)] {
            refuses(
                tx.recorded_content(&owner.principal, &invalid, &mut work)
                    .await,
                "recording native name grammar",
            );
        }
        refuses(
            tx.recorded_content(&owner.principal, &"0".repeat(64), &mut work)
                .await,
            "recording full content unavailable",
        );
        tx.execute("INSERT INTO delta(rid,srcid) VALUES(1,1)")
            .await
            .unwrap();
        refuses(
            tx.recorded_content(&owner.principal, &name, &mut work)
                .await,
            "recording full content unavailable",
        );
        assert_eq!(
            tx.query("SELECT * FROM blob ORDER BY rid").await.unwrap(),
            before
        );
        assert_eq!(
            tx.query("SELECT rid,srcid FROM delta").await.unwrap(),
            vec![vec![Value::Integer(1), Value::Integer(1)]]
        );
        tx.rollback().await.unwrap();
        drop(owner);
        drop(LocalOwnerRepository::open(&repo, bounds()).await.unwrap());
    }

    #[test]
    /// Scalar decoders preserve exact native type and cardinality requirements.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit after every malformed provider shape refuses.
    /// # Panics
    /// Panics if a coercion or ambiguous cardinality is accepted.
    fn scalar_is_exact() {
        assert_eq!(scalar_i64(&[vec![Value::Integer(2)]]).unwrap(), 2);
        for rows in [
            vec![],
            vec![vec![]],
            vec![vec![Value::Null]],
            vec![vec![Value::Real(2.0)]],
            vec![vec![Value::Text("2".into())]],
            vec![vec![Value::Integer(1)], vec![Value::Integer(2)]],
        ] {
            assert!(scalar_i64(&rows).is_err());
        }
    }

    #[tokio::test]
    /// Generic fingerprint verification uses its embedded historical witness.
    /// # Arguments
    /// No arguments.
    /// # Returns
    /// Unit when old-witness algorithm compatibility remains independent of policy.
    /// # Panics
    /// Panics on latest-witness substitution, noncanonical spelling or fixture failure.
    /// The extra receipt is explicit unit fixture SQL, never a produced/admitted history.
    async fn generic_fingerprint_verifies_exact_embedded_receipt() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let content = Limits {
            max_stored_bytes: 1 << 20,
            max_output_bytes: 1 << 20,
            max_chain_depth: 128,
            max_total_stored_bytes: 64 << 20,
            max_total_decoded_bytes: 256 << 20,
        };
        let parser = crate::artifact::ParseLimits {
            input_bytes: 4 << 20,
            cards: 65536,
            owned_bytes: 16 << 20,
            entries: 8192,
        };
        let mut owner = LocalOwnerRepository::initialize(
            &repo,
            InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "owner",
                date: "2026-11-20T00:00:00",
                user: b"Owner",
                password: "secret",
                sha1: false,
                content_limits: content,
                parse_limits: parser,
            },
        )
        .await
        .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        let mut work = Budget::new(content).unwrap();
        let first = tx.recording_fingerprint(1, 4096, &mut work).await.unwrap();
        for receipt in [0, -1, i64::from(i32::MAX) + 1] {
            refuses(
                tx.recording_fingerprint(receipt, 4096, &mut work).await,
                "recording fingerprint witness",
            );
        }
        refuses(
            tx.recording_fingerprint(1, 1, &mut work).await,
            "recording fingerprint conversion bound",
        );
        let sizes = tx.query("SELECT coalesce(length(CAST(uid AS BLOB)),0)+coalesce(length(CAST(mtime AS BLOB)),0)+coalesce(length(CAST(nonce AS BLOB)),0)+coalesce(length(CAST(ipaddr AS BLOB)),0) FROM rcvfrom WHERE rcvid=1").await.unwrap();
        let bound = (scalar_i64(&sizes).unwrap() as usize) * 2 + 128;
        assert_eq!(
            tx.recording_fingerprint(1, bound, &mut work).await.unwrap(),
            first
        );
        refuses(
            tx.recording_fingerprint(1, bound - 1, &mut work).await,
            "recording fingerprint conversion bound",
        );

        tx.execute(
            "INSERT INTO rcvfrom(uid,mtime,nonce,ipaddr) VALUES(0,julianday('now'),NULL,NULL)",
        )
        .await
        .unwrap();
        assert_eq!(
            tx.verify_recording_fingerprint(&first, 4096, &mut work)
                .await
                .unwrap(),
            1
        );
        assert!(
            tx.verify_recording_fingerprint(
                &format!("01/{}", first.split_once('/').unwrap().1),
                4096,
                &mut work
            )
            .await
            .is_err()
        );
        assert!(
            tx.verify_recording_fingerprint(
                "999/00000000000000000000000000000000",
                4096,
                &mut work
            )
            .await
            .is_err()
        );
        tx.execute("UPDATE rcvfrom SET mtime=1e100 WHERE rcvid=2")
            .await
            .unwrap();
        let before = tx
            .query("SELECT * FROM rcvfrom ORDER BY rcvid")
            .await
            .unwrap();
        refuses(
            tx.recording_fingerprint(2, 4096, &mut work).await,
            "recording fingerprint conversion unavailable",
        );
        assert_eq!(
            tx.query("SELECT * FROM rcvfrom ORDER BY rcvid")
                .await
                .unwrap(),
            before
        );
        tx.rollback().await.unwrap();
    }
}
