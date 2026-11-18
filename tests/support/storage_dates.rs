//! Characterize the reviewed local provider through the private storage boundary.
use super::*;

/// Compare SQLite storage classes and binary64 values, including SQL NULL.
#[tokio::test]
async fn pinned_date_and_legacy_numeric_corpora() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = LocalStore::open(temp.path().join("dates.db"))
        .await
        .unwrap();
    let mut tx = store.begin_write().await.unwrap();
    for (corpus, count) in [
        (include_str!("../fixtures/libsql-date/main.json"), 489),
        (include_str!("../fixtures/libsql-date/threshold.json"), 30),
    ] {
        let cases: serde_json::Value = serde_json::from_str(corpus).unwrap();
        assert_eq!(cases.as_array().unwrap().len(), count);
        for case in cases.as_array().unwrap() {
            let token = case["token"].as_str().unwrap();
            let quoted = format!("'{}'", token.replace('\'', "''"));
            for (i, sql) in [
                format!("SELECT julianday({quoted})"),
                format!("SELECT CAST({quoted} AS REAL)"),
                format!("SELECT {quoted}+0.0"),
            ]
            .iter()
            .enumerate()
            {
                let rows = tx.query(sql).await.unwrap();
                let (kind, bits) = match rows[0][0] {
                    Value::Real(x) => (2, x.to_bits()),
                    Value::Null => (5, 0),
                    _ => panic!("unexpected value for {token:?}: {:?}", rows[0][0]),
                };
                assert_eq!(
                    kind,
                    case["expected"][i]["type"].as_u64().unwrap(),
                    "storage class: {sql}"
                );
                assert_eq!(
                    format!("{bits:016x}"),
                    case["expected"][i]["bits"].as_str().unwrap(),
                    "binary64: {sql}"
                );
            }
        }
    }
    tx.rollback().await.unwrap();
}
