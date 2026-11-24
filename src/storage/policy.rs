//! Principal authority remains separate from stored owner identity.
//!
//! ## Recommendations
//! Bind local owner authority through OS file access; reject missing policy.
use super::{
    StorageError, WriteTransaction, content::Budget, metered_query::SqlWork, schema::NEWEST_CATALOG,
};
use libsql::Value;

/// The stored repository, versions, policy flag and owner, from their singleton rows.
const PRINCIPAL_ROW: &str = "SELECT s.repository_id,s.storage_version,s.projection_version,s.policy_complete,o.owner_id FROM g_storage s CROSS JOIN g_local_owner o WHERE s.singleton=1 AND o.singleton=1";

/// The stored checkout count beside the actual number of registrations.
const CHECKOUT_INVENTORY: &str =
    "SELECT checkout_count,(SELECT count(*) FROM g_checkout) FROM g_storage WHERE singleton=1";

/// The three native compatibility markers, in name order.
const NATIVE_MARKERS: &str = "SELECT name,value FROM config WHERE name IN ('project-code','content-schema','aux-schema') ORDER BY name";

/// Sealed local-owner context read from its opened file, never a caller-selected capability.
pub(super) struct Principal {
    /// Repository identity admitted from this local file.
    pub(super) repository: String,
    /// Owner binding admitted through local operating-system access.
    pub(super) owner: String,
}

/// Refuse a principal whose identities cannot name a local repository and owner.
///
/// # Arguments
/// * `principal` - Sealed local repository and owner binding.
///
/// # Returns
/// Unit when the owner is nonempty and the repository is forty lowercase hexadecimal digits.
///
/// # Errors
/// Rejects an empty owner or a malformed repository identity.
fn check_identity(principal: &Principal) -> Result<(), StorageError> {
    let hex = |b: u8| b.is_ascii_digit() || (b'a'..=b'f').contains(&b);
    if principal.owner.is_empty()
        || principal.repository.len() != 40
        || !principal.repository.bytes().all(hex)
    {
        return Err(StorageError::state(
            "invalid local repository identity binding",
        ));
    }
    Ok(())
}

/// Read the stored catalog version and require it to be one that the route admits.
///
/// # Arguments
/// * `rows` - Materialized rows of [`PRINCIPAL_ROW`].
/// * `admitted` - Catalog versions that the calling route admits.
///
/// # Returns
/// The stored catalog version.
///
/// # Errors
/// Rejects an absent or duplicate singleton, and a version outside `admitted`.
fn stored_version(rows: &[Vec<Value>], admitted: &[i64]) -> Result<i64, StorageError> {
    match rows {
        [row] => match row.get(1) {
            Some(Value::Integer(version)) if admitted.contains(version) => Ok(*version),
            _ => Err(StorageError::state("unsupported storage version")),
        },
        _ => Err(StorageError::state(
            "repository principal or complete policy unavailable",
        )),
    }
}

/// Require the checkout count of catalog 4 to be zero or one, and to equal the registrations.
///
/// # Arguments
/// * `rows` - Materialized rows of [`CHECKOUT_INVENTORY`].
///
/// # Returns
/// Unit when the stored count is zero or one and equals the actual count.
///
/// # Errors
/// Rejects any other count, and a malformed row.
fn check_inventory(rows: &[Vec<Value>]) -> Result<(), StorageError> {
    let exact = matches!(
        rows,
        [row] if matches!(
            row.as_slice(),
            [Value::Integer(count @ (0 | 1)), Value::Integer(actual)] if count == actual
        )
    );
    if !exact {
        return Err(StorageError::state(
            "unsupported populated catalog4 checkout scope",
        ));
    }
    Ok(())
}

/// Require the singleton rows to bind this principal under complete policy.
///
/// # Arguments
/// * `rows` - Materialized rows of [`PRINCIPAL_ROW`].
/// * `principal` - Sealed local repository and owner binding.
/// * `version` - Stored catalog version that [`stored_version`] admitted.
///
/// # Returns
/// Unit when the repository, versions, policy flag and owner all match.
///
/// # Errors
/// Rejects any difference, including incomplete policy.
fn check_principal(
    rows: &[Vec<Value>],
    principal: &Principal,
    version: i64,
) -> Result<(), StorageError> {
    let expected = [vec![
        Value::Text(principal.repository.clone()),
        Value::Integer(version),
        Value::Text("gest-ticket-v1".into()),
        Value::Integer(1),
        Value::Text(principal.owner.clone()),
    ]];
    if rows != expected {
        return Err(StorageError::state(
            "repository principal or complete policy unavailable",
        ));
    }
    Ok(())
}

/// Require the native markers of content schema 2 for this repository.
///
/// # Arguments
/// * `rows` - Materialized rows of [`NATIVE_MARKERS`].
/// * `principal` - Sealed local repository binding that the project code must name.
///
/// # Returns
/// Unit when exactly the three expected markers exist.
///
/// # Errors
/// Rejects a missing, extra or different marker.
fn check_markers(rows: &[Vec<Value>], principal: &Principal) -> Result<(), StorageError> {
    let expected = [
        vec![
            Value::Text("aux-schema".into()),
            Value::Text("2015-01-24".into()),
        ],
        vec![
            Value::Text("content-schema".into()),
            Value::Text("2".into()),
        ],
        vec![
            Value::Text("project-code".into()),
            Value::Text(principal.repository.clone()),
        ],
    ];
    if rows != expected {
        return Err(StorageError::state(
            "unsupported or missing native repository markers",
        ));
    }
    Ok(())
}

impl WriteTransaction<'_> {
    /// Validate durable policy before any private existence lookup.
    /// # Arguments
    /// * `principal` - Sealed local repository and owner binding.
    /// # Returns
    /// Success when durable policy matches the sealed local binding and catalog4 has an exact zero-or-one checkout count.
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
            check_identity(principal)?;
            self.validate_catalog().await?;
            let rows = self.query_bound(PRINCIPAL_ROW, vec![]).await?;
            let version = stored_version(&rows, &[1, 4])?;
            if version == 4 {
                check_inventory(&self.query_bound(CHECKOUT_INVENTORY, vec![]).await?)?;
            }
            check_principal(&rows, principal, version)?;
            let markers = self.query_bound(NATIVE_MARKERS, vec![]).await?;
            check_markers(&markers, principal)
        }
        .await;
        self.record(result)
    }

    /// Validate durable policy on the newest catalog, and charge every result to the
    /// operation's allowances.
    ///
    /// The checks are those of [`Self::validate_principal`], except that only
    /// [`NEWEST_CATALOG`] is admitted.
    ///
    /// # Arguments
    /// * `principal` - Sealed local repository and owner binding.
    /// * `sql` - The operation's remaining SQL rows and payload bytes.
    /// * `content` - The operation's decoded content budget, with no reset.
    ///
    /// # Returns
    /// Unit when the newest catalog, complete policy, owner, checkout count and native
    /// markers all bind this principal.
    ///
    /// # Errors
    /// Returns an earlier latched failure, and latches an invalid identity, an older or
    /// unsupported catalog, mismatched policy, exhausted SQL or content work and engine
    /// failures.
    pub(super) async fn validate_principal_with_sql(
        &mut self,
        principal: &Principal,
        sql: &mut SqlWork,
        content: &mut Budget,
    ) -> Result<(), StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            check_identity(principal)?;
            self.validate_catalog_with_sql(sql, content).await?;
            let rows = self
                .query_bound_work(PRINCIPAL_ROW, vec![], 5, sql, content)
                .await?;
            let version = stored_version(&rows, &[NEWEST_CATALOG])?;
            let inventory = self
                .query_bound_work(CHECKOUT_INVENTORY, vec![], 2, sql, content)
                .await?;
            check_inventory(&inventory)?;
            check_principal(&rows, principal, version)?;
            let markers = self
                .query_bound_work(NATIVE_MARKERS, vec![], 2, sql, content)
                .await?;
            check_markers(&markers, principal)
        }
        .await;
        self.record(result)
    }
}

#[cfg(test)]
/// Unit tests of the row checks that the policy routes share.
mod tests {
    use libsql::Value;

    /// The singleton must hold an integer version that the route admits.
    #[test]
    fn the_stored_version_must_be_an_admitted_integer() {
        let row = |version| vec![vec![Value::Integer(1), version]];
        assert_eq!(
            super::stored_version(&row(Value::Integer(4)), &[4]).unwrap(),
            4
        );
        for (rows, admitted) in [
            (row(Value::Integer(1)), [4]),
            (row(Value::Text("4".into())), [4]),
            (vec![vec![Value::Integer(1)]], [4]),
        ] {
            assert_eq!(
                super::stored_version(&rows, &admitted)
                    .unwrap_err()
                    .operation,
                "unsupported storage version"
            );
        }
        for rows in [
            vec![],
            [row(Value::Integer(4)), row(Value::Integer(4))].concat(),
        ] {
            assert_eq!(
                super::stored_version(&rows, &[4]).unwrap_err().operation,
                "repository principal or complete policy unavailable"
            );
        }
    }
}
