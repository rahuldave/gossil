//! New controls for decoded fields. This file ports none of the exact tests of the source primitives.
use gossil::gest_relationship_values::{
    RelationshipValueInputs, relationship_from_values, relationship_to_values,
};
use serde_json::{Value, json};

/// Every field name, in the order of the upstream row reader.
const FIELDS: [&str; 8] = [
    "gest_id",
    "rel_type",
    "source_id",
    "source_type",
    "target_id",
    "target_type",
    "created_at",
    "updated_at",
];

/// Build a complete fixture. The caller still owns the admission of raw fields.
fn inputs(v: &Value) -> RelationshipValueInputs {
    assert_eq!(v.as_object().unwrap().len(), 8);
    RelationshipValueInputs {
        gest_id: v["gest_id"].clone(),
        rel_type: v["rel_type"].clone(),
        source_id: v["source_id"].clone(),
        source_type: v["source_type"].clone(),
        target_id: v["target_id"].clone(),
        target_type: v["target_type"].clone(),
        created_at: v["created_at"].clone(),
        updated_at: v["updated_at"].clone(),
    }
}

/// Load the explicit base record.
fn base() -> RelationshipValueInputs {
    inputs(
        &serde_json::from_str::<Value>(include_str!(
            "fixtures/gest_relationship_values/records.json"
        ))
        .unwrap()["base"],
    )
}

/// Replace one named field for type and boundary controls.
fn set(f: &mut RelationshipValueInputs, field: &str, value: Value) {
    match field {
        "gest_id" => f.gest_id = value,
        "rel_type" => f.rel_type = value,
        "source_id" => f.source_id = value,
        "source_type" => f.source_type = value,
        "target_id" => f.target_id = value,
        "target_type" => f.target_type = value,
        "created_at" => f.created_at = value,
        "updated_at" => f.updated_at = value,
        _ => panic!("unknown fixture field"),
    }
}

/// Observe a successful complete conversion.
fn output(f: RelationshipValueInputs) -> RelationshipValueInputs {
    relationship_to_values(&relationship_from_values(f).unwrap())
}

/// Reject a field without requiring third-party error wording.
fn rejects(f: RelationshipValueInputs, field: &str) {
    let error = relationship_from_values(f).unwrap_err();
    assert!(error.to_string().starts_with(&format!("{field}:")));
    assert!(error.parse_cause().is_none() || field.ends_with("_at"));
}

/// Preserve every field and keep the output independent of the record.
#[test]
fn complete_output_round_trips_and_is_independent() {
    let f = base();
    let record = relationship_from_values(f.clone()).unwrap();
    assert_eq!(relationship_to_values(&record), f);
    assert_eq!(
        relationship_from_values(relationship_to_values(&record)).unwrap(),
        record
    );
    let mut changed = relationship_to_values(&record);
    for field in FIELDS {
        set(&mut changed, field, json!("changed"));
    }
    assert_ne!(changed, f);
    assert_eq!(relationship_to_values(&record), f);
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
            let error = relationship_from_values(f).unwrap_err();
            assert!(error.to_string().starts_with(&format!("{field}:")));
            assert!(!error.to_string().contains("SENSITIVE"));
            assert!(error.parse_cause().is_none());
        }
    }
}

/// Keep the full identifier domain and reject prefixes or alphabet substitutions.
#[test]
fn full_identifier_domain_for_every_id_field() {
    for field in ["gest_id", "source_id", "target_id"] {
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

/// Relationship types accept the five hyphenated source spellings in any case.
#[test]
fn rel_type_follows_source_spellings() {
    for (input, expected) in [
        ("blocked-by", "blocked-by"),
        ("BLOCKED-BY", "blocked-by"),
        ("blocks", "blocks"),
        // The source lowercases with Unicode rules, so the Kelvin sign becomes `k`.
        ("BLOC\u{212A}S", "blocks"),
        ("Child-Of", "child-of"),
        ("parent-of", "parent-of"),
        ("PARENT-OF", "parent-of"),
        ("relates-to", "relates-to"),
        ("Relates-To", "relates-to"),
    ] {
        let mut f = base();
        f.rel_type = json!(input);
        assert_eq!(output(f).rel_type, json!(expected));
    }
    for input in [
        "",
        "blocked_by",
        "child_of",
        "parent_of",
        "relates_to",
        "blockedby",
        "relates",
        "depends-on",
        " blocks",
        "blocks ",
        "child-of\n",
    ] {
        let mut f = base();
        f.rel_type = json!(input);
        rejects(f, "rel_type");
    }
}

/// Both endpoint types accept the three source spellings in any case and return lowercase.
#[test]
fn endpoint_types_follow_source_spellings() {
    for field in ["source_type", "target_type"] {
        for (input, expected) in [
            ("ARTIFACT", "artifact"),
            ("Iteration", "iteration"),
            ("tAsK", "task"),
        ] {
            let mut f = base();
            set(&mut f, field, json!(input));
            let out = output(f);
            let actual = if field == "source_type" {
                out.source_type
            } else {
                out.target_type
            };
            assert_eq!(actual, json!(expected));
        }
        for input in ["", "project", "note", "tag", "author", " task", "task "] {
            let mut f = base();
            set(&mut f, field, json!(input));
            rejects(f, field);
        }
    }
}

/// The record keeps the direction as given and never applies the inverse type.
#[test]
fn direction_is_kept_without_inverse_or_endpoint_checks() {
    for rel_type in [
        "blocked-by",
        "blocks",
        "child-of",
        "parent-of",
        "relates-to",
    ] {
        let mut f = base();
        f.rel_type = json!(rel_type);
        assert_eq!(output(f.clone()), f);
    }
    let mut swapped = base();
    swapped.source_id = base().target_id;
    swapped.source_type = base().target_type;
    swapped.target_id = base().source_id;
    swapped.target_type = base().source_type;
    assert_eq!(output(swapped.clone()), swapped);
    assert_ne!(
        relationship_from_values(swapped).unwrap(),
        relationship_from_values(base()).unwrap()
    );
    let mut self_link = base();
    self_link.target_id = self_link.source_id.clone();
    self_link.target_type = self_link.source_type.clone();
    assert_eq!(output(self_link.clone()), self_link);
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
            let record = relationship_from_values(f.clone()).unwrap();
            set(&mut f, field, case["output"].clone());
            assert_eq!(relationship_to_values(&record), f);
            assert_eq!(
                relationship_from_values(relationship_to_values(&record)).unwrap(),
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
            let error = relationship_from_values(f).unwrap_err();
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

/// Check that the source map names all 8 fields, existing controls and 15 later obligations.
#[test]
fn source_map_names_every_field_and_existing_control() {
    let map: Value = serde_json::from_str(include_str!(
        "fixtures/gest_relationship_values/source-map.json"
    ))
    .unwrap();
    let records: Value = serde_json::from_str(include_str!(
        "fixtures/gest_relationship_values/records.json"
    ))
    .unwrap();
    let this_file = include_str!("gest_relationship_values.rs");
    assert_eq!(map["source"], "src/store/model/relationship.rs");
    assert_eq!(map["exact_inherited_tests_ported"], 0);
    let fields = map["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 8);
    assert_eq!(map["source_field_count"], 8);
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
    // The rows copy the EntityType and RelationshipType rows of the governing
    // addendum, whose hash the map records.
    let obligations = map["later_exact_api_obligations"].as_array().unwrap();
    let ids: Vec<&str> = obligations
        .iter()
        .map(|row| row["test_id"].as_str().unwrap())
        .collect();
    let entity =
        "eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8:src/store/model/primitives/entity_type.rs";
    let rel =
        "eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8:src/store/model/primitives/relationship_type.rs";
    assert_eq!(
        ids,
        [
            format!("{entity}:54:it_formats_artifact"),
            format!("{entity}:59:it_formats_iteration"),
            format!("{entity}:64:it_formats_task"),
            format!("{entity}:73:it_parses_case_insensitively"),
            format!("{entity}:79:it_rejects_invalid"),
            format!("{rel}:79:it_formats_blocked_by"),
            format!("{rel}:84:it_formats_blocks"),
            format!("{rel}:89:it_formats_child_of"),
            format!("{rel}:94:it_formats_parent_of"),
            format!("{rel}:99:it_formats_relates_to"),
            format!("{rel}:110:it_inverts_blocked_by_to_blocks"),
            format!("{rel}:115:it_inverts_blocks_to_blocked_by"),
            format!("{rel}:120:it_inverts_child_of_to_parent_of"),
            format!("{rel}:125:it_inverts_parent_of_to_child_of"),
            format!("{rel}:130:it_keeps_relates_to_symmetric"),
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
