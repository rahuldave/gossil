//! Convert 4 explicit decoded `entity_tag` fields into owned validated records.
//!
//! An `entity_tag` row attaches one tag to one task, artifact or iteration. It has
//! no Gest ID of its own. Its key is the tuple of entity type, entity ID and tag ID.
//!
//! Callers own raw admission, field presence, unknown fields, budgets and history.
//! Callers also own the key, the association incarnation and its lifecycle, and the
//! check that the entity and the tag exist. Each of those needs other records.
//! This module reads no clock and performs no storage, reference lookup or ID generation.
//! `chrono` parses the timestamp as RFC3339, and `entity_tag_to_values` returns it in canonical UTC.

use crate::gest_value_validation::{ValidationError, entity_type, identifier, timestamp};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::fmt;

/// Holds the complete owned input and output fields.
///
/// The caller must give every field explicitly. The row has no `gest_id`.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityTagValueInputs {
    /// Full ID of the tagged task, artifact or iteration.
    pub entity_id: Value,
    /// Kind of the tagged entity, one of `artifact`, `iteration` or `task` in any case.
    pub entity_type: Value,
    /// Full ID of the attached tag.
    pub tag_id: Value,
    /// RFC3339 attachment timestamp with representable canonical UTC output.
    pub created_at: Value,
}

/// Holds validated `entity_tag` data in private fields.
///
/// Construct it with [`entity_tag_from_values`].
#[derive(Clone, Debug, PartialEq)]
pub struct EntityTagValues {
    /// Full ID of the tagged entity.
    entity_id: String,
    /// Canonical lowercase entity kind.
    entity_type: &'static str,
    /// Full ID of the attached tag.
    tag_id: String,
    /// RFC3339 attachment timestamp with representable canonical UTC output.
    created_at: DateTime<Utc>,
}

/// Reports a failed field without its input, with an optional typed `chrono` parse cause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityTagValueError {
    /// Complete internal failure, including the typed cause.
    failure: ValidationError,
}

impl From<ValidationError> for EntityTagValueError {
    /// Preserve every shared validation failure field.
    fn from(failure: ValidationError) -> Self {
        Self { failure }
    }
}

impl EntityTagValueError {
    /// Return the `chrono` parse error of the timestamp, without the supplied data.
    ///
    /// # Returns
    /// The initial or canonical-output parse cause, or `None` for other failures.
    /// The alloc-only parser error of `chrono` does not implement `std::error::Error`.
    pub fn parse_cause(&self) -> Option<&chrono::ParseError> {
        self.failure.parse_cause.as_ref()
    }
}

impl fmt::Display for EntityTagValueError {
    /// Render only the field and static reason, without supplied data.
    ///
    /// # Errors
    /// Returns the formatter error if writing fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.failure.field, self.failure.reason)
    }
}

impl std::error::Error for EntityTagValueError {}

/// Validate every explicit decoded field and preserve the complete `entity_tag` value.
///
/// # Arguments
/// * `fields` - Complete fields admitted by the caller before this conversion.
///
/// # Returns
/// An owned validated record with private fields. The entity type takes its
/// lowercase spelling, so two inputs that differ only in the case of the entity
/// type give equal records and the same key tuple.
///
/// # Errors
/// Returns [`EntityTagValueError`] for wrong types, invalid full IDs, an entity
/// type other than artifact, iteration or task, invalid timestamp input or UTC
/// output outside the canonical RFC3339 domain.
///
/// # Examples
/// ```
/// use gossil::gest_entity_tag_values::{
///     EntityTagValueInputs, entity_tag_from_values, entity_tag_to_values,
/// };
/// use serde_json::json;
///
/// let fields = EntityTagValueInputs {
///     entity_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
///     entity_type: json!("Artifact"),
///     tag_id: json!("klmnopqrstuvwxyzklmnopqrstuvwxyz"),
///     created_at: json!("2026-11-18T07:00:00-05:00"),
/// };
/// let record = entity_tag_from_values(fields).unwrap();
/// let output = entity_tag_to_values(&record);
/// assert_eq!(output.entity_type, json!("artifact"));
/// assert_eq!(output.created_at, json!("2026-11-18T12:00:00+00:00"));
/// assert_eq!(entity_tag_from_values(output).unwrap(), record);
/// ```
pub fn entity_tag_from_values(
    fields: EntityTagValueInputs,
) -> Result<EntityTagValues, EntityTagValueError> {
    let entity_id = identifier(fields.entity_id, "entity_id")?;
    let entity_type = entity_type(fields.entity_type, "entity_type")?;
    let tag_id = identifier(fields.tag_id, "tag_id")?;
    let created_at = timestamp(fields.created_at, "created_at")?;
    Ok(EntityTagValues {
        entity_id,
        entity_type,
        tag_id,
        created_at,
    })
}

/// Return all 4 fields as independent owned values.
///
/// # Arguments
/// * `record` - A record validated by [`entity_tag_from_values`].
///
/// # Returns
/// Complete model values with the lowercase entity type and a UTC RFC3339
/// timestamp string. UTC uses `+00:00` and at most nine fractional digits.
/// Output can be validated again, and a change to the output does not change
/// the record.
pub fn entity_tag_to_values(record: &EntityTagValues) -> EntityTagValueInputs {
    EntityTagValueInputs {
        entity_id: Value::from(record.entity_id.clone()),
        entity_type: Value::from(record.entity_type),
        tag_id: Value::from(record.tag_id.clone()),
        created_at: Value::from(record.created_at.to_rfc3339()),
    }
}
