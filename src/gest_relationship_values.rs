//! Convert 8 explicit decoded relationship fields into owned validated records.
//!
//! A relationship is one directed row from a source entity to a target entity.
//! Callers own raw admission, field presence, unknown fields, budgets and history.
//! Callers also own endpoint existence, the uniqueness of the directed tuple and
//! the inverse row, because each of those checks needs other records.
//! This module reads no clock and performs no storage, reference lookup or ID generation.
//! `chrono` parses the two timestamps as RFC3339, and `relationship_to_values` returns them in canonical UTC.

use crate::gest_value_validation::{ValidationError, entity_type, identifier, string, timestamp};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::fmt;

/// Holds the complete owned input and output fields.
///
/// The caller must give every field explicitly.
#[derive(Clone, Debug, PartialEq)]
pub struct RelationshipValueInputs {
    /// Full relationship ID, called `id` upstream.
    pub gest_id: Value,
    /// Relationship type: `blocked-by`, `blocks`, `child-of`, `parent-of` or `relates-to`, in any case.
    pub rel_type: Value,
    /// Full ID of the source entity.
    pub source_id: Value,
    /// Kind of the source entity: `artifact`, `iteration` or `task`, in any case.
    pub source_type: Value,
    /// Full ID of the target entity.
    pub target_id: Value,
    /// Kind of the target entity: `artifact`, `iteration` or `task`, in any case.
    pub target_type: Value,
    /// RFC3339 creation timestamp with representable canonical UTC output.
    pub created_at: Value,
    /// RFC3339 modification timestamp with representable canonical UTC output.
    pub updated_at: Value,
}

/// Holds validated relationship data in private fields.
///
/// Construct it with [`relationship_from_values`].
#[derive(Clone, Debug, PartialEq)]
pub struct RelationshipValues {
    /// Full relationship ID, called `id` upstream.
    gest_id: String,
    /// Canonical lowercase relationship type.
    rel_type: &'static str,
    /// Full ID of the source entity.
    source_id: String,
    /// Canonical lowercase source entity kind.
    source_type: &'static str,
    /// Full ID of the target entity.
    target_id: String,
    /// Canonical lowercase target entity kind.
    target_type: &'static str,
    /// RFC3339 creation timestamp with representable canonical UTC output.
    created_at: DateTime<Utc>,
    /// RFC3339 modification timestamp with representable canonical UTC output.
    updated_at: DateTime<Utc>,
}

/// Reports a failed field without its input, with an optional typed `chrono` parse cause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelationshipValueError {
    /// Complete internal failure, including the typed cause.
    failure: ValidationError,
}

impl From<ValidationError> for RelationshipValueError {
    /// Preserve every field of the shared validation failure.
    fn from(failure: ValidationError) -> Self {
        Self { failure }
    }
}

impl RelationshipValueError {
    /// Return the `chrono` parse error of a timestamp, without the supplied data.
    ///
    /// # Returns
    /// The initial or canonical-output parse cause, or `None` for other failures.
    /// The alloc-only parser error of `chrono` does not implement `std::error::Error`.
    pub fn parse_cause(&self) -> Option<&chrono::ParseError> {
        self.failure.parse_cause.as_ref()
    }
}

impl fmt::Display for RelationshipValueError {
    /// Render only the field and static reason, without supplied data.
    ///
    /// # Errors
    /// Returns the formatter error if writing fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.failure.field, self.failure.reason)
    }
}

impl std::error::Error for RelationshipValueError {}

/// Parse a relationship type as the pinned source enum does.
///
/// The source lowercases the whole string with Unicode rules and then matches
/// the five hyphenated spellings. Underscore spellings and whitespace fail.
///
/// # Arguments
/// * `value` - Already admitted decoded relationship type value.
///
/// # Returns
/// The canonical lowercase spelling of the relationship type.
///
/// # Errors
/// Returns [`ValidationError`] for a non-string value or any other spelling.
fn rel_type(value: Value) -> Result<&'static str, ValidationError> {
    match string(value, "rel_type")?.to_lowercase().as_str() {
        "blocked-by" => Ok("blocked-by"),
        "blocks" => Ok("blocks"),
        "child-of" => Ok("child-of"),
        "parent-of" => Ok("parent-of"),
        "relates-to" => Ok("relates-to"),
        _ => Err(ValidationError {
            field: "rel_type",
            reason: "expected blocked-by, blocks, child-of, parent-of or relates-to",
            parse_cause: None,
        }),
    }
}

/// Validate every explicit decoded field and preserve the complete relationship value.
///
/// # Arguments
/// * `fields` - Complete fields admitted by the caller before this conversion.
///
/// # Returns
/// An owned validated record with private fields. The relationship type and
/// both entity types take their lowercase spellings. The record keeps the
/// direction as given and does not invert it. The conversion imposes no order
/// on the timestamps and does not compare the record with any other record.
///
/// # Errors
/// Returns [`RelationshipValueError`] for wrong types, invalid full IDs, an
/// unknown relationship type, an entity type other than artifact, iteration or
/// task, invalid timestamp input or UTC output outside the canonical RFC3339 domain.
///
/// # Examples
/// ```
/// use gossil::gest_relationship_values::{
///     RelationshipValueInputs, relationship_from_values, relationship_to_values,
/// };
/// use serde_json::json;
///
/// let fields = RelationshipValueInputs {
///     gest_id: json!("klmnopqrstuvwxyzklmnopqrstuvwxyz"),
///     rel_type: json!("Child-Of"),
///     source_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
///     source_type: json!("task"),
///     target_id: json!("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz"),
///     target_type: json!("TASK"),
///     created_at: json!("2026-11-18T07:00:00-05:00"),
///     updated_at: json!("2026-11-18T12:00:00Z"),
/// };
/// let record = relationship_from_values(fields).unwrap();
/// let output = relationship_to_values(&record);
/// assert_eq!(output.rel_type, json!("child-of"));
/// assert_eq!(output.target_type, json!("task"));
/// assert_eq!(output.created_at, json!("2026-11-18T12:00:00+00:00"));
/// assert_eq!(relationship_from_values(output).unwrap(), record);
/// ```
pub fn relationship_from_values(
    fields: RelationshipValueInputs,
) -> Result<RelationshipValues, RelationshipValueError> {
    let gest_id = identifier(fields.gest_id, "gest_id")?;
    let rel_type = rel_type(fields.rel_type)?;
    let source_id = identifier(fields.source_id, "source_id")?;
    let source_type = entity_type(fields.source_type, "source_type")?;
    let target_id = identifier(fields.target_id, "target_id")?;
    let target_type = entity_type(fields.target_type, "target_type")?;
    let created_at = timestamp(fields.created_at, "created_at")?;
    let updated_at = timestamp(fields.updated_at, "updated_at")?;
    Ok(RelationshipValues {
        gest_id,
        rel_type,
        source_id,
        source_type,
        target_id,
        target_type,
        created_at,
        updated_at,
    })
}

/// Return all 8 fields as independent owned values.
///
/// # Arguments
/// * `record` - A record validated by [`relationship_from_values`].
///
/// # Returns
/// Complete model values with lowercase type spellings and UTC RFC3339
/// timestamp strings. UTC uses `+00:00` and at most nine fractional digits.
/// Output can be validated again, and a change to the output does not change
/// the record.
pub fn relationship_to_values(record: &RelationshipValues) -> RelationshipValueInputs {
    RelationshipValueInputs {
        gest_id: Value::from(record.gest_id.clone()),
        rel_type: Value::from(record.rel_type),
        source_id: Value::from(record.source_id.clone()),
        source_type: Value::from(record.source_type),
        target_id: Value::from(record.target_id.clone()),
        target_type: Value::from(record.target_type),
        created_at: Value::from(record.created_at.to_rfc3339()),
        updated_at: Value::from(record.updated_at.to_rfc3339()),
    }
}
