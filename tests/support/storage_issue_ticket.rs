//! Hold the unit controls of the issue ticket writer and reader that need the owner's
//! private state. They cover the GEST-T08 failure injection before commit and the
//! refusal by admission of each changed row.
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

/// The K card of the fixture project.
const PROJECT_K: &str = "0000000000000000000000000000000000000001";

/// Emit the creation artifact of a fixture project.
/// # Arguments
/// * `k` - The K card.
/// * `letter` - The letter that the 32 letters of the Gest ID repeat.
/// # Returns
/// The artifact bytes.
fn project_for(k: &str, letter: &str) -> Vec<u8> {
    use serde_json::json;
    let project = crate::gest_project_values::project_from_values(
        crate::gest_project_values::ProjectValueInputs {
            gest_id: json!(letter.repeat(32)),
            archived_at: json!(null),
            created_at: json!("2026-11-18T12:00:00Z"),
            updated_at: json!("2026-11-18T12:00:00Z"),
        },
    )
    .unwrap();
    let date = NativeDate::parse(b"2026-11-18T12:00:00Z").unwrap();
    crate::gest_ticket::project_creation_bytes(&project, k.as_bytes(), b"agent", date, 65_536)
        .unwrap()
}

/// Copy the rows that one query selects from one repository into the same table of another.
/// # Arguments
/// * `from` - The owner whose rows are read.
/// * `to` - The owner whose table gets the rows.
/// * `table` - The fixed table name.
/// * `filter` - The fixed WHERE clause.
async fn copy_rows(
    from: &mut LocalOwnerRepository,
    to: &mut LocalOwnerRepository,
    table: &str,
    filter: &str,
) {
    let mut tx = from.store.begin_write().await.unwrap();
    let rows = tx
        .query(&format!("SELECT * FROM {table} WHERE {filter}"))
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert!(!rows.is_empty(), "{table}");
    let mut tx = to.store.begin_write().await.unwrap();
    for row in rows {
        let marks = vec!["?"; row.len()].join(",");
        // The bound writer takes fixed SQL, so the fixture leaks each small statement.
        let sql: &'static str = Box::leak(format!("INSERT INTO {table} VALUES({marks})").into());
        tx.execute_bound(sql, row).await.unwrap();
    }
    let mut budget = Budget::new(limits().admission.content).unwrap();
    let proof = tx.capture_admission_budgeted(&mut budget).await.unwrap();
    tx.commit().await.unwrap();
    to.admission = proof;
}

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
    // The reader uses no SQL ceiling, so only the admission ceilings refuse it.
    let cases: [(&str, IssueLimits, bool); 6] = [
        ("one SQL row", tight(|l| l.sql_rows = 1), false),
        (
            "SQL rows that end after the first inserts",
            tight(|l| l.sql_rows = 60),
            false,
        ),
        ("one SQL byte", tight(|l| l.sql_bytes = 1), false),
        (
            "a small content limit",
            tight(|l| l.admission.content.max_stored_bytes = 64),
            true,
        ),
        (
            "a small decoded total",
            tight(|l| l.admission.content.max_total_decoded_bytes = 64),
            true,
        ),
        (
            "a small parser input",
            tight(|l| l.admission.parser.input_bytes = 64),
            true,
        ),
    ];
    let second = artifact_for(OTHER_K, "n");
    for (case, tight, refuses_read) in cases {
        let temp = tempfile::tempdir().unwrap();
        let mut owner = repository(&temp.path().join("repo")).await;
        create(&mut owner).await.unwrap();
        let before = snapshot(&mut owner).await;
        assert!(
            create_with(&mut owner, &second, tight).await.is_err(),
            "{case}"
        );
        assert_eq!(snapshot(&mut owner).await, before, "{case}");
        assert_eq!(
            owner.read_private_issue(K, tight).await.is_err(),
            refuses_read,
            "{case}"
        );
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
/// Admission refuses each stored row that is not exactly what replay of the issue
/// artifacts gives, in every admitted table. The reader and a new open both refuse,
/// and nothing changes.
async fn admission_refuses_each_changed_issue_row() {
    let tag = format!("(SELECT tagid FROM tag WHERE tagname='tkt-{K}')");
    let last = "(SELECT max(rcvid) FROM rcvfrom)";
    const PROJECTION: &str = "recording native projection differs";
    let cases = [
        ("a second artifact", format!("INSERT INTO tagxref(tagid,tagtype,srcid,origid,value,mtime,rid) VALUES({tag},1,1,1,NULL,0,1)"), "root branch crosslinks differ from canonical bytes"),
        ("an attachment", format!("INSERT INTO attachment(attachid,isLatest,mtime,src,target,filename,comment,user) VALUES(1,1,0,NULL,'{K}','f','c','u')"), "unsupported nonempty catalog6 table: attachment"),
        ("an artifact that is not a ticket", format!("UPDATE tagxref SET rid=1 WHERE tagid={tag}"), "root branch crosslinks differ from canonical bytes"),
        ("a changed ticket title", "UPDATE ticket SET title='\"other\"' WHERE tkt_uuid IS NOT NULL".to_owned(), DIFFERS),
        ("a changed change row", "UPDATE ticketchng SET tkt_user='other'".to_owned(), DIFFERS),
        ("a changed event comment", "UPDATE event SET comment='other' WHERE type='t'".to_owned(), PROJECTION),
        ("a tag row without a RID", "INSERT INTO tagxref(tagid,tagtype,srcid,origid,value,mtime,rid) VALUES(8,2,1,1,'other',0,NULL)".to_owned(), PROJECTION),
        ("a changed tag row", format!("UPDATE tagxref SET tagtype=2 WHERE tagid={tag}"), PROJECTION),
        ("a renamed tag", format!("UPDATE tag SET tagname='tkt-x' WHERE tagid={tag}"), DIFFERS),
        ("an unrelated tag", "INSERT INTO tag(tagname) VALUES('unrelated')".to_owned(), PROJECTION),
        ("a public ticket", format!("DELETE FROM private WHERE rid=(SELECT rid FROM tagxref WHERE tagid={tag})"), PROJECTION),
        ("a changed task row", "UPDATE g_gest_tasks SET title='other'".to_owned(), DIFFERS),
        ("a changed project row", "UPDATE g_gest_projects SET archived_at='2026-11-19T00:00:00Z'".to_owned(), DIFFERS),
        ("a missing project row", "DELETE FROM g_gest_projects".to_owned(), DIFFERS),
        ("an unclustered row", format!("INSERT INTO unclustered(rid) SELECT rid FROM tagxref WHERE tagid={tag}"), "recording SQL count limit"),
        ("an unsent row", format!("INSERT INTO unsent(rid) SELECT rid FROM tagxref WHERE tagid={tag}"), "unsupported nonempty catalog6 table: unsent"),
        ("a leaf row", format!("INSERT INTO leaf(rid) SELECT rid FROM tagxref WHERE tagid={tag}"), PROJECTION),
        ("a blob without a receipt", format!("UPDATE blob SET rcvid=NULL WHERE rid=(SELECT rid FROM tagxref WHERE tagid={tag})"), DIFFERS),
        ("a blob under the root receipt", format!("UPDATE blob SET rcvid=1 WHERE rid=(SELECT rid FROM tagxref WHERE tagid={tag})"), "recording unexplained receipt"),
        ("a receipt from another user", format!("UPDATE rcvfrom SET uid=1 WHERE rcvid={last}"), "recording local receipt policy"),
        ("a receipt with a nonce", format!("UPDATE rcvfrom SET nonce='n' WHERE rcvid={last}"), "recording local receipt policy"),
        ("a receipt from an address", format!("UPDATE rcvfrom SET ipaddr='127.0.0.1' WHERE rcvid={last}"), "recording local receipt policy"),
        ("a receipt that no artifact owns", "INSERT INTO rcvfrom(uid,mtime,nonce,ipaddr) VALUES(0,julianday('now'),NULL,NULL)".to_owned(), "recording unexplained receipt"),
        ("a backlink from the ticket", format!("INSERT INTO backlink(target,srctype,srcid,mtime) SELECT 'x',1,rid,0 FROM tagxref WHERE tagid={tag}"), "unsupported nonempty catalog6 table: backlink"),
        ("a change row of the artifact under another ticket", format!("INSERT INTO ticketchng(tkt_id,tkt_rid,tkt_mtime,tkt_user) SELECT 999,rid,0,'u' FROM tagxref WHERE tagid={tag}"), DIFFERS),
        ("a second event of the ticket", format!("INSERT INTO event(type,tagid,mtime,objid,user,comment) VALUES('t',{tag},0,999,'u','forged')"), PROJECTION),
        ("a second typed row", "INSERT INTO g_gest_tasks SELECT 'shared',id,project_id,title,priority,status,description,assigned_to,metadata,resolved_at,created_at,updated_at FROM g_gest_tasks".to_owned(), DIFFERS),
        ("a ticket row without an artifact", "INSERT INTO ticket(tkt_uuid) VALUES('x')".to_owned(), DIFFERS),
        ("an artifact whose K is not its tag", format!("UPDATE tag SET tagname='tkt-{OTHER_K}' WHERE tagname='tkt-{K}';UPDATE ticket SET tkt_uuid='{OTHER_K}' WHERE tkt_uuid='{K}'"), DIFFERS),
    ];
    for (case, sql, message) in cases {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let mut owner = repository(&path).await;
        create(&mut owner).await.unwrap();
        create_with(&mut owner, &project_for(PROJECT_K, "p"), limits())
            .await
            .unwrap();
        tamper(&mut owner, &sql).await;
        let before = snapshot(&mut owner).await;
        let error = owner.read_private_issue(K, limits()).await.unwrap_err();
        assert_eq!(snapshot(&mut owner).await, before, "{case}");
        assert_eq!(error.to_string(), message, "{case}");
        let write = create_with(&mut owner, &artifact_for(OTHER_K, "n"), limits())
            .await
            .unwrap_err();
        assert_eq!(write.to_string(), message, "{case}");
        assert_eq!(snapshot(&mut owner).await, before, "{case}");
        drop(owner);
        let refused = LocalOwnerRepository::open(&path, limits().admission)
            .await
            .err()
            .unwrap();
        assert_eq!(refused.to_string(), error.to_string(), "{case}");
    }
}

#[tokio::test]
/// One parser ledger serves every artifact that admission parses, so its entry ceiling
/// bounds the count of issues. In this fixture each task takes 14 entries, so 30
/// entries admit two tasks, and refuse one entry fewer or a third task.
async fn the_entry_ceiling_bounds_the_issue_count() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let bounded = |entries| {
        let mut limits = limits().admission;
        limits.parser.entries = entries;
        limits
    };
    let refused = "recording native artifact validation";
    let mut owner = repository(&path).await;
    for (k, letter) in [(K, "m"), (OTHER_K, "n")] {
        create_with(&mut owner, &artifact_for(k, letter), limits())
            .await
            .unwrap();
    }
    drop(owner);
    assert!(LocalOwnerRepository::open(&path, bounded(30)).await.is_ok());
    let error = LocalOwnerRepository::open(&path, bounded(29))
        .await
        .err()
        .unwrap();
    assert_eq!(error.to_string(), refused);
    let mut owner = LocalOwnerRepository::open(&path, limits().admission)
        .await
        .unwrap();
    create_with(&mut owner, &artifact_for(PROJECT_K, "p"), limits())
        .await
        .unwrap();
    drop(owner);
    let error = LocalOwnerRepository::open(&path, bounded(30))
        .await
        .err()
        .unwrap();
    assert_eq!(error.to_string(), refused);
}

#[tokio::test]
/// The admission proof holds every row of the four issue tables, so a change that the
/// owner's own connection makes, with the change counter refreshed, is still refused
/// before the next write.
async fn the_admission_proof_covers_the_issue_tables() {
    for sql in [
        "UPDATE ticket SET title='\"other\"'",
        "UPDATE ticketchng SET tkt_user='other'",
        "UPDATE g_gest_tasks SET title='other'",
        "UPDATE g_gest_projects SET archived_at='2026-11-19T00:00:00Z'",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut owner = repository(&temp.path().join("repo")).await;
        create(&mut owner).await.unwrap();
        create_with(&mut owner, &project_for(PROJECT_K, "p"), limits())
            .await
            .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        tx.execute(sql).await.unwrap();
        tx.commit().await.unwrap();
        owner.admission.total_changes = owner.store.connection.as_ref().unwrap().total_changes();
        let before = snapshot(&mut owner).await;
        let error = create_with(&mut owner, &artifact_for(OTHER_K, "n"), limits())
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "repository authority changed; explicit readmission required",
            "{sql}"
        );
        assert_eq!(snapshot(&mut owner).await, before, "{sql}");
    }
}

#[tokio::test]
/// The writer refuses a project whose Gest ID a task holds, and a task whose Gest ID a
/// project holds, before any effect.
async fn the_writer_refuses_a_gest_id_of_the_other_kind() {
    let temp = tempfile::tempdir().unwrap();
    let mut owner = repository(&temp.path().join("repo")).await;
    create(&mut owner).await.unwrap();
    create_with(&mut owner, &project_for(PROJECT_K, "p"), limits())
        .await
        .unwrap();
    for bytes in [
        project_for("0000000000000000000000000000000000000002", "m"),
        artifact_for(OTHER_K, "p"),
    ] {
        let before = snapshot(&mut owner).await;
        let error = create_with(&mut owner, &bytes, limits()).await.unwrap_err();
        assert_eq!(error.to_string(), UNSUPPORTED);
        assert_eq!(snapshot(&mut owner).await, before);
    }
}

#[tokio::test]
/// Admission refuses a project whose artifact reuses the Gest ID of a task, though its
/// own rows are exact. Admission accepts the same copy with its own Gest ID.
async fn admission_refuses_a_project_that_reuses_a_task_gest_id() {
    for (letter, admitted) in [("m", false), ("p", true)] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let mut owner = repository(&path).await;
        create(&mut owner).await.unwrap();
        // A second repository holds the project at the RID, receipt, tag and ticket ID
        // that it would take in the first, so its rows can be copied unchanged.
        let mut donor = repository(&temp.path().join("donor")).await;
        create_with(&mut donor, &artifact_for(OTHER_K, "z"), limits())
            .await
            .unwrap();
        let project = create_with(&mut donor, &project_for(PROJECT_K, letter), limits())
            .await
            .unwrap();
        for (table, filter) in [
            ("rcvfrom", "rcvid=3"),
            ("blob", "rid=3"),
            ("private", "rid=3"),
            ("tag", "tagid=14"),
            ("tagxref", "rid=3"),
            ("ticket", "tkt_id=2"),
            ("ticketchng", "tkt_rid=3"),
            ("event", "objid=3"),
            ("g_gest_projects", "1"),
        ] {
            copy_rows(&mut donor, &mut owner, table, filter).await;
        }
        let read = owner.read_private_issue(PROJECT_K, limits()).await;
        drop(owner);
        let opened = LocalOwnerRepository::open(&path, limits().admission).await;
        if admitted {
            assert_eq!(read.unwrap(), project);
            assert!(opened.is_ok());
        } else {
            let error = read.unwrap_err().to_string();
            assert_eq!(error, "stored issue rows differ from the ticket artifact");
            assert_eq!(opened.err().unwrap().to_string(), error);
        }
    }
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

/// The refusal of the binding admission.
const BINDING_STATE: &str = "unsupported project binding state";

/// Create a catalog 6 repository with the checkout `work` registered at `DIR/root`.
/// # Arguments
/// * `dir` - An empty directory, which gets the repository `repo` and the root `root`.
/// # Returns
/// The admitted owner.
async fn registered(dir: &std::path::Path) -> LocalOwnerRepository {
    let root = dir.join("root");
    std::fs::create_dir(&root).unwrap();
    let mut owner = repository(&dir.join("repo")).await;
    owner
        .register_checkout("work", &root.canonicalize().unwrap(), limits().admission)
        .await
        .unwrap();
    owner
}

/// Write one artifact through the `issue init` writer for a checkout.
/// # Arguments
/// * `owner` - The admitted owner.
/// * `bytes` - The creation artifact.
/// * `checkout` - The checkout ID.
/// # Returns
/// The writer's result.
async fn init_with(
    owner: &mut LocalOwnerRepository,
    bytes: &[u8],
    checkout: &str,
) -> Result<IssueTicket, StorageError> {
    owner
        .init_private_project(
            ProjectInitRequest {
                artifact: bytes,
                checkout_id: checkout,
            },
            limits(),
        )
        .await
}

/// Emit the creation artifact of a fixture project that is archived.
/// # Arguments
/// * `k` - The K card.
/// * `letter` - The letter that the 32 letters of the Gest ID repeat.
/// # Returns
/// The artifact bytes.
fn archived_project(k: &str, letter: &str) -> Vec<u8> {
    use serde_json::json;
    let project = crate::gest_project_values::project_from_values(
        crate::gest_project_values::ProjectValueInputs {
            gest_id: json!(letter.repeat(32)),
            archived_at: json!("2026-11-19T00:00:00Z"),
            created_at: json!("2026-11-18T12:00:00Z"),
            updated_at: json!("2026-11-18T12:00:00Z"),
        },
    )
    .unwrap();
    let date = NativeDate::parse(b"2026-11-18T12:00:00Z").unwrap();
    crate::gest_ticket::project_creation_bytes(&project, k.as_bytes(), b"agent", date, 65_536)
        .unwrap()
}

#[tokio::test]
/// CAT-T01 and GEST-T08 for `issue init`: a failure injected after each write step,
/// the binding step included, leaves every table and every byte unchanged and the
/// checkout unbound. The same owner then binds the project, and a new open admits it.
async fn an_init_failure_before_commit_leaves_every_byte_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let mut owner = registered(temp.path()).await;
    create(&mut owner).await.unwrap();
    let project = project_for(PROJECT_K, "p");
    for phase in [
        IssuePhase::Receipt,
        IssuePhase::Content,
        IssuePhase::Tag,
        IssuePhase::Ticket,
        IssuePhase::Event,
        IssuePhase::Typed,
        IssuePhase::Binding,
        IssuePhase::Admission,
    ] {
        let tables = snapshot(&mut owner).await;
        let files = file_bytes(&path);
        ISSUE_FAILURE.with(|selected| selected.set(Some(phase)));
        let error = init_with(&mut owner, &project, "work").await.unwrap_err();
        ISSUE_FAILURE.with(|selected| selected.set(None));
        assert_eq!(
            error.to_string(),
            "injected issue ticket failure",
            "{phase:?}"
        );
        assert_eq!(file_bytes(&path), files, "{phase:?}");
        assert_eq!(snapshot(&mut owner).await, tables, "{phase:?}");
        assert_eq!(owner.bound_project("work", limits()).await.unwrap(), None);
    }
    let stored = init_with(&mut owner, &project, "work").await.unwrap();
    assert_eq!(stored.ticket_id, PROJECT_K);
    let bound = Some("p".repeat(32));
    assert_eq!(owner.bound_project("work", limits()).await.unwrap(), bound);
    assert_eq!(owner.bound_project("other", limits()).await.unwrap(), None);
    drop(owner);
    let mut reopened = LocalOwnerRepository::open(&path, limits().admission)
        .await
        .unwrap();
    assert_eq!(
        reopened.bound_project("work", limits()).await.unwrap(),
        bound
    );
    assert_eq!(
        reopened
            .read_private_issue(PROJECT_K, limits())
            .await
            .unwrap(),
        stored
    );
}

#[tokio::test]
/// The `issue init` writer refuses a task creation, an unknown checkout, a checkout
/// that already has a project, a repository that holds an unbound project, and a
/// repository without a registration, before any effect.
async fn init_refuses_a_task_an_unknown_checkout_and_a_bound_checkout() {
    let temp = tempfile::tempdir().unwrap();
    let mut owner = registered(temp.path()).await;
    for (bytes, checkout) in [(artifact(), "work"), (project_for(PROJECT_K, "p"), "other")] {
        let before = snapshot(&mut owner).await;
        let error = init_with(&mut owner, &bytes, checkout).await.unwrap_err();
        assert_eq!(error.to_string(), INIT_REFUSED);
        assert_eq!(snapshot(&mut owner).await, before);
    }
    init_with(&mut owner, &project_for(PROJECT_K, "p"), "work")
        .await
        .unwrap();
    let before = snapshot(&mut owner).await;
    let error = init_with(&mut owner, &project_for(OTHER_K, "q"), "work")
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), INIT_REFUSED);
    assert_eq!(snapshot(&mut owner).await, before);
    let unbound = tempfile::tempdir().unwrap();
    let mut owner = registered(unbound.path()).await;
    create_with(&mut owner, &project_for(OTHER_K, "q"), limits())
        .await
        .unwrap();
    let before = snapshot(&mut owner).await;
    let error = init_with(&mut owner, &project_for(PROJECT_K, "p"), "work")
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), INIT_REFUSED);
    assert_eq!(snapshot(&mut owner).await, before);
    let unregistered = tempfile::tempdir().unwrap();
    let mut owner = repository(&unregistered.path().join("repo")).await;
    let error = init_with(&mut owner, &project_for(PROJECT_K, "p"), "work")
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), INIT_REFUSED);
}

#[tokio::test]
/// Admission accepts only the two binding rows that `issue init` writes. It refuses
/// each changed, missing or extra row, a project that no admitted project ticket
/// holds, a task, an archived project, and values that are not text. The reader of
/// the binding and a new open both refuse, and nothing changes.
async fn admission_refuses_each_changed_binding_row() {
    let (task, archived, unknown) = ("m".repeat(32), "r".repeat(32), "q".repeat(32));
    let both = |id: &str| {
        format!(
            "UPDATE g_project_binding SET project_id='{id}';UPDATE g_checkout_project SET project_id='{id}'"
        )
    };
    let cases = [
        (
            "a binding without its checkout project",
            "DELETE FROM g_checkout_project".to_owned(),
        ),
        (
            "a checkout project without its binding",
            "DELETE FROM g_project_binding".to_owned(),
        ),
        (
            "another root",
            "UPDATE g_project_binding SET root=root||'/x'".to_owned(),
        ),
        ("a project that no ticket holds", both(&unknown)),
        ("a task as the project", both(&task)),
        ("an archived project", both(&archived)),
        (
            "a checkout project of another project",
            format!("UPDATE g_checkout_project SET project_id='{archived}'"),
        ),
        (
            "a second binding",
            format!(
                "INSERT INTO g_project_binding SELECT checkout_id,'{archived}',root||'/x' FROM g_project_binding"
            ),
        ),
        (
            "a binding project ID as a blob",
            "UPDATE g_project_binding SET project_id=CAST(project_id AS BLOB)".to_owned(),
        ),
        (
            "a checkout project ID as a blob",
            "UPDATE g_checkout_project SET project_id=CAST(project_id AS BLOB)".to_owned(),
        ),
        (
            "a root as a blob",
            "UPDATE g_project_binding SET root=CAST(root AS BLOB)".to_owned(),
        ),
    ];
    for (case, sql) in cases {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let mut owner = registered(temp.path()).await;
        create(&mut owner).await.unwrap();
        init_with(&mut owner, &project_for(PROJECT_K, "p"), "work")
            .await
            .unwrap();
        create_with(&mut owner, &archived_project(OTHER_K, "r"), limits())
            .await
            .unwrap();
        tamper(&mut owner, &sql).await;
        let before = snapshot(&mut owner).await;
        let error = owner.bound_project("work", limits()).await.unwrap_err();
        assert_eq!(error.to_string(), BINDING_STATE, "{case}");
        assert_eq!(snapshot(&mut owner).await, before, "{case}");
        drop(owner);
        let refused = LocalOwnerRepository::open(&path, limits().admission)
            .await
            .err()
            .unwrap();
        assert_eq!(refused.to_string(), BINDING_STATE, "{case}");
    }
}

#[tokio::test]
/// A checkout ID in a binding table that is not the text ID of the registered
/// checkout breaks a foreign key. The writer cannot store it. When a write with the
/// foreign keys off stores it, the foreign key check of admission refuses it, before
/// the binding check.
async fn admission_refuses_a_binding_of_another_checkout() {
    for sql in [
        "UPDATE g_project_binding SET checkout_id=CAST(checkout_id AS BLOB)",
        "UPDATE g_checkout_project SET checkout_id=CAST(checkout_id AS BLOB)",
        "UPDATE g_checkout_project SET checkout_id='other'",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        let mut owner = registered(temp.path()).await;
        init_with(&mut owner, &project_for(PROJECT_K, "p"), "work")
            .await
            .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        assert!(tx.execute(sql).await.is_err(), "{sql}");
        // The failed statement latches its error, so the rollback reports it again.
        assert!(tx.rollback().await.is_err(), "{sql}");
        drop(owner);
        let database = libsql::Builder::new_local(&path).build().await.unwrap();
        let connection = database.connect().unwrap();
        connection
            .execute_batch(&format!("PRAGMA foreign_keys=OFF;{sql}"))
            .await
            .unwrap();
        drop(connection);
        let refused = LocalOwnerRepository::open(&path, limits().admission)
            .await
            .err()
            .unwrap();
        assert_eq!(
            refused.to_string(),
            "repository foreign key integrity failure",
            "{sql}"
        );
    }
}

#[tokio::test]
/// The admission proof holds the rows of both binding tables. So the next read
/// refuses a change that the owner's own connection makes, with the change counter
/// refreshed.
async fn the_admission_proof_covers_the_binding_tables() {
    for sql in [
        "UPDATE g_project_binding SET root=root||'/x'",
        "UPDATE g_checkout_project SET project_id='q'",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut owner = registered(temp.path()).await;
        init_with(&mut owner, &project_for(PROJECT_K, "p"), "work")
            .await
            .unwrap();
        let mut tx = owner.store.begin_write().await.unwrap();
        tx.execute(sql).await.unwrap();
        tx.commit().await.unwrap();
        owner.admission.total_changes = owner.store.connection.as_ref().unwrap().total_changes();
        let error = owner.bound_project("work", limits()).await.unwrap_err();
        assert_eq!(
            error.to_string(),
            "repository authority changed; explicit readmission required",
            "{sql}"
        );
    }
}
