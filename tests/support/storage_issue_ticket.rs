//! Hold the unit controls of the issue ticket writer and reader that need the owner's
//! private state. They cover the GEST-T08 failure injection before commit and the
//! reader's refusal of changed rows.
use super::*;
use crate::storage::{InitializeOptions, Limits};

/// Supply finite ceilings that every fixture fits.
/// # Returns
/// Generous content, parser and SQL ceilings.
fn limits() -> IssueLimits {
    IssueLimits {
        admission: AdmissionLimits {
            content: Limits {
                max_stored_bytes: 1 << 20,
                max_output_bytes: 1 << 20,
                max_chain_depth: 128,
                max_total_stored_bytes: 64 << 20,
                max_total_decoded_bytes: 256 << 20,
            },
            parser: crate::artifact::ParseLimits {
                input_bytes: 4 << 20,
                cards: 65536,
                owned_bytes: 16 << 20,
                entries: 8192,
            },
        },
        sql_rows: 100_000,
        sql_bytes: 64 << 20,
    }
}

/// The K card of the fixture task.
const K: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678";

/// The K card of a second fixture task.
const OTHER_K: &str = "ffffffffffffffffffffffffffffffffffffffff";

/// Emit the creation artifact of the fixture task.
/// # Returns
/// The artifact bytes.
fn artifact() -> Vec<u8> {
    artifact_for(K, "m")
}

/// Emit the creation artifact of a fixture task with its own ticket and Gest IDs.
/// # Arguments
/// * `k` - The K card.
/// * `letter` - The letter that the 32 letters of the Gest ID repeat.
/// # Returns
/// The artifact bytes.
fn artifact_for(k: &str, letter: &str) -> Vec<u8> {
    use serde_json::json;
    let task = task_from_values(TaskValueInputs {
        gest_id: json!(letter.repeat(32)),
        project_id: json!("k".repeat(32)),
        title: json!("Task"),
        priority: json!(1),
        status: json!("open"),
        description: json!(""),
        assigned_to: json!(null),
        metadata: json!({}),
        resolved_at: json!(null),
        created_at: json!("2026-11-18T12:00:00Z"),
        updated_at: json!("2026-11-18T12:00:00Z"),
    })
    .unwrap();
    let date = NativeDate::parse(b"2026-11-18T12:00:00Z").unwrap();
    crate::task_ticket::task_creation_bytes(&task, k.as_bytes(), b"agent", date, 65_536).unwrap()
}

/// Create a new catalog 6 repository through the product initializer.
/// # Arguments
/// * `path` - The absent repository file.
/// # Returns
/// The admitted owner.
async fn repository(path: &std::path::Path) -> LocalOwnerRepository {
    let b = limits().admission;
    LocalOwnerRepository::initialize(
        path,
        InitializeOptions {
            repository_id: "0123456789abcdef0123456789abcdef01234567",
            owner_id: "owner",
            date: "2026-11-20T00:00:00",
            user: b"Owner",
            password: "secret",
            sha1: false,
            content_limits: b.content,
            parse_limits: b.parser,
        },
    )
    .await
    .unwrap()
}

/// Write the fixture task through the writer.
/// # Arguments
/// * `owner` - The admitted owner.
/// # Returns
/// The writer's result.
async fn create(owner: &mut LocalOwnerRepository) -> Result<IssueTicket, StorageError> {
    create_with(owner, &artifact(), limits()).await
}

/// Write one artifact through the writer with explicit limits.
/// # Arguments
/// * `owner` - The admitted owner.
/// * `bytes` - The creation artifact.
/// * `limits` - The ceilings of the call.
/// # Returns
/// The writer's result.
async fn create_with(
    owner: &mut LocalOwnerRepository,
    bytes: &[u8],
    limits: IssueLimits,
) -> Result<IssueTicket, StorageError> {
    owner
        .create_private_issue(
            PrivateIssueRequest {
                artifact: bytes,
                scope: PathScope::Private,
            },
            limits,
        )
        .await
}

/// Read every table through the owner's own connection.
/// # Arguments
/// * `owner` - An owner with no open transaction.
/// # Returns
/// Each table name with its rows, sorted.
async fn snapshot(owner: &mut LocalOwnerRepository) -> Vec<(String, Vec<Vec<Value>>)> {
    let mut tx = owner.store.begin_write().await.unwrap();
    let names = tx
        .query("SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name")
        .await
        .unwrap();
    let mut all = Vec::new();
    for row in names {
        let name = row[0].as_text().unwrap().clone();
        let mut values = tx
            .query(&format!("SELECT * FROM \"{name}\""))
            .await
            .unwrap();
        values.sort_by_key(|row| format!("{row:?}"));
        all.push((name, values));
    }
    tx.rollback().await.unwrap();
    all
}

/// Change the stored rows with fixture SQL, then admit the changed state again.
/// # Arguments
/// * `owner` - The owner whose admission proof the change would make stale.
/// * `sql` - Fixture statements, separated by semicolons.
async fn tamper(owner: &mut LocalOwnerRepository, sql: &str) {
    let mut tx = owner.store.begin_write().await.unwrap();
    for statement in sql.split(';') {
        tx.execute(statement).await.unwrap();
    }
    let mut budget = Budget::new(limits().admission.content).unwrap();
    let proof = tx.capture_admission_budgeted(&mut budget).await.unwrap();
    tx.commit().await.unwrap();
    owner.admission = proof;
}

/// Read the bytes of the repository file and its write-ahead log.
/// # Arguments
/// * `path` - The repository file.
/// # Returns
/// The bytes of both files. An absent log reads as empty.
fn file_bytes(path: &std::path::Path) -> (Vec<u8>, Vec<u8>) {
    let mut wal = path.as_os_str().to_owned();
    wal.push("-wal");
    (
        std::fs::read(path).unwrap(),
        std::fs::read(wal).unwrap_or_default(),
    )
}

#[tokio::test]
/// GEST-T08: a failure injected after each write step, up to the last step before
/// COMMIT, leaves every table and every byte of the repository files unchanged. The
/// same owner then still reads the ticket that it stored before, and takes the write.
async fn a_failure_before_commit_leaves_every_byte_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let mut owner = repository(&path).await;
    create(&mut owner).await.unwrap();
    let second = artifact_for(OTHER_K, "n");
    for phase in [
        IssuePhase::Receipt,
        IssuePhase::Content,
        IssuePhase::Tag,
        IssuePhase::Ticket,
        IssuePhase::Event,
        IssuePhase::Typed,
        IssuePhase::Admission,
    ] {
        let tables = snapshot(&mut owner).await;
        let files = file_bytes(&path);
        ISSUE_FAILURE.with(|selected| selected.set(Some(phase)));
        let error = create_with(&mut owner, &second, limits())
            .await
            .unwrap_err();
        ISSUE_FAILURE.with(|selected| selected.set(None));
        assert_eq!(
            error.to_string(),
            "injected issue ticket failure",
            "{phase:?}"
        );
        assert_eq!(file_bytes(&path), files, "{phase:?}");
        assert_eq!(snapshot(&mut owner).await, tables, "{phase:?}");
        let read = owner.read_private_issue(K, limits()).await;
        assert_eq!(read.unwrap().ticket_id, K, "{phase:?}");
    }
    let stored = create_with(&mut owner, &second, limits()).await.unwrap();
    assert_eq!(stored.ticket_id, OTHER_K);
    let read = owner.read_private_issue(OTHER_K, limits()).await.unwrap();
    assert_eq!(read, stored);
}

#[tokio::test]
/// A ceiling of the call that the work exceeds refuses the write or the read, leaves
/// every table unchanged, and leaves the owner able to read and write afterwards.
async fn an_exhausted_ceiling_refuses_without_effect() {
    let tight = |change: fn(&mut IssueLimits)| {
        let mut limits = limits();
        change(&mut limits);
        limits
    };
    let cases: [(&str, IssueLimits); 6] = [
        ("one SQL row", tight(|l| l.sql_rows = 1)),
        (
            "SQL rows that end after the first inserts",
            tight(|l| l.sql_rows = 60),
        ),
        ("one SQL byte", tight(|l| l.sql_bytes = 1)),
        (
            "a small content limit",
            tight(|l| l.admission.content.max_stored_bytes = 64),
        ),
        (
            "a small decoded total",
            tight(|l| l.admission.content.max_total_decoded_bytes = 64),
        ),
        (
            "a small parser input",
            tight(|l| l.admission.parser.input_bytes = 64),
        ),
    ];
    let second = artifact_for(OTHER_K, "n");
    for (case, tight) in cases {
        let temp = tempfile::tempdir().unwrap();
        let mut owner = repository(&temp.path().join("repo")).await;
        create(&mut owner).await.unwrap();
        let before = snapshot(&mut owner).await;
        assert!(
            create_with(&mut owner, &second, tight).await.is_err(),
            "{case}"
        );
        assert_eq!(snapshot(&mut owner).await, before, "{case}");
        assert!(owner.read_private_issue(K, tight).await.is_err(), "{case}");
        assert_eq!(snapshot(&mut owner).await, before, "{case}");
        assert_eq!(
            owner
                .read_private_issue(K, limits())
                .await
                .unwrap()
                .ticket_id,
            K,
            "{case}"
        );
        assert_eq!(
            create_with(&mut owner, &second, limits())
                .await
                .unwrap()
                .ticket_id,
            OTHER_K,
            "{case}"
        );
    }
}

#[tokio::test]
/// The reader refuses each stored shape that is not one creation, and each stored
/// row that differs from the artifact, and it changes nothing.
async fn the_reader_refuses_changed_rows_and_other_shapes() {
    let differ = "stored issue rows differ from the ticket artifact";
    let tag = format!("(SELECT tagid FROM tag WHERE tagname='tkt-{K}')");
    let cases = [
        ("a second artifact", format!("INSERT INTO tagxref(tagid,tagtype,srcid,origid,value,mtime,rid) VALUES({tag},1,1,1,NULL,0,1)"), UNSUPPORTED),
        ("an attachment", format!("INSERT INTO attachment(attachid,isLatest,mtime,src,target,filename,comment,user) VALUES(1,1,0,NULL,'{K}','f','c','u')"), UNSUPPORTED),
        ("an artifact that is not a ticket", format!("UPDATE tagxref SET rid=1 WHERE tagid={tag}"), UNSUPPORTED),
        ("a changed ticket title", "UPDATE ticket SET title='\"other\"'".to_owned(), differ),
        ("a changed change row", "UPDATE ticketchng SET tkt_user='other'".to_owned(), differ),
        ("a changed event comment", "UPDATE event SET comment='other' WHERE type='t'".to_owned(), differ),
        ("a changed tag row", format!("UPDATE tagxref SET tagtype=2 WHERE tagid={tag}"), differ),
        ("a public ticket", "DELETE FROM private".to_owned(), differ),
        ("a changed typed row", "UPDATE g_gest_tasks SET title='other'".to_owned(), differ),
        ("an unclustered row", format!("INSERT INTO unclustered(rid) SELECT rid FROM tagxref WHERE tagid={tag}"), differ),
        ("an unsent row", format!("INSERT INTO unsent(rid) SELECT rid FROM tagxref WHERE tagid={tag}"), differ),
        ("a receipt from another user", "UPDATE rcvfrom SET uid=1 WHERE rcvid=(SELECT max(rcvid) FROM rcvfrom)".to_owned(), differ),
        ("a receipt with a nonce", "UPDATE rcvfrom SET nonce='n' WHERE rcvid=(SELECT max(rcvid) FROM rcvfrom)".to_owned(), differ),
        ("a receipt from an address", "UPDATE rcvfrom SET ipaddr='127.0.0.1' WHERE rcvid=(SELECT max(rcvid) FROM rcvfrom)".to_owned(), differ),
        ("a backlink from the ticket", format!("INSERT INTO backlink(target,srctype,srcid,mtime) SELECT 'x',1,rid,0 FROM tagxref WHERE tagid={tag}"), differ),
        ("a change row of the artifact under another ticket", format!("INSERT INTO ticketchng(tkt_id,tkt_rid,tkt_mtime,tkt_user) SELECT 999,rid,0,'u' FROM tagxref WHERE tagid={tag}"), differ),
        ("a second event of the ticket", format!("INSERT INTO event(type,tagid,mtime,objid,user,comment) VALUES('t',{tag},0,999,'u','forged')"), differ),
        ("a second typed row", "INSERT INTO g_gest_tasks SELECT 'shared',id,project_id,title,priority,status,description,assigned_to,metadata,resolved_at,created_at,updated_at FROM g_gest_tasks".to_owned(), differ),
    ];
    for (case, sql, message) in cases {
        let temp = tempfile::tempdir().unwrap();
        let mut owner = repository(&temp.path().join("repo")).await;
        create(&mut owner).await.unwrap();
        tamper(&mut owner, &sql).await;
        let before = snapshot(&mut owner).await;
        let error = owner.read_private_issue(K, limits()).await.unwrap_err();
        assert_eq!(error.to_string(), message, "{case}");
        assert_eq!(snapshot(&mut owner).await, before, "{case}");
    }
    let temp = tempfile::tempdir().unwrap();
    let mut owner = repository(&temp.path().join("repo")).await;
    create(&mut owner).await.unwrap();
    tamper(
        &mut owner,
        &format!("UPDATE tag SET tagname='tkt-{OTHER_K}' WHERE tagname='tkt-{K}';UPDATE ticket SET tkt_uuid='{OTHER_K}' WHERE tkt_uuid='{K}'"),
    )
    .await;
    let error = owner
        .read_private_issue(OTHER_K, limits())
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        differ,
        "an artifact whose K is not the asked ticket ID"
    );
}

#[tokio::test]
/// A write that the admission check refuses as stale does not admit the change that
/// made it stale, so the next call of the same owner is refused too.
async fn a_stale_refusal_does_not_admit_the_change() {
    let temp = tempfile::tempdir().unwrap();
    let mut owner = repository(&temp.path().join("repo")).await;
    create(&mut owner).await.unwrap();
    let mut tx = owner.store.begin_write().await.unwrap();
    tx.execute(
        "INSERT INTO ticketchng(tkt_id,tkt_rid,tkt_mtime,tkt_user) VALUES(999,999,0,'hidden')",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let stale = "repository admission stale; reopen with explicit admission limits";
    let second = artifact_for(OTHER_K, "n");
    let error = create_with(&mut owner, &second, limits())
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), stale);
    let error = owner.read_private_issue(K, limits()).await.unwrap_err();
    assert_eq!(error.to_string(), stale);
}

#[tokio::test]
/// A writer that holds the admission of a repository whose marker now names
/// catalog 4 refuses before any effect.
async fn the_writer_refuses_an_older_catalog() {
    let temp = tempfile::tempdir().unwrap();
    let mut owner = repository(&temp.path().join("repo")).await;
    tamper(
        &mut owner,
        "PRAGMA ignore_check_constraints=ON;UPDATE g_storage SET storage_version=4",
    )
    .await;
    let before = snapshot(&mut owner).await;
    let error = create(&mut owner).await.unwrap_err();
    assert!(error.to_string().contains("catalog"), "{error}");
    assert_eq!(snapshot(&mut owner).await, before);
}

/// Fossil `htmlize` escapes exactly five characters.
#[test]
fn htmlize_escapes_the_five_fossil_characters() {
    assert_eq!(
        htmlize("<a href='x'>&\"é"),
        "&lt;a href=&#39;x&#39;&gt;&amp;&quot;é"
    );
}
