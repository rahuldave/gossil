//! New decoded-field controls; no exact source primitive tests are ported here.
use gossil::gest_tag_values::{TagValueInputs, tag_from_values, tag_to_values};
use serde_json::{Value, json};

/// Build a complete fixture; raw field admission remains caller-owned.
fn inputs(v: &Value) -> TagValueInputs {
    assert_eq!(v.as_object().unwrap().len(), 4);
    TagValueInputs {
        gest_id: v["gest_id"].clone(),
        label: v["label"].clone(),
        created_at: v["created_at"].clone(),
        updated_at: v["updated_at"].clone(),
    }
}

/// Load the explicit base record.
fn base() -> TagValueInputs {
    inputs(
        &serde_json::from_str::<Value>(include_str!("fixtures/gest_tag_values/records.json"))
            .unwrap()["base"],
    )
}

/// Replace one named field for type and boundary controls.
fn set(f: &mut TagValueInputs, field: &str, value: Value) {
    match field {
        "gest_id" => f.gest_id = value,
        "label" => f.label = value,
        "created_at" => f.created_at = value,
        "updated_at" => f.updated_at = value,
        _ => panic!("unknown fixture field"),
    }
}

/// Observe a successful complete conversion.
fn output(f: TagValueInputs) -> TagValueInputs {
    tag_to_values(&tag_from_values(f).unwrap())
}

/// Reject a field without requiring third-party error wording.
fn rejects(f: TagValueInputs, field: &str) {
    let error = tag_from_values(f).unwrap_err();
    assert!(error.to_string().starts_with(&format!("{field}:")));
}

/// Preserve every field and keep the output independent of the record.
#[test]
fn complete_output_round_trips_and_is_independent() {
    let f = base();
    let record = tag_from_values(f.clone()).unwrap();
    assert_eq!(tag_to_values(&record), f);
    assert_eq!(tag_from_values(tag_to_values(&record)).unwrap(), record);
    let mut changed = tag_to_values(&record);
    changed.gest_id = json!("changed");
    changed.label = json!("changed");
    changed.created_at = json!("changed");
    changed.updated_at = json!("changed");
    assert_ne!(changed, f);
    assert_eq!(tag_to_values(&record), f);
}

/// Reject every non-string JSON variant, including null, in each field.
#[test]
fn every_field_rejects_wrong_types_without_parser_causes() {
    for field in ["gest_id", "label", "created_at", "updated_at"] {
        for value in [
            json!(null),
            json!(false),
            json!(42),
            json!(2.5),
            json!([]),
            json!({"private": "SENSITIVE"}),
        ] {
            let mut f = base();
            set(&mut f, field, value);
            let error = tag_from_values(f).unwrap_err();
            assert!(error.to_string().starts_with(&format!("{field}:")));
            assert!(!error.to_string().contains("SENSITIVE"));
            assert!(error.parse_cause().is_none());
        }
    }
}

/// Keep the full identifier domain and reject prefixes or alphabet substitutions.
#[test]
fn full_identifier_domain_and_boundaries() {
    for id in ["k".repeat(32), "z".repeat(32), "klmnopqrstuvwxyz".repeat(2)] {
        let mut f = base();
        f.gest_id = json!(id);
        assert_eq!(output(f).gest_id, json!(id));
    }
    for id in [
        String::new(),
        "k".repeat(8),
        "k".repeat(31),
        "k".repeat(33),
        "K".repeat(32),
        "j".repeat(32),
        "é".repeat(16),
        format!("{}{{", "k".repeat(31)),
        format!("{}0", "k".repeat(31)),
    ] {
        let mut f = base();
        f.gest_id = json!(id);
        let error = tag_from_values(f).unwrap_err();
        assert!(error.to_string().starts_with("gest_id:"));
        assert!(error.parse_cause().is_none());
    }
}

/// Share the established chrono success, invalid and canonical refusal matrix.
#[test]
fn business_timestamp_matrix_for_every_timestamp_field() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/gest_task_values/records.json")).unwrap();
    for field in ["created_at", "updated_at"] {
        for case in fixture["timestamps"].as_array().unwrap() {
            let mut f = base();
            set(&mut f, field, case["input"].clone());
            let record = tag_from_values(f.clone()).unwrap();
            set(&mut f, field, case["output"].clone());
            assert_eq!(tag_to_values(&record), f);
            assert_eq!(tag_from_values(tag_to_values(&record)).unwrap(), record);
        }
        for value in fixture["invalid_timestamps"]
            .as_array()
            .unwrap()
            .iter()
            .chain(fixture["canonical_refusals"].as_array().unwrap())
        {
            let mut f = base();
            set(&mut f, field, value.clone());
            rejects(f, field);
        }
    }
}

/// Normalize offsets and keep source precision, leap seconds and year bounds.
#[test]
fn each_timestamp_normalizes_and_round_trips() {
    for field in ["created_at", "updated_at"] {
        for (input, expected) in [
            ("2026-11-18T07:00:00-05:00", "2026-11-18T12:00:00+00:00"),
            (
                "2026-11-18t12:00:00.1234567899z",
                "2026-11-18T12:00:00.123456789+00:00",
            ),
            ("2016-12-31T23:59:60Z", "2016-12-31T23:59:60+00:00"),
            ("0000-01-01T00:00:00Z", "0000-01-01T00:00:00+00:00"),
            ("9999-12-31T23:59:59Z", "9999-12-31T23:59:59+00:00"),
        ] {
            let mut f = base();
            set(&mut f, field, json!(input));
            let record = tag_from_values(f.clone()).unwrap();
            set(&mut f, field, json!(expected));
            assert_eq!(tag_to_values(&record), f);
            assert_eq!(tag_from_values(tag_to_values(&record)).unwrap(), record);
        }
    }
}

/// Preserve the exact chrono cause and keep Display free of supplied input.
#[test]
fn every_timestamp_retains_initial_and_canonical_parse_causes() {
    for field in ["created_at", "updated_at"] {
        for input in [
            "SENSITIVE-not-a-time",
            "2026-02-30T12:00:00Z",
            "0000-01-01T00:00:00+01:00",
            "9999-12-31T23:00:00-01:00",
        ] {
            let expected = match chrono::DateTime::parse_from_rfc3339(input) {
                Err(cause) => cause,
                Ok(dt) => chrono::DateTime::parse_from_rfc3339(
                    &dt.with_timezone(&chrono::Utc).to_rfc3339(),
                )
                .unwrap_err(),
            };
            let mut f = base();
            set(&mut f, field, json!(input));
            let error = tag_from_values(f).unwrap_err();
            assert_eq!(error.parse_cause(), Some(&expected));
            assert_eq!(error.parse_cause().unwrap().kind(), expected.kind());
            assert!(error.to_string().starts_with(&format!("{field}:")));
            assert!(!error.to_string().contains(input));
            assert_eq!(error.clone(), error);
        }
    }
}

/// Labels keep case, whitespace, Unicode and empty content, with no uniqueness rule.
#[test]
fn labels_remain_exact_without_global_uniqueness_policy() {
    let mut records = Vec::new();
    for label in ["", " ", "Priority", "priority", "é 中文 🦀\0\n"] {
        let mut f = base();
        f.label = json!(label);
        let record = tag_from_values(f.clone()).unwrap();
        assert_eq!(tag_to_values(&record), f);
        records.push(record);
    }
    let mut f = base();
    f.label = json!("Priority");
    assert_eq!(tag_from_values(f).unwrap(), records[2]);
    assert_ne!(records[2], records[3]);
}

/// Business conversion imposes no timestamp ordering rule.
#[test]
fn timestamps_need_not_be_chronologically_ordered() {
    let f = base();
    assert!(f.updated_at.as_str() < f.created_at.as_str());
    assert_eq!(output(f.clone()), f);
}

/// Check that the source map names all 4 fields, existing controls and 1 later obligation.
#[test]
fn source_map_names_every_field_and_existing_control() {
    let map: Value =
        serde_json::from_str(include_str!("fixtures/gest_tag_values/source-map.json")).unwrap();
    let base: Value =
        serde_json::from_str(include_str!("fixtures/gest_tag_values/records.json")).unwrap();
    let this_file = include_str!("gest_tag_values.rs");
    assert_eq!(map["source"], "src/store/model/tag.rs");
    assert_eq!(map["exact_inherited_tests_ported"], 0);
    let fields = map["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 4);
    assert_eq!(map["source_field_count"], 4);
    for field in fields {
        let name = field["canonical_field"].as_str().unwrap();
        assert!(base["base"].get(name).is_some(), "{name}");
        for control in field["controls"].as_array().unwrap() {
            let control = control.as_str().unwrap();
            assert!(this_file.contains(&format!("fn {control}()")), "{control}");
        }
    }
    for control in map["test_functions"].as_array().unwrap() {
        let control = control.as_str().unwrap();
        assert!(this_file.contains(&format!("fn {control}()")), "{control}");
    }
    let obligations = map["later_exact_api_obligations"].as_array().unwrap();
    assert_eq!(obligations.len(), 1);
    for row in obligations {
        assert_eq!(row["exact_port_in_minimal_owned_seam"], false);
        assert!(!row["reason"].as_str().unwrap().is_empty());
    }
    // The identity fields copy the row of the governing addendum, whose hash the
    // map records. The test line is the `#[test]` attribute line, as in the addendum.
    let row = &obligations[0];
    assert_eq!(
        row["test_id"],
        "eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8:src/store/model/tag.rs:87:it_creates_a_tag_with_label"
    );
    assert_eq!(
        row["fixture_sha256"],
        "6a5dd6e4fdba81e64a28b814e14d107703b55832622141144114b33fa959b7b9"
    );
    assert_eq!(
        row["assertion_ids"],
        json!(["eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8:src/store/model/tag.rs:91:assert_eq"])
    );
    assert_eq!(row["subject"], "constructor/builder/getter");
    assert_eq!(
        map["governing_addendum_sha256"],
        "afddd1c2f5e905f72126209605737cc80e63791870d3ce8bd59f26c9589d8454"
    );
}
