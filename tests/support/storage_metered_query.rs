//! Behavior tests of metered SQL queries against a real local libSQL store.
use super::*;
use crate::storage::{Limits, LocalStore};
use std::path::Path;

/// Size of one materialized SQL value.
const CELL: usize = std::mem::size_of::<Value>();
/// The fixture query: two rows of an integer and a text or null.
const PROBE: &str = "SELECT id, label FROM probe ORDER BY id";

/// A content budget whose only binding ceiling is the decoded total.
fn content(decoded: u64) -> Budget {
    Budget::new(Limits {
        max_stored_bytes: 1024,
        max_output_bytes: 1024,
        max_chain_depth: 4,
        max_total_stored_bytes: 8192,
        max_total_decoded_bytes: decoded,
    })
    .unwrap()
}

/// Create a store with the probe table of two committed rows.
async fn store(path: &Path) -> LocalStore {
    let mut store = LocalStore::open(path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    tx.execute("CREATE TABLE probe(id INTEGER PRIMARY KEY, label TEXT)")
        .await
        .unwrap();
    tx.execute("INSERT INTO probe VALUES(1, 'ab'), (2, NULL)")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    store
}

/// The committed probe rows.
fn probe_rows() -> Vec<Vec<Value>> {
    vec![
        vec![Value::Integer(1), Value::Text("ab".into())],
        vec![Value::Integer(2), Value::Null],
    ]
}

/// SQL work of the probe: the preflight row, then two rows of two cells and two text bytes.
const PROBE_WORK: SqlWork = SqlWork {
    rows: 1 + 2,
    bytes: 32 + 2 * 2 * 16 + 2,
};

#[tokio::test(flavor = "current_thread")]
async fn sql_work_admits_the_exact_result_size_and_spends_all_of_it() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = store(&temp.path().join("repo")).await;
    let mut tx = store.begin_write().await.unwrap();
    let mut work = PROBE_WORK;
    assert_eq!(
        tx.query_bound_sql_work(PROBE, vec![], 2, &mut work)
            .await
            .unwrap(),
        probe_rows()
    );
    assert_eq!(work, SqlWork { rows: 0, bytes: 0 });
    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn exhausted_sql_work_latches_rolls_back_and_a_retry_succeeds_after_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let mut store = store(&path).await;
    for (mut work, message) in [
        (
            SqlWork {
                rows: PROBE_WORK.rows - 1,
                ..PROBE_WORK
            },
            "metered query row limit",
        ),
        (
            SqlWork {
                bytes: PROBE_WORK.bytes - 1,
                ..PROBE_WORK
            },
            "metered query byte limit",
        ),
    ] {
        let mut tx = store.begin_write().await.unwrap();
        tx.execute("CREATE TABLE uncommitted(x)").await.unwrap();
        let refused = tx
            .query_bound_sql_work(PROBE, vec![], 2, &mut work)
            .await
            .unwrap_err();
        assert_eq!(refused.operation, message);
        // Only the preflight row was charged; the refused debit changed nothing.
        assert_eq!(
            (work.rows, work.bytes),
            (
                PROBE_WORK.rows - 1 - u64::from(message.contains("row")),
                PROBE_WORK.bytes - 32 - u64::from(message.contains("byte"))
            )
        );
        // The refusal is latched: later SQL returns the same failure.
        let later = tx.query_bound("SELECT 1", vec![]).await.unwrap_err();
        assert_eq!(later.operation, message);
        assert_eq!(tx.rollback().await.unwrap_err().operation, message);
    }
    drop(store);
    // Reopen: the uncommitted table is gone, and a retry with enough work succeeds.
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let mut work = SqlWork { rows: 2, bytes: 48 };
    assert_eq!(
        tx.query_bound_sql_work(
            "SELECT count(*) FROM sqlite_master WHERE name = 'uncommitted'",
            vec![],
            1,
            &mut work
        )
        .await
        .unwrap(),
        vec![vec![Value::Integer(0)]]
    );
    let mut work = PROBE_WORK;
    assert_eq!(
        tx.query_bound_sql_work(PROBE, vec![], 2, &mut work)
            .await
            .unwrap(),
        probe_rows()
    );
    tx.rollback().await.unwrap();
    assert!(!store.is_poisoned());
}

#[tokio::test(flavor = "current_thread")]
async fn sql_work_is_cumulative_across_queries_and_counts_blob_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = store(&temp.path().join("repo")).await;
    let mut tx = store.begin_write().await.unwrap();
    // One blob cell of three bytes and one real cell: 32 bytes plus three.
    let one = 32 + 2 * 16 + 3;
    let mut work = SqlWork {
        rows: 4,
        bytes: 2 * one,
    };
    for _ in 0..2 {
        assert_eq!(
            tx.query_bound_sql_work("SELECT x'000102', 1.5", vec![], 2, &mut work)
                .await
                .unwrap(),
            vec![vec![Value::Blob(vec![0, 1, 2]), Value::Real(1.5)]]
        );
    }
    assert_eq!(work, SqlWork { rows: 0, bytes: 0 });
    let refused = tx
        .query_bound_sql_work("SELECT 1", vec![], 1, &mut work)
        .await
        .unwrap_err();
    assert_eq!(refused.operation, "metered query row limit");
    tx.rollback().await.unwrap_err();
}

#[tokio::test(flavor = "current_thread")]
async fn multibyte_text_is_charged_in_utf8_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = store(&temp.path().join("repo")).await;
    // One character of two UTF-8 bytes: the preflight row, one cell and two bytes.
    let exact = SqlWork {
        rows: 2,
        bytes: 32 + 16 + 2,
    };
    let mut tx = store.begin_write().await.unwrap();
    let mut work = exact;
    assert_eq!(
        tx.query_bound_sql_work("SELECT 'é'", vec![], 1, &mut work)
            .await
            .unwrap(),
        vec![vec![Value::Text("é".into())]]
    );
    assert_eq!(work, SqlWork { rows: 0, bytes: 0 });
    tx.rollback().await.unwrap();

    let mut tx = store.begin_write().await.unwrap();
    let mut work = SqlWork {
        bytes: exact.bytes - 1,
        ..exact
    };
    let refused = tx
        .query_bound_sql_work("SELECT 'é'", vec![], 1, &mut work)
        .await
        .unwrap_err();
    assert_eq!(refused.operation, "metered query byte limit");
    tx.rollback().await.unwrap_err();
}

#[tokio::test(flavor = "current_thread")]
async fn content_work_charges_parameters_and_retained_cells_exactly() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = store(&temp.path().join("repo")).await;
    let sql = "SELECT id, label FROM probe WHERE label = ?1 OR ?2 IS NOT NULL ORDER BY id";
    let params = || vec![Value::Text("ab".into()), Value::Blob(vec![7; 5])];
    // Parameters: the preflight row, then each value and its bytes.
    let parameters = 2 * CELL + (CELL + 2) + (CELL + 5);
    // Result: four retained cells and two text bytes.
    let retained = 4 * CELL + 2;
    let exact = (parameters + retained) as u64;

    let mut tx = store.begin_write().await.unwrap();
    let mut work = PROBE_WORK;
    let mut budget = content(exact);
    assert_eq!(
        tx.query_bound_work(sql, params(), 2, &mut work, &mut budget)
            .await
            .unwrap(),
        probe_rows()
    );
    assert_eq!(work, SqlWork { rows: 0, bytes: 0 });
    // The content budget is spent exactly.
    assert!(budget.charge_decoded(1).is_err());
    tx.rollback().await.unwrap();

    // One byte short fails after the SQL debit, and the failure latches.
    let mut tx = store.begin_write().await.unwrap();
    let mut work = PROBE_WORK;
    let mut budget = content(exact - 1);
    let refused = tx
        .query_bound_work(sql, params(), 2, &mut work, &mut budget)
        .await
        .unwrap_err();
    assert_eq!(refused.operation, "total decoded content limit");
    assert_eq!(work, SqlWork { rows: 0, bytes: 0 });
    budget.charge_decoded(retained - 1).unwrap();
    assert_eq!(
        tx.rollback().await.unwrap_err().operation,
        refused.operation
    );
}

#[tokio::test(flavor = "current_thread")]
async fn content_exhausted_by_parameters_spends_no_sql_work() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = store(&temp.path().join("repo")).await;
    let mut tx = store.begin_write().await.unwrap();
    let mut work = PROBE_WORK;
    let mut budget = content((3 * CELL) as u64 - 1);
    let refused = tx
        .query_bound_work(
            "SELECT ?1",
            vec![Value::Integer(1)],
            1,
            &mut work,
            &mut budget,
        )
        .await
        .unwrap_err();
    assert_eq!(refused.operation, "total decoded content limit");
    assert_eq!(work, PROBE_WORK);
    tx.rollback().await.unwrap_err();
}

#[tokio::test(flavor = "current_thread")]
async fn invalid_and_mismatched_column_counts_are_refused() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = store(&temp.path().join("repo")).await;
    for columns in [0, 129] {
        let mut tx = store.begin_write().await.unwrap();
        let mut work = PROBE_WORK;
        let refused = tx
            .query_bound_sql_work(PROBE, vec![], columns, &mut work)
            .await
            .unwrap_err();
        assert_eq!(refused.operation, "metered query column limit");
        assert_eq!(work, PROBE_WORK);
        tx.rollback().await.unwrap_err();
    }
    // A declared count that differs from the query is an engine error after the
    // preflight row was charged.
    let mut tx = store.begin_write().await.unwrap();
    let mut work = PROBE_WORK;
    let mut budget = content(1 << 20);
    let refused = tx
        .query_bound_work(PROBE, vec![], 3, &mut work, &mut budget)
        .await
        .unwrap_err();
    assert!(refused.source.is_some(), "{refused}");
    assert_eq!(
        work,
        SqlWork {
            rows: PROBE_WORK.rows - 1,
            bytes: PROBE_WORK.bytes - 32
        }
    );
    tx.rollback().await.unwrap_err();
    // The widest supported count is accepted.
    let wide = (0..128)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let mut tx = store.begin_write().await.unwrap();
    let mut work = SqlWork {
        rows: 2,
        bytes: 32 + 128 * 16,
    };
    let rows = tx
        .query_bound_sql_work(&format!("SELECT {wide}"), vec![], 128, &mut work)
        .await
        .unwrap();
    assert_eq!(rows[0].len(), 128);
    tx.rollback().await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn an_earlier_failure_returns_first_and_charges_nothing() {
    let temp = tempfile::tempdir().unwrap();
    let mut store = store(&temp.path().join("repo")).await;
    let mut tx = store.begin_write().await.unwrap();
    let original = tx
        .query_bound("SELECT * FROM missing_table", vec![])
        .await
        .unwrap_err();
    let mut work = PROBE_WORK;
    let mut budget = content(1 << 20);
    let sql_only = tx
        .query_bound_sql_work(PROBE, vec![], 2, &mut work)
        .await
        .unwrap_err();
    let with_content = tx
        .query_bound_work(PROBE, vec![], 2, &mut work, &mut budget)
        .await
        .unwrap_err();
    for later in [sql_only, with_content] {
        assert_eq!(later.operation, original.operation);
        assert!(std::sync::Arc::ptr_eq(
            later.source.as_ref().unwrap(),
            original.source.as_ref().unwrap()
        ));
    }
    assert_eq!(work, PROBE_WORK);
    budget.charge_decoded(1 << 20).unwrap();
    tx.rollback().await.unwrap_err();
}

#[test]
fn debit_changes_neither_count_when_one_is_short() {
    let mut work = SqlWork { rows: 5, bytes: 10 };
    assert_eq!(
        work.debit(1, 11).unwrap_err().operation,
        "metered query byte limit"
    );
    assert_eq!(work, SqlWork { rows: 5, bytes: 10 });
    assert_eq!(
        work.debit(6, 1).unwrap_err().operation,
        "metered query row limit"
    );
    assert_eq!(work, SqlWork { rows: 5, bytes: 10 });
    work.debit(5, 10).unwrap();
    assert_eq!(work, SqlWork { rows: 0, bytes: 0 });
}

#[test]
fn preflight_results_of_the_wrong_shape_are_refused() {
    for (rows, message) in [
        (vec![], "metered query size unavailable"),
        (
            vec![
                vec![Value::Integer(1), Value::Integer(0)],
                vec![Value::Integer(1), Value::Integer(0)],
            ],
            "metered query size unavailable",
        ),
        (
            vec![vec![Value::Integer(1), Value::Real(0.0)]],
            "metered query size class",
        ),
        (vec![vec![Value::Integer(1)]], "metered query size class"),
        (
            vec![vec![Value::Integer(-1), Value::Integer(0)]],
            "metered query negative size",
        ),
        (
            vec![vec![Value::Integer(0), Value::Integer(-1)]],
            "metered query negative size",
        ),
    ] {
        assert_eq!(ResultSize::decode(&rows).unwrap_err().operation, message);
    }
    assert_eq!(
        ResultSize::decode(&[vec![Value::Integer(3), Value::Integer(4)]]).unwrap(),
        ResultSize {
            rows: 3,
            payload: 4
        }
    );
}

#[test]
fn result_charges_refuse_overflow() {
    let size = ResultSize {
        rows: 3,
        payload: 4,
    };
    assert_eq!(size.sql_bytes(2).unwrap(), 3 * 2 * 16 + 4);
    assert_eq!(size.retained_bytes(2).unwrap(), 3 * 2 * CELL + 4);
    for huge in [
        ResultSize {
            rows: u64::MAX,
            payload: 0,
        },
        ResultSize {
            rows: 1,
            payload: u64::MAX,
        },
    ] {
        assert_eq!(
            huge.sql_bytes(1).unwrap_err().operation,
            "metered query size overflow"
        );
        assert_eq!(
            huge.retained_bytes(1).unwrap_err().operation,
            "metered query size overflow"
        );
    }
}

#[test]
fn materialized_results_must_match_the_charged_size() {
    let size = ResultSize {
        rows: 1,
        payload: 0,
    };
    let row = vec![Value::Integer(1)];
    assert_eq!(size.admit(vec![row.clone()], 1).unwrap(), vec![row.clone()]);
    // Text and blob bytes count; integers and reals do not.
    let mixed = vec![
        Value::Text("ab".into()),
        Value::Blob(vec![0; 3]),
        Value::Real(1.0),
    ];
    let charged = ResultSize {
        rows: 1,
        payload: 5,
    };
    assert_eq!(
        charged.admit(vec![mixed.clone()], 3).unwrap(),
        vec![mixed.clone()]
    );
    for payload in [4, 6] {
        assert_eq!(
            ResultSize { rows: 1, payload }
                .admit(vec![mixed.clone()], 3)
                .unwrap_err()
                .operation,
            "metered query inventory changed"
        );
    }
    for (output, columns) in [
        (vec![], 1),
        (vec![row.clone(), row.clone()], 1),
        (vec![row.clone()], 2),
    ] {
        assert_eq!(
            size.admit(output, columns).unwrap_err().operation,
            "metered query inventory changed"
        );
    }
}
