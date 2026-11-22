//! Convert 4 explicit decoded tag fields into owned validated records.
//!
//! Callers own raw admission, field presence, unknown fields, budgets and history.
//! Callers also own label uniqueness across records, because that check needs every live tag record.
//! This module reads no clock and performs no storage, reference lookup or ID generation.
//! `chrono` parses the two timestamps as RFC3339, and `tag_to_values` returns them in canonical UTC.

use crate::gest_model::primitives::Id;
use crate::gest_value_validation::{ValidationError, identifier, string, timestamp};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::fmt;

/// Holds the complete owned input and output fields.
///
/// The caller must give every field explicitly.
#[derive(Clone, Debug, PartialEq)]
pub struct TagValueInputs {
    /// Full tag ID, called `id` upstream.
    pub gest_id: Value,
    /// Exact case-sensitive label, including empty strings and Unicode.
    pub label: Value,
    /// RFC3339 creation timestamp with representable canonical UTC output.
    pub created_at: Value,
    /// RFC3339 modification timestamp with representable canonical UTC output.
    pub updated_at: Value,
}

/// Holds validated tag data in private fields.
///
/// Construct it with [`tag_from_values`].
#[derive(Clone, Debug, PartialEq)]
pub struct TagValues {
    /// Full tag ID, called `id` upstream.
    pub(crate) gest_id: Id,
    /// Exact case-sensitive label, including empty strings and Unicode.
    pub(crate) label: String,
    /// RFC3339 creation timestamp with representable canonical UTC output.
    pub(crate) created_at: DateTime<Utc>,
    /// RFC3339 modification timestamp with representable canonical UTC output.
    pub(crate) updated_at: DateTime<Utc>,
}

/// Reports a failed field without its input, with an optional typed `chrono` parse cause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagValueError {
    /// Complete internal failure, including the typed cause.
    failure: ValidationError,
}

impl From<ValidationError> for TagValueError {
    /// Preserve every shared validation failure field.
    fn from(failure: ValidationError) -> Self {
        Self { failure }
    }
}

impl TagValueError {
    /// Return the `chrono` parse error of a timestamp, without the supplied data.
    ///
    /// # Returns
    /// The initial or canonical-output parse cause, or `None` for other failures.
    /// The alloc-only parser error of `chrono` does not implement `std::error::Error`.
    pub fn parse_cause(&self) -> Option<&chrono::ParseError> {
        self.failure.parse_cause.as_ref()
    }
}

impl fmt::Display for TagValueError {
    /// Render only the field and static reason, without supplied data.
    ///
    /// # Errors
    /// Returns the formatter error if writing fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.failure.field, self.failure.reason)
    }
}

impl std::error::Error for TagValueError {}

/// Validate every explicit decoded field and preserve the complete tag value.
///
/// # Arguments
/// * `fields` - Complete fields admitted by the caller before this conversion.
///
/// # Returns
/// An owned validated record with private fields. The label keeps its exact
/// content and case. The conversion imposes no order on the timestamps and does
/// not compare the label with any other record.
///
/// # Errors
/// Returns [`TagValueError`] for wrong types, invalid full IDs, invalid timestamp
/// input or UTC output outside the canonical RFC3339 domain.
///
/// # Examples
/// ```
/// use gossil::gest_tag_values::{TagValueInputs, tag_from_values, tag_to_values};
/// use serde_json::json;
///
/// let fields = TagValueInputs {
///     gest_id: json!("klmnopqrstuvwxyzklmnopqrstuvwxyz"),
///     label: json!("Urgent"),
///     created_at: json!("2026-11-18T07:00:00-05:00"),
///     updated_at: json!("2026-11-18T12:00:00Z"),
/// };
/// let record = tag_from_values(fields).unwrap();
/// let output = tag_to_values(&record);
/// assert_eq!(output.label, json!("Urgent"));
/// assert_eq!(output.created_at, json!("2026-11-18T12:00:00+00:00"));
/// assert_eq!(tag_from_values(output).unwrap(), record);
/// ```
pub fn tag_from_values(fields: TagValueInputs) -> Result<TagValues, TagValueError> {
    let gest_id = identifier(fields.gest_id, "gest_id")?;
    let label = string(fields.label, "label")?;
    let created_at = timestamp(fields.created_at, "created_at")?;
    let updated_at = timestamp(fields.updated_at, "updated_at")?;
    Ok(TagValues {
        gest_id,
        label,
        created_at,
        updated_at,
    })
}

/// Return all 4 fields as independent owned values.
///
/// # Arguments
/// * `record` - A record validated by [`tag_from_values`].
///
/// # Returns
/// Complete model values with UTC RFC3339 timestamp strings.
/// UTC uses `+00:00` and at most nine fractional digits. Output can be validated
/// again, and a change to the output does not change the record.
pub fn tag_to_values(record: &TagValues) -> TagValueInputs {
    TagValueInputs {
        gest_id: Value::from(record.gest_id.to_string()),
        label: Value::from(record.label.clone()),
        created_at: Value::from(record.created_at.to_rfc3339()),
        updated_at: Value::from(record.updated_at.to_rfc3339()),
    }
}
