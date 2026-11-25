//! Behavior tests of the metered content boundary against a real local libSQL file.
//!
//! The product initializer makes each fixture repository at the newest catalog.
//! Plain SQL adds only delta rows, which no product writer makes yet, and the damage
//! that a refusal test needs.
use super::*;
use crate::{
    artifact::ParseLimits,
    content_hash,
    storage::{
        AdmissionLimits, CreateOptions, InitializeOptions, LocalOwnerRepository, LocalStore,
    },
};
use std::path::{Path, PathBuf};

/// Repository identity of every fixture.
const REPOSITORY: &str = "0123456789abcdef0123456789abcdef01234567";
/// Full baseline bytes of the fixture chain.
const BASE: &[u8] = b"native baseline bytes";
/// Bytes of the delta child, stored as a delta against [`BASE`].
const CHILD: &[u8] = b"native baseline bytes plus child";
/// RID of the delta child.
const CHILD_RID: i64 = 100;

/// Generous content limits, so that only the ceiling under test binds.
fn limits() -> Limits {
    Limits {
        max_stored_bytes: 4096,
        max_output_bytes: 4096,
        max_chain_depth: 4,
        max_total_stored_bytes: 65536,
        max_total_decoded_bytes: 4_000_000,
    }
}

/// Parser limits for the empty root.
fn parser() -> ParseLimits {
    ParseLimits {
        input_bytes: 4096,
        cards: 4096,
        owned_bytes: 1_048_576,
        entries: 64,
    }
}

/// Generous SQL work, so that only the ceiling under test binds.
fn work() -> SqlWork {
    SqlWork {
        rows: 1_000_000,
        bytes: 100_000_000,
    }
}

/// A fresh content budget with the generous limits.
fn budget() -> Budget {
    Budget::new(limits()).unwrap()
}

/// SHA3-256 native name of `bytes`.
fn name(bytes: &[u8]) -> String {
    content_hash::hex(&content_hash::sha3(bytes, 256).unwrap())
}

/// The principal of every fixture.
fn principal() -> Principal {
    Principal {
        repository: REPOSITORY.into(),
        owner: "owner".into(),
    }
}

/// The SQL work spent between two ledger states.
fn spent(before: SqlWork, after: SqlWork) -> SqlWork {
    SqlWork {
        rows: before.rows - after.rows,
        bytes: before.bytes - after.bytes,
    }
}

/// Make an empty repository at the newest catalog with the product initializer.
async fn repository(dir: &Path) -> PathBuf {
    let path = dir.join("repository");
    let options = InitializeOptions {
        repository_id: REPOSITORY,
        owner_id: "owner",
        date: "2026-11-18 12:34:56.125",
        user: b"Local Owner",
        password: "secret",
        sha1: false,
        content_limits: limits(),
        parse_limits: parser(),
    };
    drop(
        LocalOwnerRepository::initialize(&path, options)
            .await
            .unwrap(),
    );
    path
}

/// Make a repository that holds [`BASE`] as private full content.
async fn with_base(dir: &Path) -> PathBuf {
    let path = repository(dir).await;
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    tx.put_private_file_content_budget_with_sql(
        &principal(),
        &name(BASE),
        BASE,
        &mut budget(),
        &mut work(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    path
}

/// Make a repository that also holds [`CHILD`] as a private delta against [`BASE`].
async fn with_chain(dir: &Path) -> PathBuf {
    let path = with_base(dir).await;
    let delta = crate::delta::create(BASE, CHILD).unwrap();
    let packed = compression::pack(&delta, -1).unwrap();
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    tx.execute_bound(
        "INSERT INTO blob(rid,size,uuid,content) VALUES(?1,?2,?3,?4)",
        vec![
            Value::Integer(CHILD_RID),
            Value::Integer(CHILD.len() as i64),
            Value::Text(name(CHILD)),
            Value::Blob(packed),
        ],
    )
    .await
    .unwrap();
    tx.execute_bound(
        "INSERT INTO delta(rid,srcid) SELECT ?1,rid FROM blob WHERE uuid=?2",
        vec![Value::Integer(CHILD_RID), Value::Text(name(BASE))],
    )
    .await
    .unwrap();
    tx.execute_bound(
        "INSERT INTO private(rid) VALUES(?1)",
        vec![Value::Integer(CHILD_RID)],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    path
}

/// Stored envelope lengths of [`BASE`] and of the delta of [`CHILD`].
fn envelope_lengths() -> (u64, u64) {
    let full = compression::pack(BASE, -1).unwrap().len() as u64;
    let delta = crate::delta::create(BASE, CHILD).unwrap();
    let delta = compression::pack(&delta, -1).unwrap().len() as u64;
    (full, delta)
}

/// Every table that a content operation reads or writes, in a fixed order.
async fn snapshot(tx: &mut WriteTransaction<'_>) -> Vec<Vec<Vec<Value>>> {
    let mut state = Vec::new();
    for query in [
        "SELECT rid,size,uuid,content FROM blob ORDER BY rid",
        "SELECT rid,srcid FROM delta ORDER BY rid",
        "SELECT rid FROM phantom ORDER BY rid",
        "SELECT rid FROM private ORDER BY rid",
        "SELECT * FROM shun ORDER BY uuid",
        "SELECT rid,baseline FROM orphan ORDER BY rid",
        "SELECT * FROM g_storage",
        "SELECT name,value FROM config ORDER BY name",
    ] {
        state.push(tx.query_bound(query, vec![]).await.unwrap());
    }
    state
}

/// Reopen the file and take its [`snapshot`].
async fn snapshot_of(path: &Path) -> Vec<Vec<Vec<Value>>> {
    let mut store = LocalStore::open(path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let state = snapshot(&mut tx).await;
    tx.rollback().await.unwrap();
    state
}

/// The SQL work that one metered principal validation spends on this file.
async fn policy_cost(path: &Path) -> SqlWork {
    let mut store = LocalStore::open(path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let mut sql = work();
    tx.validate_principal_with_sql(&principal(), &mut sql, &mut budget())
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    spent(work(), sql)
}

/// CONTENT-01: a private put reads back after reopen with its exact size and private
/// membership, and the product admission of catalog 6 opens no shared view of it.
#[tokio::test(flavor = "current_thread")]
async fn a_private_put_reads_back_after_reopen_and_opens_no_shared_view() {
    let temp = tempfile::tempdir().unwrap();
    let path = repository(temp.path()).await;
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let rid = tx
        .put_private_file_content_budget_with_sql(
            &principal(),
            &name(BASE),
            BASE,
            &mut budget(),
            &mut work(),
        )
        .await
        .unwrap();
    tx.commit().await.unwrap();
    drop(store);

    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    assert_eq!(
        tx.read_owner_content_budget_with_sql(
            &principal(),
            &name(BASE),
            &mut budget(),
            &mut work()
        )
        .await
        .unwrap(),
        BASE
    );
    // The unmetered route reads the same bytes.
    assert_eq!(
        tx.read_owner_content(&principal(), &name(BASE), limits())
            .await
            .unwrap(),
        BASE
    );
    assert_eq!(
        tx.query_bound(
            "SELECT b.size,EXISTS(SELECT 1 FROM private p WHERE p.rid=b.rid) FROM blob b WHERE b.rid=?1",
            vec![Value::Integer(rid)]
        )
        .await
        .unwrap(),
        vec![vec![Value::Integer(BASE.len() as i64), Value::Integer(1)]]
    );
    tx.rollback().await.unwrap();
    drop(store);

    let admission = AdmissionLimits {
        content: limits(),
        parser: parser(),
    };
    // Catalog 6 admission refuses private content that no check-in or issue owns, so
    // no shared view of this file exists. A shared reader cannot learn that the name
    // is present.
    let refused = LocalOwnerRepository::open(&path, admission)
        .await
        .err()
        .unwrap();
    assert_eq!(refused.operation, "recording native projection differs");
}

/// Full and delta reads spend the exact preflight and result cells on one ledger.
#[tokio::test(flavor = "current_thread")]
async fn reads_spend_exact_sql_work_for_full_and_delta_chains() {
    let temp = tempfile::tempdir().unwrap();
    let path = with_chain(temp.path()).await;
    let policy = policy_cost(&path).await;
    let (full, delta) = envelope_lengths();
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let mut sql = work();
    let mut content = budget();
    assert_eq!(
        tx.read_owner_content_budget_with_sql(&principal(), &name(BASE), &mut content, &mut sql)
            .await
            .unwrap(),
        BASE
    );
    // Two preflights, one row of five cells with a 64-byte name, and one envelope cell.
    assert_eq!(
        spent(work(), sql),
        SqlWork {
            rows: policy.rows + 4,
            bytes: policy.bytes + 224 + full,
        }
    );
    assert_eq!(
        tx.read_owner_content_budget_with_sql(&principal(), &name(CHILD), &mut content, &mut sql)
            .await
            .unwrap(),
        CHILD
    );
    // The chain adds a second policy check, two links and the baseline name lookup.
    assert_eq!(
        spent(work(), sql),
        SqlWork {
            rows: 2 * policy.rows + 14,
            bytes: 2 * policy.bytes + 784 + 2 * full + delta,
        }
    );
    assert_eq!(content.stored, 2 * full + delta);
    assert_eq!(content.visited.len(), 2);
    tx.rollback().await.unwrap();
}

/// One row or one byte too few refuses the read, latches it and rolls back every write.
#[tokio::test(flavor = "current_thread")]
async fn one_row_or_byte_short_refuses_the_read_and_rolls_back() {
    let temp = tempfile::tempdir().unwrap();
    let path = with_chain(temp.path()).await;
    let policy = policy_cost(&path).await;
    let (full, delta) = envelope_lengths();
    let exact = SqlWork {
        rows: policy.rows + 10,
        bytes: policy.bytes + 560 + full + delta,
    };
    let before = snapshot_of(&path).await;
    for (short, message) in [
        (None, None),
        (Some(true), Some("metered query row limit")),
        (Some(false), Some("metered query byte limit")),
    ] {
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute_bound(
            "INSERT INTO shun(uuid) VALUES(?1)",
            vec![Value::Text("rollback-marker".into())],
        )
        .await
        .unwrap();
        let mut sql = exact;
        match short {
            Some(true) => sql.rows -= 1,
            Some(false) => sql.bytes -= 1,
            None => {}
        }
        let result = tx
            .read_owner_content_budget_with_sql(&principal(), &name(CHILD), &mut budget(), &mut sql)
            .await;
        match message {
            Some(message) => {
                assert_eq!(result.unwrap_err().operation, message);
                let later = tx.query_bound("SELECT 1", vec![]).await.unwrap_err();
                assert_eq!(later.operation, message);
                assert_eq!(tx.commit().await.unwrap_err().operation, message);
            }
            None => {
                assert_eq!(result.unwrap(), CHILD);
                assert_eq!(sql, SqlWork { rows: 0, bytes: 0 });
                tx.rollback().await.unwrap();
            }
        }
        drop(store);
        assert_eq!(snapshot_of(&path).await, before);
    }
}

/// Deduplication pays for each query it runs, and exhausted work rolls it back.
#[tokio::test(flavor = "current_thread")]
async fn deduplication_spends_its_queries_and_exhausted_work_rolls_back() {
    let temp = tempfile::tempdir().unwrap();
    let path = with_chain(temp.path()).await;
    let policy = policy_cost(&path).await;
    let (full, delta) = envelope_lengths();
    // Two policy checks; the empty shun lookup and the identity row; two three-cell
    // availability rows; then the two-link read without its policy check.
    let exact = SqlWork {
        rows: 2 * policy.rows + 1 + 2 + 4 + 10,
        bytes: 2 * policy.bytes + 32 + 64 + 160 + 560 + full + delta,
    };
    let before = snapshot_of(&path).await;
    for short in [None, Some(true), Some(false)] {
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let mut sql = exact;
        match short {
            Some(true) => sql.rows -= 1,
            Some(false) => sql.bytes -= 1,
            None => {}
        }
        let result = tx
            .put_private_file_content_budget_with_sql(
                &principal(),
                &name(CHILD),
                CHILD,
                &mut budget(),
                &mut sql,
            )
            .await;
        match short {
            None => {
                assert_eq!(result.unwrap(), CHILD_RID);
                assert_eq!(sql, SqlWork { rows: 0, bytes: 0 });
                tx.commit().await.unwrap();
            }
            Some(rows) => {
                let expected = if rows {
                    "metered query row limit"
                } else {
                    "metered query byte limit"
                };
                assert_eq!(result.unwrap_err().operation, expected);
                assert_eq!(tx.commit().await.unwrap_err().operation, expected);
            }
        }
        drop(store);
        assert_eq!(snapshot_of(&path).await, before);
    }
}

/// A refusal after the blob insert removes the blob and its private row.
#[tokio::test(flavor = "current_thread")]
async fn a_refusal_after_a_new_blob_write_rolls_the_write_back() {
    let temp = tempfile::tempdir().unwrap();
    let path = with_base(temp.path()).await;
    let bytes = b"new native content";
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let mut sql = work();
    tx.put_private_file_content_budget_with_sql(
        &principal(),
        &name(bytes),
        bytes,
        &mut budget(),
        &mut sql,
    )
    .await
    .unwrap();
    let exact = spent(work(), sql);
    tx.rollback().await.unwrap();
    drop(store);
    let before = snapshot_of(&path).await;
    for (mut sql, message) in [
        (
            SqlWork {
                bytes: exact.bytes - 1,
                ..exact
            },
            "metered query byte limit",
        ),
        (
            SqlWork {
                rows: exact.rows - 1,
                ..exact
            },
            "metered query row limit",
        ),
    ] {
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let refused = tx
            .put_private_file_content_budget_with_sql(
                &principal(),
                &name(bytes),
                bytes,
                &mut budget(),
                &mut sql,
            )
            .await
            .unwrap_err();
        assert_eq!(refused.operation, message);
        // The refusal comes from the RID lookup that follows the insert. Its preflight
        // was charged, and the debit of its one integer cell was refused whole.
        let left = if message.contains("row") {
            SqlWork { rows: 0, bytes: 16 }
        } else {
            SqlWork { rows: 1, bytes: 15 }
        };
        assert_eq!(sql, left);
        assert_eq!(tx.commit().await.unwrap_err().operation, message);
        drop(store);
        assert_eq!(snapshot_of(&path).await, before);
    }
}

/// CONTENT-02: a principal or policy refusal latches the owner, and an earlier write in
/// the same transaction does not survive.
#[tokio::test(flavor = "current_thread")]
async fn principal_and_policy_refusals_latch_and_discard_earlier_writes() {
    let temp = tempfile::tempdir().unwrap();
    let path = with_base(temp.path()).await;
    let before = snapshot_of(&path).await;
    let other = |repository: &str, owner: &str| Principal {
        repository: repository.into(),
        owner: owner.into(),
    };
    let cases: [(&str, &[&str], Principal, &str); 9] = [
        (
            "wrong owner",
            &[],
            other(REPOSITORY, "intruder"),
            "repository principal or complete policy unavailable",
        ),
        (
            "wrong repository",
            &[],
            other(&"f".repeat(40), "owner"),
            "repository principal or complete policy unavailable",
        ),
        (
            "malformed repository",
            &[],
            other("short", "owner"),
            "invalid local repository identity binding",
        ),
        (
            "empty owner",
            &[],
            other(REPOSITORY, ""),
            "invalid local repository identity binding",
        ),
        (
            "missing policy",
            &["UPDATE g_storage SET policy_complete=0"],
            principal(),
            "repository principal or complete policy unavailable",
        ),
        (
            "populated checkout count",
            &["UPDATE g_storage SET checkout_count=1"],
            principal(),
            "unsupported populated catalog6 checkout scope",
        ),
        (
            "missing native marker",
            &["DELETE FROM config WHERE name='aux-schema'"],
            principal(),
            "unsupported or missing native repository markers",
        ),
        (
            "differing storage marker",
            &[
                "PRAGMA ignore_check_constraints=ON",
                "UPDATE g_storage SET storage_version=1",
            ],
            principal(),
            "storage marker differs from exact repository catalog",
        ),
        (
            "unregistered schema object",
            &["CREATE TABLE extra(x)"],
            principal(),
            "unregistered persistent schema object: extra",
        ),
    ];
    for (case, damage, caller, message) in cases {
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let first = b"first object of the transaction";
        tx.put_private_file_content_budget_with_sql(
            &principal(),
            &name(first),
            first,
            &mut budget(),
            &mut work(),
        )
        .await
        .unwrap();
        for damage in damage {
            tx.execute(damage).await.unwrap();
        }
        let refused = tx
            .put_private_file_content_budget_with_sql(
                &caller,
                &name(CHILD),
                CHILD,
                &mut budget(),
                &mut work(),
            )
            .await
            .unwrap_err();
        assert_eq!(refused.operation, message, "{case}");
        let read = tx
            .read_owner_content_budget_with_sql(
                &principal(),
                &name(BASE),
                &mut budget(),
                &mut work(),
            )
            .await
            .unwrap_err();
        assert_eq!(read.operation, message, "{case}");
        assert_eq!(tx.commit().await.unwrap_err().operation, message, "{case}");
        drop(store);
        assert_eq!(snapshot_of(&path).await, before, "{case}");
    }
}

/// The metered routes refuse an older catalog, here catalog 1 from the bound creator,
/// before any effect.
#[tokio::test(flavor = "current_thread")]
async fn an_older_catalog_is_refused_before_any_effect() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("catalog1");
    let options = CreateOptions {
        repository_id: REPOSITORY,
        owner_id: "owner",
        checkout_id: "checkout",
        checkout_root: temp.path(),
        date: b"2026-11-18T00:00:00.000",
        user: b"Owner",
        content_limits: limits(),
        parse_limits: parser(),
    };
    drop(LocalOwnerRepository::create(&path, options).await.unwrap());
    let before = snapshot_of(&path).await;
    let message = "repository catalog 1 is not the newest catalog 6";
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let refused = tx
        .put_private_file_content_budget_with_sql(
            &principal(),
            &name(BASE),
            BASE,
            &mut budget(),
            &mut work(),
        )
        .await
        .unwrap_err();
    assert_eq!(refused.operation, message);
    assert_eq!(tx.commit().await.unwrap_err().operation, message);
    let mut tx = store.begin_write().await.unwrap();
    let refused = tx
        .validate_principal_with_sql(&principal(), &mut work(), &mut budget())
        .await
        .unwrap_err();
    assert_eq!(refused.operation, message);
    tx.rollback().await.unwrap_err();
    drop(store);
    assert_eq!(snapshot_of(&path).await, before);
}

/// One content failure case of [`content_failures_latch_and_restore_the_pre_call_state`].
struct Failure {
    /// Name of the case in assertion messages.
    case: &'static str,
    /// Plain SQL damage applied inside the transaction, with its parameters.
    damage: Option<(&'static str, Vec<Value>)>,
    /// Read the name instead of putting the bytes.
    read: bool,
    /// Bytes whose name the operation uses.
    named: &'static [u8],
    /// Bytes that a put stores.
    bytes: &'static [u8],
    /// Content limits of the operation.
    bounds: Limits,
    /// Prefix of the expected refusal.
    message: &'static str,
}

/// CONTENT-03: content and decoding failures latch, and the reopened file equals its
/// state before the call.
#[tokio::test(flavor = "current_thread")]
async fn content_failures_latch_and_restore_the_pre_call_state() {
    let temp = tempfile::tempdir().unwrap();
    let path = with_base(temp.path()).await;
    let before = snapshot_of(&path).await;
    let other = b"other bytes of the same length";
    let wrong = compression::pack(&other[..BASE.len()], -1).unwrap();
    let small = Limits {
        max_output_bytes: (CHILD.len() - 1) as u32,
        ..limits()
    };
    let cases = [
        Failure {
            case: "hash mismatch",
            damage: None,
            read: false,
            named: BASE,
            bytes: CHILD,
            bounds: limits(),
            message: "native content hash mismatch",
        },
        Failure {
            case: "shunned target",
            damage: Some((
                "INSERT INTO shun(uuid) VALUES(?1)",
                vec![Value::Text(name(CHILD))],
            )),
            read: false,
            named: CHILD,
            bytes: CHILD,
            bounds: limits(),
            message: "content unavailable",
        },
        Failure {
            case: "output limit",
            damage: None,
            read: false,
            named: CHILD,
            bytes: CHILD,
            bounds: small,
            message: "content output limit",
        },
        Failure {
            case: "identity decoding",
            damage: Some((
                "UPDATE blob SET size='large' WHERE uuid=?1",
                vec![Value::Text(name(BASE))],
            )),
            read: false,
            named: BASE,
            bytes: BASE,
            bounds: limits(),
            message: "corrupt native content identity",
        },
        Failure {
            case: "codec",
            damage: Some((
                "UPDATE blob SET content=X'00000010ff' WHERE uuid=?1",
                vec![Value::Text(name(BASE))],
            )),
            read: true,
            named: BASE,
            bytes: BASE,
            bounds: limits(),
            message: "native content envelope: ",
        },
        Failure {
            case: "stored bytes differ from their name",
            damage: Some((
                "UPDATE blob SET content=?2 WHERE uuid=?1",
                vec![Value::Text(name(BASE)), Value::Blob(wrong.clone())],
            )),
            read: true,
            named: BASE,
            bytes: BASE,
            bounds: limits(),
            message: "native content hash mismatch",
        },
    ];
    for Failure {
        case,
        damage,
        read,
        named,
        bytes,
        bounds,
        message,
    } in cases
    {
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let first = b"first object of the transaction";
        tx.put_private_file_content_budget_with_sql(
            &principal(),
            &name(first),
            first,
            &mut budget(),
            &mut work(),
        )
        .await
        .unwrap();
        if let Some((sql, params)) = damage {
            tx.execute_bound(sql, params).await.unwrap();
        }
        let mut content = Budget::new(bounds).unwrap();
        let result = if read {
            tx.read_owner_content_budget_with_sql(
                &principal(),
                &name(named),
                &mut content,
                &mut work(),
            )
            .await
            .map(|_| 0)
        } else {
            tx.put_private_file_content_budget_with_sql(
                &principal(),
                &name(named),
                bytes,
                &mut content,
                &mut work(),
            )
            .await
        };
        let refused = result.unwrap_err();
        assert!(refused.operation.starts_with(message), "{case}: {refused}");
        assert_eq!(
            tx.commit().await.unwrap_err().operation,
            refused.operation,
            "{case}"
        );
        drop(store);
        assert_eq!(snapshot_of(&path).await, before, "{case}");
    }
}

/// CONTENT-04: the native semantics of the envelope stay, and the repository checks the size
/// separately.
#[tokio::test(flavor = "current_thread")]
async fn envelope_semantics_stay_native_and_the_size_is_checked_apart() {
    let temp = tempfile::tempdir().unwrap();
    let path = with_base(temp.path()).await;
    let mut lenient = compression::pack(BASE, -1).unwrap();
    lenient[..4].copy_from_slice(&64_u32.to_be_bytes());
    lenient.extend_from_slice(b"native trailing bytes");
    let cases: [(&str, Value, i64, Option<&str>); 3] = [
        (
            "excess capacity and trailing bytes",
            Value::Blob(lenient),
            BASE.len() as i64,
            None,
        ),
        (
            "no payload",
            Value::Blob(vec![0, 0, 0, 0]),
            0,
            Some("native content envelope has no payload"),
        ),
        (
            "size differs",
            Value::Blob(compression::pack(BASE, -1).unwrap()),
            BASE.len() as i64 + 1,
            Some("native content size mismatch"),
        ),
    ];
    for (case, envelope, size, message) in cases {
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute_bound(
            "UPDATE blob SET content=?1,size=?2 WHERE uuid=?3",
            vec![envelope, Value::Integer(size), Value::Text(name(BASE))],
        )
        .await
        .unwrap();
        let result = tx
            .read_owner_content_budget_with_sql(
                &principal(),
                &name(BASE),
                &mut budget(),
                &mut work(),
            )
            .await;
        match message {
            None => assert_eq!(result.unwrap(), BASE, "{case}"),
            Some(message) => assert_eq!(result.unwrap_err().operation, message, "{case}"),
        }
        let _ = tx.rollback().await;
    }
}

/// The route under test in the delta chain cases.
#[derive(Clone, Copy, Debug)]
enum Route {
    /// The metered read of [`CHILD`] or [`BASE`].
    MeteredRead,
    /// The unmetered read, which shares the chain decoders.
    UnmeteredRead,
    /// The metered put of the same name and bytes.
    MeteredPut,
}

/// Damage the fixture chain for one case, inside the open transaction.
async fn damage_chain(tx: &mut WriteTransaction<'_>, case: &str) {
    let base = Value::Text(name(BASE));
    let statements: Vec<(&'static str, Vec<Value>)> = match case {
        "complete" | "depth" => vec![],
        "missing base" => vec![(
            "UPDATE delta SET srcid=999999 WHERE rid=?1",
            vec![Value::Integer(CHILD_RID)],
        )],
        "phantom base" => vec![
            (
                "UPDATE blob SET size=-1,content=NULL WHERE uuid=?1",
                vec![base.clone()],
            ),
            (
                "INSERT INTO phantom(rid) SELECT rid FROM blob WHERE uuid=?1",
                vec![base],
            ),
        ],
        "shunned base" => vec![("INSERT INTO shun(uuid) VALUES(?1)", vec![base])],
        "cycle" => vec![(
            "UPDATE delta SET srcid=?1 WHERE rid=?1",
            vec![Value::Integer(CHILD_RID)],
        )],
        "malformed base size" => vec![("UPDATE blob SET size='large' WHERE uuid=?1", vec![base])],
        "malformed baseline" => vec![(
            "UPDATE delta SET srcid='other' WHERE rid=?1",
            vec![Value::Integer(CHILD_RID)],
        )],
        _ => unreachable!("unknown chain case"),
    };
    for (sql, params) in statements {
        tx.execute_bound(sql, params).await.unwrap();
    }
}

/// CONTENT-05: a complete chain reconstructs, and each unavailable or corrupt dependency
/// refuses without a partial mutation.
#[tokio::test(flavor = "current_thread")]
async fn delta_chain_variants_refuse_without_partial_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let path = with_chain(temp.path()).await;
    let original = snapshot_of(&path).await;
    // Each case gives the result of the metered read, the unmetered read and the put of CHILD.
    let cases: [(&str, [Option<&str>; 3]); 8] = [
        ("complete", [None, None, None]),
        (
            "missing base",
            [
                Some("content unavailable"),
                Some("content unavailable"),
                None,
            ],
        ),
        (
            "phantom base",
            [
                Some("content unavailable"),
                Some("content unavailable"),
                None,
            ],
        ),
        (
            "shunned base",
            [
                Some("content unavailable"),
                Some("content unavailable"),
                Some("content unavailable"),
            ],
        ),
        (
            "cycle",
            [
                Some("content delta cycle or invalid RID"),
                Some("content delta cycle or invalid RID"),
                Some("content delta cycle"),
            ],
        ),
        (
            "depth",
            [
                Some("content chain depth limit"),
                Some("content chain depth limit"),
                Some("content chain depth limit"),
            ],
        ),
        (
            "malformed base size",
            [
                Some("corrupt native content row"),
                Some("corrupt native content row"),
                Some("corrupt native availability row"),
            ],
        ),
        (
            "malformed baseline",
            [
                Some("corrupt delta baseline RID"),
                Some("corrupt delta baseline RID"),
                Some("corrupt delta baseline RID"),
            ],
        ),
    ];
    for (case, expected) in cases {
        for (route, expected) in [Route::MeteredRead, Route::UnmeteredRead, Route::MeteredPut]
            .into_iter()
            .zip(expected)
        {
            let mut store = LocalStore::open(&path).await.unwrap();
            let mut tx = store.begin_write().await.unwrap();
            damage_chain(&mut tx, case).await;
            let bounds = Limits {
                max_chain_depth: if case == "depth" { 1 } else { 4 },
                ..limits()
            };
            let mut content = Budget::new(bounds).unwrap();
            let mut sql = work();
            let result = match route {
                Route::MeteredRead => tx
                    .read_owner_content_budget_with_sql(
                        &principal(),
                        &name(CHILD),
                        &mut content,
                        &mut sql,
                    )
                    .await
                    .map(|bytes| assert_eq!(bytes, CHILD)),
                Route::UnmeteredRead => tx
                    .read_owner_content_budget(&principal(), &name(CHILD), &mut content)
                    .await
                    .map(|bytes| assert_eq!(bytes, CHILD)),
                Route::MeteredPut => tx
                    .put_private_file_content_budget_with_sql(
                        &principal(),
                        &name(CHILD),
                        CHILD,
                        &mut content,
                        &mut sql,
                    )
                    .await
                    .map(|rid| assert_eq!(rid, CHILD_RID)),
            };
            match expected {
                Some(message) => {
                    let refused = result.unwrap_err();
                    assert_eq!(refused.operation, message, "{case} {route:?}");
                    // A later call returns the first failure and spends no more work.
                    let after = (sql, content.decoded, content.stored);
                    let later = tx
                        .put_private_file_content_budget_with_sql(
                            &principal(),
                            &name(CHILD),
                            CHILD,
                            &mut content,
                            &mut sql,
                        )
                        .await
                        .unwrap_err();
                    assert_eq!(later.operation, message, "{case} {route:?}");
                    assert_eq!((sql, content.decoded, content.stored), after);
                    assert_eq!(tx.rollback().await.unwrap_err().operation, message);
                }
                None => {
                    result.unwrap();
                    if matches!(route, Route::MeteredPut) && case != "complete" {
                        // The repair replaced the delta with full bytes.
                        assert!(
                            tx.query_bound(
                                "SELECT rid FROM delta WHERE rid=?1",
                                vec![Value::Integer(CHILD_RID)]
                            )
                            .await
                            .unwrap()
                            .is_empty(),
                            "{case}"
                        );
                        assert_eq!(
                            tx.read_owner_content_budget_with_sql(
                                &principal(),
                                &name(CHILD),
                                &mut budget(),
                                &mut work()
                            )
                            .await
                            .unwrap(),
                            CHILD,
                            "{case}"
                        );
                    }
                    tx.rollback().await.unwrap();
                }
            }
            drop(store);
            assert_eq!(snapshot_of(&path).await, original, "{case} {route:?}");
        }
    }
}

/// CONTENT-06: each content ceiling admits its exact boundary and refuses one less.
#[tokio::test(flavor = "current_thread")]
async fn each_content_ceiling_admits_its_boundary_and_refuses_one_less() {
    let temp = tempfile::tempdir().unwrap();
    let path = with_chain(temp.path()).await;
    let (full, delta) = envelope_lengths();
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let mut measured = budget();
    tx.read_owner_content_budget_with_sql(&principal(), &name(CHILD), &mut measured, &mut work())
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(measured.stored, full + delta);
    let exact = Limits {
        max_stored_bytes: full.max(delta),
        max_output_bytes: CHILD.len() as u32,
        max_chain_depth: 2,
        max_total_stored_bytes: full + delta,
        max_total_decoded_bytes: measured.decoded,
    };
    let cases = [
        (
            "per envelope",
            Limits {
                max_stored_bytes: exact.max_stored_bytes - 1,
                ..exact
            },
            "stored content limit",
        ),
        (
            "total stored",
            Limits {
                max_total_stored_bytes: exact.max_total_stored_bytes - 1,
                ..exact
            },
            "total stored content limit",
        ),
        (
            "chain depth",
            Limits {
                max_chain_depth: 1,
                ..exact
            },
            "content chain depth limit",
        ),
        (
            "output",
            Limits {
                max_output_bytes: exact.max_output_bytes - 1,
                ..exact
            },
            "native content delta: ",
        ),
        (
            // The last decoded charge is the child itself, so the delta output ceiling,
            // which the remaining decoded work bounds, refuses first.
            "total decoded",
            Limits {
                max_total_decoded_bytes: exact.max_total_decoded_bytes - 1,
                ..exact
            },
            "native content delta: ",
        ),
    ];
    let mut tx = store.begin_write().await.unwrap();
    let mut content = Budget::new(exact).unwrap();
    assert_eq!(
        tx.read_owner_content_budget_with_sql(
            &principal(),
            &name(CHILD),
            &mut content,
            &mut work()
        )
        .await
        .unwrap(),
        CHILD
    );
    assert_eq!(
        (content.stored, content.decoded),
        (exact.max_total_stored_bytes, exact.max_total_decoded_bytes)
    );
    tx.rollback().await.unwrap();
    for (case, bounds, message) in cases {
        let mut tx = store.begin_write().await.unwrap();
        let refused = tx
            .read_owner_content_budget_with_sql(
                &principal(),
                &name(CHILD),
                &mut Budget::new(bounds).unwrap(),
                &mut work(),
            )
            .await
            .unwrap_err();
        assert!(refused.operation.starts_with(message), "{case}: {refused}");
        assert_eq!(
            tx.rollback().await.unwrap_err().operation,
            refused.operation
        );
    }
}

/// CONTENT-07: full bytes fulfill a phantom that kept a stale delta row, unless a
/// required dependent waits on it.
#[tokio::test(flavor = "current_thread")]
async fn full_bytes_fulfill_a_phantom_with_a_stale_delta_row() {
    let temp = tempfile::tempdir().unwrap();
    let path = with_chain(temp.path()).await;
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    tx.execute_bound(
        "UPDATE blob SET size=-1,content=NULL WHERE rid=?1",
        vec![Value::Integer(CHILD_RID)],
    )
    .await
    .unwrap();
    tx.execute_bound(
        "INSERT INTO phantom(rid) VALUES(?1)",
        vec![Value::Integer(CHILD_RID)],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    drop(store);
    let phantom = snapshot_of(&path).await;

    // A waiting orphan blocks the fulfillment, and the refusal keeps the phantom.
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    tx.execute_bound(
        "INSERT INTO orphan(rid,baseline) VALUES(200,?1)",
        vec![Value::Integer(CHILD_RID)],
    )
    .await
    .unwrap();
    let refused = tx
        .put_private_file_content_budget_with_sql(
            &principal(),
            &name(CHILD),
            CHILD,
            &mut budget(),
            &mut work(),
        )
        .await
        .unwrap_err();
    assert_eq!(
        refused.operation,
        "required dependent crosslink unavailable"
    );
    assert_eq!(tx.commit().await.unwrap_err().operation, refused.operation);
    drop(store);
    assert_eq!(snapshot_of(&path).await, phantom);

    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    assert_eq!(
        tx.put_private_file_content_budget_with_sql(
            &principal(),
            &name(CHILD),
            CHILD,
            &mut budget(),
            &mut work(),
        )
        .await
        .unwrap(),
        CHILD_RID
    );
    tx.commit().await.unwrap();
    drop(store);
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let id = vec![Value::Integer(CHILD_RID)];
    for table in [
        "SELECT rid FROM delta WHERE rid=?1",
        "SELECT rid FROM phantom WHERE rid=?1",
    ] {
        assert!(tx.query_bound(table, id.clone()).await.unwrap().is_empty());
    }
    assert_eq!(
        tx.query_bound("SELECT size FROM blob WHERE rid=?1", id)
            .await
            .unwrap(),
        vec![vec![Value::Integer(CHILD.len() as i64)]]
    );
    assert_eq!(
        tx.read_owner_content_budget_with_sql(
            &principal(),
            &name(CHILD),
            &mut budget(),
            &mut work()
        )
        .await
        .unwrap(),
        CHILD
    );
    tx.rollback().await.unwrap();
}

/// CONTENT-08: putting public bytes privately deduplicates and adds no private row.
#[tokio::test(flavor = "current_thread")]
async fn public_content_stays_public_when_put_privately() {
    let temp = tempfile::tempdir().unwrap();
    let path = repository(temp.path()).await;
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let roots = tx
        .query_bound("SELECT native_hash FROM g_public_receipt", vec![])
        .await
        .unwrap();
    let [row] = roots.as_slice() else {
        panic!("one public root expected")
    };
    let [Value::Text(root)] = row.as_slice() else {
        panic!("public root name expected")
    };
    let bytes = tx
        .read_owner_content_budget_with_sql(&principal(), root, &mut budget(), &mut work())
        .await
        .unwrap();
    assert_eq!(
        tx.put_private_file_content_budget_with_sql(
            &principal(),
            root,
            &bytes,
            &mut budget(),
            &mut work(),
        )
        .await
        .unwrap(),
        1
    );
    tx.commit().await.unwrap();
    drop(store);
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    assert!(
        tx.query_bound("SELECT rid FROM private", vec![])
            .await
            .unwrap()
            .is_empty()
    );
    tx.rollback().await.unwrap();
}

/// Principal validation charges its SQL and its content, the fingerprint encoding
/// included, and refuses one row, one byte or one decoded byte less.
#[tokio::test(flavor = "current_thread")]
async fn principal_validation_charges_both_ledgers_to_the_exact_boundary() {
    let temp = tempfile::tempdir().unwrap();
    let path = repository(temp.path()).await;
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let mut sql = work();
    let mut content = budget();
    tx.validate_principal_with_sql(&principal(), &mut sql, &mut content)
        .await
        .unwrap();
    // The unmetered route admits the same file.
    tx.validate_principal(&principal()).await.unwrap();
    tx.rollback().await.unwrap();
    let exact_sql = spent(work(), sql);
    let decoded = content.decoded;
    let schema = {
        let mut tx = store.begin_write().await.unwrap();
        let rows = tx
            .query_bound(
                "SELECT sum(length(CAST(type AS BLOB))+length(CAST(name AS BLOB))+length(CAST(tbl_name AS BLOB))+coalesce(length(CAST(sql AS BLOB)),0)),count(*) FROM sqlite_schema WHERE name NOT LIKE 'sqlite_%'",
                vec![],
            )
            .await
            .unwrap();
        tx.rollback().await.unwrap();
        rows
    };
    let [row] = schema.as_slice() else {
        panic!("one schema size row expected")
    };
    let [Value::Integer(text), Value::Integer(objects)] = row.as_slice() else {
        panic!("integer schema sizes expected")
    };
    // Five queries without parameters retain two cells each for the preflight row. The
    // catalog query retains four cells per object and its text, and its fingerprint
    // encoding eight bytes per value and the text again. The marker, principal, inventory
    // and three native marker rows retain one, five, two and six cells, and 59 and 87 text bytes.
    let cell = std::mem::size_of::<Value>() as u64;
    let (text, objects) = (*text as u64, *objects as u64);
    assert_eq!(
        decoded,
        24 * cell + 4 * cell * objects + 32 * objects + 2 * text + 146
    );
    let decoded_only = |total| {
        Budget::new(Limits {
            max_total_decoded_bytes: total,
            ..limits()
        })
        .unwrap()
    };
    for (mut sql, mut content, message) in [
        (exact_sql, decoded_only(decoded), None),
        (
            SqlWork {
                rows: exact_sql.rows - 1,
                ..exact_sql
            },
            decoded_only(decoded),
            Some("metered query row limit"),
        ),
        (
            SqlWork {
                bytes: exact_sql.bytes - 1,
                ..exact_sql
            },
            decoded_only(decoded),
            Some("metered query byte limit"),
        ),
        (
            exact_sql,
            decoded_only(decoded - 1),
            Some("total decoded content limit"),
        ),
    ] {
        let mut tx = store.begin_write().await.unwrap();
        let result = tx
            .validate_principal_with_sql(&principal(), &mut sql, &mut content)
            .await;
        match message {
            None => {
                result.unwrap();
                assert_eq!(sql, SqlWork { rows: 0, bytes: 0 });
                assert_eq!(content.decoded, decoded);
                tx.rollback().await.unwrap();
            }
            Some(message) => {
                assert_eq!(result.unwrap_err().operation, message);
                assert_eq!(tx.rollback().await.unwrap_err().operation, message);
            }
        }
    }
}

/// Each row decoder refuses an absent row and a value of the wrong type.
#[test]
fn row_decoders_refuse_absent_and_mistyped_rows() {
    let blob = || Value::Blob(b"packed".to_vec());
    let text = || Value::Text("name".into());
    let link = |hash: Value, length: Value| {
        vec![vec![
            Value::Integer(1),
            Value::Integer(4),
            hash,
            length,
            Value::Null,
        ]]
    };
    let four = || Value::Integer(4);
    assert_eq!(chain_link(&link(text(), four())).unwrap().hash, "name");
    assert_eq!(
        chain_link(&link(text(), text()))
            .err()
            .expect("a text length is corrupt")
            .operation,
        "corrupt native content row"
    );
    assert_eq!(
        chain_link(&link(blob(), four()))
            .err()
            .expect("a blob hash is corrupt")
            .operation,
        "corrupt native content row"
    );
    assert_eq!(payload(&[vec![blob()]]).unwrap(), b"packed");
    assert_eq!(payload(&[]).unwrap_err().operation, "content unavailable");
    assert_eq!(
        payload(&[vec![text()]]).unwrap_err().operation,
        "corrupt native content payload"
    );
    assert_eq!(baseline_name(&[vec![text()]]).unwrap(), "name");
    assert_eq!(
        baseline_name(&[vec![blob()]]).unwrap_err().operation,
        "corrupt native baseline"
    );
    assert_eq!(stored_rid(&[vec![Value::Integer(9)]]).unwrap(), 9);
    assert_eq!(
        stored_rid(&[]).unwrap_err().operation,
        "missing stored content identity"
    );
    assert_eq!(
        stored_rid(&[vec![text()]]).unwrap_err().operation,
        "corrupt stored content identity"
    );
}

/// The metered policy and catalog checks return a latched failure before any query, so
/// they charge neither ledger.
#[tokio::test(flavor = "current_thread")]
async fn metered_checks_return_a_latched_failure_without_spending() {
    let temp = tempfile::tempdir().unwrap();
    let path = repository(temp.path()).await;
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let first = tx
        .query_bound("SELECT * FROM no_such_table", vec![])
        .await
        .unwrap_err();
    let mut sql = work();
    let mut content = budget();
    assert_eq!(
        tx.validate_principal_with_sql(&principal(), &mut sql, &mut content)
            .await
            .unwrap_err()
            .operation,
        first.operation
    );
    assert_eq!(
        tx.validate_catalog_with_sql(&mut sql, &mut content)
            .await
            .unwrap_err()
            .operation,
        first.operation
    );
    assert_eq!(sql, work());
    assert_eq!((content.stored, content.decoded), (0, 0));
    assert!(content.visited.is_empty());
    assert_eq!(tx.commit().await.unwrap_err().operation, first.operation);
}

/// The unmetered put refuses a shunned name and a phantom that a dependent waits on,
/// and each refusal leaves the repository as it was.
#[tokio::test(flavor = "current_thread")]
async fn the_unmetered_put_refuses_shunned_names_and_waiting_dependents() {
    let temp = tempfile::tempdir().unwrap();
    let path = with_chain(temp.path()).await;
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    tx.execute_bound(
        "UPDATE blob SET size=-1,content=NULL WHERE rid=?1",
        vec![Value::Integer(CHILD_RID)],
    )
    .await
    .unwrap();
    for insert in [
        "INSERT INTO phantom(rid) VALUES(?1)",
        "INSERT INTO orphan(rid,baseline) VALUES(200,?1)",
    ] {
        tx.execute_bound(insert, vec![Value::Integer(CHILD_RID)])
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    drop(store);
    let before = snapshot_of(&path).await;

    let other = b"unrelated shunned bytes";
    let cases: [(&[u8], Option<&str>, &str); 2] = [
        (
            other,
            Some("INSERT INTO shun(uuid) VALUES(?1)"),
            "content unavailable",
        ),
        (CHILD, None, "required dependent crosslink unavailable"),
    ];
    for (bytes, damage, message) in cases {
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        if let Some(damage) = damage {
            tx.execute_bound(damage, vec![Value::Text(name(bytes))])
                .await
                .unwrap();
        }
        let refused = tx
            .put_private_file_content(&principal(), &name(bytes), bytes, limits())
            .await
            .unwrap_err();
        assert_eq!(refused.operation, message);
        assert_eq!(tx.commit().await.unwrap_err().operation, message);
        drop(store);
        assert_eq!(snapshot_of(&path).await, before);
    }
}

/// Turn the stored row of `rid` into a phantom, the way a missing artifact looks.
async fn make_phantom(path: &Path, rid: i64) {
    let mut store = LocalStore::open(path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    tx.execute_bound(
        "UPDATE blob SET size=-1,content=NULL WHERE rid=?1",
        vec![Value::Integer(rid)],
    )
    .await
    .unwrap();
    tx.execute_bound(
        "INSERT INTO phantom(rid) VALUES(?1)",
        vec![Value::Integer(rid)],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// A new put charges two amounts as decoded bytes, its input and the retained bind of
/// its envelope, and it charges its envelope once as stored bytes. It admits the exact
/// ceilings and refuses one byte less with no effect.
#[tokio::test(flavor = "current_thread")]
async fn a_new_put_charges_its_envelope_and_input_to_the_exact_ceiling() {
    let long = [b"long put bytes ".as_slice(); 40].concat();
    let short = b"short put bytes, little to compress".to_vec();
    let mut measured = Vec::new();
    for bytes in [&long, &short] {
        let temp = tempfile::tempdir().unwrap();
        let path = repository(temp.path()).await;
        let packed = compression::pack(bytes, -1).unwrap().len() as u64;
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let mut used = budget();
        tx.put_private_file_content_budget_with_sql(
            &principal(),
            &name(bytes),
            bytes,
            &mut used,
            &mut work(),
        )
        .await
        .unwrap();
        tx.rollback().await.unwrap();
        drop(store);
        assert_eq!(used.stored, packed);
        measured.push((bytes.len() as u64, packed, used.decoded));

        let before = snapshot_of(&path).await;
        let exact = Limits {
            max_stored_bytes: packed,
            max_output_bytes: bytes.len() as u32,
            max_chain_depth: 1,
            max_total_stored_bytes: packed,
            max_total_decoded_bytes: used.decoded,
        };
        let cases = [
            (
                Limits {
                    max_stored_bytes: packed - 1,
                    ..exact
                },
                "native content compression: ",
            ),
            (
                Limits {
                    max_total_stored_bytes: packed - 1,
                    ..exact
                },
                "native content compression: ",
            ),
            (
                Limits {
                    max_total_decoded_bytes: used.decoded - 1,
                    ..exact
                },
                "total decoded content limit",
            ),
        ];
        for (limits, message) in cases {
            let mut store = LocalStore::open(&path).await.unwrap();
            let mut tx = store.begin_write().await.unwrap();
            let refused = tx
                .put_private_file_content_budget_with_sql(
                    &principal(),
                    &name(bytes),
                    bytes,
                    &mut Budget::new(limits).unwrap(),
                    &mut work(),
                )
                .await
                .unwrap_err();
            assert!(
                refused.operation.starts_with(message),
                "{}",
                refused.operation
            );
            assert_eq!(tx.commit().await.unwrap_err().operation, refused.operation);
            drop(store);
            assert_eq!(snapshot_of(&path).await, before);
        }
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.put_private_file_content_budget_with_sql(
            &principal(),
            &name(bytes),
            bytes,
            &mut Budget::new(exact).unwrap(),
            &mut work(),
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    // Both puts run the same queries on names of one length, so the difference of
    // their decoded totals is the difference of their inputs plus their envelopes.
    let [
        (long_len, long_packed, long_decoded),
        (short_len, short_packed, short_decoded),
    ] = measured.as_slice()
    else {
        panic!("two measured puts expected")
    };
    assert_ne!(long_len - short_len, long_packed.abs_diff(*short_packed));
    assert_eq!(
        i128::from(*long_decoded) - i128::from(*short_decoded),
        i128::from(*long_len) - i128::from(*short_len) + i128::from(*long_packed)
            - i128::from(*short_packed)
    );
}

/// A delta or a required typed record that waits on a phantom blocks its
/// fulfillment, and a typed record that does not require it does not.
#[tokio::test(flavor = "current_thread")]
async fn delta_and_required_typed_dependents_block_fulfillment() {
    let cases: [(&str, Vec<Value>, bool); 3] = [
        (
            "INSERT INTO delta(rid,srcid) VALUES(300,?1)",
            vec![Value::Integer(CHILD_RID)],
            true,
        ),
        (
            "INSERT INTO g_dependency VALUES('scope','source','role',?1,1)",
            vec![Value::Text(name(CHILD))],
            true,
        ),
        (
            "INSERT INTO g_dependency VALUES('scope','source','role',?1,0)",
            vec![Value::Text(name(CHILD))],
            false,
        ),
    ];
    for (dependent, params, blocks) in cases {
        let temp = tempfile::tempdir().unwrap();
        let path = with_chain(temp.path()).await;
        make_phantom(&path, CHILD_RID).await;
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        tx.execute_bound(dependent, params).await.unwrap();
        tx.commit().await.unwrap();
        drop(store);
        let before = snapshot_of(&path).await;
        let mut store = LocalStore::open(&path).await.unwrap();
        let mut tx = store.begin_write().await.unwrap();
        let put = tx
            .put_private_file_content_budget_with_sql(
                &principal(),
                &name(CHILD),
                CHILD,
                &mut budget(),
                &mut work(),
            )
            .await;
        if blocks {
            let refused = put.unwrap_err();
            assert_eq!(
                refused.operation,
                "required dependent crosslink unavailable"
            );
            assert_eq!(tx.commit().await.unwrap_err().operation, refused.operation);
            drop(store);
            assert_eq!(snapshot_of(&path).await, before, "{dependent}");
        } else {
            assert_eq!(put.unwrap(), CHILD_RID);
            tx.commit().await.unwrap();
            drop(store);
            assert_ne!(snapshot_of(&path).await, before);
        }
    }
}

/// Full bytes fulfill a phantom that a public receipt names, and the put adds no
/// private row for them.
#[tokio::test(flavor = "current_thread")]
async fn fulfilling_a_public_phantom_adds_no_private_row() {
    let temp = tempfile::tempdir().unwrap();
    let path = repository(temp.path()).await;
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    let roots = tx
        .query_bound(
            "SELECT b.rid,b.uuid FROM g_public_receipt r JOIN blob b ON b.uuid=r.native_hash",
            vec![],
        )
        .await
        .unwrap();
    let [row] = roots.as_slice() else {
        panic!("one public root expected")
    };
    let [Value::Integer(rid), Value::Text(root)] = row.as_slice() else {
        panic!("public root row expected")
    };
    let (rid, root) = (*rid, root.clone());
    let bytes = tx
        .read_owner_content_budget_with_sql(&principal(), &root, &mut budget(), &mut work())
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    drop(store);
    make_phantom(&path, rid).await;

    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    assert_eq!(
        tx.put_private_file_content_budget_with_sql(
            &principal(),
            &root,
            &bytes,
            &mut budget(),
            &mut work(),
        )
        .await
        .unwrap(),
        rid
    );
    tx.commit().await.unwrap();
    drop(store);
    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    assert!(
        tx.query_bound("SELECT rid FROM private", vec![])
            .await
            .unwrap()
            .is_empty()
    );
    assert!(
        tx.query_bound("SELECT rid FROM phantom", vec![])
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        tx.read_owner_content_budget_with_sql(&principal(), &root, &mut budget(), &mut work())
            .await
            .unwrap(),
        bytes
    );
    tx.rollback().await.unwrap();
}

/// A repository with its one registered checkout passes the inventory check, so the
/// metered put and read both admit it.
#[tokio::test(flavor = "current_thread")]
async fn one_registered_checkout_is_admitted_by_the_metered_routes() {
    let temp = tempfile::tempdir().unwrap();
    let path = repository(temp.path()).await;
    let root = temp.path().join("work");
    std::fs::create_dir(&root).unwrap();
    let admission = AdmissionLimits {
        content: limits(),
        parser: parser(),
    };
    let mut owner = LocalOwnerRepository::open(&path, admission).await.unwrap();
    owner
        .register_checkout("work", &root, admission)
        .await
        .unwrap();
    drop(owner);

    let mut store = LocalStore::open(&path).await.unwrap();
    let mut tx = store.begin_write().await.unwrap();
    assert_eq!(
        tx.query_bound(
            "SELECT checkout_count,(SELECT count(*) FROM g_checkout) FROM g_storage",
            vec![],
        )
        .await
        .unwrap(),
        vec![vec![Value::Integer(1), Value::Integer(1)]]
    );
    tx.put_private_file_content_budget_with_sql(
        &principal(),
        &name(BASE),
        BASE,
        &mut budget(),
        &mut work(),
    )
    .await
    .unwrap();
    assert_eq!(
        tx.read_owner_content_budget_with_sql(
            &principal(),
            &name(BASE),
            &mut budget(),
            &mut work()
        )
        .await
        .unwrap(),
        BASE
    );
    tx.commit().await.unwrap();
}
