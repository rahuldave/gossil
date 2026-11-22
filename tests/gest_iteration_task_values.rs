//! New controls for decoded fields. This file ports none of the exact tests of the source primitives.
use gossil::gest_iteration_task_values::{
    IterationTaskValueInputs, iteration_task_from_values, iteration_task_to_values,
};
use serde_json::{Value, json};

/// Every field name, in the order of the upstream table declaration.
const FIELDS: [&str; 4] = ["iteration_id", "task_id", "phase", "created_at"];

/// Build a complete fixture; raw field admission remains caller-owned.
fn inputs(v: &Value) -> IterationTaskValueInputs {
    assert_eq!(v.as_object().unwrap().len(), 4);
    IterationTaskValueInputs {
        iteration_id: v["iteration_id"].clone(),
        task_id: v["task_id"].clone(),
        phase: v["phase"].clone(),
        created_at: v["created_at"].clone(),
    }
}

/// Load the explicit base record.
fn base() -> IterationTaskValueInputs {
    inputs(
        &serde_json::from_str::<Value>(include_str!(
            "fixtures/gest_iteration_task_values/records.json"
        ))
        .unwrap()["base"],
    )
}

/// Replace one named field for type and boundary controls.
fn set(f: &mut IterationTaskValueInputs, field: &str, value: Value) {
    match field {
        "iteration_id" => f.iteration_id = value,
        "task_id" => f.task_id = value,
        "phase" => f.phase = value,
        "created_at" => f.created_at = value,
        _ => panic!("unknown fixture field"),
    }
}

/// Observe a successful complete conversion.
fn output(f: IterationTaskValueInputs) -> IterationTaskValueInputs {
    iteration_task_to_values(&iteration_task_from_values(f).unwrap())
}

/// Reject a field without requiring third-party error wording.
fn rejects(f: IterationTaskValueInputs, field: &str) {
    let error = iteration_task_from_values(f).unwrap_err();
    assert!(error.to_string().starts_with(&format!("{field}:")));
    assert!(error.parse_cause().is_none() || field.ends_with("_at"));
}

/// Preserve every field and keep the output independent of the record.
#[test]
fn complete_output_round_trips_and_is_independent() {
    let f = base();
    let record = iteration_task_from_values(f.clone()).unwrap();
    assert_eq!(iteration_task_to_values(&record), f);
    assert_eq!(
        iteration_task_from_values(iteration_task_to_values(&record)).unwrap(),
        record
    );
    let mut changed = iteration_task_to_values(&record);
    for field in FIELDS {
        set(&mut changed, field, json!("changed"));
    }
    assert_ne!(changed, f);
    assert_eq!(iteration_task_to_values(&record), f);
}

/// Reject every wrong JSON variant, including null, in each field.
#[test]
fn every_field_rejects_wrong_types_without_parser_causes() {
    for field in FIELDS {
        for value in [
            json!(null),
            json!(false),
            json!([]),
            json!({"private": "SENSITIVE"}),
            json!("SENSITIVE"),
            json!(2.5),
        ] {
            // A string is the right type for every field except the phase,
            // and a float is the wrong type for every field.
            if field != "phase" && value.is_string() {
                continue;
            }
            let mut f = base();
            set(&mut f, field, value);
            let error = iteration_task_from_values(f).unwrap_err();
            assert!(error.to_string().starts_with(&format!("{field}:")));
            assert!(!error.to_string().contains("SENSITIVE"));
            assert!(error.parse_cause().is_none());
        }
        if field != "phase" {
            let mut f = base();
            set(&mut f, field, json!(42));
            rejects(f, field);
        }
    }
}

/// Keep the full identifier domain and reject prefixes or alphabet substitutions.
#[test]
fn full_identifier_domain_for_every_id_field() {
    for field in ["iteration_id", "task_id"] {
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

/// The phase keeps every value from 0 through u32::MAX and refuses wrapping.
#[test]
fn phase_is_a_checked_unsigned_32_bit_integer() {
    for phase in [0u64, 1, 3, 4, 2_147_483_648, u64::from(u32::MAX)] {
        let mut f = base();
        f.phase = json!(phase);
        let out = output(f.clone());
        assert_eq!(out.phase, json!(phase));
        assert_eq!(out.phase.as_u64(), Some(phase));
    }
    for value in [
        json!(u64::from(u32::MAX) + 1),
        json!(u64::MAX),
        json!(-1),
        json!(i64::MIN),
        json!(1.0),
        json!(0.0),
        json!(1e3),
        json!("1"),
        json!("4294967295"),
        json!(true),
    ] {
        let mut f = base();
        f.phase = value;
        let error = iteration_task_from_values(f).unwrap_err();
        assert_eq!(
            error.to_string(),
            "phase: expected an integer from 0 through 4294967295"
        );
        assert!(error.parse_cause().is_none());
    }
}

/// The join has no Gest ID, and the key fields give one record for one tuple.
#[test]
fn join_has_no_gest_id_and_phase_is_not_part_of_the_key() {
    let records: Value = serde_json::from_str(include_str!(
        "fixtures/gest_iteration_task_values/records.json"
    ))
    .unwrap();
    assert!(records["base"].get("gest_id").is_none());
    assert!(records["base"].get("id").is_none());
    for (field, value) in [
        ("iteration_id", json!("z".repeat(32))),
        ("task_id", json!("z".repeat(32))),
    ] {
        let mut other = base();
        set(&mut other, field, value);
        assert_ne!(
            iteration_task_from_values(other).unwrap(),
            iteration_task_from_values(base()).unwrap()
        );
    }
    // Two phases for one tuple give two different records. The caller keeps the
    // existing phase on a repeated add, as Gest does.
    let mut moved = base();
    moved.phase = json!(7);
    let moved = iteration_task_to_values(&iteration_task_from_values(moved).unwrap());
    assert_eq!(moved.iteration_id, base().iteration_id);
    assert_eq!(moved.task_id, base().task_id);
    assert_ne!(moved.phase, base().phase);
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
            let record = iteration_task_from_values(f.clone()).unwrap();
            set(&mut f, field, case["output"].clone());
            assert_eq!(iteration_task_to_values(&record), f);
            assert_eq!(
                iteration_task_from_values(iteration_task_to_values(&record)).unwrap(),
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
        let error = iteration_task_from_values(f).unwrap_err();
        assert_eq!(error.parse_cause(), Some(&expected));
        assert!(error.to_string().starts_with("created_at:"));
        assert!(!error.to_string().contains(input));
    }
}

/// Check that the source map names all 4 fields, existing controls and no later obligation.
#[test]
fn source_map_names_every_field_and_existing_control() {
    let map: Value = serde_json::from_str(include_str!(
        "fixtures/gest_iteration_task_values/source-map.json"
    ))
    .unwrap();
    let records: Value = serde_json::from_str(include_str!(
        "fixtures/gest_iteration_task_values/records.json"
    ))
    .unwrap();
    let this_file = include_str!("gest_iteration_task_values.rs");
    assert_eq!(map["source"], "src/store/repo/iteration.rs");
    assert_eq!(map["exact_inherited_tests_ported"], 0);
    assert_eq!(map["key_tuple"], json!(["iteration_id", "task_id"]));
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
    // The governing addendum lists no primitive test for this join.
    assert_eq!(map["later_exact_api_obligations"], json!([]));
    assert_eq!(
        map["governing_addendum_sha256"],
        "afddd1c2f5e905f72126209605737cc80e63791870d3ce8bd59f26c9589d8454"
    );
}
