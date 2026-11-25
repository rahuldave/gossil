//! Bounded native envelopes and reconstruction accounting.
//!
//! ## Recommendations
//! Supply finite operation budgets and verify every reconstructed intermediate.
use super::{StorageError, WriteTransaction, metered_query::SqlWork, policy::Principal};
use crate::compression::{self, Unpacked};
use libsql::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Finite caller-selected limits; no implicit unbounded fallback exists.
#[derive(Clone, Copy)]
pub struct Limits {
    /// Largest compressed envelope accepted for one RID, before reading its payload.
    pub max_stored_bytes: u64,
    /// Largest decoded envelope or reconstructed blob; at most i32::MAX.
    pub max_output_bytes: u32,
    /// Largest number of distinct RIDs traversed in one chain; positive.
    pub max_chain_depth: u32,
    /// Total compressed envelope bytes consumed by the operation; positive.
    pub max_total_stored_bytes: u64,
    /// Total decoded content and reconstructed intermediates; positive.
    /// Catalog6 operations also charge metadata, parser, capture and retained SQL work.
    pub max_total_decoded_bytes: u64,
}
impl Limits {
    /// Validate before loading or allocating repository bytes.
    /// # Returns
    /// Validated finite operation limits.
    /// # Errors
    /// Rejects zero stored, depth or aggregate ceilings, or an output ceiling above i32::MAX.
    fn validate(self) -> Result<Self, StorageError> {
        if self.max_stored_bytes == 0
            || self.max_output_bytes > i32::MAX as u32
            || self.max_chain_depth == 0
            || self.max_total_stored_bytes == 0
            || self.max_total_decoded_bytes == 0
        {
            return Err(StorageError::state("invalid finite content limits"));
        }
        Ok(self)
    }
}

/// One complete operation's work accounting, including every traversed RID.
pub(super) struct Budget {
    /// Validated caller ceilings for this operation.
    limits: Limits,
    /// Aggregate compressed bytes charged so far.
    stored: u64,
    /// Aggregate decoded and reconstructed bytes charged so far.
    decoded: u64,
    /// Native row identifiers already visited in this reconstruction.
    visited: BTreeSet<i64>,
    /// The largest reservation charged for each whole shared surface since the current admission began.
    reserved: BTreeMap<&'static str, u64>,
    /// The table comparisons with replay that passed on this ledger.
    compared: usize,
}
impl Budget {
    /// Bound a new compression result by both remaining work ledgers.
    /// # Returns
    /// Largest packed output that fits the envelope, stored and ownership ceilings.
    /// The caller must still debit actual stored bytes and owned bind bytes afterward.
    pub(super) fn packed_output_ceiling(&self) -> usize {
        usize::try_from(
            self.limits
                .max_stored_bytes
                .min(
                    self.limits
                        .max_total_stored_bytes
                        .saturating_sub(self.stored),
                )
                .min(
                    self.limits
                        .max_total_decoded_bytes
                        .saturating_sub(self.decoded),
                ),
        )
        .unwrap_or(usize::MAX)
    }

    /// Charge a newly compressed output without entering a reconstruction chain.
    /// # Arguments
    /// * `length` - Complete compressed envelope length.
    /// # Returns
    /// Success when individual and cumulative stored ceilings admit the output.
    /// # Errors
    /// Refuses an oversized envelope, arithmetic overflow or exhausted stored work.
    pub(super) fn charge_stored(&mut self, length: usize) -> Result<(), StorageError> {
        let length = u64::try_from(length)
            .map_err(|_| StorageError::state("stored content size overflow"))?;
        if length > self.limits.max_stored_bytes {
            return Err(StorageError::state("stored content limit"));
        }
        self.stored = self
            .stored
            .checked_add(length)
            .filter(|total| *total <= self.limits.max_total_stored_bytes)
            .ok_or_else(|| StorageError::state("total stored content limit"))?;
        Ok(())
    }
    /// Start empty accounting after validating every finite ceiling.
    /// # Arguments
    /// * `limits` - Explicit finite bounds for this operation.
    /// # Returns
    /// Empty accounting with validated finite limits.
    /// # Errors
    /// Rejects invalid finite limits.
    pub(super) fn new(limits: Limits) -> Result<Self, StorageError> {
        Ok(Self {
            limits: limits.validate()?,
            stored: 0,
            decoded: 0,
            visited: BTreeSet::new(),
            reserved: BTreeMap::new(),
            compared: 0,
        })
    }
    /// Charge before consuming an envelope; a repeated RID always fails.
    /// # Arguments
    /// * `rid` - Native artifact row identifier.
    /// * `length` - Byte length to charge to this budget.
    /// # Returns
    /// Success after charging one previously unseen native row.
    /// # Errors
    /// Rejects invalid or repeated RIDs, chain depth, stored envelope size or aggregate stored work.
    fn enter(&mut self, rid: i64, length: u64) -> Result<(), StorageError> {
        if rid <= 0 || self.visited.contains(&rid) {
            return Err(StorageError::state("content delta cycle or invalid RID"));
        }
        if self.visited.len() as u64 >= u64::from(self.limits.max_chain_depth) {
            return Err(StorageError::state("content chain depth limit"));
        }
        if length > self.limits.max_stored_bytes {
            return Err(StorageError::state("stored content limit"));
        }
        self.stored = self
            .stored
            .checked_add(length)
            .filter(|n| *n <= self.limits.max_total_stored_bytes)
            .ok_or_else(|| StorageError::state("total stored content limit"))?;
        self.visited.insert(rid);
        Ok(())
    }
    /// Charge decompressed instructions and all reconstructed intermediate bytes.
    /// # Arguments
    /// * `length` - Byte length to charge to this budget.
    /// # Returns
    /// Success after charging decoded work within the total ceiling.
    /// # Errors
    /// Rejects aggregate decoded work overflow or a total above the caller ceiling.
    pub(super) fn charge_decoded(&mut self, length: usize) -> Result<(), StorageError> {
        self.decoded = self
            .decoded
            .checked_add(length as u64)
            .filter(|n| *n <= self.limits.max_total_decoded_bytes)
            .ok_or_else(|| StorageError::state("total decoded content limit"))?;
        Ok(())
    }
    /// Return the decoded work charged so far, for the unit controls of admission cost.
    /// # Returns
    /// The aggregate decoded bytes charged to this ledger.
    #[cfg(test)]
    pub(super) fn decoded(&self) -> u64 {
        self.decoded
    }
    /// Count one table comparison with replay that passed.
    pub(super) fn count_comparison(&mut self) {
        self.compared = self.compared.saturating_add(1);
    }
    /// Return the table comparisons with replay that passed on this ledger.
    /// # Returns
    /// The count, which `gos v verify` prints.
    pub(super) fn compared(&self) -> usize {
        self.compared
    }
    /// Start a new admission, so that each shared surface is reserved again, once.
    /// # Returns
    /// Nothing. The work already charged stays charged.
    pub(super) fn begin_admission(&mut self) {
        self.reserved.clear();
    }
    /// Charge a reservation for one whole shared surface once per admission.
    ///
    /// A query that reads a whole surface, such as the repository text or the schema,
    /// holds its rows only until the next query. The first reservation of a surface
    /// after [`Self::begin_admission`] charges all of it. A later one charges only its
    /// growth over the largest reservation of that surface so far.
    /// # Arguments
    /// * `surface` - The fixed name of the surface.
    /// * `reservation` - Bytes that one read of the whole surface could hold now.
    /// # Returns
    /// Success after charging the growth.
    /// # Errors
    /// Refuses exhausted decoded work, and then charges and records nothing.
    pub(super) fn charge_reserved(
        &mut self,
        surface: &'static str,
        reservation: usize,
    ) -> Result<(), StorageError> {
        let reservation = reservation as u64;
        let held = self.reserved.get(surface).copied().unwrap_or(0);
        self.decoded = self
            .decoded
            .checked_add(reservation.saturating_sub(held))
            .filter(|n| *n <= self.limits.max_total_decoded_bytes)
            .ok_or_else(|| StorageError::state("total decoded content limit"))?;
        self.reserved.insert(surface, held.max(reservation));
        Ok(())
    }
    /// Bound the output of one delta application by both decoded ceilings.
    /// # Returns
    /// The smaller of the per-object output ceiling and the remaining decoded work.
    fn delta_output_ceiling(&self) -> u32 {
        let remaining = self
            .limits
            .max_total_decoded_bytes
            .saturating_sub(self.decoded);
        // The minimum is at most `max_output_bytes`, so it fits `u32`.
        u32::try_from(u64::from(self.limits.max_output_bytes).min(remaining)).unwrap_or(u32::MAX)
    }
    /// Preserve native capacity/trailing semantics while rejecting the short no-op.
    /// # Arguments
    /// * `bytes` - Input bytes for this operation.
    /// # Returns
    /// Owned unpacked bytes charged to this operation.
    /// # Errors
    /// Rejects malformed or short envelopes, decode limits and exhausted decoded work.
    fn unpack(&mut self, bytes: &[u8]) -> Result<Vec<u8>, StorageError> {
        let remaining = self
            .limits
            .max_total_decoded_bytes
            .checked_sub(self.decoded)
            .ok_or_else(|| StorageError::state("total decoded content limit"))?;
        let ceiling = u64::from(self.limits.max_output_bytes).min(remaining) as usize;
        let output = compression::unpack(bytes, ceiling).map_err(|error| {
            StorageError::caused(format!("native content envelope: {error}"), error)
        })?;
        match output {
            Unpacked::NoPayload => Err(StorageError::state(
                "native content envelope has no payload",
            )),
            Unpacked::Bytes(output) => {
                self.charge_decoded(output.len())?;
                Ok(output)
            }
        }
    }
}
/// Decode the fixed identity query into owned integers; reject duplicate or malformed engine rows.
/// # Arguments
/// * `rows` - Materialized native identity query rows.
/// # Returns
/// The native row identifier and exact size, if the name exists.
/// # Errors
/// Rejects duplicate identity rows or fields with unexpected native storage types.
fn content_identity(rows: &[Vec<Value>]) -> Result<Option<(i64, i64)>, StorageError> {
    match rows {
        [] => Ok(None),
        [row] => match row.as_slice() {
            [Value::Integer(rid), Value::Integer(size)] => Ok(Some((*rid, *size))),
            _ => Err(StorageError::state("corrupt native content identity")),
        },
        _ => Err(StorageError::state("duplicate native content identity")),
    }
}

/// Availability, shun state and delta baseline of one RID.
const AVAILABILITY_ROW: &str = "SELECT b.size,d.srcid,EXISTS(SELECT 1 FROM shun s WHERE s.uuid=b.uuid) FROM blob b LEFT JOIN delta d ON d.rid=b.rid WHERE b.rid=?1";
/// Identity, size, stored length and delta baseline of one unshunned name.
const CHAIN_ROW: &str = "SELECT b.rid,b.size,b.uuid,length(b.content),d.srcid FROM blob b LEFT JOIN delta d ON d.rid=b.rid WHERE b.uuid=?1 AND NOT EXISTS(SELECT 1 FROM shun WHERE uuid=b.uuid)";
/// The stored envelope of one RID.
const PAYLOAD_ROW: &str = "SELECT content FROM blob WHERE rid=?1";
/// The native name of one RID.
const NAME_ROW: &str = "SELECT uuid FROM blob WHERE rid=?1";
/// A shun row for one name.
const SHUN_ROW: &str = "SELECT 1 FROM shun WHERE uuid=?1";
/// The RID and size of one name.
const IDENTITY_ROW: &str = "SELECT rid,size FROM blob WHERE uuid=?1";
/// The RID of one name.
const RID_ROW: &str = "SELECT rid FROM blob WHERE uuid=?1";
/// Private membership for one RID, unless a public receipt names its content.
const PRIVATE_INSERT: &str = "INSERT OR IGNORE INTO private(rid) SELECT ?1 WHERE NOT EXISTS(SELECT 1 FROM g_public_receipt WHERE native_hash=?2)";
/// Every orphan, delta or required typed dependent of one RID and name.
const DEPENDENT_ROWS: &str = "SELECT 1 FROM orphan WHERE baseline=?1 UNION ALL SELECT 1 FROM delta WHERE srcid=?1 UNION ALL SELECT 1 FROM g_dependency WHERE target_hash=?2 AND required=1";

/// One step of an availability walk along an old delta chain.
#[derive(Debug, PartialEq, Eq)]
enum Availability {
    /// A missing row or a phantom stops reconstruction.
    Unavailable,
    /// A full envelope ends the chain.
    Full,
    /// A delta continues at this baseline RID.
    Delta(i64),
}

/// Decode one row of [`AVAILABILITY_ROW`].
/// # Arguments
/// * `rows` - Materialized availability rows of one RID.
/// # Returns
/// Whether the RID is unavailable, full or a delta, and its baseline.
/// # Errors
/// Rejects a shunned RID, a malformed row and a baseline that is not an integer.
fn availability(rows: &[Vec<Value>]) -> Result<Availability, StorageError> {
    let [row] = rows else {
        return Ok(Availability::Unavailable);
    };
    let [Value::Integer(size), source, Value::Integer(shunned)] = row.as_slice() else {
        return Err(StorageError::state("corrupt native availability row"));
    };
    if *shunned != 0 {
        return Err(StorageError::state("content unavailable"));
    }
    if *size < 0 {
        return Ok(Availability::Unavailable);
    }
    match source {
        Value::Null => Ok(Availability::Full),
        Value::Integer(source) => Ok(Availability::Delta(*source)),
        _ => Err(StorageError::state("corrupt delta baseline RID")),
    }
}

/// One stored link of a content chain, before reconstruction.
struct ChainLink {
    /// Native RID of the link.
    rid: i64,
    /// Exact size of the reconstructed bytes of the link.
    size: i64,
    /// Full native name of the reconstructed bytes.
    hash: String,
    /// Length of the stored envelope.
    length: u64,
    /// Delta baseline RID, or `None` for a full envelope.
    source: Option<i64>,
}

/// Decode one row of [`CHAIN_ROW`].
/// # Arguments
/// * `rows` - Materialized chain rows of one name.
/// # Returns
/// The stored link of that name.
/// # Errors
/// Rejects an absent name or phantom as unavailable, and a malformed row as corrupt.
fn chain_link(rows: &[Vec<Value>]) -> Result<ChainLink, StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("content unavailable"));
    };
    let [
        Value::Integer(rid),
        Value::Integer(size),
        Value::Text(hash),
        length,
        source,
    ] = row.as_slice()
    else {
        return Err(StorageError::state("corrupt native content row"));
    };
    if *size < 0 {
        return Err(StorageError::state("content unavailable"));
    }
    let Value::Integer(length) = length else {
        return Err(StorageError::state("corrupt native content row"));
    };
    let length = u64::try_from(*length).map_err(|_| StorageError::state("content unavailable"))?;
    let source = match source {
        Value::Null => None,
        Value::Integer(source) => Some(*source),
        _ => return Err(StorageError::state("corrupt delta baseline RID")),
    };
    Ok(ChainLink {
        rid: *rid,
        size: *size,
        hash: hash.clone(),
        length,
        source,
    })
}

/// Decode one row of [`PAYLOAD_ROW`].
/// # Arguments
/// * `rows` - Materialized payload rows of one RID.
/// # Returns
/// The stored envelope bytes.
/// # Errors
/// Rejects an absent row and a payload that is not a blob.
fn payload(rows: &[Vec<Value>]) -> Result<&[u8], StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("content unavailable"));
    };
    let [Value::Blob(packed)] = row.as_slice() else {
        return Err(StorageError::state("corrupt native content payload"));
    };
    Ok(packed)
}

/// Decode one row of [`NAME_ROW`] for a delta baseline.
/// # Arguments
/// * `rows` - Materialized name rows of one RID.
/// # Returns
/// The native name of the baseline.
/// # Errors
/// Rejects an absent baseline and a name that is not text.
fn baseline_name(rows: &[Vec<Value>]) -> Result<String, StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("content unavailable"));
    };
    let [Value::Text(hash)] = row.as_slice() else {
        return Err(StorageError::state("corrupt native baseline"));
    };
    Ok(hash.clone())
}

/// Decode one row of [`RID_ROW`] after a write.
/// # Arguments
/// * `rows` - Materialized RID rows of one name.
/// # Returns
/// The RID of the stored name.
/// # Errors
/// Rejects an absent row and a RID that is not an integer.
fn stored_rid(rows: &[Vec<Value>]) -> Result<i64, StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("missing stored content identity"));
    };
    let [Value::Integer(rid)] = row.as_slice() else {
        return Err(StorageError::state("corrupt stored content identity"));
    };
    Ok(*rid)
}
/// Record one RID of an availability walk, before its row is read.
/// # Arguments
/// * `seen` - RIDs that the walk has visited.
/// * `rid` - Next RID of the walk.
/// * `limits` - Explicit finite bounds for this operation.
/// # Returns
/// Unit when the RID is new and the walk is within the chain depth.
/// # Errors
/// Rejects a repeated RID as a cycle, and a walk longer than the chain depth.
fn visit_availability(
    seen: &mut BTreeSet<i64>,
    rid: i64,
    limits: Limits,
) -> Result<(), StorageError> {
    if !seen.insert(rid) {
        return Err(StorageError::state("content delta cycle"));
    }
    if seen.len() as u64 > u64::from(limits.max_chain_depth) {
        return Err(StorageError::state("content chain depth limit"));
    }
    Ok(())
}
impl WriteTransaction<'_> {
    /// Identify missing/phantom old delta dependencies without treating corruption as recoverable.
    /// # Arguments
    /// * `start` - First native row identifier in the old delta chain.
    /// * `limits` - Explicit finite bounds for this operation.
    /// # Returns
    /// Whether a missing or phantom baseline prevents reconstruction.
    /// # Errors
    /// Rejects cycles, excess depth, shunned content, corrupt rows and SQL failures.
    async fn old_delta_is_unavailable(
        &mut self,
        start: i64,
        limits: Limits,
    ) -> Result<bool, StorageError> {
        let mut rid = start;
        let mut seen = BTreeSet::new();
        loop {
            visit_availability(&mut seen, rid, limits)?;
            let rows = self
                .query_bound(AVAILABILITY_ROW, vec![Value::Integer(rid)])
                .await?;
            match availability(&rows)? {
                Availability::Unavailable => return Ok(true),
                Availability::Full => return Ok(false),
                Availability::Delta(source) => rid = source,
            }
        }
    }
    /// Identify missing or phantom old delta dependencies, and charge every result to the
    /// operation's allowances.
    /// # Arguments
    /// * `start` - First native row identifier in the old delta chain.
    /// * `limits` - Explicit finite bounds for this operation.
    /// * `sql` - The operation's remaining SQL rows and payload bytes.
    /// * `budget` - The operation's content budget, with no reset.
    /// # Returns
    /// Whether a missing or phantom baseline prevents reconstruction.
    /// # Errors
    /// Rejects cycles, excess depth, shunned content, corrupt rows, exhausted work and SQL failures.
    async fn old_delta_is_unavailable_with_sql(
        &mut self,
        start: i64,
        limits: Limits,
        sql: &mut SqlWork,
        budget: &mut Budget,
    ) -> Result<bool, StorageError> {
        let mut rid = start;
        let mut seen = BTreeSet::new();
        loop {
            visit_availability(&mut seen, rid, limits)?;
            let rows = self
                .query_bound_work(AVAILABILITY_ROW, vec![Value::Integer(rid)], 3, sql, budget)
                .await?;
            match availability(&rows)? {
                Availability::Unavailable => return Ok(true),
                Availability::Full => return Ok(false),
                Availability::Delta(source) => rid = source,
            }
        }
    }
    /// Store private file bytes, keeping public identities public and refusing unresolved dependents.
    /// # Arguments
    /// * `principal` - Sealed local repository and owner binding.
    /// * `name` - Full native content hash to verify.
    /// * `bytes` - Input bytes for this operation.
    /// * `limits` - Explicit finite bounds for this operation.
    /// # Returns
    /// Success after verified private storage or supported deduplication.
    /// # Errors
    /// Rejects invalid policy, hash mismatch, limits, corrupt content, required dependents and SQL failures.
    pub(super) async fn put_private_file_content(
        &mut self,
        principal: &super::policy::Principal,
        name: &str,
        bytes: &[u8],
        limits: Limits,
    ) -> Result<i64, StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            let limits = limits.validate()?;
            if bytes.len() as u64 > u64::from(limits.max_output_bytes) {
                return Err(StorageError::state("content output limit"));
            }
            let remaining_decoded = limits
                .max_total_decoded_bytes
                .checked_sub(bytes.len() as u64)
                .ok_or_else(|| StorageError::state("total decoded content limit"))?;
            self.verify_content_name(name, bytes)?;
            self.validate_principal(principal).await?;
            let named = || vec![Value::Text(name.into())];
            if !self.query_bound(SHUN_ROW, named()).await?.is_empty() {
                return Err(StorageError::state("content unavailable"));
            }
            let existing = self.query_bound(IDENTITY_ROW, named()).await?;
            let rid = match content_identity(&existing)? {
                Some((rid, size)) => {
                    if size >= 0 && !self.old_delta_is_unavailable(rid, limits).await? {
                        let read = Limits {
                            max_total_decoded_bytes: remaining_decoded,
                            ..limits
                        };
                        // The read verifies the stored bytes against `name`, which `bytes`
                        // already matches, so a readable copy equals `bytes`.
                        self.read_owner_content(principal, name, read).await?;
                        return Ok(rid);
                    }
                    Some(rid)
                }
                None => None,
            };
            let stored_ceiling =
                usize::try_from(limits.max_stored_bytes.min(limits.max_total_stored_bytes))
                    .unwrap_or(usize::MAX);
            let packed = compression::pack_bounded(bytes, -1, stored_ceiling)
                .map_err(|e| StorageError::caused(format!("native content compression: {e}"), e))?;
            let mut budget = Budget::new(limits)?;
            budget.enter(rid.unwrap_or(1), packed.len() as u64)?;
            budget.charge_decoded(bytes.len())?;
            let size = Value::Integer(bytes.len() as i64);
            if let Some(rid) = rid {
                // An orphan, a delta or a required typed record that waits on this RID needs
                // a crosslink that this build lacks, so the put refuses to fulfill it.
                let dependents = self
                    .query_bound(
                        DEPENDENT_ROWS,
                        vec![Value::Integer(rid), Value::Text(name.into())],
                    )
                    .await?;
                if !dependents.is_empty() {
                    return Err(StorageError::state(
                        "required dependent crosslink unavailable",
                    ));
                }
                let rid = Value::Integer(rid);
                self.execute_bound("DELETE FROM delta WHERE rid=?1", vec![rid.clone()])
                    .await?;
                self.execute_bound(
                    "UPDATE blob SET size=?1,content=?2 WHERE rid=?3",
                    vec![size, Value::Blob(packed), rid.clone()],
                )
                .await?;
                self.execute_bound("DELETE FROM phantom WHERE rid=?1", vec![rid])
                    .await?;
            } else {
                self.execute_bound(
                    "INSERT INTO blob(size,uuid,content) VALUES(?1,?2,?3)",
                    vec![size, Value::Text(name.into()), Value::Blob(packed)],
                )
                .await?;
            }
            let rows = self.query_bound(RID_ROW, named()).await?;
            let rid = stored_rid(&rows)?;
            self.execute_bound(
                PRIVATE_INSERT,
                vec![Value::Integer(rid), Value::Text(name.into())],
            )
            .await?;
            Ok(rid)
        }
        .await;
        self.record(result)
    }
    /// Store private file bytes with one caller-owned budget, and charge every result to the
    /// operation's SQL allowance.
    ///
    /// The method follows the rules of [`Self::put_private_file_content`]. Public identities
    /// stay public, identical bytes deduplicate, and full bytes replace a phantom or an
    /// unavailable delta target unless a required dependent waits on it. The route admits
    /// only the newest catalog, through [`Self::validate_principal_with_sql`].
    /// # Arguments
    /// * `principal` - Sealed local repository and owner binding.
    /// * `name` - Full native content hash to verify.
    /// * `bytes` - Complete uncompressed content.
    /// * `budget` - The operation's content budget, charged for the input, every result
    ///   and the new envelope, with no reset.
    /// * `sql` - The operation's remaining SQL rows and payload bytes.
    /// # Returns
    /// The RID of the stored or deduplicated content.
    /// # Errors
    /// Returns an earlier latched failure, and latches invalid policy, a hash mismatch,
    /// shunned or corrupt content, required dependents, exhausted allowances, and
    /// compression and SQL failures.
    pub(super) async fn put_private_file_content_budget_with_sql(
        &mut self,
        principal: &Principal,
        name: &str,
        bytes: &[u8],
        budget: &mut Budget,
        sql: &mut SqlWork,
    ) -> Result<i64, StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            let limits = budget.limits;
            if bytes.len() as u64 > u64::from(limits.max_output_bytes) {
                return Err(StorageError::state("content output limit"));
            }
            budget.charge_decoded(bytes.len())?;
            self.verify_content_name(name, bytes)?;
            self.validate_principal_with_sql(principal, sql, budget)
                .await?;
            let named = || vec![Value::Text(name.into())];
            let shunned = self
                .query_bound_work(SHUN_ROW, named(), 1, sql, budget)
                .await?;
            if !shunned.is_empty() {
                return Err(StorageError::state("content unavailable"));
            }
            let existing = self
                .query_bound_work(IDENTITY_ROW, named(), 2, sql, budget)
                .await?;
            let rid = match content_identity(&existing)? {
                Some((rid, size)) => {
                    let unavailable = size < 0
                        || self
                            .old_delta_is_unavailable_with_sql(rid, limits, sql, budget)
                            .await?;
                    if !unavailable {
                        // The read verifies the stored bytes against `name`, which `bytes`
                        // already matches, so a readable copy equals `bytes`.
                        self.read_owner_content_budget_with_sql(principal, name, budget, sql)
                            .await?;
                        return Ok(rid);
                    }
                    Some(rid)
                }
                None => None,
            };
            let packed = compression::pack_bounded(bytes, -1, budget.packed_output_ceiling())
                .map_err(|e| StorageError::caused(format!("native content compression: {e}"), e))?;
            // The envelope is stored, and its bind value is retained until the write ends.
            budget.charge_stored(packed.len())?;
            budget.charge_decoded(packed.len())?;
            let size = Value::Integer(bytes.len() as i64);
            if let Some(rid) = rid {
                // An orphan, a delta or a required typed record that waits on this RID needs
                // a crosslink that this build lacks, so the put refuses to fulfill it.
                let dependents = self
                    .query_bound_work(
                        DEPENDENT_ROWS,
                        vec![Value::Integer(rid), Value::Text(name.into())],
                        1,
                        sql,
                        budget,
                    )
                    .await?;
                if !dependents.is_empty() {
                    return Err(StorageError::state(
                        "required dependent crosslink unavailable",
                    ));
                }
                let rid = Value::Integer(rid);
                self.execute_bound("DELETE FROM delta WHERE rid=?1", vec![rid.clone()])
                    .await?;
                self.execute_bound(
                    "UPDATE blob SET size=?1,content=?2 WHERE rid=?3",
                    vec![size, Value::Blob(packed), rid.clone()],
                )
                .await?;
                self.execute_bound("DELETE FROM phantom WHERE rid=?1", vec![rid])
                    .await?;
            } else {
                self.execute_bound(
                    "INSERT INTO blob(size,uuid,content) VALUES(?1,?2,?3)",
                    vec![size, Value::Text(name.into()), Value::Blob(packed)],
                )
                .await?;
            }
            let rows = self
                .query_bound_work(RID_ROW, named(), 1, sql, budget)
                .await?;
            let rid = stored_rid(&rows)?;
            self.execute_bound(
                PRIVATE_INSERT,
                vec![Value::Integer(rid), Value::Text(name.into())],
            )
            .await?;
            Ok(rid)
        }
        .await;
        self.record(result)
    }
    /// Reconstruct under the same validated owner and finite operation budget.
    /// # Arguments
    /// * `principal` - Sealed local repository and owner binding.
    /// * `name` - Full native content hash to verify.
    /// * `limits` - Explicit finite bounds for this operation.
    /// # Returns
    /// Owned reconstructed bytes with verified size and hash.
    /// # Errors
    /// Rejects invalid policy, absent or shunned content, corrupt chains, limits and SQL failures.
    pub(super) async fn read_owner_content(
        &mut self,
        principal: &super::policy::Principal,
        name: &str,
        limits: Limits,
    ) -> Result<Vec<u8>, StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let new_budget = Budget::new(limits);
        let mut budget = self.record(new_budget)?;
        self.read_owner_content_budget(principal, name, &mut budget)
            .await
    }
    /// Read verified content using one caller-owned aggregate allowance.
    /// # Arguments
    /// * `principal` - Sealed local owner binding.
    /// * `name` - Native full content identity.
    /// * `budget` - Aggregate stored and decoded allowances retained by the caller.
    /// # Returns
    /// Owned authenticated reconstructed bytes.
    /// # Errors
    /// Rejects invalid policy, unavailable content, corrupt chains, exhausted allowances and SQL failures.
    pub(super) async fn read_owner_content_budget(
        &mut self,
        principal: &super::policy::Principal,
        name: &str,
        budget: &mut Budget,
    ) -> Result<Vec<u8>, StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            budget.visited.clear();
            self.validate_principal(principal).await?;
            let mut wanted = name.to_owned();
            let mut chain = Vec::new();
            loop {
                let rows = self
                    .query_bound(CHAIN_ROW, vec![Value::Text(wanted)])
                    .await?;
                let link = chain_link(&rows)?;
                budget.enter(link.rid, link.length)?;
                let data = self
                    .query_bound(PAYLOAD_ROW, vec![Value::Integer(link.rid)])
                    .await?;
                let bytes = budget.unpack(payload(&data)?)?;
                let source = link.source;
                chain.push((link, bytes));
                let Some(source) = source else { break };
                let rows = self
                    .query_bound(NAME_ROW, vec![Value::Integer(source)])
                    .await?;
                wanted = baseline_name(&rows)?;
            }
            self.reconstruct(chain, budget)
        }
        .await;
        self.record(result)
    }
    /// Read verified content with one caller-owned budget, and charge every result to the
    /// operation's SQL allowance.
    ///
    /// The route admits only the newest catalog, through
    /// [`Self::validate_principal_with_sql`].
    /// # Arguments
    /// * `principal` - Sealed local owner binding.
    /// * `name` - Native full content identity.
    /// * `budget` - Aggregate stored and decoded allowances retained by the caller.
    /// * `sql` - The operation's remaining SQL rows and payload bytes.
    /// # Returns
    /// Owned authenticated reconstructed bytes.
    /// # Errors
    /// Rejects invalid policy, unavailable content, corrupt chains, exhausted allowances and SQL failures.
    pub(super) async fn read_owner_content_budget_with_sql(
        &mut self,
        principal: &Principal,
        name: &str,
        budget: &mut Budget,
        sql: &mut SqlWork,
    ) -> Result<Vec<u8>, StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            budget.visited.clear();
            self.validate_principal_with_sql(principal, sql, budget)
                .await?;
            let mut wanted = name.to_owned();
            let mut chain = Vec::new();
            loop {
                let rows = self
                    .query_bound_work(CHAIN_ROW, vec![Value::Text(wanted)], 5, sql, budget)
                    .await?;
                let link = chain_link(&rows)?;
                budget.enter(link.rid, link.length)?;
                let data = self
                    .query_bound_work(PAYLOAD_ROW, vec![Value::Integer(link.rid)], 1, sql, budget)
                    .await?;
                let bytes = budget.unpack(payload(&data)?)?;
                let source = link.source;
                chain.push((link, bytes));
                let Some(source) = source else { break };
                let rows = self
                    .query_bound_work(NAME_ROW, vec![Value::Integer(source)], 1, sql, budget)
                    .await?;
                wanted = baseline_name(&rows)?;
            }
            self.reconstruct(chain, budget)
        }
        .await;
        self.record(result)
    }
    /// Apply a read chain from its full envelope to its first link, and verify each output.
    /// # Arguments
    /// * `chain` - Links in read order, each with its unpacked bytes, and the last is full.
    /// * `budget` - The operation's content budget, charged for each reconstructed output.
    /// # Returns
    /// The reconstructed bytes of the first link.
    /// # Errors
    /// Rejects an empty chain, a delta failure, exhausted decoded work, and a size or
    /// hash mismatch of any link.
    fn reconstruct(
        &mut self,
        chain: Vec<(ChainLink, Vec<u8>)>,
        budget: &mut Budget,
    ) -> Result<Vec<u8>, StorageError> {
        let mut output: Option<Vec<u8>> = None;
        for (link, bytes) in chain.into_iter().rev() {
            let reconstructed = match output {
                None => bytes,
                Some(base) => {
                    let limits = crate::delta::Limits {
                        max_output_bytes: budget.delta_output_ceiling(),
                    };
                    crate::delta::apply(&base, &bytes, limits).map_err(|error| {
                        StorageError::caused(format!("native content delta: {error}"), error)
                    })?
                }
            };
            budget.charge_decoded(reconstructed.len())?;
            if link.size as u64 != reconstructed.len() as u64 {
                return Err(StorageError::state("native content size mismatch"));
            }
            self.verify_content_name(&link.hash, &reconstructed)?;
            output = Some(reconstructed);
        }
        output.ok_or_else(|| StorageError::state("content unavailable"))
    }
    /// Read a full private-recording envelope under the complete shared ledger.
    /// The bounded recording family produces no deltas; this entry refuses them.
    /// # Arguments
    /// * `principal` - Sealed owner binding reauthenticated under this ledger.
    /// * `name` - Exact native SHA-1 or SHA3-256 lowercase identity.
    /// * `budget` - Whole-operation stored and decoded work.
    /// # Returns
    /// Complete verified bytes with all SQL materialization and hash work charged.
    /// # Errors
    /// Refuses malformed identity, policy, delta rows, unavailable content and exhausted work.
    pub(super) async fn read_recording_content_budget(
        &mut self,
        principal: &super::policy::Principal,
        name: &str,
        budget: &mut Budget,
    ) -> Result<Vec<u8>, StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            if !matches!(name.len(), 40 | 64) || !name.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
                return Err(StorageError::state("recording content name invalid"));
            }
            self.validate_recording_principal_budgeted(principal, budget).await?;
            // Scalar output and bind storage are bounded before asking for any payload.
            budget.charge_decoded(name.len().checked_mul(2).and_then(|n| n.checked_add(1024))
                .ok_or_else(||StorageError::state("recording content metadata work overflow"))?)?;
            let rows = self.query_bound("SELECT b.rid,b.size,length(b.content),typeof(b.content),EXISTS(SELECT 1 FROM delta d WHERE d.rid=b.rid) FROM blob b WHERE b.uuid=?1 AND NOT EXISTS(SELECT 1 FROM shun s WHERE s.uuid=b.uuid)", vec![Value::Text(name.into())]).await?;
            let (rid, size, length) = recording_content_metadata(&rows, budget.limits.max_output_bytes)?;
            budget.visited.clear();
            budget.enter(rid, length as u64)?;
            // Account for the owned provider envelope separately from decoded bytes.
            budget.charge_decoded(usize::try_from(length).map_err(|_| StorageError::state("recording stored length overflow"))?)?;
            let rows = self.query_bound("SELECT content FROM blob WHERE rid=?1", vec![Value::Integer(rid)]).await?;
            let [row] = rows.as_slice() else { return Err(StorageError::state("content unavailable")); };
            let [Value::Blob(packed)] = row.as_slice() else { return Err(StorageError::state("corrupt recording payload")); };
            let bytes = budget.unpack(packed)?;
            if bytes.len() as u64 != size as u64 { return Err(StorageError::state("native content size mismatch")); }
            budget.charge_decoded(bytes.len())?;
            self.verify_content_name(name, &bytes)?;
            Ok(bytes)
        }.await;
        self.record(result)
    }

    /// Decode and verify one complete full-byte row through the operation latch.
    /// # Arguments
    /// * `name` - Full native content hash to verify.
    /// * `size` - Expected exact reconstructed length.
    /// * `packed` - Native compressed envelope.
    /// * `limits` - Explicit finite bounds for this operation.
    /// # Returns
    /// Owned full bytes with verified size and hash.
    /// # Errors
    /// Rejects envelope errors, decoded limits, size or hash mismatch, or a prior latched failure.
    pub(super) fn decode_full_content(
        &mut self,
        name: &str,
        size: i64,
        packed: &[u8],
        limits: Limits,
    ) -> Result<Vec<u8>, StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = (|| {
            let mut budget = Budget::new(limits)?;
            budget.enter(1, packed.len() as u64)?;
            let bytes = budget.unpack(packed)?;
            if size < 0 || size as u64 != bytes.len() as u64 {
                return Err(StorageError::state("native content size mismatch"));
            }
            self.verify_content_name(name, &bytes)?;
            Ok(bytes)
        })();
        self.record(result)
    }
}

/// Decode the bounded recording envelope metadata before payload ownership.
/// # Arguments
/// * `rows` - Fixed metadata query result, excluding shunned identities.
/// * `max_output_bytes` - Caller ceiling for one reconstructed content object.
/// # Returns
/// Native RID, decoded size and packed length for one supported full envelope.
/// # Errors
/// Refuses absent or malformed rows, negative sizes, deltas and oversized envelopes.
fn recording_content_metadata(
    rows: &[Vec<Value>],
    max_output_bytes: u32,
) -> Result<(i64, i64, i64), StorageError> {
    let [row] = rows else {
        return Err(StorageError::state("content unavailable"));
    };
    let [
        Value::Integer(rid),
        Value::Integer(size),
        Value::Integer(length),
        Value::Text(kind),
        Value::Integer(delta),
    ] = row.as_slice()
    else {
        return Err(StorageError::state("corrupt recording content metadata"));
    };
    if *size < 0
        || *length < 0
        || *size as u64 > u64::from(max_output_bytes)
        || kind != "blob"
        || *delta != 0
    {
        return Err(StorageError::state(
            "unsupported recording content envelope",
        ));
    }

    Ok((*rid, *size, *length))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{content_hash, storage::LocalStore};
    fn limits() -> Limits {
        Limits {
            max_stored_bytes: 1024,
            max_output_bytes: 16,
            max_chain_depth: 4,
            max_total_stored_bytes: 4096,
            max_total_decoded_bytes: 64,
        }
    }
    fn name(bytes: &[u8]) -> String {
        content_hash::hex(&content_hash::sha3(bytes, 256).unwrap())
    }

    #[test]
    fn owned_identity_decoder_rejects_duplicate_and_wrong_type_rows() {
        use libsql::Value;
        assert!(content_identity(&[vec![Value::Text("1".into()), Value::Integer(0)]]).is_err());
        assert!(
            content_identity(&[
                vec![Value::Integer(1), Value::Integer(0)],
                vec![Value::Integer(2), Value::Integer(0)]
            ])
            .is_err()
        );
    }
    #[tokio::test]
    async fn native_envelope_exceptions_and_exact_repository_validation() {
        for bytes in [b"".as_slice(), b"abc".as_slice()] {
            let mut packed = compression::pack(bytes, -1).unwrap();
            packed[..4].copy_from_slice(&16_u32.to_be_bytes());
            packed.extend_from_slice(b"native trailing bytes");
            let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
            let mut tx = store.begin_write().await.unwrap();
            assert_eq!(
                tx.decode_full_content(&name(bytes), bytes.len() as i64, &packed, limits())
                    .unwrap(),
                bytes
            );
            tx.commit().await.unwrap();
        }
        for packed in [vec![], vec![0, 0, 0, 0], vec![0, 0, 0, 1, 0xff]] {
            let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
            let mut tx = store.begin_write().await.unwrap();
            assert!(
                tx.decode_full_content(&name(b""), 0, &packed, limits())
                    .is_err()
            );
            assert!(tx.commit().await.is_err());
        }
    }
    #[tokio::test]
    async fn exact_size_failure_latches_all_content_helpers() {
        let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let packed = compression::pack(b"abc", -1).unwrap();
        let error = tx
            .decode_full_content(&name(b"abc"), 4, &packed, limits())
            .unwrap_err();
        assert!(error.operation.contains("size mismatch"));
        let principal = super::super::policy::Principal {
            repository: "not-authority".into(),
            owner: "not-authority".into(),
        };
        assert_eq!(
            tx.put_private_file_content(&principal, &name(b"abc"), b"abc", limits())
                .await
                .unwrap_err()
                .operation,
            error.operation
        );
        assert_eq!(
            tx.read_owner_content(&principal, &name(b"abc"), limits())
                .await
                .unwrap_err()
                .operation,
            error.operation
        );
        assert_eq!(
            tx.decode_full_content(&name(b"abc"), 3, &packed, limits())
                .unwrap_err()
                .operation,
            error.operation
        );
        assert_eq!(tx.commit().await.unwrap_err().operation, error.operation);
    }
    #[test]
    fn exact_budget_boundaries_and_cycles() {
        let mut budget = Budget::new(limits()).unwrap();
        for rid in 1..=4 {
            budget.enter(rid, 1024).unwrap();
        }
        assert!(budget.enter(5, 1).is_err());
        assert!(budget.enter(1, 0).is_err());
        budget.charge_decoded(64).unwrap();
        assert!(budget.charge_decoded(1).is_err());
        let mut budget = Budget::new(limits()).unwrap();
        assert!(budget.enter(1, 1025).is_err());
        assert!(
            Budget::new(Limits {
                max_chain_depth: 0,
                ..limits()
            })
            .is_err()
        );
    }
    /// A prior transaction failure takes precedence over later invalid content allowances.
    #[tokio::test]
    async fn owner_content_read_preserves_first_failure_before_invalid_budget() {
        let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let first = tx
            .record::<()>(Err(StorageError::caused(
                "first",
                std::io::Error::from_raw_os_error(13),
            )))
            .unwrap_err();
        let principal = super::super::policy::Principal {
            repository: "not-authority".into(),
            owner: "not-authority".into(),
        };
        let invalid = Limits {
            max_chain_depth: 0,
            ..limits()
        };
        let error = tx
            .read_owner_content(&principal, &name(b"abc"), invalid)
            .await
            .unwrap_err();
        assert_eq!(error.operation, first.operation);
        assert_eq!(
            std::error::Error::source(&error)
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .raw_os_error(),
            Some(13)
        );
        let terminal = tx.commit().await.unwrap_err();
        assert_eq!(terminal.operation, first.operation);
        assert_eq!(
            std::error::Error::source(&terminal)
                .unwrap()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .raw_os_error(),
            Some(13)
        );
    }

    /// New packed outputs share stored work with reads without consuming chain slots.
    #[test]
    fn packed_outputs_share_stored_ledger_without_resetting_traversal() {
        let mut budget = Budget::new(Limits {
            max_stored_bytes: 4,
            max_total_stored_bytes: 10,
            ..limits()
        })
        .unwrap();
        budget.enter(1, 3).unwrap();
        budget.charge_stored(4).unwrap();
        assert_eq!(budget.stored, 7);
        assert_eq!(budget.packed_output_ceiling(), 3);
        assert_eq!(budget.visited.iter().copied().collect::<Vec<_>>(), [1]);
        assert!(budget.charge_stored(5).is_err());
        assert_eq!(budget.stored, 7);
        budget.charge_stored(3).unwrap();
        assert_eq!(budget.stored, 10);
        assert_eq!(budget.packed_output_ceiling(), 0);
        assert!(budget.charge_stored(1).is_err());
        assert_eq!(budget.stored, 10);
        assert!(budget.enter(1, 0).is_err());
        assert_eq!(budget.limits.max_total_stored_bytes, 10);
    }

    /// Repeated verified reads reset traversal identity while retaining both aggregate byte counters.
    #[tokio::test]
    async fn cumulative_content_budget_retains_charges_across_verified_reads() {
        use crate::storage::{CreateOptions, LocalOwnerRepository};
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let repository = "0123456789abcdef0123456789abcdef01234567";
        let mut owner = LocalOwnerRepository::create(
            &path,
            CreateOptions {
                repository_id: repository,
                owner_id: "owner",
                checkout_id: "checkout",
                checkout_root: temp.path(),
                date: b"2026-11-18T00:00:00.000",
                user: b"Owner",
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
                    entries: 64,
                },
            },
        )
        .await
        .unwrap();
        let bytes = b"owner bytes";
        let hash = name(bytes);
        owner
            .put_private_file_content(&hash, bytes, limits())
            .await
            .unwrap();
        drop(owner);
        let stored = compression::pack(bytes, -1).unwrap().len() as u64;
        let mut budget = Budget::new(Limits {
            max_stored_bytes: stored,
            max_total_stored_bytes: stored * 4,
            max_total_decoded_bytes: bytes.len() as u64 * 4,
            ..limits()
        })
        .unwrap();
        let principal = super::super::policy::Principal {
            repository: repository.into(),
            owner: "owner".into(),
        };
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        for reads in [1, 2] {
            assert_eq!(
                tx.read_owner_content_budget(&principal, &hash, &mut budget)
                    .await
                    .unwrap(),
                bytes
            );
            assert_eq!(budget.stored, stored * reads);
            assert_eq!(budget.decoded, bytes.len() as u64 * 2 * reads);
            assert_eq!(budget.visited.len(), 1);
        }
        let first = tx
            .read_owner_content_budget(&principal, &hash, &mut budget)
            .await
            .unwrap_err();
        assert_eq!(
            first.operation,
            "native content envelope: declared capacity exceeds caller limit"
        );
        assert!(matches!(
            std::error::Error::source(&first)
                .unwrap()
                .downcast_ref::<compression::BlobError>(),
            Some(compression::BlobError::Limit)
        ));
        assert_eq!(budget.decoded, bytes.len() as u64 * 4);
        assert_eq!(budget.limits.max_total_decoded_bytes - budget.decoded, 0);
        assert_eq!(budget.stored, stored * 3);
        let terminal = tx.commit().await.unwrap_err();
        assert_eq!(terminal.operation, first.operation);
        assert!(matches!(
            std::error::Error::source(&terminal)
                .unwrap()
                .downcast_ref::<compression::BlobError>(),
            Some(compression::BlobError::Limit)
        ));
    }
    /// Malformed metadata cannot reach payload loading, even when earlier admission shadows it.
    #[test]
    fn recording_metadata_requires_one_typed_full_envelope() {
        use libsql::Value;
        let valid = vec![
            Value::Integer(7),
            Value::Integer(3),
            Value::Integer(12),
            Value::Text("blob".into()),
            Value::Integer(0),
        ];
        assert_eq!(
            recording_content_metadata(std::slice::from_ref(&valid), 3).unwrap(),
            (7, 3, 12)
        );
        for rows in [vec![], vec![valid.clone(), valid.clone()]] {
            assert_eq!(
                recording_content_metadata(&rows, 3).unwrap_err().operation,
                "content unavailable"
            );
        }
        for index in 0..valid.len() {
            let mut row = valid.clone();
            row[index] = Value::Null;
            assert_eq!(
                recording_content_metadata(&[row], 3).unwrap_err().operation,
                "corrupt recording content metadata"
            );
        }
        for (index, value) in [
            (1, Value::Integer(-1)),
            (2, Value::Integer(-1)),
            (3, Value::Text("text".into())),
            (4, Value::Integer(1)),
        ] {
            let mut row = valid.clone();
            row[index] = value;
            assert_eq!(
                recording_content_metadata(&[row], 3).unwrap_err().operation,
                "unsupported recording content envelope"
            );
        }
        assert_eq!(
            recording_content_metadata(&[valid], 2)
                .unwrap_err()
                .operation,
            "unsupported recording content envelope"
        );
    }

    /// Invalid names latch before any SQL, and a later valid name preserves that first refusal.
    #[tokio::test]
    async fn recording_content_name_failure_preserves_the_transaction_latch() {
        for invalid in [
            "bad".to_owned(),
            "A".repeat(40),
            "g".repeat(64),
            format!("{}\0", "a".repeat(39)),
        ] {
            let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
            let mut tx = store.begin_write().await.unwrap();
            let principal = crate::storage::policy::Principal {
                repository: "0".repeat(40),
                owner: "owner".into(),
            };
            let mut budget = Budget::new(limits()).unwrap();
            let first = tx
                .read_recording_content_budget(&principal, &invalid, &mut budget)
                .await
                .unwrap_err();
            assert_eq!(first.operation, "recording content name invalid");
            let later = tx
                .read_recording_content_budget(&principal, &name(b"abc"), &mut budget)
                .await
                .unwrap_err();
            assert_eq!(later.operation, first.operation);
            assert_eq!(budget.decoded, 0);
            assert_eq!(tx.commit().await.unwrap_err().operation, first.operation);
        }
    }

    #[test]
    /// A shared surface is charged once per admission, and later only by its growth.
    fn reserved_surfaces_charge_once_per_admission() {
        let wide = Limits {
            max_total_decoded_bytes: 100,
            ..limits()
        };
        let mut budget = Budget::new(wide).unwrap();
        budget.charge_reserved("text", 30).unwrap();
        assert_eq!(budget.decoded(), 30);
        budget.charge_reserved("text", 30).unwrap();
        budget.charge_reserved("text", 10).unwrap();
        assert_eq!(budget.decoded(), 30);
        budget.charge_reserved("text", 40).unwrap();
        assert_eq!(budget.decoded(), 40);
        budget.charge_reserved("schema", 20).unwrap();
        assert_eq!(budget.decoded(), 60);
        budget.begin_admission();
        budget.charge_reserved("text", 40).unwrap();
        assert_eq!(budget.decoded(), 100);
        // A refused growth charges nothing and keeps the held reservation.
        let refused = budget.charge_reserved("schema", 21).unwrap_err();
        assert_eq!(refused.operation, "total decoded content limit");
        assert_eq!(budget.decoded(), 100);
        // Nothing of the refused schema reservation is held, so one more byte refuses.
        assert!(budget.charge_reserved("schema", 1).is_err());
        budget.charge_reserved("text", 40).unwrap();
        assert_eq!(budget.decoded(), 100);
    }
}

#[cfg(test)]
#[path = "../../tests/support/storage_content_metered.rs"]
mod metered_tests;
