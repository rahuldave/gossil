//! Install six accepted DDL inputs for catalog1 or empty catalog4 inside the borrowed writer.
//!
//! ## Recommendations
//! Install only as part of complete repository initialization; validate before admission.
use super::{StorageError, WriteTransaction, content::Budget, metered_query::SqlWork};

/// Exact catalog1 schema inputs; catalog4 substitutes its adapter at installation.
const DDL: [&str; 6] = [
    include_str!("schema/native-repository.sql"),
    include_str!("schema/adapter-v4.sql"),
    include_str!("schema/checkout.sql"),
    include_str!("schema/local-bindings-v3.sql"),
    include_str!("schema/typed-projections.sql"),
    include_str!("schema/discussion-projections-v3.sql"),
];

#[cfg(test)]
pub(super) mod catalog_injection {
    use std::cell::RefCell;
    thread_local! {
        pub(in crate::storage) static ROWS: RefCell<Option<Vec<Vec<libsql::Value>>>> = const { RefCell::new(None) };
    }
    pub(in crate::storage) struct Reset;
    impl Reset {
        pub(in crate::storage) fn consumed(&self) {
            ROWS.with(|slot| assert!(slot.borrow().is_none()));
        }
    }
    impl Drop for Reset {
        fn drop(&mut self) {
            ROWS.with(|slot| *slot.borrow_mut() = None);
        }
    }
}

/// Encode owned catalog fields, including native nullable SQL definitions.
/// # Arguments
/// * `value` - Owned typed catalog field to encode.
/// * `bytes` - Input bytes for this operation.
/// # Returns
/// Success after encoding the typed catalog field.
/// # Errors
/// Rejects unsupported native catalog value types.
fn encode_catalog_value(value: libsql::Value, bytes: &mut Vec<u8>) -> Result<(), StorageError> {
    match value {
        libsql::Value::Text(value) => {
            bytes.extend_from_slice(&(value.len() as u64).to_be_bytes());
            bytes.extend_from_slice(value.as_bytes());
        }
        libsql::Value::Null => bytes.extend_from_slice(&u64::MAX.to_be_bytes()),
        _ => return Err(StorageError::state("corrupt schema catalog value")),
    }
    Ok(())
}

/// The catalog that this build creates and that new routes admit (CAT-04).
///
/// Older routes still admit catalog 1. A metered route admits only this catalog.
pub(super) const NEWEST_CATALOG: i64 = 4;

/// Every schema object outside SQLite's own, with its stored definition, in a fixed order.
const CATALOG_ROWS: &str = "SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name";

/// At most two storage markers, so that a duplicate marker is visible.
const MARKER_ROWS: &str = "SELECT storage_version FROM g_storage LIMIT 2";

/// Count the bytes that [`catalog_version`] encodes for `rows`, before it encodes them.
///
/// # Arguments
/// * `rows` - Materialized schema rows of [`CATALOG_ROWS`].
///
/// # Returns
/// Eight length bytes for each value, plus the bytes of each text value.
///
/// # Errors
/// Rejects arithmetic overflow.
fn encoded_catalog_len(rows: &[Vec<libsql::Value>]) -> Result<usize, StorageError> {
    rows.iter()
        .flatten()
        .try_fold(0usize, |total, value| {
            let text = match value {
                libsql::Value::Text(text) => text.len(),
                _ => 0,
            };
            total.checked_add(8)?.checked_add(text)
        })
        .ok_or_else(|| StorageError::state("catalog encoding size overflow"))
}

/// Map the schema rows of a repository to the catalog that they define.
///
/// # Arguments
/// * `rows` - Materialized schema rows of [`CATALOG_ROWS`].
///
/// # Returns
/// The catalog version whose registered fingerprint the rows match.
///
/// # Errors
/// Rejects an unregistered object, a missing required object, a value of the wrong
/// type and a fingerprint that no supported catalog has.
fn catalog_version(rows: Vec<Vec<libsql::Value>>) -> Result<i64, StorageError> {
    use std::collections::BTreeSet;
    let required: BTreeSet<&str> = include_str!("schema/required-names.txt").lines().collect();
    let required4: BTreeSet<&str> = include_str!("schema/required-names-catalog4.txt")
        .lines()
        .collect();
    let mut seen = BTreeSet::new();
    let mut bytes = Vec::new();
    for row in rows {
        let Some(libsql::Value::Text(name)) = row.get(1) else {
            return Err(StorageError::state("corrupt schema object name"));
        };
        if !required.contains(name.as_str()) && !required4.contains(name.as_str()) {
            let trigger = row.first() == Some(&libsql::Value::Text("trigger".into()));
            let optional = ["fx_", "ticket", "fts", "subscriber", "alert"]
                .iter()
                .any(|prefix| name.starts_with(prefix));
            let reason = if trigger {
                "unregistered persistent trigger"
            } else if optional {
                "unsupported optional native schema family"
            } else {
                "unregistered persistent schema object"
            };
            return Err(StorageError::state(format!("{reason}: {name}")));
        }
        seen.insert(name.clone());
        for value in row {
            encode_catalog_value(value, &mut bytes)?;
        }
    }
    let seen: BTreeSet<&str> = seen.iter().map(String::as_str).collect();
    if seen != required && seen != required4 {
        return Err(StorageError::state(
            "missing required repository catalog object",
        ));
    }
    let digest = crate::content_hash::sha3(&bytes, 256)
        .ok_or_else(|| StorageError::state("catalog fingerprint unavailable"))?;
    match crate::content_hash::hex(&digest).as_str() {
        "35862e3d85100f9415d63bbb04e6a8e9540e2a53595de25bc44a7ddf478c3e1e" => Ok(1),
        "8ba7ef53cb917460fbe325d04b1078f4f64aaf7d7438756b44cd7776dbca49e4" => Ok(4),
        hash => Err(StorageError::state(format!(
            "unsupported repository catalog: {hash}"
        ))),
    }
}

/// Require any stored marker to name the catalog that the schema defines.
///
/// During construction, a bare schema has no marker yet.
///
/// # Arguments
/// * `markers` - Materialized rows of [`MARKER_ROWS`].
/// * `version` - Catalog version that the schema rows define.
///
/// # Returns
/// Unit when no marker exists or the one marker equals `version`.
///
/// # Errors
/// Rejects a different, duplicate or mistyped marker.
fn check_marker(markers: &[Vec<libsql::Value>], version: i64) -> Result<(), StorageError> {
    if !markers.is_empty() && markers != [vec![libsql::Value::Integer(version)]] {
        return Err(StorageError::state(
            "storage marker differs from exact repository catalog",
        ));
    }
    Ok(())
}

impl WriteTransaction<'_> {
    /// Compare every catalog object and stored definition against the accepted full schema.
    /// # Returns
    /// Success when the stored schema exactly matches the required catalog.
    /// # Errors
    /// Rejects missing or changed required objects, unsupported schemas, unexpected triggers and SQL errors.
    pub(super) async fn validate_catalog(&mut self) -> Result<(), StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            let rows = self.query_bound(CATALOG_ROWS, vec![]).await?;
            #[cfg(test)]
            let rows = catalog_injection::ROWS
                .with(|slot| slot.borrow_mut().take())
                .unwrap_or(rows);
            let version = catalog_version(rows)?;
            let markers = self.query_bound(MARKER_ROWS, vec![]).await?;
            check_marker(&markers, version)
        }
        .await;
        self.record(result)
    }

    /// Admit only the newest catalog, and charge every result to the operation's allowances.
    ///
    /// The checks are those of [`Self::validate_catalog`]. This route also refuses an
    /// older catalog that the schema defines exactly.
    ///
    /// # Arguments
    /// * `sql` - The operation's remaining SQL rows and payload bytes.
    /// * `content` - The operation's decoded content budget, which also pays for the
    ///   fingerprint encoding.
    ///
    /// # Returns
    /// Unit when the schema is exactly the newest catalog and any marker agrees.
    ///
    /// # Errors
    /// Returns an earlier latched failure, and latches every schema refusal, an older
    /// catalog, exhausted SQL or content work and engine failures.
    pub(super) async fn validate_catalog_with_sql(
        &mut self,
        sql: &mut SqlWork,
        content: &mut Budget,
    ) -> Result<(), StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            let rows = self
                .query_bound_work(CATALOG_ROWS, vec![], 4, sql, content)
                .await?;
            content.charge_decoded(encoded_catalog_len(&rows)?)?;
            let version = catalog_version(rows)?;
            if version != NEWEST_CATALOG {
                return Err(StorageError::state(format!(
                    "repository catalog {version} is not the newest catalog {NEWEST_CATALOG}"
                )));
            }
            let markers = self
                .query_bound_work(MARKER_ROWS, vec![], 1, sql, content)
                .await?;
            check_marker(&markers, version)
        }
        .await;
        self.record(result)
    }
    /// Install the complete catalog1 schema for bound repository creation.
    /// # Returns
    /// Success after installing all required schema objects.
    /// # Errors
    /// Returns SQL errors while installing the required schema.
    pub(super) async fn install_schema(&mut self) -> Result<(), StorageError> {
        self.install_schema_catalog(false).await
    }
    /// Install the exact catalog4 adapter with the five unchanged base DDL inputs.
    /// # Returns
    /// Success after all schema inputs execute inside the owned writer.
    /// # Errors
    /// Returns the first latched failure or SQL installation error.
    pub(super) async fn install_schema_catalog4(&mut self) -> Result<(), StorageError> {
        self.install_schema_catalog(true).await
    }
    /// Select the constructor's exact schema without migrating an existing file.
    /// # Arguments
    /// * `catalog4` - Select the unbound native initializer adapter.
    /// # Returns
    /// Success after complete installation.
    /// # Errors
    /// Preserves the first SQL or transaction failure.
    async fn install_schema_catalog(&mut self, catalog4: bool) -> Result<(), StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            for (index, sql) in DDL.into_iter().enumerate() {
                let sql = if catalog4 && index == 1 {
                    include_str!("schema/adapter-catalog4.sql")
                } else {
                    sql
                };
                self.connection().execute_batch(sql).await?;
            }
            Ok(())
        }
        .await;
        self.record(result)
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::LocalStore;
    use libsql::Value;

    #[tokio::test(flavor = "current_thread")]
    async fn owned_catalog_fields_decode_native_null_and_reject_typed_corruption_atomically() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("catalog-result-rollback");
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("CREATE TABLE audit(value)").await.unwrap();
        tx.commit().await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let rows = tx.query("SELECT NULL, 'native', 42").await.unwrap();
        let mut encoded = Vec::new();
        super::encode_catalog_value(rows[0][0].clone(), &mut encoded).unwrap();
        super::encode_catalog_value(rows[0][1].clone(), &mut encoded).unwrap();
        assert_eq!(
            encoded,
            [
                u64::MAX.to_be_bytes().as_slice(),
                6u64.to_be_bytes().as_slice(),
                b"native"
            ]
            .concat()
        );
        tx.execute("INSERT INTO audit VALUES(1)").await.unwrap();
        let reset = super::catalog_injection::Reset;
        super::catalog_injection::ROWS.with(|slot| {
            *slot.borrow_mut() = Some(vec![vec![
                Value::Text("table".into()),
                Value::Text("g_storage".into()),
                Value::Text("g_storage".into()),
                rows[0][2].clone(),
            ]])
        });
        let failure = tx.validate_catalog().await.unwrap_err();
        reset.consumed();
        assert_eq!(failure.operation, "corrupt schema catalog value");
        assert_eq!(tx.commit().await.unwrap_err().operation, failure.operation);
        drop(store);
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        assert_eq!(
            tx.query("SELECT count(*) FROM audit").await.unwrap(),
            vec![vec![Value::Integer(0)]]
        );
        tx.rollback().await.unwrap();
    }
    /// The full DDL, including the checkout trigger, installs and rolls back atomically.
    #[tokio::test]
    async fn full_schema_install_is_owned_and_atomic() {
        let mut store = LocalStore::open_inner(":memory:", false).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.install_schema().await.unwrap();
        let catalog = tx
            .query("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")
            .await
            .unwrap();
        assert!(catalog.len() > 70);
        assert!(catalog.contains(&vec![Value::Text("g_storage".into())]));
        assert!(catalog.contains(&vec![Value::Text("g_vmerge".into())]));
        tx.rollback().await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        assert_eq!(
            tx.query("SELECT count(*) FROM sqlite_schema")
                .await
                .unwrap(),
            vec![vec![Value::Integer(0)]]
        );
        tx.install_schema().await.unwrap();
        tx.validate_catalog().await.unwrap();
        tx.commit().await.unwrap();
    }

    /// The encoded length counts eight bytes for every value and the bytes of text only.
    #[test]
    fn encoded_catalog_length_counts_text_bytes_and_eight_bytes_per_value() {
        let rows = vec![vec![
            Value::Text("table".into()),
            Value::Null,
            Value::Integer(7),
        ]];
        assert_eq!(super::encoded_catalog_len(&rows).unwrap(), 3 * 8 + 5);
    }

    /// A schema row whose name is not text is corrupt, before any fingerprint.
    #[test]
    fn a_schema_row_without_a_text_name_is_corrupt() {
        for name in [Value::Null, Value::Integer(1)] {
            let rows = vec![vec![Value::Text("table".into()), name]];
            assert_eq!(
                super::catalog_version(rows).unwrap_err().operation,
                "corrupt schema object name"
            );
        }
    }

    /// No marker or the one equal marker passes, and every other marker set is refused.
    #[test]
    fn the_storage_marker_must_be_absent_or_equal_the_catalog() {
        super::check_marker(&[], 4).unwrap();
        super::check_marker(&[vec![Value::Integer(4)]], 4).unwrap();
        for markers in [
            vec![vec![Value::Integer(1)]],
            vec![vec![Value::Text("4".into())]],
            vec![vec![Value::Integer(4)], vec![Value::Integer(4)]],
        ] {
            assert_eq!(
                super::check_marker(&markers, 4).unwrap_err().operation,
                "storage marker differs from exact repository catalog"
            );
        }
    }
}
