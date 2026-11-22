//! New controls for decoded fields. This file ports none of the exact tests of the source primitives.
use gossil::gest_entity_tag_values::{
    EntityTagValueInputs, entity_tag_from_values, entity_tag_to_values,
};
use serde_json::{Value, json};

/// Every field name, in the order of the upstream table declaration.
const FIELDS: [&str; 4] = ["entity_id", "entity_type", "tag_id", "created_at"];

/// Build a complete fixture; raw field admission remains caller-owned.
fn inputs(v: &Value) -> EntityTagValueInputs {
    assert_eq!(v.as_object().unwrap().len(), 4);
    EntityTagValueInputs {
        entity_id: v["entity_id"].clone(),
        entity_type: v["entity_type"].clone(),
        tag_id: v["tag_id"].clone(),
        created_at: v["created_at"].clone(),
    }
}

/// Load the explicit base record.
fn base() -> EntityTagValueInputs {
    inputs(
        &serde_json::from_str::<Value>(include_str!(
            "fixtures/gest_entity_tag_values/records.json"
        ))
        .unwrap()["base"],
    )
}

/// Replace one named field for type and boundary controls.
fn set(f: &mut EntityTagValueInputs, field: &str, value: Value) {
    match field {
        "entity_id" => f.entity_id = value,
        "entity_type" => f.entity_type = value,
        "tag_id" => f.tag_id = value,
        "created_at" => f.created_at = value,
        _ => panic!("unknown fixture field"),
    }
}

/// Observe a successful complete conversion.
fn output(f: EntityTagValueInputs) -> EntityTagValueInputs {
    entity_tag_to_values(&entity_tag_from_values(f).unwrap())
}

/// Reject a field without requiring third-party error wording.
fn rejects(f: EntityTagValueInputs, field: &str) {
    let error = entity_tag_from_values(f).unwrap_err();
    assert!(error.to_string().starts_with(&format!("{field}:")));
    assert!(error.parse_cause().is_none() || field.ends_with("_at"));
}

/// Preserve every field and keep the output independent of the record.
#[test]
fn complete_output_round_trips_and_is_independent() {
    let f = base();
    let record = entity_tag_from_values(f.clone()).unwrap();
    assert_eq!(entity_tag_to_values(&record), f);
    assert_eq!(
        entity_tag_from_values(entity_tag_to_values(&record)).unwrap(),
        record
    );
    let mut changed = entity_tag_to_values(&record);
    for field in FIELDS {
        set(&mut changed, field, json!("changed"));
    }
    assert_ne!(changed, f);
    assert_eq!(entity_tag_to_values(&record), f);
}

/// Reject every non-string JSON variant, including null, in each field.
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
            let mut f = base();
            set(&mut f, field, value);
            let error = entity_tag_from_values(f).unwrap_err();
            assert!(error.to_string().starts_with(&format!("{field}:")));
            assert!(!error.to_string().contains("SENSITIVE"));
            assert!(error.parse_cause().is_none());
        }
    }
}

/// Keep the full identifier domain and reject prefixes or alphabet substitutions.
#[test]
fn full_identifier_domain_for_every_id_field() {
    for field in ["entity_id", "tag_id"] {
        for id in ["k".repeat(32), "z".repeat(32), "klmnopqrstuvwxyz".repeat(2)] {
            let mut f = base();
            set(&mut f, field, json!(id));
            assert_eq!(output(f.clone()), f);
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

/// The join has no Gest ID, and the key fields give one record for one tuple.
#[test]
fn join_has_no_gest_id_and_one_record_per_tuple() {
    let records: Value =
        serde_json::from_str(include_str!("fixtures/gest_entity_tag_values/records.json")).unwrap();
    assert!(records["base"].get("gest_id").is_none());
    assert!(records["base"].get("id").is_none());
    let mut upper = base();
    upper.entity_type = json!("TASK");
    assert_eq!(
        entity_tag_from_values(upper).unwrap(),
        entity_tag_from_values(base()).unwrap()
    );
    for (field, value) in [
        ("entity_id", json!("z".repeat(32))),
        ("entity_type", json!("artifact")),
        ("tag_id", json!("z".repeat(32))),
    ] {
        let mut other = base();
        set(&mut other, field, value);
        assert_ne!(
            entity_tag_from_values(other).unwrap(),
            entity_tag_from_values(base()).unwrap()
        );
    }
}

/// Share the established chrono success, invalid and canonical refusal matrix.
#[test]
fn business_timestamp_matrix_for_every_timestamp_field() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/gest_task_values/records.json")).unwrap();
    for field in ["created_at"] {
        for case in fixture["timestamps"].as_array().unwrap() {
            let mut f = base();
            set(&mut f, field, case["input"].clone());
            let record = entity_tag_from_values(f.clone()).unwrap();
            set(&mut f, field, case["output"].clone());
            assert_eq!(entity_tag_to_values(&record), f);
            assert_eq!(
                entity_tag_from_values(entity_tag_to_values(&record)).unwrap(),
                record
            );
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
    for input in [
        "SENSITIVE-not-a-time",
        "2026-02-30T12:00:00Z",
        "0000-01-01T00:00:00+01:00",
        "9999-12-31T23:00:00-01:00",
    ] {
        let expected = match chrono::DateTime::parse_from_rfc3339(input) {
            Err(cause) => cause,
            Ok(dt) => {
                chrono::DateTime::parse_from_rfc3339(&dt.with_timezone(&chrono::Utc).to_rfc3339())
                    .unwrap_err()
            }
        };
        let mut f = base();
        f.created_at = json!(input);
        let error = entity_tag_from_values(f).unwrap_err();
        assert_eq!(error.parse_cause(), Some(&expected));
        assert!(error.to_string().starts_with("created_at:"));
        assert!(!error.to_string().contains(input));
    }
}

/// Check that the source map names all 4 fields, existing controls and 5 later obligations.
#[test]
fn source_map_names_every_field_and_existing_control() {
    let map: Value = serde_json::from_str(include_str!(
        "fixtures/gest_entity_tag_values/source-map.json"
    ))
    .unwrap();
    let records: Value =
        serde_json::from_str(include_str!("fixtures/gest_entity_tag_values/records.json")).unwrap();
    let this_file = include_str!("gest_entity_tag_values.rs");
    assert_eq!(map["source"], "src/store/model/entity_tag.rs");
    assert_eq!(map["exact_inherited_tests_ported"], 0);
    assert_eq!(
        map["key_tuple"],
        json!(["entity_type", "entity_id", "tag_id"])
    );
    let fields = map["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 4);
    assert_eq!(map["source_field_count"], 4);
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
    let entity =
        "eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8:src/store/model/primitives/entity_type.rs";
    assert_eq!(
        ids,
        [
            format!("{entity}:54:it_formats_artifact"),
            format!("{entity}:59:it_formats_iteration"),
            format!("{entity}:64:it_formats_task"),
            format!("{entity}:73:it_parses_case_insensitively"),
            format!("{entity}:79:it_rejects_invalid"),
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
}
