//! Install the six accepted DDL inputs inside the borrowed writer.
//!
//! ## Recommendations
//! Install only as part of complete repository initialization; validate before admission.
use super::{StorageError, WriteTransaction};

/// Exact reviewed schema inputs; order follows their foreign-key dependencies.
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
            let rows = self.query_bound("SELECT type,name,tbl_name,sql FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name", vec![]).await?;
            #[cfg(test)]
            let rows = catalog_injection::ROWS.with(|slot| slot.borrow_mut().take()).unwrap_or(rows);
            let required: std::collections::BTreeSet<&str> = include_str!("schema/required-names.txt").lines().collect();
            let mut seen = std::collections::BTreeSet::new();
            let mut bytes = Vec::new();
            for row in rows {
                let Some(libsql::Value::Text(name)) = row.get(1) else { return Err(StorageError::state("corrupt schema object name")); };
                if !required.contains(name.as_str()) {
                    let trigger = row.first() == Some(&libsql::Value::Text("trigger".into()));
                    let reason = if trigger { "unregistered persistent trigger" }
                        else if name.starts_with("fx_") || name.starts_with("ticket") || name.starts_with("fts") || name.starts_with("subscriber") || name.starts_with("alert") {
                            "unsupported optional native schema family"
                        } else { "unregistered persistent schema object" };
                    return Err(StorageError::state(format!("{reason}: {name}")));
                }
                seen.insert(name.clone());
                for value in row {
                    encode_catalog_value(value, &mut bytes)?;
                }
            }
            if seen.len() != required.len() { return Err(StorageError::state("missing required repository catalog object")); }
            let hash = crate::content_hash::hex(&crate::content_hash::sha3(&bytes, 256).unwrap());
            if hash != "35862e3d85100f9415d63bbb04e6a8e9540e2a53595de25bc44a7ddf478c3e1e" { return Err(StorageError::state(format!("unsupported repository catalog: {hash}"))); }
            Ok(())
        }.await;
        self.record(result)
    }
    /// Install the complete accepted schema, never an independently usable product stage.
    /// # Returns
    /// Success after installing all required schema objects.
    /// # Errors
    /// Returns SQL errors while installing the required schema.
    pub(super) async fn install_schema(&mut self) -> Result<(), StorageError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        let result = async {
            for sql in DDL {
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
}
