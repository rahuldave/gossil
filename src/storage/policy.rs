//! Principal authority remains separate from stored owner identity.
//!
//! ## Recommendations
//! Bind local owner authority through OS file access; reject missing policy.
use super::{StorageError, WriteTransaction};
use libsql::Value;

/// Sealed local-owner context read from its opened file, never a caller-selected capability.
pub(super) struct Principal {
    /// Repository identity admitted from this local file.
    pub(super) repository: String,
    /// Owner binding admitted through local operating-system access.
    pub(super) owner: String,
}
impl WriteTransaction<'_> {
    /// Validate durable policy before any private existence lookup.
    /// # Arguments
    /// * `principal` - Sealed local repository and owner binding.
    /// # Returns
    /// Success when durable policy matches the sealed local binding.
    /// # Errors
    /// Rejects absent or mismatched complete policy and owner bindings, corrupt rows and SQL failures.
    pub(super) async fn validate_principal(
        &mut self,
        principal: &Principal,
    ) -> Result<(), StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            if principal.owner.is_empty() || principal.repository.len()!=40 || !principal.repository.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) {
                return Err(StorageError::state("invalid local repository identity binding"));
            }
            self.validate_catalog().await?;
            let rows = self.query_bound("SELECT s.repository_id,s.storage_version,s.projection_version,s.policy_complete,o.owner_id FROM g_storage s CROSS JOIN g_local_owner o WHERE s.singleton=1 AND o.singleton=1", vec![]).await?;
            let version = match rows.as_slice() {
                [row] => match row.get(1) { Some(Value::Integer(version @ (1 | 4))) => *version, _ => return Err(StorageError::state("unsupported storage version")) },
                _ => return Err(StorageError::state("repository principal or complete policy unavailable")),
            };
            if version == 4 && self.query_bound("SELECT checkout_count FROM g_storage WHERE singleton=1",vec![]).await? != vec![vec![Value::Integer(0)]] {
                return Err(StorageError::state("unsupported populated catalog4 checkout scope"));
            }
            let expected = vec![vec![Value::Text(principal.repository.clone()), Value::Integer(version), Value::Text("gest-ticket-v1".into()), Value::Integer(1), Value::Text(principal.owner.clone())]];
            if rows != expected { return Err(StorageError::state("repository principal or complete policy unavailable")); }
            let markers = self.query_bound("SELECT name,value FROM config WHERE name IN ('project-code','content-schema','aux-schema') ORDER BY name", vec![]).await?;
            let expected = vec![
                vec![Value::Text("aux-schema".into()), Value::Text("2015-01-24".into())],
                vec![Value::Text("content-schema".into()), Value::Text("2".into())],
                vec![Value::Text("project-code".into()), Value::Text(principal.repository.clone())],
            ];
            if markers != expected { return Err(StorageError::state("unsupported or missing native repository markers")); }
            Ok(())
        }.await;
        self.record(result)
    }
}
