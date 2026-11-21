//! Bounded native envelopes and reconstruction accounting.
//!
//! ## Recommendations
//! Supply finite operation budgets and verify every reconstructed intermediate.
use super::{StorageError, WriteTransaction};
use crate::compression::{self, Unpacked};
use std::collections::BTreeSet;

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
    /// Catalog4 operations also charge metadata, parser, capture and retained SQL work.
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
}
impl Budget {
    /// Return the unchanged finite ceilings used to create this ledger.
    /// # Returns
    /// Caller-selected content limits, without resetting consumed work.
    pub(super) fn limits(&self) -> Limits {
        self.limits
    }

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
fn content_identity(rows: &[Vec<libsql::Value>]) -> Result<Option<(i64, i64)>, StorageError> {
    use libsql::Value;
    match rows {
        [] => Ok(None),
        [row] => match row.as_slice() {
            [Value::Integer(rid), Value::Integer(size)] => Ok(Some((*rid, *size))),
            _ => Err(StorageError::state("corrupt native content identity")),
        },
        _ => Err(StorageError::state("duplicate native content identity")),
    }
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
        use libsql::Value;
        let mut rid = start;
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(rid) {
                return Err(StorageError::state("content delta cycle"));
            }
            if seen.len() as u64 > u64::from(limits.max_chain_depth) {
                return Err(StorageError::state("content chain depth limit"));
            }
            let rows = self.query_bound("SELECT b.size,d.srcid,EXISTS(SELECT 1 FROM shun s WHERE s.uuid=b.uuid) FROM blob b LEFT JOIN delta d ON d.rid=b.rid WHERE b.rid=?1",vec![Value::Integer(rid)]).await?;
            let [row] = rows.as_slice() else {
                return Ok(true);
            };
            let [Value::Integer(size), source, Value::Integer(shunned)] = row.as_slice() else {
                return Err(StorageError::state("corrupt native availability row"));
            };
            if *shunned != 0 {
                return Err(StorageError::state("content unavailable"));
            }
            if *size < 0 {
                return Ok(true);
            }
            match source {
                Value::Null => return Ok(false),
                Value::Integer(source) => rid = *source,
                _ => return Err(StorageError::state("corrupt delta baseline RID")),
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
        use libsql::Value;
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            let limits = limits.validate()?;
            if bytes.len() as u64 > u64::from(limits.max_output_bytes) {
                return Err(StorageError::state("content output limit"));
            }
            let remaining_decoded = limits.max_total_decoded_bytes.checked_sub(bytes.len() as u64)
                .ok_or_else(|| StorageError::state("total decoded content limit"))?;
            self.verify_content_name(name, bytes)?;
            self.validate_principal(principal).await?;
            if !self.query_bound("SELECT 1 FROM shun WHERE uuid=?1", vec![Value::Text(name.into())]).await?.is_empty() { return Err(StorageError::state("content unavailable")); }
            let existing = self.query_bound("SELECT rid,size FROM blob WHERE uuid=?1", vec![Value::Text(name.into())]).await?;
            let rid = match content_identity(&existing)? {
                Some((rid,size)) => {
                    if size >= 0 && !self.old_delta_is_unavailable(rid,limits).await? {
                        let original = self.read_owner_content(principal,name,Limits { max_total_decoded_bytes:remaining_decoded,..limits }).await?;
                        if original != bytes { return Err(StorageError::state("conflicting native content bytes")); }
                        return Ok(rid);
                    }
                    Some(rid)
                }
                None => None,
            };
            let stored_ceiling = usize::try_from(limits.max_stored_bytes.min(limits.max_total_stored_bytes)).unwrap_or(usize::MAX);
            let packed = compression::pack_bounded(bytes, -1, stored_ceiling).map_err(|e| StorageError::caused(format!("native content compression: {e}"), e))?;
            let mut budget = Budget::new(limits)?;
            budget.enter(rid.unwrap_or(1), packed.len() as u64)?;
            budget.charge_decoded(bytes.len())?;
            if let Some(rid) = rid {
                // Dependent artifact/typed semantics must execute before fulfillment can complete.
                let dependents = self.query_bound("SELECT 1 FROM orphan WHERE baseline=?1 UNION ALL SELECT 1 FROM delta WHERE srcid=?1 UNION ALL SELECT 1 FROM g_dependency WHERE target_hash=?2 AND required=1", vec![Value::Integer(rid), Value::Text(name.into())]).await?;
                if !dependents.is_empty() { return Err(StorageError::state("required dependent crosslink unavailable")); }
                self.execute_bound("DELETE FROM delta WHERE rid=?1", vec![Value::Integer(rid)]).await?;
                self.execute_bound("UPDATE blob SET size=?1,content=?2 WHERE rid=?3", vec![Value::Integer(bytes.len() as i64), Value::Blob(packed), Value::Integer(rid)]).await?;
                self.execute_bound("DELETE FROM phantom WHERE rid=?1", vec![Value::Integer(rid)]).await?;
            } else {
                self.execute_bound("INSERT INTO blob(size,uuid,content) VALUES(?1,?2,?3)", vec![Value::Integer(bytes.len() as i64), Value::Text(name.into()), Value::Blob(packed)]).await?;
            }
            let rows = self.query_bound("SELECT rid FROM blob WHERE uuid=?1", vec![Value::Text(name.into())]).await?;
            let [row] = rows.as_slice() else { return Err(StorageError::state("missing stored content identity")); };
            let [Value::Integer(rid)] = row.as_slice() else { return Err(StorageError::state("corrupt stored content identity")); };
            self.execute_bound("INSERT OR IGNORE INTO private(rid) SELECT ?1 WHERE NOT EXISTS(SELECT 1 FROM g_public_receipt WHERE native_hash=?2)", vec![Value::Integer(*rid), Value::Text(name.into())]).await?;
            Ok(*rid)
        }.await;
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
            let limits = budget.limits;
            self.validate_principal(principal).await?;
            let mut wanted = name.to_owned();
            let mut chain = Vec::new();
            loop {
                let rows = self.query_bound("SELECT b.rid,b.size,b.uuid,length(b.content),d.srcid FROM blob b LEFT JOIN delta d ON d.rid=b.rid WHERE b.uuid=?1 AND NOT EXISTS(SELECT 1 FROM shun WHERE uuid=b.uuid)", vec![libsql::Value::Text(wanted)]).await?;
                let [row] = rows.as_slice() else { return Err(StorageError::state("content unavailable")); };
                let [libsql::Value::Integer(rid), libsql::Value::Integer(size), libsql::Value::Text(hash), libsql::Value::Integer(length), source] = row.as_slice() else { return Err(StorageError::state("corrupt native content row")); };
                if *size < 0 || *length < 0 { return Err(StorageError::state("content unavailable")); }
                budget.enter(*rid, *length as u64)?;
                let data = self.query_bound("SELECT content FROM blob WHERE rid=?1", vec![libsql::Value::Integer(*rid)]).await?;
                let [data] = data.as_slice() else { return Err(StorageError::state("content unavailable")); };
                let [libsql::Value::Blob(packed)] = data.as_slice() else { return Err(StorageError::state("corrupt native content payload")); };
                let bytes = budget.unpack(packed)?;
                chain.push((hash.clone(), *size, bytes));
                match source {
                    libsql::Value::Null => break,
                    libsql::Value::Integer(source) => {
                        let rows = self.query_bound("SELECT uuid FROM blob WHERE rid=?1", vec![libsql::Value::Integer(*source)]).await?;
                        let [row] = rows.as_slice() else { return Err(StorageError::state("content unavailable")); };
                        let [libsql::Value::Text(hash)] = row.as_slice() else { return Err(StorageError::state("corrupt native baseline")); };
                        wanted = hash.clone();
                    }
                    _ => return Err(StorageError::state("corrupt delta baseline RID")),
                }
            }
            let mut output: Option<Vec<u8>> = None;
            for (hash, size, bytes) in chain.into_iter().rev() {
                let reconstructed = match output {
                    None => bytes,
                    Some(base) => crate::delta::apply(&base, &bytes, crate::delta::Limits { max_output_bytes: u64::from(limits.max_output_bytes).min(budget.limits.max_total_decoded_bytes - budget.decoded) as u32 })
                        .map_err(|error| StorageError::caused(format!("native content delta: {error}"), error))?,
                };
                budget.charge_decoded(reconstructed.len())?;
                if size as u64 != reconstructed.len() as u64 { return Err(StorageError::state("native content size mismatch")); }
                self.verify_content_name(&hash, &reconstructed)?;
                output = Some(reconstructed);
            }
            output.ok_or_else(|| StorageError::state("content unavailable"))
        }.await;
        self.record(result)
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
        use libsql::Value;
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
    rows: &[Vec<libsql::Value>],
    max_output_bytes: u32,
) -> Result<(i64, i64, i64), StorageError> {
    use libsql::Value;
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
        assert_eq!(budget.limits().max_total_stored_bytes, 10);
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
}
