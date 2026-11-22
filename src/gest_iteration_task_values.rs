//! Convert 4 explicit decoded `iteration_task` fields into owned validated records.
//!
//! An `iteration_task` row places one task in one phase of one iteration. It has
//! no Gest ID of its own. Its key is the tuple of iteration ID and task ID.
//!
//! Callers own raw admission, field presence, unknown fields, budgets and history.
//! Callers also own the key, the membership incarnation and its lifecycle, the
//! check that the iteration and the task exist, and the rule that a repeated add
//! keeps the existing phase. Each of those needs other records.
//! This module reads no clock and performs no storage, reference lookup or ID generation.
//! `chrono` parses the timestamp as RFC3339, and `iteration_task_to_values` returns it in canonical UTC.

use crate::gest_model::primitives::Id;
use crate::gest_value_validation::{ValidationError, identifier, timestamp};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::fmt;

/// Holds the complete owned input and output fields.
///
/// The caller must give every field explicitly. The row has no `gest_id`.
#[derive(Clone, Debug, PartialEq)]
pub struct IterationTaskValueInputs {
    /// Full ID of the iteration.
    pub iteration_id: Value,
    /// Full ID of the task.
    pub task_id: Value,
    /// Phase number, a JSON integer from 0 through 4294967295.
    pub phase: Value,
    /// RFC3339 membership timestamp with representable canonical UTC output.
    pub created_at: Value,
}

/// Holds validated `iteration_task` data in private fields.
///
/// Construct it with [`iteration_task_from_values`].
#[derive(Clone, Debug, PartialEq)]
pub struct IterationTaskValues {
    /// Full ID of the iteration.
    iteration_id: Id,
    /// Full ID of the task.
    task_id: Id,
    /// Phase number in the unsigned 32-bit range.
    phase: u32,
    /// RFC3339 membership timestamp with representable canonical UTC output.
    created_at: DateTime<Utc>,
}

/// Reports a failed field without its input, with an optional typed `chrono` parse cause.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IterationTaskValueError {
    /// Complete internal failure, including the typed cause.
    failure: ValidationError,
}

impl From<ValidationError> for IterationTaskValueError {
    /// Preserve every shared validation failure field.
    fn from(failure: ValidationError) -> Self {
        Self { failure }
    }
}

impl IterationTaskValueError {
    /// Return the `chrono` parse error of the timestamp, without the supplied data.
    ///
    /// # Returns
    /// The initial or canonical-output parse cause, or `None` for other failures.
    /// The alloc-only parser error of `chrono` does not implement `std::error::Error`.
    pub fn parse_cause(&self) -> Option<&chrono::ParseError> {
        self.failure.parse_cause.as_ref()
    }
}

impl fmt::Display for IterationTaskValueError {
    /// Render only the field and static reason, without supplied data.
    ///
    /// # Errors
    /// Returns the formatter error if writing fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.failure.field, self.failure.reason)
    }
}

impl std::error::Error for IterationTaskValueError {}

/// Check that the phase is a JSON integer in the unsigned 32-bit range.
///
/// Upstream reads the stored integer with a wrapping `as u32` cast. The Gossil
/// contract requires a checked range, so a value outside it fails here.
///
/// # Arguments
/// * `value` - Already admitted decoded phase value.
///
/// # Returns
/// The phase as a `u32`.
///
/// # Errors
/// Returns [`ValidationError`] for a value that is not a JSON integer from 0
/// through 4294967295, for example a float, a negative integer or a string.
fn phase(value: Value) -> Result<u32, ValidationError> {
    value
        .as_u64()
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(ValidationError {
            field: "phase",
            reason: "expected an integer from 0 through 4294967295",
            parse_cause: None,
        })
}

/// Validate every explicit decoded field and preserve the complete `iteration_task` value.
///
/// # Arguments
/// * `fields` - Complete fields admitted by the caller before this conversion.
///
/// # Returns
/// An owned validated record with private fields. The phase keeps its exact value.
///
/// # Errors
/// Returns [`IterationTaskValueError`] for wrong types, invalid full IDs, a phase
/// outside the unsigned 32-bit range, invalid timestamp input or UTC output
/// outside the canonical RFC3339 domain.
///
/// # Examples
/// ```
/// use gossil::gest_iteration_task_values::{
///     IterationTaskValueInputs, iteration_task_from_values, iteration_task_to_values,
/// };
/// use serde_json::json;
///
/// let fields = IterationTaskValueInputs {
///     iteration_id: json!("klmnopqrstuvwxyzklmnopqrstuvwxyz"),
///     task_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
///     phase: json!(4294967295u32),
///     created_at: json!("2026-11-18T07:00:00-05:00"),
/// };
/// let record = iteration_task_from_values(fields).unwrap();
/// let output = iteration_task_to_values(&record);
/// assert_eq!(output.phase, json!(4294967295u32));
/// assert_eq!(output.created_at, json!("2026-11-18T12:00:00+00:00"));
/// assert_eq!(iteration_task_from_values(output).unwrap(), record);
/// ```
pub fn iteration_task_from_values(
    fields: IterationTaskValueInputs,
) -> Result<IterationTaskValues, IterationTaskValueError> {
    let iteration_id = identifier(fields.iteration_id, "iteration_id")?;
    let task_id = identifier(fields.task_id, "task_id")?;
    let phase = phase(fields.phase)?;
    let created_at = timestamp(fields.created_at, "created_at")?;
    Ok(IterationTaskValues {
        iteration_id,
        task_id,
        phase,
        created_at,
    })
}

/// Return all 4 fields as independent owned values.
///
/// # Arguments
/// * `record` - A record validated by [`iteration_task_from_values`].
///
/// # Returns
/// Complete model values with the phase as a JSON integer and a UTC RFC3339
/// timestamp string. UTC uses `+00:00` and at most nine fractional digits.
/// Output can be validated again, and a change to the output does not change
/// the record.
pub fn iteration_task_to_values(record: &IterationTaskValues) -> IterationTaskValueInputs {
    IterationTaskValueInputs {
        iteration_id: Value::from(record.iteration_id.to_string()),
        task_id: Value::from(record.task_id.to_string()),
        phase: Value::from(record.phase),
        created_at: Value::from(record.created_at.to_rfc3339()),
    }
}
