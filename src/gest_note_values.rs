//! Convert 7 explicit decoded note fields into owned validated records.
//!
//! Callers own raw admission, field presence, unknown fields, budgets and history.
//! Callers also check that the entity and the author exist, because that check needs other records.
//! This module reads no clock and performs no storage, reference lookup or ID generation.
//! `chrono` parses the two timestamps as RFC3339, and `note_to_values` returns them in canonical UTC.

use crate::gest_model::primitives::{EntityType, Id};
use crate::gest_value_validation::{ValidationError, entity_type, identifier, string, timestamp};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::fmt;

/// Holds the complete owned input and output fields.
///
/// The caller must give every field explicitly, including a null author.
#[derive(Clone, Debug, PartialEq)]
pub struct NoteValueInputs {
    /// Full note ID, called `id` upstream.
    pub gest_id: Value,
    /// Full ID of the task, artifact or iteration that the note is attached to.
    pub entity_id: Value,
    /// Kind of the entity: `artifact`, `iteration` or `task`, in any case.
    pub entity_type: Value,
    /// Full ID of the author, or JSON null when the author is unknown.
    pub author_id: Value,
    /// Exact body text, including empty strings, Unicode and line endings.
    pub body: Value,
    /// RFC3339 creation timestamp with representable canonical UTC output.
    pub created_at: Value,
    /// RFC3339 modification timestamp with representable canonical UTC output.
    pub updated_at: Value,
}

/// Holds validated note data in private fields.
///
/// Construct it with [`note_from_values`].
#[derive(Clone, Debug, PartialEq)]
pub struct NoteValues {
    /// Full note ID, called `id` upstream.
    pub(crate) gest_id: Id,
    /// Full ID of the entity that the note is attached to.
    pub(crate) entity_id: Id,
    /// Canonical lowercase entity kind.
    pub(crate) entity_type: EntityType,
    /// Full author ID, or `None` when the author is unknown.
    pub(crate) author_id: Option<Id>,
    /// Exact body text.
    pub(crate) body: String,
    /// RFC3339 creation timestamp with representable canonical UTC output.
    pub(crate) created_at: DateTime<Utc>,
    /// RFC3339 modification timestamp with representable canonical UTC output.
    pub(crate) updated_at: DateTime<Utc>,
}

/// Reports a failed field without its input, with an optional typed `chrono` parse cause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NoteValueError {
    /// Complete internal failure, including the typed cause.
    failure: ValidationError,
}

impl From<ValidationError> for NoteValueError {
    /// Preserve every field of the shared validation failure.
    fn from(failure: ValidationError) -> Self {
        Self { failure }
    }
}

impl NoteValueError {
    /// Return the `chrono` parse error of a timestamp, without the supplied data.
    ///
    /// # Returns
    /// The initial or canonical-output parse cause, or `None` for other failures.
    /// The alloc-only parser error of `chrono` does not implement `std::error::Error`.
    pub fn parse_cause(&self) -> Option<&chrono::ParseError> {
        self.failure.parse_cause.as_ref()
    }
}

impl fmt::Display for NoteValueError {
    /// Render only the field and static reason, without supplied data.
    ///
    /// # Errors
    /// Returns the formatter error if writing fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.failure.field, self.failure.reason)
    }
}

impl std::error::Error for NoteValueError {}

/// Validate every explicit decoded field and preserve the complete note value.
///
/// # Arguments
/// * `fields` - Complete fields admitted by the caller before this conversion.
///
/// # Returns
/// An owned validated record with private fields. The body keeps its exact
/// bytes, a null author stays null, and the entity type takes its lowercase
/// spelling. The conversion imposes no order on the timestamps.
///
/// # Errors
/// Returns [`NoteValueError`] for wrong types, invalid full IDs, an entity type
/// other than artifact, iteration or task, invalid timestamp input or UTC output
/// outside the canonical RFC3339 domain.
///
/// # Examples
/// ```
/// use gossil::gest_note_values::{NoteValueInputs, note_from_values, note_to_values};
/// use serde_json::json;
///
/// let fields = NoteValueInputs {
///     gest_id: json!("klmnopqrstuvwxyzklmnopqrstuvwxyz"),
///     entity_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
///     entity_type: json!("Task"),
///     author_id: json!(null),
///     body: json!("Checked the *build*.\n"),
///     created_at: json!("2026-11-18T07:00:00-05:00"),
///     updated_at: json!("2026-11-18T12:00:00Z"),
/// };
/// let record = note_from_values(fields).unwrap();
/// let output = note_to_values(&record);
/// assert_eq!(output.entity_type, json!("task"));
/// assert_eq!(output.author_id, json!(null));
/// assert_eq!(output.created_at, json!("2026-11-18T12:00:00+00:00"));
/// assert_eq!(note_from_values(output).unwrap(), record);
/// ```
pub fn note_from_values(fields: NoteValueInputs) -> Result<NoteValues, NoteValueError> {
    let gest_id = identifier(fields.gest_id, "gest_id")?;
    let entity_id = identifier(fields.entity_id, "entity_id")?;
    let entity_type = entity_type(fields.entity_type, "entity_type")?;
    let author_id = match fields.author_id {
        Value::Null => None,
        value => Some(identifier(value, "author_id")?),
    };
    let body = string(fields.body, "body")?;
    let created_at = timestamp(fields.created_at, "created_at")?;
    let updated_at = timestamp(fields.updated_at, "updated_at")?;
    Ok(NoteValues {
        gest_id,
        entity_id,
        entity_type,
        author_id,
        body,
        created_at,
        updated_at,
    })
}

/// Return all 7 fields as independent owned values.
///
/// # Arguments
/// * `record` - A record validated by [`note_from_values`].
///
/// # Returns
/// Complete model values with UTC RFC3339 timestamp strings and an explicit
/// JSON null for an unknown author. UTC uses `+00:00` and at most nine
/// fractional digits. Output can be validated again, and a change to the output
/// does not change the record.
pub fn note_to_values(record: &NoteValues) -> NoteValueInputs {
    NoteValueInputs {
        gest_id: Value::from(record.gest_id.to_string()),
        entity_id: Value::from(record.entity_id.to_string()),
        entity_type: Value::from(record.entity_type.to_string()),
        author_id: record
            .author_id
            .as_ref()
            .map_or(Value::Null, |id| Value::from(id.to_string())),
        body: Value::from(record.body.clone()),
        created_at: Value::from(record.created_at.to_rfc3339()),
        updated_at: Value::from(record.updated_at.to_rfc3339()),
    }
}
