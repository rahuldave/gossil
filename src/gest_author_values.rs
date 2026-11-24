//! Validate complete canonical author values in memory.
//! Native actors and local principals stay separate.
//!
//! Supply all six fields explicitly. Conversion generates no identity or time and performs no storage.
//!
//! ## Recommendations
//! Validate complete author inputs before native integration. Keep names and nullable email exact.
use crate::gest_model::primitives::{AuthorType, Id};
use crate::gest_value_validation::{ValidationError, identifier, string, timestamp};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::fmt;

/// Supply complete author fields, with SQL-null email represented as JSON null.
#[derive(Clone, Debug, PartialEq)]
pub struct AuthorValueInputs {
    /// Supply the full inherited author identity.
    pub gest_id: Value,
    /// Supply `human` or `agent` under the inherited case-insensitive grammar.
    pub author_type: Value,
    /// Preserve the exact unrestricted display name.
    pub name: Value,
    /// Supply explicit null or an exact unrestricted email string.
    pub email: Value,
    /// Supply the explicit creation instant.
    pub created_at: Value,
    /// Supply the explicit last-modified instant.
    pub updated_at: Value,
}
/// Hold a validated owned author. Construction never generates identity or time.
#[derive(Clone, Debug, PartialEq)]
pub struct AuthorValues {
    /// Hold the complete validated canonical identity.
    pub(crate) gest_id: Id,
    /// Hold the lowercase `human` or `agent` kind.
    pub(crate) author_type: AuthorType,
    /// Preserve the exact display name, including empty strings.
    pub(crate) name: String,
    /// Preserve the exact email, with null distinct from an empty string.
    pub(crate) email: Option<String>,
    /// Hold the explicit creation instant normalized to UTC.
    pub(crate) created_at: DateTime<Utc>,
    /// Hold the explicit modification instant without ordering constraints.
    pub(crate) updated_at: DateTime<Utc>,
}
/// Report a field failure and retain its timestamp parser cause when present.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthorValueError {
    /// Hold the field diagnostic and optional original parser cause.
    failure: ValidationError,
}
impl From<ValidationError> for AuthorValueError {
    /// Wrap a shared field failure without changing its parser cause.
    fn from(failure: ValidationError) -> Self {
        Self { failure }
    }
}
impl AuthorValueError {
    /// Inspect the original chrono cause. This accessor adds no standard error source.
    /// # Returns
    /// Return the original error from parsing an input or output timestamp, or `None` for other failures.
    pub fn parse_cause(&self) -> Option<&chrono::ParseError> {
        self.failure.parse_cause.as_ref()
    }
}
impl fmt::Display for AuthorValueError {
    /// Display the field and static reason without echoing input.
    ///
    /// # Errors
    /// Return the formatter's error if it cannot write the diagnostic.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.failure.field, self.failure.reason)
    }
}
impl std::error::Error for AuthorValueError {}
/// Validate all six fields, preserving empty strings and nullable email distinctly.
/// # Arguments
/// * `fields` - Supply complete canonical author fields after admission checks.
/// # Returns
/// Return validated owned fields with canonical UTC instants and no timestamp ordering rule.
/// # Errors
/// Reject wrong types, identifiers other than 32 lowercase letters from `k` through `z`,
/// author types other than case-insensitive `human` or `agent`, and invalid RFC3339 timestamps.
/// Also reject normalized UTC output that cannot be parsed again as RFC3339.
/// Names and string emails are unrestricted. Null email differs from an empty string.
///
/// # Examples
/// ```
/// use gossil::gest_author_values::{AuthorValueInputs, author_from_values, author_to_values};
/// use serde_json::json;
/// let author = author_from_values(AuthorValueInputs {
///     gest_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
///     author_type: json!("AGENT"),
///     name: json!(" Alice "),
///     email: json!(null),
///     created_at: json!("2026-11-19T01:02:03.000000001+02:00"),
///     updated_at: json!("2020-01-01T00:00:00Z"),
/// }).unwrap();
/// let values = author_to_values(&author);
/// assert_eq!(values.author_type, json!("agent"));
/// assert_eq!(values.email, json!(null));
/// assert_eq!(author_from_values(values).unwrap(), author);
/// ```
pub fn author_from_values(fields: AuthorValueInputs) -> Result<AuthorValues, AuthorValueError> {
    let gest_id = identifier(fields.gest_id, "gest_id")?;
    let kind = string(fields.author_type, "author_type")?;
    let author_type: AuthorType = kind.parse().map_err(|_| ValidationError {
        field: "author_type",
        reason: "expected human or agent",
        parse_cause: None,
    })?;
    let name = string(fields.name, "name")?;
    let email = if fields.email.is_null() {
        None
    } else {
        Some(string(fields.email, "email")?)
    };
    let created_at = timestamp(fields.created_at, "created_at")?;
    let updated_at = timestamp(fields.updated_at, "updated_at")?;
    Ok(AuthorValues {
        gest_id,
        author_type,
        name,
        email,
        created_at,
        updated_at,
    })
}
/// Return the six canonical fields with exact strings and canonical UTC timestamps.
/// # Arguments
/// * `record` - Supply an author validated from all six explicit fields.
/// # Returns
/// Return independent owned values that preserve null email separately from a present empty string.
/// See [`author_from_values`] for a complete round-trip example.
pub fn author_to_values(record: &AuthorValues) -> AuthorValueInputs {
    AuthorValueInputs {
        gest_id: Value::String(record.gest_id.to_string()),
        author_type: Value::String(record.author_type.to_string()),
        name: Value::String(record.name.clone()),
        email: record
            .email
            .clone()
            .map(Value::String)
            .unwrap_or(Value::Null),
        created_at: Value::String(record.created_at.to_rfc3339()),
        updated_at: Value::String(record.updated_at.to_rfc3339()),
    }
}
