//! Convert 9 explicit decoded iteration fields into owned validated records.
//!
//! Callers own raw admission, field presence, unknown fields, budgets and history.
//! This module reads no clock and performs no storage, reference lookup or ID generation.
//! Business timestamps use chrono RFC3339 parsing and canonical UTC output.

use crate::gest_value_validation::{ValidationError, identifier, string, timestamp};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::fmt;

/// Holds the complete owned input and output fields.
///
/// The caller must give every field explicitly.
#[derive(Clone, Debug, PartialEq)]
pub struct IterationValueInputs {
    /// Full iteration ID, called `id` upstream.
    pub gest_id: Value,
    /// Full project ID.
    pub project_id: Value,
    /// Exact title string, including empty strings and Unicode.
    pub title: Value,
    /// Case-insensitive `active`, `completed` or `cancelled`.
    pub status: Value,
    /// Exact description string, including empty strings and Unicode.
    pub description: Value,
    /// Any JSON value, including explicit null.
    pub metadata: Value,
    /// RFC3339 timestamp for terminal status, otherwise null.
    pub completed_at: Value,
    /// RFC3339 creation timestamp with representable canonical UTC output.
    pub created_at: Value,
    /// RFC3339 modification timestamp with representable canonical UTC output.
    pub updated_at: Value,
}

/// Holds validated iteration data in private fields.
///
/// Construct it with [`iteration_from_values`].
#[derive(Clone, Debug, PartialEq)]
pub struct IterationValues {
    /// Full iteration ID, called `id` upstream.
    gest_id: String,
    /// Full project ID.
    project_id: String,
    /// Exact title string, including empty strings and Unicode.
    title: String,
    /// Case-insensitive `active`, `completed` or `cancelled`.
    status: &'static str,
    /// Exact description string, including empty strings and Unicode.
    description: String,
    /// Any JSON value, including explicit null.
    metadata: Value,
    /// RFC3339 timestamp for terminal status, otherwise null.
    completed_at: Option<DateTime<Utc>>,
    /// RFC3339 creation timestamp with representable canonical UTC output.
    created_at: DateTime<Utc>,
    /// RFC3339 modification timestamp with representable canonical UTC output.
    updated_at: DateTime<Utc>,
}

/// Reports a failed field without its input, with an optional typed chrono parse cause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IterationValueError {
    /// Complete internal failure, including the typed cause.
    failure: ValidationError,
}

impl From<ValidationError> for IterationValueError {
    /// Preserve every shared validation failure field.
    fn from(failure: ValidationError) -> Self {
        Self { failure }
    }
}

impl IterationValueError {
    /// Inspect the chrono timestamp parse error without exposing supplied data.
    ///
    /// # Returns
    /// The initial or canonical-output parse cause, or `None` for other failures.
    /// Chrono's alloc-only parser error does not implement `std::error::Error`.
    pub fn parse_cause(&self) -> Option<&chrono::ParseError> {
        self.failure.parse_cause.as_ref()
    }
}

impl fmt::Display for IterationValueError {
    /// Render only the field and static reason, without supplied data.
    ///
    /// # Errors
    /// Returns the formatter error if writing fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.failure.field, self.failure.reason)
    }
}

impl std::error::Error for IterationValueError {}

/// Validate every explicit decoded field and preserve the complete model value.
///
/// # Arguments
/// * `fields` - Complete fields admitted by the caller before this conversion.
///
/// # Returns
/// An owned validated record with private fields. Metadata accepts every JSON
/// value, and strings keep their exact content. The conversion imposes no order on the timestamps.
///
/// # Errors
/// Returns [`IterationValueError`] for wrong types, invalid full IDs, invalid timestamp
/// input or UTC output outside the canonical RFC3339 domain.
/// An unknown status also fails, as does a terminal status without `completed_at`
/// or an `active` status with one.
///
/// # Examples
/// ```
/// use gossil::gest_iteration_values::{IterationValueInputs, iteration_from_values, iteration_to_values};
/// use serde_json::json;
///
/// let fields = IterationValueInputs {
///     gest_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
///     project_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
///     title: json!(""),
///     status: json!("COMPLETED"),
///     description: json!("Complete description é"),
///     metadata: json!([null, false]),
///     completed_at: json!("2026-11-18T12:00:00Z"),
///     created_at: json!("2026-11-18T07:00:00-05:00"),
///     updated_at: json!("2026-11-18T12:00:00Z"),
/// };
/// let record = iteration_from_values(fields).unwrap();
/// let output = iteration_to_values(&record);
/// assert_eq!(output.created_at, json!("2026-11-18T12:00:00+00:00"));
/// assert_eq!(iteration_from_values(output).unwrap(), record);
/// ```
pub fn iteration_from_values(
    fields: IterationValueInputs,
) -> Result<IterationValues, IterationValueError> {
    let gest_id = identifier(fields.gest_id, "gest_id")?;
    let project_id = identifier(fields.project_id, "project_id")?;
    let title = string(fields.title, "title")?;
    let status_text = string(fields.status, "status")?;
    let status = match status_text.to_lowercase().as_str() {
        "active" => "active",
        "completed" => "completed",
        "cancelled" => "cancelled",
        _ => {
            return Err(ValidationError {
                field: "status",
                reason: "invalid iteration status",
                parse_cause: None,
            }
            .into());
        }
    };
    let description = string(fields.description, "description")?;
    let completed_at = if fields.completed_at.is_null() {
        None
    } else {
        Some(timestamp(fields.completed_at, "completed_at")?)
    };
    if matches!(status, "completed" | "cancelled") != completed_at.is_some() {
        return Err(ValidationError {
            field: "completed_at",
            reason: "terminal status requires a timestamp; nonterminal status requires null",
            parse_cause: None,
        }
        .into());
    }
    let created_at = timestamp(fields.created_at, "created_at")?;
    let updated_at = timestamp(fields.updated_at, "updated_at")?;
    Ok(IterationValues {
        gest_id,
        project_id,
        title,
        status,
        description,
        metadata: fields.metadata,
        completed_at,
        created_at,
        updated_at,
    })
}

/// Return all 9 fields as independent owned values.
///
/// # Arguments
/// * `record` - A record validated by [`iteration_from_values`].
///
/// # Returns
/// Complete model values with canonical status and UTC RFC3339 timestamp strings.
/// UTC uses `+00:00` and at most nine fractional digits. Output can be validated
/// again, and a change to the output does not change the record.
pub fn iteration_to_values(record: &IterationValues) -> IterationValueInputs {
    IterationValueInputs {
        gest_id: Value::from(record.gest_id.clone()),
        project_id: Value::from(record.project_id.clone()),
        title: Value::from(record.title.clone()),
        status: Value::from(record.status),
        description: Value::from(record.description.clone()),
        metadata: record.metadata.clone(),
        completed_at: record
            .completed_at
            .map(|dt| Value::from(dt.to_rfc3339()))
            .unwrap_or(Value::Null),
        created_at: Value::from(record.created_at.to_rfc3339()),
        updated_at: Value::from(record.updated_at.to_rfc3339()),
    }
}
