//! Convert complete, already decoded task fields without storage or raw JSON parsing.
//!
//! Callers own contributor admission, field presence, unknown-field classification,
//! duplicate lexemes, byte budgets and whole-history validation. This module does
//! not admit migration or implement the Gest API.
//!
//! [`task_from_values`](crate::gest_task_values::task_from_values) validates the eleven
//! explicit [`TaskValueInputs`](crate::gest_task_values::TaskValueInputs) fields.
//! [`task_to_values`](crate::gest_task_values::task_to_values) recovers them from the
//! private [`TaskValues`](crate::gest_task_values::TaskValues) record with
//! canonical status spelling and UTC timestamp strings. Business timestamps use
//! chrono 0.4.44 parsing, then reject normalized output that cannot be parsed again.
//! They do not use the artifact D-card grammar or impose timestamp ordering.

use chrono::{DateTime, Utc};
use serde_json::Value;
use std::fmt;

use crate::gest_model::primitives::{Id, TaskStatus};
use crate::gest_value_validation::{
    CanonicalValue, ValidationError, canonical_id, canonical_timestamp, identifier, string,
    timestamp,
};

/// All eleven owned model fields, with explicit nulls for nullable fields.
///
/// Callers assemble every field after raw input admission. No fields default.
/// A null [`Value`] is data for `metadata` and marks absence for nullable fields.
/// Validate this input with [`task_from_values`] before using it as a task record.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskValueInputs {
    /// Task identifier of exactly 32 lowercase `k` through `z` letters, upstream `id`.
    pub gest_id: Value,
    /// Project identifier of exactly 32 lowercase `k` through `z` letters.
    pub project_id: Value,
    /// Exact title string, including empty strings and Unicode.
    pub title: Value,
    /// Null or a JSON integer from 0 through 255, without named priority labels.
    pub priority: Value,
    /// Case-insensitive `open`, `in-progress`, `in_progress`, `inprogress`, `done` or `cancelled`.
    pub status: Value,
    /// Exact description string.
    pub description: Value,
    /// Null or an author identifier of exactly 32 lowercase `k` through `z` letters.
    pub assigned_to: Value,
    /// Any JSON value, including null.
    pub metadata: Value,
    /// RFC3339 string for `done` or `cancelled`, otherwise null.
    pub resolved_at: Value,
    /// RFC3339 creation string whose normalized UTC output must parse again.
    pub created_at: Value,
    /// RFC3339 modification string whose normalized UTC output must parse again.
    pub updated_at: Value,
}

/// Validated task data whose private fields prevent unchecked construction.
///
/// Use [`task_from_values`] to construct a record and [`task_to_values`] to read
/// every field as an owned [`Value`]. Cloning a record preserves its validation.
#[derive(Clone, Debug, PartialEq)]
pub struct TaskValues {
    /// Validated full task identifier.
    pub(crate) gest_id: Id,
    /// Validated full project identifier.
    pub(crate) project_id: Id,
    /// Unmodified title.
    pub(crate) title: String,
    /// Checked numeric priority.
    pub(crate) priority: Option<u8>,
    /// Status, written with the spelling of upstream `Display`.
    pub(crate) status: TaskStatus,
    /// Unmodified description.
    pub(crate) description: String,
    /// Validated optional author identifier.
    pub(crate) assigned_to: Option<Id>,
    /// Unmodified JSON model value.
    pub(crate) metadata: Value,
    /// UTC terminal timestamp.
    pub(crate) resolved_at: Option<DateTime<Utc>>,
    /// UTC creation timestamp.
    pub(crate) created_at: DateTime<Utc>,
    /// UTC modification timestamp.
    pub(crate) updated_at: DateTime<Utc>,
}

/// Report one field's type, value or terminal-status validation failure.
///
/// Its [`fmt::Display`] output names the field and reason without echoing input.
/// The converter returns the first failure in its validation order.
/// [`Self::parse_cause`] returns the original timestamp parser error. Equality
/// includes that cause, so errors with equal display text can compare unequal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TaskValueError {
    /// Field that failed validation.
    field: &'static str,
    /// Stable explanation of the rejected value.
    reason: &'static str,
    /// Actual chrono failure when timestamp parsing failed.
    parse_cause: Option<chrono::ParseError>,
}

impl From<ValidationError> for TaskValueError {
    /// Preserve all shared failure fields, including the typed parser cause.
    fn from(error: ValidationError) -> Self {
        Self {
            field: error.field,
            reason: error.reason,
            parse_cause: error.parse_cause,
        }
    }
}

impl TaskValueError {
    /// Inspect the typed chrono cause without exposing the supplied timestamp.
    ///
    /// # Returns
    /// The actual parser error for input or canonical UTC output parsing, or
    /// `None` for validation failures that did not come from the parser.
    /// Chrono's alloc-only build does not implement `std::error::Error` for
    /// [`chrono::ParseError`], so this accessor supplies the cause instead of
    /// [`std::error::Error::source`].
    pub fn parse_cause(&self) -> Option<&chrono::ParseError> {
        self.parse_cause.as_ref()
    }
}

impl fmt::Display for TaskValueError {
    /// Render the field and the validation failure without echoing user data.
    ///
    /// # Errors
    /// Returns the formatter's error if it cannot write the message.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.field, self.reason)
    }
}

impl std::error::Error for TaskValueError {}

/// Validate and own complete task fields using the adopted Gest value contract.
///
/// # Arguments
/// * `fields` - Independently admitted decoded values for every model field.
///
/// # Returns
/// A privately constructed record, or the first invalid field. Numeric priority
/// uses checked `u8` conversion rather than upstream SQL's wrapping cast. The
/// adopted Gossil contract additionally requires terminal/resolution pairing
/// and a normalized UTC representation accepted by the RFC3339 parser.
///
/// This function performs no reference lookup, clock read, ID generation, raw
/// JSON admission, SQL operation or complete-history validation.
///
/// # Errors
/// Returns [`TaskValueError`] for a non-string value in a string field, an invalid full
/// identifier, a priority other than null or an integer from 0 through 255, or
/// an unknown status. Timestamp strings must parse with chrono 0.4.44 and their
/// normalized UTC output must parse again. `resolved_at` must be a valid timestamp
/// for `done` or `cancelled` and null for `open` or `in-progress`.
/// Metadata accepts every [`Value`] variant. Title and description have no content
/// restrictions, and timestamps have no ordering constraint.
///
/// # Examples
/// ```
/// use gossil::gest_task_values::{TaskValueInputs, task_from_values, task_to_values};
/// use serde_json::json;
///
/// let fields = TaskValueInputs {
///     gest_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
///     project_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
///     title: json!("Review task values"),
///     priority: json!(255),
///     status: json!("IN_PROGRESS"),
///     description: json!(""),
///     assigned_to: json!(null),
///     metadata: json!([null, {"reviewed": false}]),
///     resolved_at: json!(null),
///     created_at: json!("2026-11-18T07:00:00-05:00"),
///     updated_at: json!("2026-11-18T12:00:00Z"),
/// };
/// let task = task_from_values(fields).unwrap();
/// let values = task_to_values(&task);
/// assert_eq!(values.status, json!("in-progress"));
/// assert_eq!(values.created_at, json!("2026-11-18T12:00:00+00:00"));
/// assert_eq!(task_from_values(values).unwrap(), task);
/// ```
pub fn task_from_values(fields: TaskValueInputs) -> Result<TaskValues, TaskValueError> {
    let gest_id = identifier(fields.gest_id, "gest_id")?;
    let project_id = identifier(fields.project_id, "project_id")?;
    let title = string(fields.title, "title")?;
    let priority = if fields.priority.is_null() {
        None
    } else {
        Some(
            fields
                .priority
                .as_u64()
                .and_then(|p| u8::try_from(p).ok())
                .ok_or(TaskValueError {
                    field: "priority",
                    reason: "expected null or an integer from 0 through 255",
                    parse_cause: None,
                })?,
        )
    };
    let status_text = string(fields.status, "status")?;
    let status: TaskStatus = status_text.parse().map_err(|_| TaskValueError {
        field: "status",
        reason: "invalid task status",
        parse_cause: None,
    })?;
    let description = string(fields.description, "description")?;
    let assigned_to = if fields.assigned_to.is_null() {
        None
    } else {
        Some(identifier(fields.assigned_to, "assigned_to")?)
    };
    let resolved_at = if fields.resolved_at.is_null() {
        None
    } else {
        Some(timestamp(fields.resolved_at, "resolved_at")?)
    };
    if status.is_terminal() != resolved_at.is_some() {
        return Err(TaskValueError {
            field: "resolved_at",
            reason: "terminal status requires a timestamp; nonterminal status requires null",
            parse_cause: None,
        });
    }
    Ok(TaskValues {
        gest_id,
        project_id,
        title,
        priority,
        status,
        description,
        assigned_to,
        metadata: fields.metadata,
        resolved_at,
        created_at: timestamp(fields.created_at, "created_at")?,
        updated_at: timestamp(fields.updated_at, "updated_at")?,
    })
}

/// Return every model field with canonical status and UTC timestamp strings.
///
/// # Arguments
/// * `task` - A record validated by [`task_from_values`].
///
/// # Returns
/// Eleven explicit owned values. Nullable fields stay null, metadata retains its
/// JSON value semantics, and RFC3339 output has at most nine fractional digits.
/// The UTC offset is `+00:00`. Output can be passed to [`task_from_values`] again;
/// its example shows both functions together. The returned fields can be changed
/// independently without changing the validated record.
pub fn task_to_values(task: &TaskValues) -> TaskValueInputs {
    TaskValueInputs {
        gest_id: Value::from(task.gest_id.to_string()),
        project_id: Value::from(task.project_id.to_string()),
        title: Value::from(task.title.clone()),
        priority: task.priority.map(Value::from).unwrap_or(Value::Null),
        status: Value::from(task.status.to_string()),
        description: Value::from(task.description.clone()),
        assigned_to: task
            .assigned_to
            .as_ref()
            .map(|id| Value::from(id.to_string()))
            .unwrap_or(Value::Null),
        metadata: task.metadata.clone(),
        resolved_at: task
            .resolved_at
            .map(|dt| Value::from(dt.to_rfc3339()))
            .unwrap_or(Value::Null),
        created_at: Value::from(task.created_at.to_rfc3339()),
        updated_at: Value::from(task.updated_at.to_rfc3339()),
    }
}

/// List the same eleven fields as `task_to_values` for bounded serialization.
///
/// The function allocates only the short identifier, status and timestamp strings.
/// The title, description and metadata, which have no size bound, remain borrowed.
/// This module owns validation and status conversion.
///
/// # Arguments
/// * `task` - Validated task whose canonical fields are borrowed.
///
/// # Returns
/// All eleven field names and borrowed canonical values.
pub(crate) fn canonical_task_fields(task: &TaskValues) -> [(&'static str, CanonicalValue<'_>); 11] {
    use CanonicalValue::{Bounded, Json, Priority, Text};
    [
        ("assigned_to", canonical_id(task.assigned_to.as_ref())),
        ("created_at", canonical_timestamp(Some(task.created_at))),
        ("description", Text(&task.description)),
        ("gest_id", canonical_id(Some(&task.gest_id))),
        ("metadata", Json(&task.metadata)),
        ("priority", Priority(task.priority)),
        ("project_id", canonical_id(Some(&task.project_id))),
        ("resolved_at", canonical_timestamp(task.resolved_at)),
        ("status", Bounded(Some(task.status.to_string()))),
        ("title", Text(&task.title)),
        ("updated_at", canonical_timestamp(Some(task.updated_at))),
    ]
}
