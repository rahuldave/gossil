//! New controls for decoded fields. This file ports none of the exact tests of the source primitives.
#[path = "support/gest_source_map.rs"]
mod gest_source_map;
use gossil::gest_note_values::{NoteValueInputs, note_from_values, note_to_values};
use serde_json::{Value, json};

/// Every field name, in the order of the upstream row reader.
const FIELDS: [&str; 7] = [
    "gest_id",
    "entity_id",
    "entity_type",
    "author_id",
    "body",
    "created_at",
    "updated_at",
];

/// Build a complete fixture. The caller still owns the admission of raw fields.
fn inputs(v: &Value) -> NoteValueInputs {
    assert_eq!(v.as_object().unwrap().len(), 7);
    NoteValueInputs {
        gest_id: v["gest_id"].clone(),
        entity_id: v["entity_id"].clone(),
        entity_type: v["entity_type"].clone(),
        author_id: v["author_id"].clone(),
        body: v["body"].clone(),
        created_at: v["created_at"].clone(),
        updated_at: v["updated_at"].clone(),
    }
}

/// Load the explicit base record, which has an unknown author.
fn base() -> NoteValueInputs {
    inputs(
        &serde_json::from_str::<Value>(include_str!("fixtures/gest_note_values/records.json"))
            .unwrap()["base"],
    )
}

/// Replace one named field for type and boundary controls.
fn set(f: &mut NoteValueInputs, field: &str, value: Value) {
    match field {
        "gest_id" => f.gest_id = value,
        "entity_id" => f.entity_id = value,
        "entity_type" => f.entity_type = value,
        "author_id" => f.author_id = value,
        "body" => f.body = value,
        "created_at" => f.created_at = value,
        "updated_at" => f.updated_at = value,
        _ => panic!("unknown fixture field"),
    }
}

/// Observe a successful complete conversion.
fn output(f: NoteValueInputs) -> NoteValueInputs {
    note_to_values(&note_from_values(f).unwrap())
}

/// Reject a field without requiring third-party error wording.
fn rejects(f: NoteValueInputs, field: &str) {
    let error = note_from_values(f).unwrap_err();
    assert!(error.to_string().starts_with(&format!("{field}:")));
    assert!(error.parse_cause().is_none() || field.ends_with("_at"));
}

/// Preserve every field and keep the output independent of the record.
#[test]
fn complete_output_round_trips_and_is_independent() {
    let f = base();
    assert_eq!(f.author_id, Value::Null);
    let record = note_from_values(f.clone()).unwrap();
    assert_eq!(note_to_values(&record), f);
    assert_eq!(note_from_values(note_to_values(&record)).unwrap(), record);
    let mut changed = note_to_values(&record);
    for field in FIELDS {
        set(&mut changed, field, json!("changed"));
    }
    assert_ne!(changed, f);
    assert_eq!(note_to_values(&record), f);
}

/// Reject every wrong JSON variant in each field; only the author accepts null.
#[test]
fn every_field_rejects_wrong_types_without_parser_causes() {
    for field in FIELDS {
        for value in [
            json!(null),
            json!(false),
            json!(42),
            json!(2.5),
            json!([]),
            json!({"private": "SENSITIVE"}),
        ] {
            if field == "author_id" && value.is_null() {
                continue;
            }
            let mut f = base();
            set(&mut f, field, value);
            let error = note_from_values(f).unwrap_err();
            assert!(error.to_string().starts_with(&format!("{field}:")));
            assert!(!error.to_string().contains("SENSITIVE"));
            assert!(error.parse_cause().is_none());
        }
    }
}

/// Keep the full identifier domain and reject prefixes or alphabet substitutions.
#[test]
fn full_identifier_domain_for_every_id_field() {
    for field in ["gest_id", "entity_id", "author_id"] {
        for id in ["k".repeat(32), "z".repeat(32), "klmnopqrstuvwxyz".repeat(2)] {
            let mut f = base();
            set(&mut f, field, json!(id));
            let out = output(f.clone());
            assert_eq!(out, f);
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
            format!(" {}", "k".repeat(31)),
        ] {
            let mut f = base();
            set(&mut f, field, json!(id));
            rejects(f, field);
        }
    }
}

/// A known author survives exactly, and an unknown author stays JSON null.
#[test]
fn author_is_explicit_null_or_a_full_id() {
    let null_record = note_from_values(base()).unwrap();
    assert_eq!(note_to_values(&null_record).author_id, Value::Null);
    let mut f = base();
    f.author_id = json!("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz");
    let known = note_from_values(f.clone()).unwrap();
    assert_eq!(
        note_to_values(&known).author_id,
        json!("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz")
    );
    assert_ne!(known, null_record);
    for value in [json!(""), json!("null"), json!(0)] {
        let mut f = base();
        f.author_id = value;
        rejects(f, "author_id");
    }
}

/// Entity types accept the three source spellings in any case and return lowercase.
#[test]
fn entity_type_follows_source_spellings() {
    for (input, expected) in [
        ("artifact", "artifact"),
        ("ARTIFACT", "artifact"),
        ("Iteration", "iteration"),
        ("tAsK", "task"),
        // The source lowercases with Unicode rules, so the Kelvin sign becomes `k`.
        ("TAS\u{212A}", "task"),
    ] {
        let mut f = base();
        f.entity_type = json!(input);
        assert_eq!(output(f).entity_type, json!(expected));
    }
    for input in [
        "", "project", "note", "tag", "author", "tasks", " task", "task ", "task\n",
    ] {
        let mut f = base();
        f.entity_type = json!(input);
        rejects(f, "entity_type");
    }
}

/// The body keeps every byte, with no format, trimming or size rule.
#[test]
fn body_keeps_exact_text() {
    for body in [
        String::new(),
        " ".to_string(),
        "  padded\t\r\n".to_string(),
        "é 中文 🦀\0".to_string(),
        "# Title\n\n- *markdown* stays text\n".to_string(),
        "x".repeat(70_000),
    ] {
        let mut f = base();
        f.body = json!(body);
        assert_eq!(output(f.clone()), f);
    }
}

/// Reuse the established `chrono` matrix of success, invalid and canonical refusal cases.
#[test]
fn business_timestamp_matrix_for_every_timestamp_field() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/gest_task_values/records.json")).unwrap();
    for field in ["created_at", "updated_at"] {
        for case in fixture["timestamps"].as_array().unwrap() {
            let mut f = base();
            set(&mut f, field, case["input"].clone());
            let record = note_from_values(f.clone()).unwrap();
            set(&mut f, field, case["output"].clone());
            assert_eq!(note_to_values(&record), f);
            assert_eq!(note_from_values(note_to_values(&record)).unwrap(), record);
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
            let error = note_from_values(f).unwrap_err();
            assert_eq!(error.parse_cause(), Some(&expected));
            assert!(error.to_string().starts_with(&format!("{field}:")));
            assert!(!error.to_string().contains(input));
        }
    }
}

/// The business conversion imposes no rule on the order of the timestamps.
#[test]
fn timestamps_need_not_be_chronologically_ordered() {
    let f = base();
    assert!(f.updated_at.as_str() < f.created_at.as_str());
    assert_eq!(output(f.clone()), f);
}

/// Check that the source map names all 7 fields, existing controls and 5 later obligations, and
/// that its pinned hashes and its exact list of fields match the committed expectations.
#[test]
fn source_map_names_every_field_and_existing_control() {
    let map: Value =
        serde_json::from_str(include_str!("fixtures/gest_note_values/source-map.json")).unwrap();
    let records: Value =
        serde_json::from_str(include_str!("fixtures/gest_note_values/records.json")).unwrap();
    let this_file = include_str!("gest_note_values.rs");
    assert_eq!(map["source"], "src/store/model/note.rs");
    assert_eq!(map["exact_inherited_tests_ported"], 0);
    let fields = map["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 7);
    assert_eq!(map["source_field_count"], 7);
    for (field, name) in fields.iter().zip(FIELDS) {
        assert_eq!(field["canonical_field"], name);
        assert!(records["base"].get(name).is_some(), "{name}");
        for control in field["controls"].as_array().unwrap() {
            let control = control.as_str().unwrap();
            assert!(this_file.contains(&format!("fn {control}()")), "{control}");
        }
    }
    for control in map["test_functions"].as_array().unwrap() {
        let control = control.as_str().unwrap();
        assert!(this_file.contains(&format!("fn {control}()")), "{control}");
    }
    for source in map["supporting_sources"].as_array().unwrap() {
        assert!(!source["path"].as_str().unwrap().starts_with('/'));
        assert_eq!(source["sha256"].as_str().unwrap().len(), 64);
    }
    // The rows copy the five EntityType rows of the governing addendum, whose hash the map records.
    let obligations = map["later_exact_api_obligations"].as_array().unwrap();
    let ids: Vec<&str> = obligations
        .iter()
        .map(|row| row["test_id"].as_str().unwrap())
        .collect();
    let pin = "eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8:src/store/model/primitives/entity_type.rs";
    assert_eq!(
        ids,
        [
            format!("{pin}:54:it_formats_artifact"),
            format!("{pin}:59:it_formats_iteration"),
            format!("{pin}:64:it_formats_task"),
            format!("{pin}:73:it_parses_case_insensitively"),
            format!("{pin}:79:it_rejects_invalid"),
        ]
    );
    for row in obligations {
        assert_eq!(row["exact_port_in_minimal_owned_seam"], false);
        assert_eq!(row["adopted_replacement"], false);
        assert_eq!(row["fixture_sha256"].as_str().unwrap().len(), 64);
        assert!(!row["reason"].as_str().unwrap().is_empty());
    }
    assert_eq!(
        map["governing_addendum_sha256"],
        "afddd1c2f5e905f72126209605737cc80e63791870d3ce8bd59f26c9589d8454"
    );
    gest_source_map::assert_rejects_stale_and_malformed(
        &map,
        &gest_source_map::Expected {
            sources: &[
                "src/store/model/note.rs",
                "src/store/migration/m00009_create_notes.rs",
                "src/store/model/primitives/entity_type.rs",
                "src/store/model/primitives/id.rs",
            ],
            fields: &[
                ("id", "gest_id"),
                ("entity_id", "entity_id"),
                ("entity_type", "entity_type"),
                ("author_id", "author_id"),
                ("body", "body"),
                ("created_at", "created_at"),
                ("updated_at", "updated_at"),
            ],
            control_file: "tests/gest_note_values.rs",
        },
    );
}
