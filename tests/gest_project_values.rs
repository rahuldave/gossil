//! New converter behavior controls; zero exact inherited source API ports.
use gossil::gest_project_values::{ProjectValueInputs, project_from_values, project_to_values};
use serde_json::{Value, json};

/// Read an explicit complete fixture, without testing raw JSON admission.
fn base() -> ProjectValueInputs {
    let v: Value =
        serde_json::from_str(include_str!("fixtures/gest_project_values/records.json")).unwrap();
    let v = &v["base"];
    assert_eq!(v.as_object().unwrap().len(), 4);
    ProjectValueInputs {
        gest_id: v["gest_id"].clone(),
        archived_at: v["archived_at"].clone(),
        created_at: v["created_at"].clone(),
        updated_at: v["updated_at"].clone(),
    }
}

/// Change one explicit field for type and boundary controls.
fn set(f: &mut ProjectValueInputs, field: &str, value: Value) {
    match field {
        "gest_id" => f.gest_id = value,
        "archived_at" => f.archived_at = value,
        "created_at" => f.created_at = value,
        "updated_at" => f.updated_at = value,
        _ => panic!("unknown fixture field"),
    }
}

/// Observe every output field and owned-record round trips.
#[test]
fn complete_output_round_trips_and_is_independent() {
    let f = base();
    let record = project_from_values(f.clone()).unwrap();
    assert_eq!(project_to_values(&record), f);
    assert_eq!(
        project_from_values(project_to_values(&record)).unwrap(),
        record
    );
    let mut changed = project_to_values(&record);
    changed.gest_id = json!("changed");
    changed.archived_at = json!("changed");
    changed.created_at = json!("changed");
    changed.updated_at = json!("changed");
    assert_ne!(changed, f);
    assert_eq!(project_to_values(&record), f);
}

/// Check every field against non-string values without exposing input.
#[test]
fn every_field_rejects_wrong_types_without_parser_causes() {
    for field in ["gest_id", "archived_at", "created_at", "updated_at"] {
        for value in [
            json!(false),
            json!(42),
            json!([]),
            json!({"private": "SENSITIVE"}),
        ] {
            let mut f = base();
            set(&mut f, field, value);
            let error = project_from_values(f).unwrap_err();
            assert!(error.to_string().starts_with(&format!("{field}:")));
            assert!(!error.to_string().contains("SENSITIVE"));
            assert!(error.parse_cause().is_none());
        }
    }
}

/// Keep full identifier bounds and reject prefixes or alphabet substitutions.
#[test]
fn full_identifier_domain_and_boundaries() {
    for id in ["k".repeat(32), "z".repeat(32), "klmnopqrstuvwxyz".repeat(2)] {
        let mut f = base();
        f.gest_id = json!(id);
        assert_eq!(
            project_to_values(&project_from_values(f).unwrap()).gest_id,
            json!(id)
        );
    }
    for id in [
        "".to_string(),
        "k".repeat(31),
        "k".repeat(33),
        "K".repeat(32),
        "j".repeat(32),
        "é".repeat(16),
        format!("{}0", "k".repeat(31)),
    ] {
        let mut f = base();
        f.gest_id = json!(id);
        let error = project_from_values(f).unwrap_err();
        assert!(error.to_string().starts_with("gest_id:"));
        assert!(error.parse_cause().is_none());
    }
}

/// Normalize every timestamp field and preserve source precision and leap seconds.
#[test]
fn each_timestamp_normalizes_and_round_trips() {
    for field in ["archived_at", "created_at", "updated_at"] {
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
            let record = project_from_values(f.clone()).unwrap();
            set(&mut f, field, json!(expected));
            assert_eq!(project_to_values(&record), f);
            assert_eq!(
                project_from_values(project_to_values(&record)).unwrap(),
                record
            );
        }
    }
}

/// Preserve each original parser cause with safe field-only display.
#[test]
fn every_timestamp_retains_initial_and_canonical_parse_causes() {
    for field in ["archived_at", "created_at", "updated_at"] {
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
            let error = project_from_values(f).unwrap_err();
            assert_eq!(error.parse_cause(), Some(&expected));
            assert!(error.to_string().starts_with(&format!("{field}:")));
            assert!(!error.to_string().contains("SENSITIVE"));
            assert_eq!(error.clone(), error);
        }
    }
}

/// Required fields do not silently use null defaults.
#[test]
fn required_fields_reject_null() {
    for field in ["gest_id", "created_at", "updated_at"] {
        let mut f = base();
        set(&mut f, field, Value::Null);
        let error = project_from_values(f).unwrap_err();
        assert!(error.to_string().starts_with(&format!("{field}:")));
        assert!(error.parse_cause().is_none());
    }
}

/// Archive state stays explicit data and never imposes time ordering.
#[test]
fn archive_null_and_present_are_distinct() {
    let active = base();
    let mut archived = base();
    archived.archived_at = json!("2000-01-01T00:00:00+00:00");
    let active_record = project_from_values(active.clone()).unwrap();
    let archived_record = project_from_values(archived.clone()).unwrap();
    assert_ne!(active_record, archived_record);
    assert_eq!(project_to_values(&active_record), active);
    assert_eq!(project_to_values(&archived_record), archived);
}
