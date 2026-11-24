//! Check artifact and project creation tickets: golden bytes, GEST-T01 round trips,
//! the inherited field mapping, byte limits and a stock Fossil 2.29 oracle.
use gossil::artifact::{ArtifactType, ParseLimits, parse_artifact};
use gossil::gest_artifact_values::{
    ArtifactValueInputs, ArtifactValues, artifact_from_values, artifact_to_values,
};
use gossil::gest_project_values::{
    ProjectValueInputs, ProjectValues, project_from_values, project_to_values,
};
use gossil::gest_ticket::{artifact_creation_bytes, project_creation_bytes};
use gossil::native_date::NativeDate;
use gossil::task_ticket::TicketError;
use serde_json::{Value, json};
use std::collections::BTreeMap;
#[path = "support/observed_allocator.rs"]
mod observed_allocator;
use observed_allocator::largest_allocation;

/// Ticket identity of the artifact fixture, independent of its Gest ID.
const ARTIFACT_K: &[u8] = b"fedcba9876543210fedcba9876543210fedcba98";
/// Ticket identity of the project fixture, independent of its Gest ID.
const PROJECT_K: &[u8] = b"0123456789abcdef0123456789abcdef01234567";
/// Names of the three fixed fields that every Gest creation ticket adds.
const FIXED: [&str; 3] = ["gossil_deleted", "gossil_kind", "gossil_schema"];

/// Build the artifact of `artifact-creation.artifact`, with offset input timestamps.
fn artifact_inputs() -> ArtifactValueInputs {
    ArtifactValueInputs {
        gest_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
        project_id: json!("k".repeat(32)),
        title: json!("Plan é"),
        body: json!("Step 1\n\tC:\\path"),
        metadata: json!({"a": [null, false], "b": "x y"}),
        archived_at: Value::Null,
        created_at: json!("2026-11-18T07:00:00-05:00"),
        updated_at: json!("2026-11-18T12:30:00Z"),
    }
}

/// Build the project of `project-creation.artifact`, with a non-null archive time.
fn project_inputs() -> ProjectValueInputs {
    ProjectValueInputs {
        gest_id: json!("klmnopqrstuvwxyzklmnopqrstuvwxyz"),
        archived_at: json!("2026-11-19T00:00:00Z"),
        created_at: json!("2026-11-18T12:00:00Z"),
        updated_at: json!("2026-11-18T12:30:00Z"),
    }
}

/// The D card instant of both fixtures.
fn fixture_date() -> NativeDate {
    NativeDate::parse(b"2026-11-20T01:02:03.000000004").unwrap()
}

/// Emit an artifact ticket with the fixture's K, actor and date.
fn emit_artifact(record: &ArtifactValues, limit: usize) -> Result<Vec<u8>, TicketError> {
    artifact_creation_bytes(record, ARTIFACT_K, b"gest agent", fixture_date(), limit)
}

/// Emit a project ticket with the fixture's K, actor and date.
fn emit_project(record: &ProjectValues, limit: usize) -> Result<Vec<u8>, TicketError> {
    project_creation_bytes(record, PROJECT_K, b"gest agent", fixture_date(), limit)
}

/// Match independently spelled artifact bytes and reject an undersized limit or a malformed K.
#[test]
fn artifact_golden_and_exact_bound() {
    // Spelled by tests/fixtures/gest-ticket/generate.py; checksum from Python hashlib.
    let expected = include_bytes!("fixtures/gest-ticket/artifact-creation.artifact");
    let record = artifact_from_values(artifact_inputs()).unwrap();
    assert_eq!(emit_artifact(&record, expected.len()).unwrap(), expected);
    assert!(matches!(
        emit_artifact(&record, expected.len() - 1),
        Err(TicketError::Size)
    ));
    let gest_id_as_k = [b'z'; 40];
    for k in [
        b"".as_slice(),
        b"FEDCBA9876543210fedcba9876543210fedcba98",
        &gest_id_as_k,
    ] {
        assert!(matches!(
            artifact_creation_bytes(&record, k, b"a", fixture_date(), 10_000),
            Err(TicketError::TicketId)
        ));
    }
}

/// Match independently spelled project bytes and reject an undersized limit or a malformed K.
#[test]
fn project_golden_and_exact_bound() {
    // Spelled by tests/fixtures/gest-ticket/generate.py; checksum from Python hashlib.
    let expected = include_bytes!("fixtures/gest-ticket/project-creation.artifact");
    let record = project_from_values(project_inputs()).unwrap();
    assert_eq!(emit_project(&record, expected.len()).unwrap(), expected);
    assert!(matches!(
        emit_project(&record, expected.len() - 1),
        Err(TicketError::Size)
    ));
    assert!(matches!(
        project_creation_bytes(&record, &PROJECT_K[1..], b"a", fixture_date(), 10_000),
        Err(TicketError::TicketId)
    ));
}

/// Supply a fixed parser date result without consulting SQL or a clock.
struct Dates;
impl gossil::artifact::DateProvider for Dates {
    /// Return a fixed valid Julian day for structural parser controls.
    ///
    /// # Errors
    /// This test adapter always succeeds.
    async fn julianday(
        &self,
        _: Option<&[u8]>,
    ) -> Result<Option<f64>, gossil::artifact::DateProviderError> {
        Ok(Some(2460000.5))
    }
}

/// Parse a creation ticket and return its decoded J fields by name, after the card checks.
///
/// # Arguments
/// * `bytes` - Complete emitted artifact.
/// * `kind` - Expected raw `gossil_kind` value.
/// * `ticket` - Expected K card bytes.
///
/// # Returns
/// Each model field name with its decoded value bytes. The fixed fields are checked and removed.
async fn model_fields(bytes: &[u8], kind: &[u8], ticket: &[u8]) -> BTreeMap<String, Vec<u8>> {
    let limits = ParseLimits {
        input_bytes: 10_000,
        owned_bytes: 40_000,
        cards: 30,
        entries: 30,
    };
    let parsed = parse_artifact(bytes, limits, &Dates).await.unwrap();
    assert_eq!(parsed.artifact_type(), ArtifactType::Ticket);
    assert_eq!(parsed.ticket_id().unwrap(), ticket);
    let fields = parsed.ticket_fields();
    assert!(fields.windows(2).all(|w| w[0].name() < w[1].name()));
    let mut decoded: BTreeMap<String, Vec<u8>> = fields
        .iter()
        .map(|f| {
            (
                String::from_utf8(f.name().to_vec()).unwrap(),
                f.value().bytes.clone(),
            )
        })
        .collect();
    assert_eq!(decoded.len(), fields.len());
    assert_eq!(decoded.remove("gossil_deleted").unwrap(), b"false");
    assert_eq!(decoded.remove("gossil_kind").unwrap(), kind);
    assert_eq!(decoded.remove("gossil_schema").unwrap(), b"1");
    decoded
}

/// Return the JSON value of one decoded field.
fn json_field(fields: &BTreeMap<String, Vec<u8>>, name: &str) -> Value {
    serde_json::from_slice(&fields[name]).unwrap()
}

/// Check GEST-T01 for artifacts. Every field survives the artifact, with K and the Gest ID kept apart.
#[tokio::test]
async fn artifact_fields_round_trip_through_the_ticket() {
    let variants = [
        artifact_inputs(),
        ArtifactValueInputs {
            title: json!(""),
            body: json!(""),
            metadata: Value::Null,
            archived_at: json!("2026-11-21T00:00:00.123456789+02:00"),
            ..artifact_inputs()
        },
        ArtifactValueInputs {
            title: json!("雪\u{0}"),
            body: json!("x".repeat(3000)),
            metadata: json!([1, "two", {"three": 3.5}]),
            ..artifact_inputs()
        },
        ArtifactValueInputs {
            metadata: json!("text"),
            ..artifact_inputs()
        },
    ];
    for inputs in variants {
        let record = artifact_from_values(inputs).unwrap();
        let bytes = emit_artifact(&record, usize::MAX).unwrap();
        let fields = model_fields(&bytes, b"artifact", ARTIFACT_K).await;
        assert_eq!(fields.len(), 8);
        let decoded = ArtifactValueInputs {
            gest_id: json_field(&fields, "gest_id"),
            project_id: json_field(&fields, "project_id"),
            title: json_field(&fields, "title"),
            body: json_field(&fields, "body"),
            metadata: json_field(&fields, "metadata"),
            archived_at: json_field(&fields, "archived_at"),
            created_at: json_field(&fields, "created_at"),
            updated_at: json_field(&fields, "updated_at"),
        };
        assert_eq!(decoded, artifact_to_values(&record));
        assert_eq!(artifact_from_values(decoded).unwrap(), record);
    }
}

/// Check GEST-T01 for projects. Every field survives the artifact, with a null or a set archive time.
#[tokio::test]
async fn project_fields_round_trip_through_the_ticket() {
    for archived_at in [Value::Null, json!("2026-11-19T00:00:00.5-03:00")] {
        let record = project_from_values(ProjectValueInputs {
            archived_at,
            ..project_inputs()
        })
        .unwrap();
        let bytes = emit_project(&record, usize::MAX).unwrap();
        let fields = model_fields(&bytes, b"project", PROJECT_K).await;
        assert_eq!(fields.len(), 4);
        let decoded = ProjectValueInputs {
            gest_id: json_field(&fields, "gest_id"),
            archived_at: json_field(&fields, "archived_at"),
            created_at: json_field(&fields, "created_at"),
            updated_at: json_field(&fields, "updated_at"),
        };
        assert_eq!(decoded, project_to_values(&record));
        assert_eq!(project_from_values(decoded).unwrap(), record);
    }
}

/// Read the canonical field names of one converter source map.
fn mapped_names(map: &str, key: &str) -> Vec<String> {
    let map: Value = serde_json::from_str(map).unwrap();
    let mut names: Vec<String> = map[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["canonical_field"].as_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

/// Check that each J name comes from the inherited Gest mapping, and each value is its JSON bytes.
///
/// The names come from the committed converter source maps of the pinned Gest
/// tables, so a field that the ticket adds, drops or renames fails here. Each
/// decoded value must equal the compact JSON of the converter output, byte for byte.
#[tokio::test]
async fn field_names_and_bytes_follow_the_inherited_mapping() {
    let artifact = artifact_from_values(artifact_inputs()).unwrap();
    let fields = model_fields(
        &emit_artifact(&artifact, usize::MAX).unwrap(),
        b"artifact",
        ARTIFACT_K,
    )
    .await;
    let names = mapped_names(
        include_str!("fixtures/gest_artifact_values/source-map.json"),
        "fields",
    );
    assert_eq!(fields.keys().cloned().collect::<Vec<_>>(), names);
    let v = artifact_to_values(&artifact);
    for (name, value) in [
        ("archived_at", v.archived_at),
        ("body", v.body),
        ("created_at", v.created_at),
        ("gest_id", v.gest_id),
        ("metadata", v.metadata),
        ("project_id", v.project_id),
        ("title", v.title),
        ("updated_at", v.updated_at),
    ] {
        assert_eq!(fields[name], serde_json::to_vec(&value).unwrap(), "{name}");
    }

    let project = project_from_values(project_inputs()).unwrap();
    let fields = model_fields(
        &emit_project(&project, usize::MAX).unwrap(),
        b"project",
        PROJECT_K,
    )
    .await;
    let names = mapped_names(
        include_str!("fixtures/gest_project_values/source-map.json"),
        "canonical_fields",
    );
    // The local checkout root is not in the canonical mapping, so it never reaches the ticket.
    assert!(!names.iter().any(|n| n == "root"));
    assert_eq!(fields.keys().cloned().collect::<Vec<_>>(), names);
    let v = project_to_values(&project);
    for (name, value) in [
        ("archived_at", v.archived_at),
        ("created_at", v.created_at),
        ("gest_id", v.gest_id),
        ("updated_at", v.updated_at),
    ] {
        assert_eq!(fields[name], serde_json::to_vec(&value).unwrap(), "{name}");
    }
}

/// Reject oversized artifact text or metadata before making model-sized allocation requests.
#[test]
fn insufficient_budget_does_not_copy_unbounded_artifact_data() {
    // Use 256 KiB strings to test each field's independent size boundary.
    for field in ["title", "body", "metadata"] {
        let large = Value::String("x".repeat(256 * 1024));
        let mut inputs = artifact_inputs();
        match field {
            "title" => inputs.title = large,
            "body" => inputs.body = large,
            _ => inputs.metadata = json!({"nested": [large]}),
        }
        let record = artifact_from_values(inputs).unwrap();
        for budget in [0, 1024, 128 * 1024] {
            let (result, largest) = largest_allocation(|| emit_artifact(&record, budget));
            assert!(matches!(result, Err(TicketError::Size)));
            assert!(
                largest < 4096,
                "{field}, budget {budget}: allocated {largest} bytes before rejection"
            );
        }
    }
}

/// Run the pinned Fossil reference through jlog and return its standard output.
///
/// # Arguments
/// * `fossil` - Explicit reference executable.
/// * `args` - Fossil command and arguments.
///
/// # Returns
/// Standard output bytes of a successful run. A failed run fails the test.
fn fossil_run(fossil: &std::ffi::OsStr, args: &[&std::ffi::OsStr]) -> Vec<u8> {
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
    output.stdout
}

/// Return the encoded value of each J card of an emitted artifact, by field name.
fn j_cards(bytes: &[u8]) -> BTreeMap<String, String> {
    std::str::from_utf8(bytes)
        .unwrap()
        .lines()
        .filter_map(|line| line.strip_prefix("J "))
        .map(|card| {
            let (name, value) = card.split_once(' ').unwrap();
            (name.to_owned(), value.to_owned())
        })
        .collect()
}

/// Load emitted artifact, project and task tickets into a stock Fossil 2.29 repository.
///
/// The repository's ticket table has one column per Gest field. After a full
/// rebuild, `test-integrity --parse` must find no error, `ticket show` must
/// return each J value byte for byte, `ticket history` must list each field
/// change, and `artifact` must return the emitted bytes unchanged.
#[test]
#[ignore = "requires explicit pinned native Fossil; run just gest-ticket-interop"]
fn stock_fossil_reads_emitted_gest_tickets() {
    use std::ffi::OsStr;
    let fossil =
        std::env::var_os("GOSSIL_FOSSIL_REFERENCE").expect("explicit reference executable");
    let fossil = fossil.as_os_str();
    let version = fossil_run(fossil, &[OsStr::new("version")]);
    assert!(
        String::from_utf8_lossy(&version).contains("2.29 [7e8ecf0ce4]"),
        "unpinned Fossil reference"
    );
    let task =
        gossil::gest_task_values::task_from_values(gossil::gest_task_values::TaskValueInputs {
            gest_id: json!("m".repeat(32)),
            project_id: json!("klmnopqrstuvwxyzklmnopqrstuvwxyz"),
            title: json!("Task é"),
            priority: json!(2),
            status: json!("open"),
            description: json!("line\nnext"),
            assigned_to: Value::Null,
            metadata: json!({}),
            resolved_at: Value::Null,
            created_at: json!("2026-11-18T12:00:00Z"),
            updated_at: json!("2026-11-18T12:00:00Z"),
        })
        .unwrap();
    let task_k: &[u8] = b"1111111111111111111111111111111111111111";
    let tickets = [
        (
            ARTIFACT_K,
            emit_artifact(&artifact_from_values(artifact_inputs()).unwrap(), 65_536).unwrap(),
        ),
        (
            PROJECT_K,
            emit_project(&project_from_values(project_inputs()).unwrap(), 65_536).unwrap(),
        ),
        (
            task_k,
            gossil::task_ticket::task_creation_bytes(
                &task,
                task_k,
                b"gest agent",
                fixture_date(),
                65_536,
            )
            .unwrap(),
        ),
    ];
    let mut columns: Vec<String> = tickets
        .iter()
        .flat_map(|(_, b)| j_cards(b).into_keys())
        .collect();
    columns.sort();
    columns.dedup();
    let typed: Vec<String> = columns.iter().map(|c| format!("{c} TEXT")).collect();
    let typed = typed.join(",");
    let schema = format!(
        "CREATE TABLE ticket(tkt_id INTEGER PRIMARY KEY,tkt_uuid TEXT UNIQUE,tkt_mtime DATE,\
         tkt_ctime DATE,{typed}); CREATE TABLE ticketchng(tkt_id INTEGER,tkt_rid INTEGER,\
         tkt_mtime DATE,tkt_user TEXT,{typed});"
    );
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("oracle.fossil");
    let repo = repo.as_os_str();
    let r = OsStr::new("-R");
    let user = [OsStr::new("--user"), OsStr::new("audit")];
    fossil_run(
        fossil,
        &[
            OsStr::new("init"),
            OsStr::new("--admin-user"),
            OsStr::new("audit"),
            repo,
        ],
    );
    let sql = format!("REPLACE INTO config(name,value,mtime) VALUES('ticket-table','{schema}',0);");
    fossil_run(fossil, &[OsStr::new("sql"), r, repo, OsStr::new(&sql)]);
    fossil_run(
        fossil,
        &[
            OsStr::new("test-ticket-rebuild"),
            OsStr::new("all"),
            r,
            repo,
        ],
    );
    for (index, (_, bytes)) in tickets.iter().enumerate() {
        let path = temp.path().join(format!("{index}.artifact"));
        std::fs::write(&path, bytes).unwrap();
        fossil_run(
            fossil,
            &[
                OsStr::new("test-content-put"),
                user[0],
                user[1],
                OsStr::new("--crosslink"),
                r,
                repo,
                path.as_os_str(),
            ],
        );
    }
    fossil_run(fossil, &[OsStr::new("rebuild"), repo]);
    let integrity = String::from_utf8(fossil_run(
        fossil,
        &[OsStr::new("test-integrity"), OsStr::new("--parse"), r, repo],
    ))
    .unwrap();
    assert!(
        integrity
            .lines()
            .any(|line| line.trim() == "0 control artifacts failed to parse"),
        "{integrity}"
    );
    assert!(integrity.contains("  3 tickets\n"), "{integrity}");
    for (k, bytes) in &tickets {
        let k = std::str::from_utf8(k).unwrap();
        let cards = j_cards(bytes);
        let filter = format!("tkt_uuid='{k}'");
        let show = String::from_utf8(fossil_run(
            fossil,
            &[
                OsStr::new("ticket"),
                OsStr::new("show"),
                OsStr::new("0"),
                OsStr::new(&filter),
                OsStr::new("--quote"),
                user[0],
                user[1],
                r,
                repo,
            ],
        ))
        .unwrap();
        let mut lines = show.lines();
        let header: Vec<&str> = lines.next().unwrap().split('\t').collect();
        let row: Vec<&str> = lines.next().unwrap().split('\t').collect();
        assert!(lines.next().is_none(), "{show}");
        let shown: BTreeMap<&str, &str> = header.into_iter().zip(row).collect();
        assert_eq!(shown["tkt_uuid"], k);
        for column in &columns {
            let expected = cards.get(column).map_or("", String::as_str);
            assert_eq!(shown[column.as_str()], expected, "{k} {column}");
        }
        let history = String::from_utf8(fossil_run(
            fossil,
            &[
                OsStr::new("ticket"),
                OsStr::new("history"),
                OsStr::new(k),
                user[0],
                user[1],
                r,
                repo,
            ],
        ))
        .unwrap();
        assert!(
            history.starts_with("Ticket Change by gest agent on 2026-11-20 01:02:03:\n"),
            "{history}"
        );
        for (name, value) in &cards {
            let decoded = gossil::token_codec::defossilize(value.as_bytes())
                .unwrap()
                .bytes;
            let line = format!("  Change {name}: {}\n", String::from_utf8(decoded).unwrap());
            assert!(history.contains(&line), "{k}: {line:?} not in {history}");
        }
        assert_eq!(cards.len(), history.matches("  Change ").count());
        let hash = gossil::content_hash::hex(&gossil::content_hash::sha3(bytes, 256).unwrap());
        let stored = fossil_run(
            fossil,
            &[OsStr::new("artifact"), OsStr::new(&hash), r, repo],
        );
        assert_eq!(&stored, bytes);
    }
    assert!(FIXED.iter().all(|f| columns.iter().any(|c| c == f)));
}
