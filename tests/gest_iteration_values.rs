//! New decoded-field controls; no exact source primitive tests are ported here.
#[path = "support/gest_source_map.rs"]
mod gest_source_map;
use gossil::gest_iteration_values::{
    IterationValueInputs, iteration_from_values, iteration_to_values,
};
use serde_json::{Value, json};

/// Build a complete fixture; raw field admission remains caller-owned.
fn inputs(v: &Value) -> IterationValueInputs {
    assert_eq!(v.as_object().unwrap().len(), 9);
    IterationValueInputs {
        gest_id: v["gest_id"].clone(),
        project_id: v["project_id"].clone(),
        title: v["title"].clone(),
        status: v["status"].clone(),
        description: v["description"].clone(),
        metadata: v["metadata"].clone(),
        completed_at: v["completed_at"].clone(),
        created_at: v["created_at"].clone(),
        updated_at: v["updated_at"].clone(),
    }
}

/// Load the explicit base record.
fn base() -> IterationValueInputs {
    inputs(
        &serde_json::from_str::<Value>(include_str!("fixtures/gest_iteration_values/records.json"))
            .unwrap()["base"],
    )
}

/// Observe a successful complete conversion.
fn output(f: IterationValueInputs) -> IterationValueInputs {
    iteration_to_values(&iteration_from_values(f).unwrap())
}

/// Reject a field without requiring third-party error wording.
fn rejects(f: IterationValueInputs, field: &str) {
    let error = iteration_from_values(f).unwrap_err();
    assert!(error.to_string().starts_with(&format!("{field}:")));
}

/// Preserve every field, Unicode and owned output independently of the record.
#[test]
fn complete_fields_round_trip_and_output_is_owned() {
    let mut f = base();
    f.title = json!("\u{0} é 中文 🦀\n");
    f.description = json!("body\u{0}\nUnicode é");
    f.metadata = json!([null, {"nested": [false, 12, "é"]}]);
    let record = iteration_from_values(f.clone()).unwrap();
    assert_eq!(iteration_to_values(&record), f);
    assert_eq!(
        iteration_from_values(iteration_to_values(&record)).unwrap(),
        record
    );
    let mut changed = iteration_to_values(&record);
    changed.metadata[1]["nested"][0] = json!(true);
    changed.title = json!("changed");
    changed.description = json!("");
    assert_eq!(iteration_to_values(&record), f);
}

/// Preserve all metadata variants, including explicit null and empty values.
#[test]
fn arbitrary_metadata_and_empty_strings_survive() {
    for metadata in [
        json!(null),
        json!(false),
        json!(42),
        json!(2.5),
        json!(""),
        json!([]),
        json!({}),
    ] {
        let mut f = base();
        f.metadata = metadata;
        assert_eq!(output(f.clone()), f);
    }
}

/// Reject every non-string JSON variant in each required string field.
#[test]
fn required_string_type_matrix() {
    for value in [
        json!(null),
        json!(false),
        json!(1),
        json!(1.5),
        json!([]),
        json!({}),
    ] {
        let mut f = base();
        f.gest_id = value;
        rejects(f, "gest_id");
    }
    for value in [
        json!(null),
        json!(false),
        json!(1),
        json!(1.5),
        json!([]),
        json!({}),
    ] {
        let mut f = base();
        f.project_id = value;
        rejects(f, "project_id");
    }
    for value in [
        json!(null),
        json!(false),
        json!(1),
        json!(1.5),
        json!([]),
        json!({}),
    ] {
        let mut f = base();
        f.title = value;
        rejects(f, "title");
    }
    for value in [
        json!(null),
        json!(false),
        json!(1),
        json!(1.5),
        json!([]),
        json!({}),
    ] {
        let mut f = base();
        f.status = value;
        rejects(f, "status");
    }
    for value in [
        json!(null),
        json!(false),
        json!(1),
        json!(1.5),
        json!([]),
        json!({}),
    ] {
        let mut f = base();
        f.description = value;
        rejects(f, "description");
    }
    for value in [
        json!(null),
        json!(false),
        json!(1),
        json!(1.5),
        json!([]),
        json!({}),
    ] {
        let mut f = base();
        f.created_at = value;
        rejects(f, "created_at");
    }
    for value in [
        json!(null),
        json!(false),
        json!(1),
        json!(1.5),
        json!([]),
        json!({}),
    ] {
        let mut f = base();
        f.updated_at = value;
        rejects(f, "updated_at");
    }
}

/// Reject abbreviated, malformed and Unicode IDs in both identifier fields.
#[test]
fn full_identifier_domain() {
    for value in [
        "",
        "kkkkkkkk",
        "kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk",
        "kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk",
        "Kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk",
        "jkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk",
        "ékkkkkkkkkkkkkkkkkkkkkkkkkkkkkk",
        "kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk{",
    ] {
        let mut f = base();
        f.gest_id = json!(value);
        rejects(f, "gest_id");
        let mut f = base();
        f.project_id = json!(value);
        rejects(f, "project_id");
    }
    let mut f = base();
    f.gest_id = json!("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz");
    assert_eq!(output(f.clone()), f);
}

/// Share the established chrono success, invalid and canonical refusal matrix.
#[test]
fn business_timestamp_matrix_for_every_timestamp_field() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/gest_task_values/records.json")).unwrap();
    for case in fixture["timestamps"].as_array().unwrap() {
        let mut f = base();
        f.created_at = case["input"].clone();
        assert_eq!(output(f).created_at, case["output"]);
    }
    for value in fixture["invalid_timestamps"]
        .as_array()
        .unwrap()
        .iter()
        .chain(fixture["canonical_refusals"].as_array().unwrap())
    {
        let mut f = base();
        f.created_at = value.clone();
        rejects(f, "created_at");
    }
    for value in [json!(false), json!(42), json!(2.5), json!([]), json!({})] {
        let mut f = base();
        f.created_at = value;
        rejects(f, "created_at");
    }
    for case in fixture["timestamps"].as_array().unwrap() {
        let mut f = base();
        f.updated_at = case["input"].clone();
        assert_eq!(output(f).updated_at, case["output"]);
    }
    for value in fixture["invalid_timestamps"]
        .as_array()
        .unwrap()
        .iter()
        .chain(fixture["canonical_refusals"].as_array().unwrap())
    {
        let mut f = base();
        f.updated_at = value.clone();
        rejects(f, "updated_at");
    }
    for value in [json!(false), json!(42), json!(2.5), json!([]), json!({})] {
        let mut f = base();
        f.updated_at = value;
        rejects(f, "updated_at");
    }
    for case in fixture["timestamps"].as_array().unwrap() {
        let mut f = base();
        f.status = json!("completed");
        f.completed_at = case["input"].clone();
        assert_eq!(output(f).completed_at, case["output"]);
    }
    for value in fixture["invalid_timestamps"]
        .as_array()
        .unwrap()
        .iter()
        .chain(fixture["canonical_refusals"].as_array().unwrap())
    {
        let mut f = base();
        f.status = json!("completed");
        f.completed_at = value.clone();
        rejects(f, "completed_at");
    }
    for value in [json!(false), json!(42), json!(2.5), json!([]), json!({})] {
        let mut f = base();
        f.status = json!("completed");
        f.completed_at = value;
        rejects(f, "completed_at");
    }
}

/// Preserve the exact chrono cause and keep Display free of supplied input.
#[test]
fn typed_chrono_causes_and_safe_display() {
    for value in [
        "SENSITIVE-invalid-time",
        "2026-02-30T12:00:00Z",
        "0000-01-01T00:00:00+01:00",
        "9999-12-31T23:00:00-01:00",
    ] {
        let expected = match chrono::DateTime::parse_from_rfc3339(value) {
            Err(cause) => cause,
            Ok(dt) => {
                chrono::DateTime::parse_from_rfc3339(&dt.with_timezone(&chrono::Utc).to_rfc3339())
                    .unwrap_err()
            }
        };
        {
            let mut f = base();
            f.created_at = json!(value);
            let error = iteration_from_values(f).unwrap_err();
            assert_eq!(error.parse_cause(), Some(&expected));
            assert_eq!(error.parse_cause().unwrap().kind(), expected.kind());
            assert!(error.to_string().starts_with("created_at:"));
            assert!(!error.to_string().contains(value));
            assert_eq!(error.clone(), error);
        }
        {
            let mut f = base();
            f.updated_at = json!(value);
            let error = iteration_from_values(f).unwrap_err();
            assert_eq!(error.parse_cause(), Some(&expected));
            assert_eq!(error.parse_cause().unwrap().kind(), expected.kind());
            assert!(error.to_string().starts_with("updated_at:"));
            assert!(!error.to_string().contains(value));
            assert_eq!(error.clone(), error);
        }
        {
            let mut f = base();
            f.status = json!("completed");
            f.completed_at = json!(value);
            let error = iteration_from_values(f).unwrap_err();
            assert_eq!(error.parse_cause(), Some(&expected));
            assert_eq!(error.parse_cause().unwrap().kind(), expected.kind());
            assert!(error.to_string().starts_with("completed_at:"));
            assert!(!error.to_string().contains(value));
            assert_eq!(error.clone(), error);
        }
    }
    let mut f = base();
    f.created_at = json!(42);
    assert!(
        iteration_from_values(f)
            .unwrap_err()
            .parse_cause()
            .is_none()
    );
    assert_eq!(
        output(base()).created_at,
        json!("2026-11-18T12:00:00+00:00")
    );
}

/// Observe source case-insensitive parsing without claiming primitive API ports.
#[test]
fn source_behavior_status_spellings() {
    for (input, canonical) in [
        ("active", "active"),
        ("ACTIVE", "active"),
        ("CaNcElLeD", "cancelled"),
        ("COMPLETED", "completed"),
    ] {
        let mut f = base();
        f.status = json!(input);
        if canonical != "active" {
            f.completed_at = json!("2026-11-18T12:00:00+00:00");
        }
        assert_eq!(output(f).status, json!(canonical));
    }
    for status in [
        "",
        "done",
        "open",
        "in-progress",
        " active",
        "completed ",
        "canceled",
    ] {
        let mut f = base();
        f.status = json!(status);
        rejects(f, "status");
    }
}

/// Enforce the adopted terminal/completed_at rule over all source statuses.
#[test]
fn adopted_terminal_timestamp_pairing() {
    for status in ["active", "completed", "cancelled"] {
        for has_time in [false, true] {
            let mut f = base();
            f.status = json!(status);
            if has_time {
                f.completed_at = json!("2026-11-18T12:00:00+00:00");
            }
            if (status != "active") == has_time {
                assert_eq!(output(f.clone()), f);
            } else {
                rejects(f, "completed_at");
            }
        }
    }
}

/// Business conversion imposes no timestamp ordering rule.
#[test]
fn timestamps_need_not_be_chronologically_ordered() {
    let mut f = base();
    f.created_at = json!("2026-11-19T12:00:00+00:00");
    assert_eq!(output(f.clone()), f);
}

/// Check that the source map names all 9 fields, existing controls and 6 later obligations, and
/// that its pinned hashes and its exact list of fields match the committed expectations.
#[test]
fn source_map_names_every_field_and_existing_control() {
    let map: Value = serde_json::from_str(include_str!(
        "fixtures/gest_iteration_values/source-map.json"
    ))
    .unwrap();
    let base: Value =
        serde_json::from_str(include_str!("fixtures/gest_iteration_values/records.json")).unwrap();
    let this_file = include_str!("gest_iteration_values.rs");
    assert_eq!(map["source"], "src/store/model/iteration.rs");
    assert_eq!(map["exact_inherited_tests_ported"], 0);
    let fields = map["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 9);
    assert_eq!(map["source_field_count"], 9);
    for field in fields {
        let name = field["canonical_field"].as_str().unwrap();
        assert!(base["base"].get(name).is_some(), "{name}");
        for control in field["controls"].as_array().unwrap() {
            let control = control.as_str().unwrap();
            assert!(this_file.contains(&format!("fn {control}()")), "{control}");
        }
    }
    let obligations = map["later_exact_api_obligations"].as_array().unwrap();
    assert_eq!(obligations.len(), 6);
    for row in obligations {
        assert_eq!(row["exact_port_in_minimal_owned_seam"], false);
        assert!(!row["reason"].as_str().unwrap().is_empty());
    }
    // The terminal pairing comes from the Gest contract, not from upstream source.
    let status = fields.iter().find(|f| f["name"] == "status").unwrap();
    let rule = status["owned_value_rule"].as_str().unwrap();
    assert!(rule.contains(
        "adopted from the Gest contract (gest.md, Typed model invariants), \
         not from upstream source: terminal/completed_at"
    ));
    gest_source_map::assert_rejects_stale_and_malformed(
        &map,
        &gest_source_map::Expected {
            sources: &["src/store/model/iteration.rs"],
            fields: &[
                ("id", "gest_id"),
                ("project_id", "project_id"),
                ("title", "title"),
                ("status", "status"),
                ("description", "description"),
                ("metadata", "metadata"),
                ("completed_at", "completed_at"),
                ("created_at", "created_at"),
                ("updated_at", "updated_at"),
            ],
            control_file: "tests/gest_iteration_values.rs",
        },
    );
}
