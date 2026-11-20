//! Functional lifecycle regressions through the core-only libSQL public API.
//!
//! These bounded checks exercise ordinary destruction and retained handles. They
//! are not deterministic memory diagnostics. Run allocator or sanitizer checks
//! separately against the same dependency composition.

use libsql::{Builder, Connection};

/// Read one integer from a query and require that it returns exactly one row.
///
/// # Panics
/// Panics if the query fails or does not return exactly one integer row.
async fn scalar(connection: &Connection, sql: &str) -> i64 {
    let mut rows = connection.query(sql, ()).await.unwrap();
    let row = rows.next().await.unwrap().unwrap();
    let value = row.get::<i64>(0).unwrap();
    assert!(rows.next().await.unwrap().is_none());
    value
}

/// Check that ordinary connection destruction preserves committed rows on reopen.
///
/// # Panics
/// Panics if setup, SQL execution or a persistence assertion fails.
#[tokio::test]
async fn plain_connection_drop_and_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("plain.db");
    for iteration in 0..8_i64 {
        let database = Builder::new_local(&path).build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute("CREATE TABLE IF NOT EXISTS entries (value INTEGER)", ())
            .await
            .unwrap();
        connection
            .execute("INSERT INTO entries VALUES (?1)", [iteration])
            .await
            .unwrap();
        assert_eq!(
            scalar(&connection, "SELECT count(*) FROM entries").await,
            iteration + 1
        );
        // No explicit close: exercise the ordinary connection/database Drop path.
        drop(connection);
        drop(database);
    }
    let database = Builder::new_local(&path).build().await.unwrap();
    let connection = database.connect().unwrap();
    assert_eq!(
        scalar(&connection, "SELECT sum(value) FROM entries").await,
        28
    );
}

/// Check that statements and rows retain their connection after original handles drop.
///
/// # Panics
/// Panics if setup, SQL execution or a retained-handle assertion fails.
#[tokio::test]
async fn rows_and_statement_retain_connection_after_original_handles_drop() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("retained.db");
    let database = Builder::new_local(&path).build().await.unwrap();
    let connection = database.connect().unwrap();
    connection
        .execute("CREATE TABLE entries (value INTEGER)", ())
        .await
        .unwrap();
    connection
        .execute("INSERT INTO entries VALUES (11), (22)", ())
        .await
        .unwrap();
    let clone = connection.clone();
    let statement = clone
        .prepare("SELECT value FROM entries ORDER BY value")
        .await
        .unwrap();
    drop(connection);
    drop(database);
    assert_eq!(scalar(&clone, "SELECT count(*) FROM entries").await, 2);
    drop(clone);
    let mut rows = statement.query(()).await.unwrap();
    drop(statement);
    assert_eq!(
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
        11
    );
    assert_eq!(
        rows.next().await.unwrap().unwrap().get::<i64>(0).unwrap(),
        22
    );
    assert!(rows.next().await.unwrap().is_none());
    drop(rows);

    let reopened = Builder::new_local(&path).build().await.unwrap();
    let connection = reopened.connect().unwrap();
    assert_eq!(
        scalar(&connection, "SELECT sum(value) FROM entries").await,
        33
    );
}

/// Check connection-local temporary tables and isolation between database files.
///
/// # Panics
/// Panics if setup, SQL execution or an isolation assertion fails.
#[tokio::test]
async fn independent_connections_keep_identity_and_database_isolation() {
    let directory = tempfile::tempdir().unwrap();
    let first = Builder::new_local(directory.path().join("first.db"))
        .build()
        .await
        .unwrap();
    let second = Builder::new_local(directory.path().join("second.db"))
        .build()
        .await
        .unwrap();
    let a = first.connect().unwrap();
    let b = first.connect().unwrap();
    let other = second.connect().unwrap();
    for (connection, value) in [(&a, 101), (&b, 202), (&other, 303)] {
        connection
            .execute("CREATE TEMP TABLE identity (value INTEGER)", ())
            .await
            .unwrap();
        connection
            .execute("INSERT INTO identity VALUES (?1)", [value])
            .await
            .unwrap();
    }
    a.execute("CREATE TABLE stored (value INTEGER)", ())
        .await
        .unwrap();
    a.execute("INSERT INTO stored VALUES (7)", ())
        .await
        .unwrap();
    other
        .execute("CREATE TABLE stored (value INTEGER)", ())
        .await
        .unwrap();
    other
        .execute("INSERT INTO stored VALUES (9)", ())
        .await
        .unwrap();
    assert_eq!(scalar(&a, "SELECT value FROM identity").await, 101);
    assert_eq!(scalar(&b, "SELECT value FROM identity").await, 202);
    assert_eq!(scalar(&other, "SELECT value FROM identity").await, 303);
    assert_eq!(scalar(&b, "SELECT value FROM stored").await, 7);
    assert_eq!(scalar(&other, "SELECT value FROM stored").await, 9);
    drop(a);
    assert_eq!(scalar(&b, "SELECT value FROM identity").await, 202);
    drop(b);
    let reopened = first.connect().unwrap();
    assert_eq!(scalar(&reopened, "SELECT value FROM stored").await, 7);
    assert_eq!(
        scalar(
            &reopened,
            "SELECT count(*) FROM sqlite_temp_master WHERE name = 'identity'"
        )
        .await,
        0
    );
}
