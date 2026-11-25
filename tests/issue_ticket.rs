//! Check the private issue ticket writer and reader through the public library.
//!
//! The cases are GEST-T01 for a task and a project, the subset of GEST-T02 that one
//! creation can meet, the refusals of this build, and the CAT-T07 subset, where
//! check-ins and issues share one repository in several orders. Two ignored oracles
//! check that stock Fossil 2.29 reads the stored tickets, alone and beside check-ins.
//!
//! ## Recommendations
//! A task needs its project, so each test stores the project first. Open checks
//! only what does not grow with the history, and `verify` gives the full proof, so
//! a tamper case asserts the refusal of `verify`.
use gossil::gest_project_values::{ProjectValueInputs, project_from_values};
use gossil::gest_task_values::{TaskValueInputs, task_from_values};
use gossil::native_date::NativeDate;
use gossil::storage::{
    AdmissionLimits, InitializeOptions, IssueLimits, IssueRecord, IssueTicket, Limits,
    LocalOwnerRepository, PathScope, PrivateIssueRequest,
};
use libsql::Value;
use serde_json::{Value as Json, json};
use std::path::Path;

#[path = "support/proof.rs"]
mod proof;

/// The K card of the task fixture.
const TASK_K: &str = "a1b2c3d4e5f60718293a4b5c6d7e8f9012345678";
/// The K card of the project fixture.
const PROJECT_K: &str = "0123456789abcdef0123456789abcdef01234567";

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
            parser: gossil::artifact::ParseLimits {
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

/// Create a new catalog 6 repository through the product initializer.
/// # Arguments
/// * `path` - The absent repository file.
/// # Returns
/// The admitted owner of the new repository.
/// # Panics
/// Panics when initialization fails.
async fn repository(path: &Path) -> LocalOwnerRepository {
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

/// Return the eleven task inputs of the fixture, with explicit nulls.
/// # Returns
/// A valid open task.
fn task_inputs() -> TaskValueInputs {
    TaskValueInputs {
        gest_id: json!("mmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmm"),
        project_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
        title: json!("Task <é> & \"quoted\" 'single'"),
        priority: json!(2),
        status: json!("open"),
        description: json!("line\nnext"),
        assigned_to: Json::Null,
        metadata: json!({"key": "value"}),
        resolved_at: Json::Null,
        created_at: json!("2026-11-18T12:00:00Z"),
        updated_at: json!("2026-11-18T12:00:00Z"),
    }
}

/// Return the D card of every fixture.
/// # Returns
/// A creation instant at noon, which is an integral native Julian day.
fn date() -> NativeDate {
    NativeDate::parse(b"2026-11-18T12:00:00Z").unwrap()
}

/// Emit the creation artifact of one task.
/// # Arguments
/// * `inputs` - The eleven task inputs.
/// * `k` - The K card.
/// # Returns
/// The artifact bytes.
fn task_artifact(inputs: TaskValueInputs, k: &str) -> Vec<u8> {
    let task = task_from_values(inputs).unwrap();
    gossil::task_ticket::task_creation_bytes(&task, k.as_bytes(), b"gest agent", date(), 65_536)
        .unwrap()
}

/// Emit the creation artifact of the project fixture.
/// # Returns
/// The artifact bytes.
fn project_artifact() -> Vec<u8> {
    let project = project_from_values(ProjectValueInputs {
        gest_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
        archived_at: Json::Null,
        created_at: json!("2026-11-18T12:00:00Z"),
        updated_at: json!("2026-11-19T08:30:00.123456789Z"),
    })
    .unwrap();
    gossil::gest_ticket::project_creation_bytes(
        &project,
        PROJECT_K.as_bytes(),
        b"gest agent",
        date(),
        65_536,
    )
    .unwrap()
}

/// Replace one card of an artifact and write its Z card again.
/// # Arguments
/// * `bytes` - A complete artifact.
/// * `from` - Exact card text to replace, with its newline.
/// * `to` - Replacement card text, or empty to drop the card.
/// # Returns
/// The edited artifact with a correct checksum.
fn respell(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
    let text = std::str::from_utf8(bytes).unwrap();
    let body = &text[..text.rfind("Z ").unwrap()];
    assert!(body.contains(from), "{from}");
    let mut body = body.replacen(from, to, 1).into_bytes();
    let checksum = gossil::content_hash::hex(&gossil::artifact_checksum::md5(&body));
    body.extend_from_slice(format!("Z {checksum}\n").as_bytes());
    body
}

/// Write one private issue through the public writer.
/// # Arguments
/// * `owner` - The admitted owner.
/// * `artifact` - The creation artifact.
/// # Returns
/// The writer's result.
async fn create(
    owner: &mut LocalOwnerRepository,
    artifact: &[u8],
) -> Result<IssueTicket, gossil::storage::StorageError> {
    owner
        .create_private_issue(
            PrivateIssueRequest {
                artifact,
                scope: PathScope::Private,
            },
            limits(),
        )
        .await
}

/// Run a query on a separate fixture connection and keep every value.
/// # Arguments
/// * `path` - Existing repository file, with no open owner.
/// * `sql` - Fixture query.
/// # Returns
/// The rows, each as owned values.
async fn rows(path: &Path, sql: &str) -> Vec<Vec<Value>> {
    let database = libsql::Builder::new_local(path).build().await.unwrap();
    let connection = database.connect().unwrap();
    let mut rows = connection.query(sql, ()).await.unwrap();
    let mut values = Vec::new();
    while let Some(row) = rows.next().await.unwrap() {
        values.push(
            (0..row.column_count())
                .map(|index| row.get_value(index).unwrap())
                .collect(),
        );
    }
    values
}

/// Read every table of a repository on a separate connection.
/// # Arguments
/// * `path` - Existing repository file, with no open owner.
/// # Returns
/// Each table name with its rows, sorted.
async fn snapshot(path: &Path) -> Vec<(String, Vec<Vec<Value>>)> {
    let mut all = Vec::new();
    for name in rows(
        path,
        "SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name",
    )
    .await
    {
        let name = name[0].as_text().unwrap().clone();
        let mut values = rows(path, &format!("SELECT * FROM \"{name}\"")).await;
        values.sort_by_key(|row| format!("{row:?}"));
        all.push((name, values));
    }
    all
}

/// Shorthand for a TEXT value.
/// # Arguments
/// * `text` - The text.
/// # Returns
/// The SQL value.
fn text(text: &str) -> Value {
    Value::Text(text.into())
}

#[tokio::test]
/// Check GEST-T01 for a task. The writer stores every field, the reader derives the
/// same task again from the artifact, and the native and typed rows hold each field.
async fn a_task_round_trips_through_its_artifact() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let mut owner = repository(&path).await;
    create(&mut owner, &project_artifact()).await.unwrap();
    let bytes = task_artifact(task_inputs(), TASK_K);
    let stored = create(&mut owner, &bytes).await.unwrap();
    let task = task_from_values(task_inputs()).unwrap();
    assert_eq!(stored.ticket_id, TASK_K);
    assert_eq!(
        stored.native_name,
        gossil::content_hash::hex(&gossil::content_hash::sha3(&bytes, 256).unwrap())
    );
    assert_eq!(stored.record, IssueRecord::Task(task));
    assert_eq!(
        owner.read_private_issue(TASK_K, limits()).await.unwrap(),
        stored
    );
    drop(owner);
    assert_eq!(
        rows(&path, "SELECT scope_key,id,project_id,title,priority,status,description,assigned_to,metadata,resolved_at,created_at,updated_at FROM g_gest_tasks").await,
        vec![vec![
            text("combined"),
            text("mmmmmmmmmmmmmmmmmmmmmmmmmmmmmmmm"),
            text("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
            text("Task <é> & \"quoted\" 'single'"),
            Value::Integer(2),
            text("open"),
            text("line\nnext"),
            Value::Null,
            text("{\"key\":\"value\"}"),
            Value::Null,
            text("2026-11-18T12:00:00+00:00"),
            text("2026-11-18T12:00:00+00:00"),
        ]]
    );
    assert_eq!(
        rows(&path, &format!("SELECT title,status,priority,metadata,assigned_to,gossil_kind,gossil_deleted,type,comment FROM ticket WHERE tkt_uuid='{TASK_K}'")).await,
        vec![vec![
            text("\"Task <é> & \\\"quoted\\\" 'single'\""),
            text("\"open\""),
            text("2"),
            text("{\"key\":\"value\"}"),
            text("null"),
            text("task"),
            text("false"),
            Value::Null,
            Value::Null,
        ]]
    );
    let short = &TASK_K[..10];
    let long = &TASK_K[..16];
    assert_eq!(
        rows(
            &path,
            &format!("SELECT e.type,e.user,e.comment,e.brief,e.mtime FROM event e JOIN tag t ON t.tagid=e.tagid WHERE t.tagname='tkt-{TASK_K}'")
        )
        .await,
        vec![vec![
            text("t"),
            text("gest agent"),
            text(&format!(
                "New ticket [{long}|{short}] <i>&quot;Task &lt;é&gt; &amp; \\&quot;quoted\\&quot; &#39;single&#39;&quot;</i>."
            )),
            text(&format!("New ticket [{long}|{short}].")),
            Value::Integer(2461363),
        ]]
    );
    assert_eq!(
        rows(&path, "SELECT (SELECT count(*) FROM private),(SELECT count(*) FROM unclustered WHERE rid>1),(SELECT count(*) FROM unsent),(SELECT count(*) FROM rcvfrom),(SELECT count(*) FROM transactions)+(SELECT count(*) FROM transaction_events)+(SELECT count(*) FROM g_command_audit)+(SELECT count(*) FROM g_selector)+(SELECT count(*) FROM backlink)").await,
        vec![[2, 0, 0, 3, 0].map(Value::Integer).to_vec()]
    );
}

#[tokio::test]
/// Check GEST-T01 for a project. The record, the typed row and the empty event title
/// round-trip, and a task and a project share one repository.
async fn a_project_round_trips_beside_a_task() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let mut owner = repository(&path).await;
    let bytes = project_artifact();
    let stored = create(&mut owner, &bytes).await.unwrap();
    assert!(matches!(stored.record, IssueRecord::Project(_)));
    create(&mut owner, &task_artifact(task_inputs(), TASK_K))
        .await
        .unwrap();
    assert_eq!(
        owner.read_private_issue(PROJECT_K, limits()).await.unwrap(),
        stored
    );
    assert_eq!(
        owner
            .read_private_issue(TASK_K, limits())
            .await
            .unwrap()
            .ticket_id,
        TASK_K
    );
    drop(owner);
    assert_eq!(
        rows(
            &path,
            "SELECT scope_key,id,created_at,updated_at,archived_at FROM g_gest_projects"
        )
        .await,
        vec![vec![
            text("combined"),
            text("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
            text("2026-11-18T12:00:00+00:00"),
            text("2026-11-19T08:30:00.123456789+00:00"),
            Value::Null,
        ]]
    );
    assert_eq!(
        rows(&path, &format!("SELECT comment FROM event e JOIN tag t ON t.tagid=e.tagid WHERE t.tagname='tkt-{PROJECT_K}'")).await,
        vec![vec![text(&format!(
            "New ticket [{}|{}] <i></i>.",
            &PROJECT_K[..16],
            &PROJECT_K[..10]
        ))]]
    );
    assert_eq!(
        rows(
            &path,
            "SELECT tagid,tagname FROM tag WHERE tagid>12 ORDER BY tagid"
        )
        .await,
        vec![
            vec![Value::Integer(13), text(&format!("tkt-{PROJECT_K}"))],
            vec![Value::Integer(14), text(&format!("tkt-{TASK_K}"))],
        ]
    );
}

#[tokio::test]
/// Check the GEST-T02 subset of one creation. The metadata is null, empty, a string,
/// a number, a boolean or an object, and the priority is null, 0 or 255. Each status
/// alias comes with its paired resolution time. Each case round-trips and keeps its
/// JSON value.
async fn metadata_priority_and_status_values_round_trip() {
    let cases = [
        (
            json!(null),
            json!(null),
            json!("open"),
            json!(null),
            "null",
            "open",
        ),
        (
            json!(""),
            json!(0),
            json!("in_progress"),
            json!(null),
            "\"\"",
            "in-progress",
        ),
        (
            json!("text"),
            json!(255),
            json!("InProgress"),
            json!(null),
            "\"text\"",
            "in-progress",
        ),
        (
            json!(7),
            json!(1),
            json!("done"),
            json!("2026-11-18T12:00:00Z"),
            "7",
            "done",
        ),
        (
            json!(true),
            json!(4),
            json!("cancelled"),
            json!("2026-11-18T13:00:00Z"),
            "true",
            "cancelled",
        ),
        (
            json!({}),
            json!(3),
            json!("in-progress"),
            json!(null),
            "{}",
            "in-progress",
        ),
    ];
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let mut owner = repository(&path).await;
    create(&mut owner, &project_artifact()).await.unwrap();
    let mut expected = Vec::new();
    for (index, (metadata, priority, status, resolved, stored, spelled)) in cases.iter().enumerate()
    {
        let k = format!("{index:040x}");
        let gest = ["l", "m", "n", "o", "p", "q"][index].repeat(32);
        let inputs = TaskValueInputs {
            gest_id: json!(gest),
            metadata: metadata.clone(),
            priority: priority.clone(),
            status: status.clone(),
            resolved_at: resolved.clone(),
            ..task_inputs()
        };
        let ticket = create(&mut owner, &task_artifact(inputs.clone(), &k))
            .await
            .unwrap();
        assert_eq!(
            ticket.record,
            IssueRecord::Task(task_from_values(inputs).unwrap())
        );
        assert_eq!(
            owner.read_private_issue(&k, limits()).await.unwrap(),
            ticket
        );
        let priority = priority.as_i64().map_or(Value::Null, Value::Integer);
        let resolved = resolved.as_str().map_or(Value::Null, |_| {
            text(&resolved.as_str().unwrap().replace('Z', "+00:00"))
        });
        expected.push(vec![
            text(&gest),
            text(stored),
            priority,
            text(spelled),
            resolved,
        ]);
    }
    drop(owner);
    assert_eq!(
        rows(
            &path,
            "SELECT id,metadata,priority,status,resolved_at FROM g_gest_tasks ORDER BY id"
        )
        .await,
        expected
    );
}

/// Add the Z card to an artifact body.
/// # Arguments
/// * `body` - Every card before Z.
/// # Returns
/// The complete artifact.
fn with_checksum(body: &str) -> Vec<u8> {
    let checksum = gossil::content_hash::hex(&gossil::artifact_checksum::md5(body.as_bytes()));
    format!("{body}Z {checksum}\n").into_bytes()
}

#[tokio::test]
/// Every ticket shape other than one canonical private creation is refused, and
/// each refusal leaves every table unchanged.
async fn other_ticket_shapes_are_refused_without_effect() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let mut owner = repository(&path).await;
    create(&mut owner, &project_artifact()).await.unwrap();
    let task = task_artifact(task_inputs(), TASK_K);
    create(&mut owner, &task).await.unwrap();
    let before = snapshot(&path).await;
    let other_id = TaskValueInputs {
        gest_id: json!("nnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn"),
        ..task_inputs()
    };
    let other_k = "ffffffffffffffffffffffffffffffffffffffff";
    let unsupported =
        "only one complete Gest task or project creation per ticket ID is supported in this build";
    let bracket = "an issue text field cannot contain '[' in this build";
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "a second artifact for one ticket ID",
            task_artifact(other_id.clone(), TASK_K),
            unsupported,
        ),
        ("the same artifact again", task.clone(), unsupported),
        (
            "the same Gest ID under another ticket ID",
            task_artifact(task_inputs(), other_k),
            unsupported,
        ),
        (
            "a left bracket in the title",
            task_artifact(
                TaskValueInputs {
                    title: json!("see [link]"),
                    ..other_id.clone()
                },
                other_k,
            ),
            bracket,
        ),
        (
            "a JSON array in the metadata",
            task_artifact(
                TaskValueInputs {
                    metadata: json!([1]),
                    ..other_id.clone()
                },
                other_k,
            ),
            bracket,
        ),
        (
            "a change that is not a creation",
            with_checksum(&format!(
                "D 2026-11-18T12:00:00.000000000\nJ status \"done\"\nK {other_k}\nU gest\\sagent\n"
            )),
            unsupported,
        ),
        (
            "a priority above 255",
            respell(
                &task_artifact(other_id.clone(), other_k),
                "J priority 2\n",
                "J priority 256\n",
            ),
            unsupported,
        ),
        (
            "an unknown field",
            respell(
                &task_artifact(other_id.clone(), other_k),
                "K ",
                "J zzz 1\nK ",
            ),
            unsupported,
        ),
        (
            "a D card without nine fraction digits",
            respell(
                &task_artifact(other_id.clone(), other_k),
                "D 2026-11-18T12:00:00.000000000\n",
                "D 2026-11-18T12:00:00\n",
            ),
            unsupported,
        ),
        (
            "an unknown kind",
            respell(
                &task_artifact(other_id.clone(), other_k),
                "J gossil_kind task\n",
                "J gossil_kind note\n",
            ),
            unsupported,
        ),
        (
            "a project with a task field",
            respell(
                &project_artifact(),
                "J updated_at",
                "J title \"x\"\nJ updated_at",
            ),
            unsupported,
        ),
        (
            "a cluster",
            with_checksum(&format!("M {}\n", "a".repeat(64))),
            unsupported,
        ),
        (
            "malformed bytes",
            b"not an artifact\n".to_vec(),
            "recording native artifact validation",
        ),
    ];
    for (case, bytes, message) in cases {
        let error = create(&mut owner, &bytes).await.unwrap_err();
        assert!(error.to_string().contains(message), "{case}: {error}");
        assert_eq!(snapshot(&path).await, before, "{case}");
    }
    let error = owner
        .create_private_issue(
            PrivateIssueRequest {
                artifact: &task_artifact(other_id, other_k),
                scope: PathScope::Shared,
            },
            limits(),
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "issue tickets are private only in this build"
    );
    let error = owner
        .read_private_issue(other_k, limits())
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "issue ticket not found");
    assert_eq!(snapshot(&path).await, before);
    assert_eq!(
        owner
            .read_private_issue(TASK_K, limits())
            .await
            .unwrap()
            .ticket_id,
        TASK_K
    );
}

#[tokio::test]
/// A repository whose marker another connection changed to catalog 4 is refused
/// before any effect. The write of an owner opened before the change checks the
/// marker in its own transaction, and a new open refuses the older catalog.
async fn a_changed_or_older_repository_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let mut owner = repository(&path).await;
    let database = libsql::Builder::new_local(&path).build().await.unwrap();
    database
        .connect()
        .unwrap()
        .execute_batch("PRAGMA ignore_check_constraints=ON; UPDATE g_storage SET storage_version=4")
        .await
        .unwrap();
    let before = snapshot(&path).await;
    let error = create(&mut owner, &task_artifact(task_inputs(), TASK_K))
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "storage marker differs from exact repository catalog"
    );
    assert_eq!(snapshot(&path).await, before);
    drop(owner);
    let error = LocalOwnerRepository::open(&path, limits().admission)
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("catalog 4"), "{error}");
    assert_eq!(snapshot(&path).await, before);
}

/// Run the pinned Fossil reference through jlog and require success.
/// # Arguments
/// * `fossil` - The reference executable.
/// * `args` - Its arguments.
/// # Returns
/// Standard output.
fn fossil_run(fossil: &std::ffi::OsStr, args: &[&std::ffi::OsStr]) -> String {
    let output = std::process::Command::new("jlog")
        .arg("run")
        .arg(fossil)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fossil {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Read the native ticket state of a repository by ticket ID and tag name, without RIDs of tickets.
///
/// Event times are compared in whole milliseconds. Fossil writes `event.mtime` with its
/// own `%.17g` conversion, which adds noise below the tenth decimal of the Julian day,
/// such as 2461363.0000000009 for 2461363, and Gossil writes the exact Julian day, as
/// it does for every other event.
/// # Arguments
/// * `path` - The repository file.
/// # Returns
/// The rows of every native table that a ticket creation writes.
async fn native_ticket_state(path: &Path) -> Vec<Vec<Vec<Value>>> {
    let mut ticket = rows(path, "SELECT * FROM ticket ORDER BY tkt_uuid").await;
    ticket.iter_mut().for_each(|row| drop(row.remove(0)));
    let mut change = rows(
        path,
        "SELECT t.tkt_uuid,c.* FROM ticketchng c JOIN ticket t USING(tkt_id) ORDER BY 1",
    )
    .await;
    change.iter_mut().for_each(|row| drop(row.remove(1)));
    let mut state = vec![ticket, change];
    for sql in [
        "SELECT g.tagname,e.type,CAST(round(e.mtime*86400000) AS INTEGER),b.uuid,e.user,e.comment,e.brief,CAST(round(e.omtime*86400000) AS INTEGER),e.uid,e.bgcolor,e.euser,e.ecomment FROM event e JOIN blob b ON b.rid=e.objid LEFT JOIN tag g ON g.tagid=e.tagid ORDER BY b.uuid",
        "SELECT g.tagname,x.tagtype,s.uuid,o.uuid,x.value,x.mtime,r.uuid FROM tagxref x JOIN tag g USING(tagid) JOIN blob s ON s.rid=x.srcid JOIN blob o ON o.rid=x.origid JOIN blob r ON r.rid=x.rid ORDER BY 1,3",
        "SELECT b.uuid FROM private p JOIN blob b USING(rid) ORDER BY 1",
        "SELECT b.uuid FROM unclustered u JOIN blob b USING(rid) ORDER BY 1",
        "SELECT b.uuid FROM unsent u JOIN blob b USING(rid) ORDER BY 1",
        "SELECT count(*) FROM backlink",
    ] {
        state.push(rows(path, sql).await);
    }
    state
}

#[test]
#[ignore = "requires explicit pinned native Fossil; run just issue-ticket-interop"]
/// Stock Fossil 2.29 checks a repository that holds a private task and project,
/// shows each ticket field, and writes the same native rows, event comments
/// included, when it rebuilds the tickets from their artifacts.
fn stock_fossil_reads_the_stored_issue_tickets() {
    use std::ffi::OsStr;
    let fossil =
        std::env::var_os("GOSSIL_FOSSIL_REFERENCE").expect("explicit reference executable");
    let fossil = fossil.as_os_str();
    assert!(fossil_run(fossil, &[OsStr::new("version")]).contains("2.29 [7e8ecf0ce4]"));
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("repo");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let task = task_artifact(task_inputs(), TASK_K);
    let project = project_artifact();
    let stored = runtime.block_on(async {
        let mut owner = repository(&path).await;
        create(&mut owner, &project).await.unwrap();
        create(&mut owner, &task).await.unwrap();
        drop(owner);
        native_ticket_state(&path).await
    });
    let r = OsStr::new("-R");
    let integrity = fossil_run(
        fossil,
        &[
            OsStr::new("test-integrity"),
            OsStr::new("--parse"),
            r,
            path.as_os_str(),
        ],
    );
    assert!(integrity.contains("0 errors"), "{integrity}");
    assert!(
        integrity
            .lines()
            .any(|line| line.trim() == "0 control artifacts failed to parse"),
        "{integrity}"
    );
    for (k, bytes) in [(TASK_K, &task), (PROJECT_K, &project)] {
        let shown = fossil_run(
            fossil,
            &[
                OsStr::new("ticket"),
                OsStr::new("show"),
                OsStr::new("0"),
                OsStr::new(&format!("tkt_uuid='{k}'")),
                OsStr::new("--quote"),
                OsStr::new("--user"),
                OsStr::new("Owner"),
                r,
                path.as_os_str(),
            ],
        );
        let mut lines = shown.lines();
        let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
        let row: Vec<&str> = lines.next().unwrap().split('\t').collect();
        let shown: std::collections::BTreeMap<&str, &str> = header.into_iter().zip(row).collect();
        for card in std::str::from_utf8(bytes)
            .unwrap()
            .lines()
            .filter_map(|line| line.strip_prefix("J "))
        {
            let (name, value) = card.split_once(' ').unwrap();
            assert_eq!(shown[name], value, "{k} {name}");
        }
    }
    for command in [
        &[OsStr::new("test-ticket-rebuild"), OsStr::new("all"), r][..],
        &[OsStr::new("rebuild")][..],
    ] {
        let copy = temp.path().join(format!("copy-{}", command.len()));
        std::fs::copy(&path, &copy).unwrap();
        let mut args = command.to_vec();
        args.push(copy.as_os_str());
        fossil_run(fossil, &args);
        let rebuilt = runtime.block_on(native_ticket_state(&copy));
        assert_eq!(rebuilt, stored, "{command:?}");
    }
}

/// Run one product command in an isolated home and return its output.
/// # Arguments
/// * `home` - The fixture home, which also holds the checkout root `root`.
/// * `args` - The arguments after the executable.
/// # Returns
/// The captured output.
fn gos(home: &Path, args: &[&std::ffi::OsStr]) -> std::process::Output {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_gos"));
    for (key, _) in std::env::vars_os() {
        if key.to_str().is_some_and(|key| key.starts_with("GEST_")) {
            command.env_remove(key);
        }
    }
    command
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home)
        .env("XDG_DATA_HOME", home)
        .current_dir(home.join("root"))
        .args(args)
        .output()
        .unwrap()
}

/// Run one `vcs` verb on the fixture repository and checkout, and require success.
/// # Arguments
/// * `home` - The fixture home.
/// * `verb` - The verb and its own flags.
/// # Returns
/// Standard output.
fn vcs(home: &Path, verb: &[&str]) -> String {
    let repo = home.join("repo");
    let mut args = vec![
        std::ffi::OsStr::new("v"),
        std::ffi::OsStr::new(verb[0]),
        std::ffi::OsStr::new("--repository"),
        repo.as_os_str(),
        std::ffi::OsStr::new("--checkout"),
        std::ffi::OsStr::new("work"),
    ];
    args.extend(verb[1..].iter().map(std::ffi::OsStr::new));
    let output = gos(home, &args);
    assert!(
        output.status.success(),
        "{verb:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// One step of a CAT-T07 order.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// Store the next task through the library.
    Issue,
    /// Register the checkout with `gos v open --root`.
    Open,
    /// Add, scope and commit the next file with the CLI.
    Commit,
}

/// Run one order of steps on a new repository made by `gos v init`.
///
/// After each step a new open admits the repository and reads every issue back. Once
/// the checkout is registered, `status` runs after each step, and `timeline` lists
/// every check-in.
/// # Arguments
/// * `home` - An empty fixture home.
/// * `steps` - The order.
/// # Returns
/// The stored tickets, the count of check-ins, and the name of each tag above the 12
/// built-in tags in the order of creation, with `BRANCH` for the private branch tag.
async fn run_order(home: &Path, steps: &[Step]) -> (Vec<IssueTicket>, usize, Vec<String>) {
    let repo = home.join("repo");
    let root = home.join("root");
    std::fs::create_dir(&root).unwrap();
    let init = gos(home, &["v".as_ref(), "init".as_ref(), repo.as_os_str()]);
    assert!(
        init.status.success(),
        "{}",
        String::from_utf8_lossy(&init.stderr)
    );
    let (mut issues, mut commits, mut registered) = (Vec::new(), 0, false);
    let mut tags = Vec::new();
    let mut project = false;
    for (index, step) in steps.iter().enumerate() {
        match step {
            Step::Issue => {
                let k = format!("{:040x}", index + 1);
                let gest_id = char::from(b'l' + index as u8).to_string().repeat(32);
                let inputs = TaskValueInputs {
                    gest_id: json!(gest_id),
                    ..task_inputs()
                };
                let mut owner = LocalOwnerRepository::open(&repo, limits().admission)
                    .await
                    .unwrap();
                if !project {
                    // The first issue step stores the project of the tasks too.
                    project = true;
                    issues.push(create(&mut owner, &project_artifact()).await.unwrap());
                    tags.push(format!("tkt-{PROJECT_K}"));
                }
                issues.push(
                    create(&mut owner, &task_artifact(inputs, &k))
                        .await
                        .unwrap(),
                );
                tags.push(format!("tkt-{k}"));
            }
            Step::Open => {
                vcs(home, &["open", "--root", root.to_str().unwrap()]);
                registered = true;
            }
            Step::Commit => {
                commits += 1;
                if commits == 1 {
                    tags.push("BRANCH".to_owned());
                }
                let file = format!("file{commits}");
                std::fs::write(root.join(&file), format!("content {commits}")).unwrap();
                vcs(home, &["add", "--path", &file]);
                vcs(home, &["scope", "--scope", "private", "--path", &file]);
                let date = format!("2099-01-01T00:00:{commits:02}");
                vcs(
                    home,
                    &[
                        "commit",
                        "--scope",
                        "private",
                        "--all-tracked",
                        "--user",
                        "CLI Owner",
                        "--comment",
                        &format!("commit {commits}"),
                        "--date-override",
                        &date,
                    ],
                );
            }
        }
        let mut owner = LocalOwnerRepository::open(&repo, limits().admission)
            .await
            .unwrap_or_else(|error| panic!("{steps:?} {index}: {error}"));
        for issue in &issues {
            let read = owner.read_private_issue(&issue.ticket_id, limits()).await;
            assert_eq!(&read.unwrap(), issue, "{steps:?} {index}");
        }
        let listed = owner.list_private_issues(limits()).await.unwrap();
        assert_eq!(listed, issues, "{steps:?} {index}");
        drop(owner);
        if registered {
            vcs(home, &["status"]);
            let timeline = vcs(home, &["timeline"]);
            assert_eq!(timeline.lines().count(), commits + 1, "{steps:?} {index}");
        }
    }
    (issues, commits, tags)
}

#[tokio::test]
/// CAT-T07 subset: a repository with check-ins and issues in several orders opens, and
/// `status`, `add`, `commit` and `timeline` work after issues exist. Issues can be
/// written before registration, before the first check-in, and after check-ins.
async fn check_ins_and_issues_share_one_repository_in_any_order() {
    use Step::{Commit, Issue, Open};
    for steps in [
        &[Issue, Open, Commit, Issue, Commit][..],
        &[Open, Issue, Issue, Commit, Issue][..],
        &[Open, Commit, Issue, Commit, Commit, Issue][..],
    ] {
        let temp = tempfile::tempdir().unwrap();
        let (issues, commits, names) = run_order(temp.path(), steps).await;
        // The first issue step stores the project of the tasks too.
        let expected = steps.iter().filter(|step| matches!(step, Issue)).count() + 1;
        assert_eq!(issues.len(), expected, "{steps:?}");
        assert!(commits > 0, "{steps:?}");
        // Tag IDs above the 12 built-in tags follow the order of creation.
        let repo = temp.path().join("repo");
        let server = rows(&repo, "SELECT value FROM config WHERE name='server-code'").await;
        let branch = format!("sym-gossil-private/{}", server[0][0].as_text().unwrap());
        let expected_tags: Vec<Vec<Value>> = names
            .iter()
            .zip(13..)
            .map(|(name, id)| vec![Value::Integer(id), text(&name.replace("BRANCH", &branch))])
            .collect();
        let tags = rows(
            &repo,
            "SELECT tagid,tagname FROM tag WHERE tagid>12 ORDER BY tagid",
        )
        .await;
        assert_eq!(tags, expected_tags, "{steps:?}");
    }
}

#[tokio::test]
/// With a registered checkout and check-ins between issues, the full proof still
/// refuses each changed issue, tag or receipt row, so it checks check-ins and issues
/// together. Open admits each change but one, since only the swapped receipt of the
/// selected check-in is a fact that open reads.
async fn admission_refuses_changed_issue_rows_beside_check_ins() {
    use Step::{Commit, Issue, Open};
    let (differs, projection, receipt) = (
        "stored issue rows differ from the ticket artifact",
        "recording native projection differs",
        "recording current receipt relation",
    );
    let cases = [
        (
            "swapped tag IDs",
            "UPDATE tag SET tagid=-1 WHERE tagid=14; UPDATE tag SET tagid=14 WHERE tagid=15; UPDATE tag SET tagid=15 WHERE tagid=-1; UPDATE tagxref SET tagid=CASE tagid WHEN 14 THEN 15 WHEN 15 THEN 14 ELSE tagid END; UPDATE event SET tagid=CASE tagid WHEN 14 THEN 15 WHEN 15 THEN 14 ELSE tagid END",
            projection,
        ),
        (
            "a change row that names the check-in",
            "UPDATE ticketchng SET tkt_rid=(SELECT max(objid) FROM event WHERE type='ci') WHERE tkt_id=2",
            differs,
        ),
        (
            "an issue under the receipt of a check-in",
            "DELETE FROM rcvfrom WHERE rcvid=(SELECT max(rcvid) FROM rcvfrom); UPDATE blob SET rcvid=(SELECT max(rcvid) FROM rcvfrom) WHERE rid=(SELECT max(rid) FROM blob)",
            "recording unexplained receipt",
        ),
        (
            "a missing typed row",
            "DELETE FROM g_gest_tasks WHERE id=(SELECT max(id) FROM g_gest_tasks)",
            differs,
        ),
        (
            "a ticket tag without an artifact",
            "INSERT INTO tag(tagname) VALUES('tkt-0000000000000000000000000000000000000abc')",
            "recording SQL count limit",
        ),
        (
            "swapped receipts of an issue and a check-in",
            "UPDATE blob SET rcvid=CASE rcvid WHEN 3 THEN 4 WHEN 4 THEN 3 ELSE rcvid END",
            receipt,
        ),
        (
            "an issue event of type ci",
            "UPDATE event SET type='ci' WHERE type='t'",
            projection,
        ),
        (
            "a leaf row for an issue",
            "INSERT INTO leaf(rid) SELECT max(rid) FROM blob",
            "recording SQL count limit",
        ),
        (
            "a tag row without a RID",
            "INSERT INTO tagxref(tagid,tagtype,srcid,origid,value,mtime,rid) VALUES(8,2,1,1,'other',0,NULL)",
            projection,
        ),
    ];
    for (case, sql, message) in cases {
        let temp = tempfile::tempdir().unwrap();
        run_order(temp.path(), &[Open, Issue, Commit, Issue]).await;
        let repo = temp.path().join("repo");
        let database = libsql::Builder::new_local(&repo).build().await.unwrap();
        database
            .connect()
            .unwrap()
            .execute_batch(sql)
            .await
            .unwrap();
        let (opened, error) = proof::verify_refusal(&repo, limits().admission).await;
        assert_eq!(error, message, "{case}");
        // Open checks that the fingerprint names the receipt of the selected check-in.
        assert_eq!(opened, message != receipt, "{case}");
    }
}

#[tokio::test]
/// The fingerprint of the checkout must name the receipt of the selected check-in once
/// one exists. Before that, registration takes the newest receipt, which may belong to
/// an issue. An older fingerprint, of the root or of an issue, is refused after a check-in.
async fn the_fingerprint_must_name_the_selected_receipt() {
    use Step::{Issue, Open};
    for (steps, receipt) in [(&[Open, Issue][..], "1/"), (&[Issue, Open][..], "3/")] {
        let temp = tempfile::tempdir().unwrap();
        run_order(temp.path(), steps).await;
        let repo = temp.path().join("repo");
        let sql = "SELECT value FROM g_vvar WHERE name='fingerprint'";
        let old = rows(&repo, sql).await[0][0].as_text().unwrap().clone();
        assert!(old.starts_with(receipt), "{steps:?}: {old}");
        std::fs::write(temp.path().join("root/file"), "content").unwrap();
        vcs(temp.path(), &["add", "--path", "file"]);
        vcs(
            temp.path(),
            &["scope", "--scope", "private", "--path", "file"],
        );
        vcs(
            temp.path(),
            &[
                "commit",
                "--scope",
                "private",
                "--all-tracked",
                "--user",
                "CLI Owner",
                "--comment",
                "c",
                "--date-override",
                "2099-01-01T00:00:01",
            ],
        );
        let database = libsql::Builder::new_local(&repo).build().await.unwrap();
        database
            .connect()
            .unwrap()
            .execute("UPDATE g_vvar SET value=?1 WHERE name='fingerprint'", [old])
            .await
            .unwrap();
        let error = LocalOwnerRepository::open(&repo, limits().admission)
            .await
            .err()
            .unwrap();
        assert_eq!(
            error.to_string(),
            "recording current receipt relation",
            "{steps:?}"
        );
    }
}

#[test]
#[ignore = "requires explicit pinned native Fossil; run just issue-ticket-interop"]
/// Stock Fossil 2.29 checks a repository that holds check-ins and an issue, written in
/// turn. `test-integrity` finds no error, `timeline` lists the check-ins and the new
/// ticket, and `ticket show` gives the fields of the issue. The timeline starts in
/// 2100, because the check-in dates of the fixture are in 2099.
fn stock_fossil_reads_check_ins_and_issues_together() {
    use std::ffi::OsStr;
    let fossil =
        std::env::var_os("GOSSIL_FOSSIL_REFERENCE").expect("explicit reference executable");
    let fossil = fossil.as_os_str();
    let temp = tempfile::tempdir().unwrap();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let (issues, _, _) = runtime.block_on(run_order(
        temp.path(),
        &[Step::Open, Step::Commit, Step::Issue, Step::Commit],
    ));
    let path = temp.path().join("repo");
    let r = OsStr::new("-R");
    let integrity = fossil_run(
        fossil,
        &[
            OsStr::new("test-integrity"),
            OsStr::new("--parse"),
            r,
            path.as_os_str(),
        ],
    );
    assert!(integrity.contains("0 errors"), "{integrity}");
    let timeline = fossil_run(
        fossil,
        &[
            OsStr::new("timeline"),
            OsStr::new("before"),
            OsStr::new("2100-01-01"),
            OsStr::new("-n"),
            OsStr::new("0"),
            r,
            path.as_os_str(),
        ],
    );
    // The first issue step stores the project, and then the task.
    let k = &issues[1].ticket_id;
    for expected in [
        "commit 1",
        "commit 2",
        &format!("New ticket [{}]", &k[..10]),
    ] {
        assert!(timeline.contains(expected), "{expected}: {timeline}");
    }
    let shown = fossil_run(
        fossil,
        &[
            OsStr::new("ticket"),
            OsStr::new("show"),
            OsStr::new("0"),
            OsStr::new(&format!("tkt_uuid='{k}'")),
            OsStr::new("--quote"),
            r,
            path.as_os_str(),
        ],
    );
    // The task of step 2 has the Gest ID of 32 letters `n`.
    assert!(shown.contains(&"n".repeat(32)), "{shown}");
    assert!(shown.contains(k.as_str()), "{shown}");
}
