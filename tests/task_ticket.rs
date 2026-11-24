//! Check native task creation bytes, date boundaries and bounded allocation planning.
use gossil::gest_task_values::{TaskValueInputs, task_from_values};
use gossil::native_date::NativeDate;
use gossil::task_ticket::{TicketError, task_creation_bytes};
use serde_json::{Value, json};
#[path = "support/observed_allocator.rs"]
mod observed_allocator;
use observed_allocator::largest_allocation;
/// Valid independent native ticket identity shared by emission controls.
const K: &[u8] = b"0123456789abcdef0123456789abcdef01234567";
/// Build all eleven decoded task fields with nullable optional values.
fn values() -> TaskValueInputs {
    TaskValueInputs {
        gest_id: json!("k".repeat(32)),
        project_id: json!("z".repeat(32)),
        title: json!("hello"),
        priority: Value::Null,
        status: json!("inprogress"),
        description: json!(""),
        assigned_to: Value::Null,
        metadata: Value::Null,
        resolved_at: Value::Null,
        created_at: json!("2024-01-01T00:00:00Z"),
        updated_at: json!("2024-01-01T00:00:00Z"),
    }
}
/// Match independently spelled artifact bytes and reject undersized output or malformed K.
#[test]
fn golden_and_exact_bound() {
    // Independent D, J, K, U and Z card bytes; checksum generated with Python hashlib.
    // Order/token/checksum basis: Fossil tkt.c:1837-1874, pin b3a1c8ef.
    let expected = include_bytes!("fixtures/task-ticket/creation.artifact");
    let task = task_from_values(values()).unwrap();
    let d = NativeDate::parse(b"2024-02-29T12:34:56.000000001Z").unwrap();
    assert_eq!(
        task_creation_bytes(&task, K, b"native actor", d, expected.len()).unwrap(),
        expected
    );
    assert!(matches!(
        task_creation_bytes(&task, K, b"native actor", d, expected.len() - 1),
        Err(TicketError::Size)
    ));
    for k in [
        b"a".as_slice(),
        b"0123456789ABCDEF0123456789abcdef01234567",
        b"kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk",
    ] {
        assert!(matches!(
            task_creation_bytes(&task, k, b"actor", d, 10000),
            Err(TicketError::TicketId)
        ));
    }
}
/// Keep the task bytes of the shared emitter equal to the bytes of the task emitter before it.
///
/// The test is a characterization control. It passes on the parent commit, and
/// the shared emitter must keep it passing. Every field has a non-null value here,
/// so the control covers priority, assignee, resolution time, metadata and
/// escaping that the null-heavy fixture above does not reach.
#[test]
fn shared_emitter_keeps_full_task_bytes() {
    // Spelled by tests/fixtures/gest-ticket/generate.py, which also writes creation.artifact.
    let expected = include_bytes!("fixtures/gest-ticket/task-creation-full.artifact");
    let task = task_from_values(TaskValueInputs {
        gest_id: json!("k".repeat(32)),
        project_id: json!("z".repeat(32)),
        title: json!("Fix é bug"),
        priority: json!(3),
        status: json!("done"),
        description: json!("a\\b"),
        assigned_to: json!("l".repeat(32)),
        metadata: json!({"labels": ["x"], "n": 1}),
        resolved_at: json!("2024-02-01T00:00:00Z"),
        created_at: json!("2024-01-01T00:00:00Z"),
        updated_at: json!("2024-02-01T00:00:00Z"),
    })
    .unwrap();
    let d = NativeDate::parse(b"2024-03-01T00:00:00").unwrap();
    assert_eq!(
        task_creation_bytes(&task, K, b"native actor", d, expected.len()).unwrap(),
        expected
    );
}
/// Preserve nanosecond instants across the supported range and reject alternate grammar.
#[test]
fn native_date_contract() {
    for (input, canonical) in [
        ("0001-01-01T00:00:00", "0001-01-01T00:00:00.000000000"),
        (
            "9999-12-31T23:59:59.999999999Z",
            "9999-12-31T23:59:59.999999999",
        ),
        ("1969-12-31T23:59:59.1", "1969-12-31T23:59:59.100000000"),
        (
            "2000-02-29T00:00:00.000000001",
            "2000-02-29T00:00:00.000000001",
        ),
    ] {
        let d = NativeDate::parse(input.as_bytes()).unwrap();
        assert_eq!(d.to_string(), canonical);
        assert_eq!(NativeDate::parse(canonical.as_bytes()).unwrap(), d);
        assert_eq!(NativeDate::from_epoch_nanos(d.epoch_nanos()).unwrap(), d);
    }
    assert_eq!(
        NativeDate::parse(b"1970-01-01T00:00:00")
            .unwrap()
            .epoch_nanos(),
        0
    );
    assert_eq!(
        NativeDate::parse(b"1969-12-31T23:59:59.999999999")
            .unwrap()
            .epoch_nanos(),
        -1
    );
    for s in [
        "0000-01-01T00:00:00",
        "202x-01-01T00:00:00",
        "1900-02-29T00:00:00",
        "2024-04-31T00:00:00",
        "2024-01-01T24:00:00",
        "2024-01-01T00:60:00",
        "2024-01-01T00:00:60",
        "2024-01-01T00:00:00+00:00",
        "2024-01-01T00:00:00.",
        "2024-01-01T00:00:00.1234567890",
        " 2024-01-01T00:00:00",
        "2024-1-01T00:00:00",
        "2024-01-01t00:00:00",
        "2024-01-01T00:00:00z",
    ] {
        assert!(NativeDate::parse(s.as_bytes()).is_err(), "{s}");
    }
    assert!(NativeDate::from_epoch_nanos(-62135596800000000001).is_err());
    assert!(NativeDate::from_epoch_nanos(253402300800000000000).is_err());
    assert!(NativeDate::from_epoch_nanos(i128::MAX).is_err());
    assert!(NativeDate::from_epoch_nanos(i128::MIN).is_err());
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
/// Recover every model value and arbitrary actor bytes through the structural parser.
#[tokio::test]
async fn decoded_model_values_and_native_actor() {
    use gossil::artifact::{ArtifactType, ParseLimits, parse_artifact};
    for metadata in [
        json!(null),
        json!(false),
        json!(7),
        json!("text"),
        json!([1, "x"]),
        json!({"雪": [null, true]}),
    ] {
        let mut v = values();
        v.metadata = metadata;
        v.title = json!("雪\\\n\t\u{0}");
        v.priority = json!(255);
        v.assigned_to = json!("l".repeat(32));
        v.status = json!("done");
        v.resolved_at = json!("2024-01-01T00:00:00Z");
        let task = task_from_values(v).unwrap();
        let bytes = task_creation_bytes(
            &task,
            K,
            b"a\0\n\\\xff",
            NativeDate::parse(b"2024-01-01T00:00:00").unwrap(),
            10000,
        )
        .unwrap();
        let parsed = parse_artifact(
            &bytes,
            ParseLimits {
                input_bytes: 10000,
                owned_bytes: 40000,
                cards: 30,
                entries: 30,
            },
            &Dates,
        )
        .await
        .unwrap();
        assert_eq!(parsed.artifact_type(), ArtifactType::Ticket);
        assert_eq!(parsed.user().unwrap().bytes.as_slice(), b"a\0\n\\\xff");
        let fields = parsed.ticket_fields();
        assert_eq!(fields.len(), 14);
        assert!(fields.windows(2).all(|w| w[0].name() < w[1].name()));
        let decoded: std::collections::BTreeMap<_, _> = fields
            .iter()
            .map(|f| {
                (
                    std::str::from_utf8(f.name()).unwrap(),
                    f.value().bytes.as_slice(),
                )
            })
            .collect();
        assert_eq!(decoded["gossil_kind"], b"task");
        assert_eq!(decoded["gossil_schema"], b"1");
        assert_eq!(decoded["gossil_deleted"], b"false");
        let v = gossil::gest_task_values::task_to_values(&task);
        for (name, value) in [
            ("assigned_to", v.assigned_to),
            ("created_at", v.created_at),
            ("description", v.description),
            ("gest_id", v.gest_id),
            ("metadata", v.metadata),
            ("priority", v.priority),
            ("project_id", v.project_id),
            ("resolved_at", v.resolved_at),
            ("status", v.status),
            ("title", v.title),
            ("updated_at", v.updated_at),
        ] {
            assert_eq!(
                serde_json::from_slice::<Value>(decoded[name]).unwrap(),
                value,
                "{name}"
            );
        }
    }
}

/// Keep empty JSON strings, null model fields and the empty native actor distinct.
#[test]
fn empty_actor_and_model_strings_remain_distinct() {
    let mut v = values();
    v.title = json!("");
    v.metadata = json!("");
    v.priority = json!(0);
    let bytes = task_creation_bytes(
        &task_from_values(v).unwrap(),
        K,
        b"",
        NativeDate::parse(b"1970-01-01T00:00:00").unwrap(),
        usize::MAX,
    )
    .unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("J title \"\"\n"));
    assert!(text.contains("J metadata \"\"\n"));
    assert!(text.contains("J priority 0\n"));
    assert!(text.contains("J assigned_to null\n"));
    assert!(text.contains("\nU \nZ "));
}

/// Reject oversized strings or metadata before making model-sized allocation requests.
#[test]
fn insufficient_budget_does_not_copy_unbounded_model_data() {
    let date = NativeDate::parse(b"2024-01-01T00:00:00").unwrap();
    // Use 256 KiB strings to test each field's independent size boundary.
    for field in ["title", "description", "metadata"] {
        let mut input = values();
        let large = Value::String("x".repeat(256 * 1024));
        match field {
            "title" => input.title = large,
            "description" => input.description = large,
            _ => input.metadata = json!({"nested": [large]}),
        }
        let task = task_from_values(input).unwrap();
        for budget in [0, 1024, 128 * 1024] {
            let (result, largest) =
                largest_allocation(|| task_creation_bytes(&task, K, b"actor", date, budget));
            assert!(matches!(result, Err(TicketError::Size)));
            assert!(
                largest < 4096,
                "{field}, budget {budget}: allocated {largest} bytes before rejection"
            );
        }
    }
}

/// Give rejected native dates a stable diagnostic without echoing the input.
#[test]
fn native_date_error_description() {
    use std::error::Error;
    let error = NativeDate::parse(b"invalid sensitive date").unwrap_err();
    assert_eq!(error.to_string(), "unsupported typed native date");
    assert!(error.source().is_none());
}
