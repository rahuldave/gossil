//! Exercise composed byte primitives through the private transaction boundary.
use super::*;
use crate::{compression, content_hash, delta};

/// Persist packed source and delta bytes, restore their content, and reject a partial write.
#[tokio::test]
async fn packed_delta_round_trip_and_atomic_rollback() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("composition.db");
    let source = b"shared prefix: abcdefghijklmnopqrstuvwxyz0123456789\0\xff original ending";
    let target = b"shared prefix: abcdefghijklmnopqrstuvwxyz0123456789\0\xff revised ending\x80";
    let encoded = delta::create(source, target).unwrap();
    let digest = content_hash::sha3(target, 256).unwrap();
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    tx.execute("CREATE TABLE composed_fixture(kind TEXT PRIMARY KEY, bytes BLOB NOT NULL)")
        .await
        .unwrap();
    for (kind, bytes) in [
        ("source", compression::pack(source, -1).unwrap()),
        ("delta", compression::pack(&encoded, -1).unwrap()),
        ("digest", digest.clone()),
    ] {
        tx.execute(&format!(
            "INSERT INTO composed_fixture VALUES('{kind}',x'{}')",
            content_hash::hex(&bytes)
        ))
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    drop(store);

    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let persisted = tx
        .query("SELECT kind,bytes FROM composed_fixture ORDER BY kind")
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    let bytes = |kind: &str| {
        let row = persisted
            .iter()
            .find(|row| row[0] == Value::Text(kind.into()))
            .unwrap();
        let Value::Blob(bytes) = &row[1] else {
            panic!("fixture content must retain its BLOB type");
        };
        bytes.clone()
    };
    let compression::Unpacked::Bytes(restored_source) =
        compression::unpack(&bytes("source"), source.len()).unwrap()
    else {
        panic!("packed source must contain a payload");
    };
    let compression::Unpacked::Bytes(restored_delta) =
        compression::unpack(&bytes("delta"), encoded.len()).unwrap()
    else {
        panic!("packed delta must contain a payload");
    };
    let restored = delta::apply(
        &restored_source,
        &restored_delta,
        delta::Limits {
            max_output_bytes: target.len() as u32,
        },
    )
    .unwrap();
    assert_eq!(restored, target);
    assert_eq!(content_hash::sha3(&restored, 256).unwrap(), bytes("digest"));
    assert_eq!(bytes("digest"), digest);

    let mut tx = store.begin_write().await.unwrap();
    tx.execute("UPDATE composed_fixture SET bytes=x'00' WHERE kind='source'")
        .await
        .unwrap();
    tx.execute("INSERT INTO composed_fixture VALUES('partial',x'ff')")
        .await
        .unwrap();
    assert!(
        tx.execute("INSERT INTO composed_fixture VALUES('delta',x'80')")
            .await
            .is_err()
    );
    assert!(tx.commit().await.is_err());
    drop(store);
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    assert_eq!(
        tx.query("SELECT kind,bytes FROM composed_fixture ORDER BY kind")
            .await
            .unwrap(),
        persisted
    );
    tx.rollback().await.unwrap();
}
