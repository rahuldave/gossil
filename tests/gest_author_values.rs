//! Test public author conversion. No database, CLI or native writer is involved.
use gossil::gest_author_values::{AuthorValueInputs, author_from_values, author_to_values};
use serde_json::json;
/// Build complete explicit author inputs for conversion controls.
fn fields(kind: &str, email: serde_json::Value) -> AuthorValueInputs {
    AuthorValueInputs {
        gest_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
        author_type: json!(kind),
        name: json!(" Alice\t雪 "),
        email,
        created_at: json!("2026-11-19T01:02:03.000000001+02:00"),
        updated_at: json!("2020-01-01T00:00:00Z"),
    }
}
#[test]
/// Check inherited author kinds and round trips for null, empty and exact strings.
fn inherited_author_kind_null_empty_and_exact_strings_roundtrip() {
    for kind in ["Human", "AGENT"] {
        for email in [json!(null), json!(""), json!(" Alice@EXAMPLE.org ")] {
            let record = author_from_values(fields(kind, email.clone())).unwrap();
            let out = author_to_values(&record);
            assert_eq!(out.author_type, json!(kind.to_lowercase()));
            assert_eq!(out.email, email);
            assert_eq!(out.name, json!(" Alice\t雪 "));
            assert_eq!(out.created_at, json!("2026-11-18T23:02:03.000000001+00:00"));
            assert_eq!(author_from_values(out).unwrap(), record);
        }
    }
    for kind in [" human", "human ", "robot", ""] {
        assert!(author_from_values(fields(kind, json!(null))).is_err());
    }
}
#[test]
/// Check that field errors keep the original parser cause without echoing input.
fn author_field_refusals_keep_real_parse_cause_and_do_not_echo_input() {
    let mut input = fields("human", json!(7));
    assert!(
        author_from_values(input.clone())
            .unwrap_err()
            .parse_cause()
            .is_none()
    );
    input.email = json!(null);
    input.created_at = json!("secret bad timestamp");
    let error = author_from_values(input).unwrap_err();
    assert!(error.parse_cause().is_some());
    assert!(!error.to_string().contains("secret"));
}

/// Replace one field to exercise its validation boundary.
fn replace(input: &mut AuthorValueInputs, field: &str, value: serde_json::Value) {
    match field {
        "gest_id" => input.gest_id = value,
        "author_type" => input.author_type = value,
        "name" => input.name = value,
        "email" => input.email = value,
        "created_at" => input.created_at = value,
        "updated_at" => input.updated_at = value,
        _ => panic!("fixture field"),
    }
}

#[test]
/// Check that all six fields reject wrong JSON types without echoing values.
fn all_six_fields_refuse_wrong_json_types_without_echoing_values() {
    for field in [
        "gest_id",
        "author_type",
        "name",
        "email",
        "created_at",
        "updated_at",
    ] {
        for value in [
            json!(null),
            json!(17),
            json!(true),
            json!(["private payload"]),
            json!({"secret": "payload"}),
        ] {
            if field == "email" && value.is_null() {
                continue;
            }
            let mut input = fields("human", json!(null));
            replace(&mut input, field, value);
            let failure = author_from_values(input).unwrap_err();
            assert!(failure.to_string().starts_with(field));
            assert!(!failure.to_string().contains("private"));
            assert!(!failure.to_string().contains("secret"));
            assert!(failure.parse_cause().is_none());
        }
    }
}

#[test]
/// Check author identity requires the full canonical alphabet without generation.
fn author_identity_requires_the_full_canonical_alphabet_without_generation() {
    for id in ["k".repeat(32), "z".repeat(32)] {
        let mut input = fields("agent", json!(null));
        input.gest_id = json!(id);
        let output = author_to_values(&author_from_values(input).unwrap());
        assert_eq!(output.gest_id, json!(id));
    }
    for id in [
        "k".repeat(31),
        "k".repeat(33),
        "K".repeat(32),
        "0".repeat(32),
        "j".repeat(32),
        "雪".repeat(32),
        format!(" {}", "k".repeat(31)),
    ] {
        let mut input = fields("human", json!(null));
        input.gest_id = json!(id);
        let failure = author_from_values(input).unwrap_err();
        assert!(failure.to_string().starts_with("gest_id:"));
        assert!(failure.parse_cause().is_none());
    }
}

#[test]
/// Check unrestricted strings and unordered explicit instants are preserved.
fn unrestricted_strings_and_unordered_explicit_instants_are_preserved() {
    for name in ["", " \t雪\n\0 "] {
        for email in [json!(null), json!(""), json!(" \n\0not-an-email雪 ")] {
            let mut input = fields("hUmAn", email.clone());
            input.name = json!(name);
            let output = author_to_values(&author_from_values(input).unwrap());
            assert_eq!(output.name, json!(name));
            assert_eq!(output.email, email);
            assert_eq!(output.author_type, json!("human"));
            assert_eq!(
                output.created_at,
                json!("2026-11-18T23:02:03.000000001+00:00")
            );
            assert_eq!(output.updated_at, json!("2020-01-01T00:00:00+00:00"));
        }
    }
}

#[test]
/// Check that each timestamp retains parser causes and rejects noncanonical UTC output.
fn each_timestamp_retains_real_parse_causes_and_refuses_noncanonical_utc_output() {
    for field in ["created_at", "updated_at"] {
        for invalid in [
            "private malformed timestamp",
            "0000-01-01T00:00:00+00:01",
            "9999-12-31T23:59:59-00:01",
        ] {
            let mut input = fields("agent", json!(null));
            replace(&mut input, field, json!(invalid));
            let failure = author_from_values(input).unwrap_err();
            assert!(failure.to_string().starts_with(field));
            assert!(!failure.to_string().contains(invalid));
            assert!(failure.parse_cause().is_some());
        }
    }
}

#[test]
/// Check returned values are owned and cannot change the validated author.
fn returned_values_are_owned_and_cannot_change_the_validated_author() {
    let original = author_from_values(fields("agent", json!("exact email"))).unwrap();
    let saved = author_to_values(&original);
    let mut output = author_to_values(&original);
    output.gest_id = json!("z".repeat(32));
    output.author_type = json!("human");
    output.name = json!("changed");
    output.email = json!(null);
    output.created_at = json!("2000-01-01T00:00:00Z");
    output.updated_at = json!("2001-01-01T00:00:00Z");
    assert_ne!(output, saved);
    assert_eq!(author_to_values(&original), saved);
}
