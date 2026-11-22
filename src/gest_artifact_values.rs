//! Convert 8 explicit decoded artifact fields into owned validated records.
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
pub struct ArtifactValueInputs {
    /// Full artifact ID, called `id` upstream.
    pub gest_id: Value,
    /// Full project ID.
    pub project_id: Value,
    /// Exact title string, including empty strings and Unicode.
    pub title: Value,
    /// Complete exact body string, including empty strings and Unicode.
    pub body: Value,
    /// Any JSON value, including explicit null.
    pub metadata: Value,
    /// Null or an RFC3339 archive timestamp. An archived row is not marked as deleted.
    pub archived_at: Value,
    /// RFC3339 creation timestamp with representable canonical UTC output.
    pub created_at: Value,
    /// RFC3339 modification timestamp with representable canonical UTC output.
    pub updated_at: Value,
}

/// Holds validated artifact data in private fields.
///
/// Construct it with [`artifact_from_values`].
#[derive(Clone, Debug, PartialEq)]
pub struct ArtifactValues {
    /// Full artifact ID, called `id` upstream.
    gest_id: String,
    /// Full project ID.
    project_id: String,
    /// Exact title string, including empty strings and Unicode.
    title: String,
    /// Complete exact body string, including empty strings and Unicode.
    body: String,
    /// Any JSON value, including explicit null.
    metadata: Value,
    /// Null or an RFC3339 archive timestamp. An archived row is not marked as deleted.
    archived_at: Option<DateTime<Utc>>,
    /// RFC3339 creation timestamp with representable canonical UTC output.
    created_at: DateTime<Utc>,
    /// RFC3339 modification timestamp with representable canonical UTC output.
    updated_at: DateTime<Utc>,
}

/// Reports a failed field without its input, with an optional typed chrono parse cause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactValueError {
    /// Complete internal failure, including the typed cause.
    failure: ValidationError,
}

impl From<ValidationError> for ArtifactValueError {
    /// Preserve every shared validation failure field.
    fn from(failure: ValidationError) -> Self {
        Self { failure }
    }
}

impl ArtifactValueError {
    /// Inspect the chrono timestamp parse error without exposing supplied data.
    ///
    /// # Returns
    /// The initial or canonical-output parse cause, or `None` for other failures.
    /// Chrono's alloc-only parser error does not implement `std::error::Error`.
    pub fn parse_cause(&self) -> Option<&chrono::ParseError> {
        self.failure.parse_cause.as_ref()
    }
}

impl fmt::Display for ArtifactValueError {
    /// Render only the field and static reason, without supplied data.
    ///
    /// # Errors
    /// Returns the formatter error if writing fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.failure.field, self.failure.reason)
    }
}

impl std::error::Error for ArtifactValueError {}

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
/// Returns [`ArtifactValueError`] for wrong types, invalid full IDs, invalid timestamp
/// input or UTC output outside the canonical RFC3339 domain.
///
/// # Examples
/// ```
/// use gossil::gest_artifact_values::{ArtifactValueInputs, artifact_from_values, artifact_to_values};
/// use serde_json::json;
///
/// let fields = ArtifactValueInputs {
///     gest_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
///     project_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
///     title: json!(""),
///     body: json!("Complete body é"),
///     metadata: json!([null, false]),
///     archived_at: json!(null),
///     created_at: json!("2026-11-18T07:00:00-05:00"),
///     updated_at: json!("2026-11-18T12:00:00Z"),
/// };
/// let record = artifact_from_values(fields).unwrap();
/// let output = artifact_to_values(&record);
/// assert_eq!(output.created_at, json!("2026-11-18T12:00:00+00:00"));
/// assert_eq!(artifact_from_values(output).unwrap(), record);
/// ```
pub fn artifact_from_values(
    fields: ArtifactValueInputs,
) -> Result<ArtifactValues, ArtifactValueError> {
    let gest_id = identifier(fields.gest_id, "gest_id")?;
    let project_id = identifier(fields.project_id, "project_id")?;
    let title = string(fields.title, "title")?;
    let body = string(fields.body, "body")?;
    let archived_at = if fields.archived_at.is_null() {
        None
    } else {
        Some(timestamp(fields.archived_at, "archived_at")?)
    };
    let created_at = timestamp(fields.created_at, "created_at")?;
    let updated_at = timestamp(fields.updated_at, "updated_at")?;
    Ok(ArtifactValues {
        gest_id,
        project_id,
        title,
        body,
        metadata: fields.metadata,
        archived_at,
        created_at,
        updated_at,
    })
}

/// Return all 8 fields as independent owned values.
///
/// # Arguments
/// * `record` - A record validated by [`artifact_from_values`].
///
/// # Returns
/// Complete model values with UTC RFC3339 timestamp strings.
/// UTC uses `+00:00` and at most nine fractional digits. Output can be validated
/// again, and a change to the output does not change the record.
pub fn artifact_to_values(record: &ArtifactValues) -> ArtifactValueInputs {
    ArtifactValueInputs {
        gest_id: Value::from(record.gest_id.clone()),
        project_id: Value::from(record.project_id.clone()),
        title: Value::from(record.title.clone()),
        body: Value::from(record.body.clone()),
        metadata: record.metadata.clone(),
        archived_at: record
            .archived_at
            .map(|dt| Value::from(dt.to_rfc3339()))
            .unwrap_or(Value::Null),
        created_at: Value::from(record.created_at.to_rfc3339()),
        updated_at: Value::from(record.updated_at.to_rfc3339()),
    }
}
