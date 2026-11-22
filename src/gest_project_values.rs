//! Complete owned conversion of the 4 canonical project fields.
//!
//! Callers own raw admission, presence, unknown fields, budgets and history.
//! Conversion performs no storage, reference lookup, clock read or ID generation.

use crate::gest_model::primitives::Id;
use crate::gest_value_validation::{ValidationError, identifier, timestamp};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::fmt;

/// Complete explicit input and output fields; null is supplied as a value.
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectValueInputs {
    /// Full 32-letter identifier, called `id` upstream.
    pub gest_id: Value,
    /// Explicit null or an RFC3339 archive timestamp.
    pub archived_at: Value,
    /// RFC3339 creation timestamp with canonical UTC output.
    pub created_at: Value,
    /// RFC3339 modification timestamp with canonical UTC output.
    pub updated_at: Value,
}

/// Privately validated project data; construct with [`project_from_values`].
#[derive(Clone, Debug, PartialEq)]
pub struct ProjectValues {
    /// Full 32-letter identifier, called `id` upstream.
    gest_id: Id,
    /// Explicit null or an RFC3339 archive timestamp.
    archived_at: Option<DateTime<Utc>>,
    /// RFC3339 creation timestamp with canonical UTC output.
    created_at: DateTime<Utc>,
    /// RFC3339 modification timestamp with canonical UTC output.
    updated_at: DateTime<Utc>,
}

/// Safe field failure retaining the actual typed timestamp parser cause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectValueError {
    /// Complete shared failure without discarding the typed cause.
    failure: ValidationError,
}

impl From<ValidationError> for ProjectValueError {
    /// Transfer all failure fields unchanged.
    fn from(failure: ValidationError) -> Self {
        Self { failure }
    }
}

impl ProjectValueError {
    /// Inspect the actual input or canonical-output timestamp parsing failure.
    ///
    /// # Returns
    /// The original typed chrono cause, or `None` for non-parser failures.
    /// Chrono's alloc-only parser error does not implement `std::error::Error`.
    pub fn parse_cause(&self) -> Option<&chrono::ParseError> {
        self.failure.parse_cause.as_ref()
    }
}

impl fmt::Display for ProjectValueError {
    /// Render the field and static reason without echoing supplied data.
    ///
    /// # Errors
    /// Returns the formatter error if writing fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.failure.field, self.failure.reason)
    }
}

impl std::error::Error for ProjectValueError {}

/// Validate every explicit decoded field into an owned project record.
///
/// # Arguments
/// * `fields` - Complete model fields after caller-owned raw admission.
///
/// # Returns
/// A privately validated record; timestamps normalize to UTC without ordering.
///
/// # Errors
/// Returns [`ProjectValueError`] for wrong types, invalid full IDs, invalid timestamps
/// or UTC output outside the canonical RFC3339 domain.
/// The local checkout root is excluded from this canonical record.
pub fn project_from_values(fields: ProjectValueInputs) -> Result<ProjectValues, ProjectValueError> {
    let gest_id = identifier(fields.gest_id, "gest_id")?;
    let archived_at = if fields.archived_at.is_null() {
        None
    } else {
        Some(timestamp(fields.archived_at, "archived_at")?)
    };
    let created_at = timestamp(fields.created_at, "created_at")?;
    let updated_at = timestamp(fields.updated_at, "updated_at")?;
    Ok(ProjectValues {
        gest_id,
        archived_at,
        created_at,
        updated_at,
    })
}

/// Return all 4 fields as independent owned values.
///
/// # Arguments
/// * `record` - A record validated by [`project_from_values`].
///
/// # Returns
/// Complete fields with UTC RFC3339 timestamps using `+00:00`.
/// Output validates again; mutating output does not change the record.
pub fn project_to_values(record: &ProjectValues) -> ProjectValueInputs {
    ProjectValueInputs {
        gest_id: Value::from(record.gest_id.to_string()),
        archived_at: record
            .archived_at
            .map(|dt| Value::from(dt.to_rfc3339()))
            .unwrap_or(Value::Null),
        created_at: Value::from(record.created_at.to_rfc3339()),
        updated_at: Value::from(record.updated_at.to_rfc3339()),
    }
}
