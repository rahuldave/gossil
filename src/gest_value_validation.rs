//! Shared decoded-value checks used by the owned Gest model converters.
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::gest_model::primitives::{EntityType, Id};

/// A safe field failure with the actual typed timestamp parser cause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ValidationError {
    /// Field that failed validation.
    pub(crate) field: &'static str,
    /// Static explanation that does not echo input.
    pub(crate) reason: &'static str,
    /// Actual input or canonical-output chrono parsing failure.
    pub(crate) parse_cause: Option<chrono::ParseError>,
}

/// Extract a required string without imposing content restrictions.
///
/// # Arguments
/// * `value` - Already admitted decoded model value.
/// * `field` - Safe static field name for validation diagnostics.
///
/// # Returns
/// The exact owned string without content restrictions.
///
/// # Errors
/// Returns [`ValidationError`] for any non-string [`Value`].
pub(crate) fn string(value: Value, field: &'static str) -> Result<String, ValidationError> {
    match value {
        Value::String(s) => Ok(s),
        _ => Err(ValidationError {
            field,
            reason: "expected a string",
            parse_cause: None,
        }),
    }
}

/// Require the full upstream 32-byte lowercase `k` through `z` ID domain.
///
/// The check is the parser of [`Id`], so the converters and the model accept
/// the same identifiers.
///
/// # Arguments
/// * `value` - Already admitted decoded model identifier.
/// * `field` - Safe static field name for validation diagnostics.
///
/// # Returns
/// The identifier that the string encodes.
///
/// # Errors
/// Returns [`ValidationError`] for a non-string value, a length other than 32 bytes,
/// or any byte outside the identifier alphabet.
pub(crate) fn identifier(value: Value, field: &'static str) -> Result<Id, ValidationError> {
    string(value, field)?.parse().map_err(|_| ValidationError {
        field,
        reason: "expected exactly 32 letters from k through z",
        parse_cause: None,
    })
}

/// Parse a Gest entity type with the parser of [`EntityType`].
///
/// The source lowercases the whole string with Unicode rules and then matches
/// it, so `TASK` is accepted, and surrounding whitespace is not.
///
/// # Arguments
/// * `value` - Already admitted decoded entity type value.
/// * `field` - Safe static field name for validation diagnostics.
///
/// # Returns
/// The entity type `artifact`, `iteration` or `task`.
///
/// # Errors
/// Returns [`ValidationError`] for a non-string value or any other spelling.
pub(crate) fn entity_type(
    value: Value,
    field: &'static str,
) -> Result<EntityType, ValidationError> {
    string(value, field)?.parse().map_err(|_| ValidationError {
        field,
        reason: "expected entity type artifact, iteration or task",
        parse_cause: None,
    })
}

/// Parse a business timestamp and require UTC output that can be parsed again.
///
/// # Arguments
/// * `value` - Already admitted decoded timestamp value.
/// * `field` - Safe static field name for validation diagnostics.
///
/// # Returns
/// The parsed UTC instant, after checking its canonical representation.
///
/// # Errors
/// Returns [`ValidationError`] for a non-string value, a chrono RFC3339 parse failure,
/// or normalized UTC output outside the parser's RFC3339 domain.
pub(crate) fn timestamp(
    value: Value,
    field: &'static str,
) -> Result<DateTime<Utc>, ValidationError> {
    let s = string(value, field)?;
    let utc = DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|cause| ValidationError {
            field,
            reason: "expected a chrono-compatible RFC3339 timestamp",
            parse_cause: Some(cause),
        })?;
    // The adopted Gossil output contract is narrower than source parsing when
    // offset normalization crosses into a signed extended UTC year. This is
    // a canonical-representation refusal, not the artifact D-card grammar.
    if let Err(cause) = DateTime::parse_from_rfc3339(&utc.to_rfc3339()) {
        return Err(ValidationError {
            field,
            reason: "normalized UTC timestamp is outside the canonical RFC3339 output domain",
            parse_cause: Some(cause),
        });
    }
    Ok(utc)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Keep the complete ID alphabet and unrestricted string content.
    #[test]
    fn string_and_identifier_success_domains() {
        assert_eq!(string(json!("\u{0} é\n"), "text").unwrap(), "\u{0} é\n");
        assert_eq!(
            identifier(json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"), "id")
                .unwrap()
                .to_string(),
            "kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"
        );
        assert_eq!(
            identifier(json!("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"), "id")
                .unwrap()
                .to_string(),
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"
        );
    }

    /// Accept the three source spellings in any case and nothing else.
    #[test]
    fn entity_type_follows_the_source_enum() {
        for (input, expected) in [
            ("artifact", "artifact"),
            ("ITERATION", "iteration"),
            ("Task", "task"),
        ] {
            assert_eq!(
                entity_type(json!(input), "kind").unwrap().to_string(),
                expected
            );
        }
        for input in ["", "project", "note", " task", "task\n", "widget"] {
            let error = entity_type(json!(input), "kind").unwrap_err();
            assert_eq!(error.field, "kind");
            assert_eq!(
                error.reason,
                "expected entity type artifact, iteration or task"
            );
            assert!(error.parse_cause.is_none());
        }
        assert_eq!(
            entity_type(json!(null), "kind").unwrap_err().reason,
            "expected a string"
        );
    }

    /// Non-parser failures contain no invented parser cause.
    #[test]
    fn wrong_type_and_identifier_failures_have_no_parse_cause() {
        for error in [
            string(json!(null), "text").unwrap_err(),
            identifier(json!("short"), "id").unwrap_err(),
            timestamp(json!(42), "time").unwrap_err(),
        ] {
            assert!(error.parse_cause.is_none());
        }
    }

    /// Initial parsing retains the exact chrono cause.
    #[test]
    fn input_parse_failure_retains_actual_cause() {
        for value in ["SENSITIVE-invalid-time", "2026-02-30T12:00:00Z"] {
            let expected = DateTime::parse_from_rfc3339(value).unwrap_err();
            let error = timestamp(json!(value), "time").unwrap_err();
            assert_eq!(error.parse_cause.as_ref(), Some(&expected));
            assert_eq!(error.field, "time");
            assert_eq!(
                error.reason,
                "expected a chrono-compatible RFC3339 timestamp"
            );
        }
    }

    /// UTC canonical reparse retains the second actual chrono error.
    #[test]
    fn canonical_parse_failure_retains_actual_cause() {
        for value in ["0000-01-01T00:00:00+01:00", "9999-12-31T23:00:00-01:00"] {
            let utc = DateTime::parse_from_rfc3339(value)
                .unwrap()
                .with_timezone(&Utc);
            let expected = DateTime::parse_from_rfc3339(&utc.to_rfc3339()).unwrap_err();
            let error = timestamp(json!(value), "time").unwrap_err();
            assert_eq!(error.parse_cause.as_ref(), Some(&expected));
            assert_eq!(
                error.reason,
                "normalized UTC timestamp is outside the canonical RFC3339 output domain"
            );
        }
    }

    /// Successful normalization preserves the established UTC representation.
    #[test]
    fn timestamp_success_normalizes_without_ordering_or_clock() {
        assert_eq!(
            timestamp(json!("2026-11-18T07:00:00-05:00"), "time")
                .unwrap()
                .to_rfc3339(),
            "2026-11-18T12:00:00+00:00"
        );
    }
}
