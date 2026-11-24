//! Emit bounded Fossil ticket creation bytes for Gest artifact and project records.
//!
//! See the [Gest ticket guide](https://rahuldave.com/gossil/gest-ticket.html)
//! for the format, caller responsibilities and a library example.
//!
//! One shared emitter writes the creation artifact of every Gest kind. Each
//! kind gives only its sorted field list and its `gossil_kind` value, so the
//! task emitter in [`crate::task_ticket`] and the two emitters here write the
//! same card syntax, size plan and checksum.
//!
//! The card order and the construction of tokens and the checksum follow Fossil
//! tkt.c:1837-1874 (b3a1c8efb2731dffbd5cc38c58ac349746de6da6). Fossil is Copyright
//! D. Richard Hipp and contributors. Its BSD two-clause license is preserved in the
//! frozen source evidence. This module performs no storage, identity generation or admission.
use crate::{
    gest_artifact_values::{ArtifactValues, canonical_artifact_fields},
    gest_project_values::{ProjectValues, canonical_project_fields},
    gest_value_validation::CanonicalValue,
    native_date::NativeDate,
};
use std::{collections::TryReserveError, fmt, io};

/// Failure to validate the native ticket identity or bound output construction.
#[derive(Debug)]
pub enum TicketError {
    /// The K card ticket identity must contain exactly forty lowercase ASCII hexadecimal bytes.
    TicketId,
    /// Encoded output exceeds the caller's bound or vector capacity.
    Size,
    /// Fallible output reservation failed.
    Allocation(TryReserveError),
    /// Fossil token codec failed.
    Token(crate::token_codec::TokenError),
    /// JSON model value serialization failed.
    Json(serde_json::Error),
}
impl fmt::Display for TicketError {
    /// Describe the ticket failure or its retained cause.
    ///
    /// # Errors
    /// Returns the formatter error if writing fails.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TicketId => f.write_str("invalid native ticket identity"),
            Self::Size => f.write_str("ticket output limit"),
            Self::Allocation(e) => write!(f, "{e}"),
            Self::Token(e) => write!(f, "{e}"),
            Self::Json(e) => write!(f, "{e}"),
        }
    }
}
impl std::error::Error for TicketError {
    /// Return the retained allocation, token or JSON error when present.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Allocation(e) => Some(e),
            Self::Token(e) => Some(e),
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

/// Emit complete uncompressed Fossil ticket artifact bytes for a validated artifact creation.
///
/// A Gest artifact is a document record, such as a spec or a plan. It is not a
/// Fossil blob. The contract CHANGE-01 uses an artifact ticket as the carrier
/// of a logical change. The ticket becomes a change only when a validated
/// `logical_change` declaration is attached at its K. This emitter writes no
/// declaration, and metadata alone never declares a change.
///
/// The eight model fields are `archived_at`, `body`, `created_at`, `gest_id`,
/// `metadata`, `project_id`, `title` and `updated_at`. Each uses JSON followed
/// by native token escaping. The three fixed fields are `gossil_deleted=false`, raw
/// `gossil_kind=artifact` and raw `gossil_schema=1`. The eleven J fields are
/// sorted by name. The body keeps its exact text. A non-null `archived_at` does
/// not mark the record as deleted.
///
/// The K, U and D cards and `max_output` follow
/// [`crate::task_ticket::task_creation_bytes`].
///
/// # Arguments
/// * `artifact` - Complete validated artifact snapshot.
/// * `ticket_id` - Independent ticket identity bytes for the K card.
/// * `actor` - Actor bytes for the U card.
/// * `date` - Validated creation instant for the D card.
/// * `max_output` - Maximum complete encoded artifact length in bytes.
///
/// # Returns
/// Complete uncompressed creation artifact bytes.
///
/// # Examples
/// ```
/// use gossil::gest_artifact_values::{ArtifactValueInputs, artifact_from_values};
/// use gossil::gest_ticket::artifact_creation_bytes;
/// use gossil::native_date::NativeDate;
/// use serde_json::json;
///
/// let artifact = artifact_from_values(ArtifactValueInputs {
///     gest_id: json!("zyxwvutsrqponmlkzyxwvutsrqponmlk"),
///     project_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
///     title: json!("Plan"),
///     body: json!("Complete body"),
///     metadata: json!({}),
///     archived_at: json!(null),
///     created_at: json!("2026-11-18T12:00:00Z"),
///     updated_at: json!("2026-11-18T12:00:00Z"),
/// })
/// .unwrap();
/// let date = NativeDate::parse(b"2026-11-18T12:00:00Z").unwrap();
/// let bytes = artifact_creation_bytes(&artifact,
///     b"0123456789abcdef0123456789abcdef01234567", b"actor", date, 65_536).unwrap();
/// assert!(bytes.windows(23).any(|w| w == b"J gossil_kind artifact\n"));
/// ```
///
/// # Errors
/// Returns an error for malformed K, byte limits, allocation, token encoding
/// or JSON serialization failure.
pub fn artifact_creation_bytes(
    artifact: &ArtifactValues,
    ticket_id: &[u8],
    actor: &[u8],
    date: NativeDate,
    max_output: usize,
) -> Result<Vec<u8>, TicketError> {
    creation_bytes(
        b"artifact",
        &canonical_artifact_fields(artifact),
        ticket_id,
        actor,
        date,
        max_output,
    )
}

/// Emit complete uncompressed Fossil ticket artifact bytes for a validated project creation.
///
/// The four model fields are `archived_at`, `created_at`, `gest_id` and
/// `updated_at`. Each uses JSON followed by native token escaping. The three fixed
/// fields are `gossil_deleted=false`, raw `gossil_kind=project` and raw
/// `gossil_schema=1`. The seven J fields are sorted by name. The local checkout
/// root is not a canonical field, so the ticket does not include its path. A
/// non-null `archived_at` does not mark the record as deleted.
///
/// The K, U and D cards and `max_output` follow
/// [`crate::task_ticket::task_creation_bytes`].
///
/// # Arguments
/// * `project` - Complete validated project snapshot.
/// * `ticket_id` - Independent ticket identity bytes for the K card.
/// * `actor` - Actor bytes for the U card.
/// * `date` - Validated creation instant for the D card.
/// * `max_output` - Maximum complete encoded artifact length in bytes.
///
/// # Returns
/// Complete uncompressed creation artifact bytes.
///
/// # Examples
/// ```
/// use gossil::gest_project_values::{ProjectValueInputs, project_from_values};
/// use gossil::gest_ticket::project_creation_bytes;
/// use gossil::native_date::NativeDate;
/// use serde_json::json;
///
/// let project = project_from_values(ProjectValueInputs {
///     gest_id: json!("kkkkkkkkkkkkkkkkkkkkkkkkkkkkkkkk"),
///     archived_at: json!(null),
///     created_at: json!("2026-11-18T12:00:00Z"),
///     updated_at: json!("2026-11-18T12:00:00Z"),
/// })
/// .unwrap();
/// let date = NativeDate::parse(b"2026-11-18T12:00:00Z").unwrap();
/// let bytes = project_creation_bytes(&project,
///     b"0123456789abcdef0123456789abcdef01234567", b"actor", date, 65_536).unwrap();
/// assert!(bytes.starts_with(b"D 2026-11-18T12:00:00.000000000\nJ archived_at null\n"));
/// ```
///
/// # Errors
/// Returns an error for malformed K, byte limits, allocation, token encoding
/// or JSON serialization failure.
pub fn project_creation_bytes(
    project: &ProjectValues,
    ticket_id: &[u8],
    actor: &[u8],
    date: NativeDate,
    max_output: usize,
) -> Result<Vec<u8>, TicketError> {
    creation_bytes(
        b"project",
        &canonical_project_fields(project),
        ticket_id,
        actor,
        date,
        max_output,
    )
}

/// Emit one creation artifact from a kind's model fields and the three fixed fields.
///
/// Every Gest kind's emitter calls this function, so they share one card
/// syntax, one size plan and one checksum.
///
/// # Arguments
/// * `kind` - Raw `gossil_kind` value, such as `task`.
/// * `fields` - Every canonical model field of one record, with unique names.
/// * `ticket_id` - Independent ticket identity bytes for the K card.
/// * `actor` - Actor bytes for the U card.
/// * `date` - Validated creation instant for the D card.
/// * `max_output` - Maximum complete encoded artifact length in bytes.
///
/// # Returns
/// Complete uncompressed creation artifact bytes, with the J cards sorted by name.
///
/// # Errors
/// Returns an error for malformed K, byte limits, allocation, token encoding
/// or JSON serialization failure.
pub(crate) fn creation_bytes(
    kind: &'static [u8],
    fields: &[(&'static str, CanonicalValue<'_>)],
    ticket_id: &[u8],
    actor: &[u8],
    date: NativeDate,
    max_output: usize,
) -> Result<Vec<u8>, TicketError> {
    if ticket_id.len() != 40
        || !ticket_id
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
    {
        return Err(TicketError::TicketId);
    }
    let limit = max_output.min(isize::MAX as usize);
    let deleted_json = serde_json::Value::Bool(false);
    let deleted = CanonicalValue::Json(&deleted_json);
    let mut values = Vec::new();
    values
        .try_reserve_exact(fields.len() + 3)
        .map_err(TicketError::Allocation)?;
    for (name, value) in fields {
        values.push((*name, FieldValue::Json(value)));
    }
    values.push(("gossil_deleted", FieldValue::Json(&deleted)));
    values.push(("gossil_kind", FieldValue::Raw(kind)));
    values.push(("gossil_schema", FieldValue::Raw(b"1")));
    values.sort_unstable_by_key(|v| v.0);
    debug_assert!(values.windows(2).all(|w| w[0].0 < w[1].0));
    let date = date.to_string();
    let mut total = 2usize
        .checked_add(date.len())
        .and_then(|n| n.checked_add(1 + 2 + 40 + 1 + 2 + 1 + 2 + 32 + 1))
        .ok_or(TicketError::Size)?;
    for (name, value) in &values {
        let encoded = value.encoded_size(limit)?;
        total = total
            .checked_add(2 + name.len() + 1)
            .and_then(|n| n.checked_add(encoded))
            .and_then(|n| n.checked_add(1))
            .ok_or(TicketError::Size)?;
    }
    total = total
        .checked_add(
            crate::token_codec::fossilized_size(actor, limit).map_err(|_| TicketError::Size)?,
        )
        .filter(|n| *n <= limit)
        .ok_or(TicketError::Size)?;
    let mut out = Vec::new();
    out.try_reserve_exact(total)
        .map_err(TicketError::Allocation)?;
    out.extend_from_slice(b"D ");
    out.extend_from_slice(date.as_bytes());
    out.push(b'\n');
    for (name, value) in values {
        let bytes = match value {
            FieldValue::Raw(bytes) => std::borrow::Cow::Borrowed(bytes),
            FieldValue::Json(value) => std::borrow::Cow::Owned(bounded_json(value, total)?),
        };
        let encoded = crate::token_codec::fossilize(&bytes).map_err(TicketError::Token)?;
        out.extend_from_slice(b"J ");
        out.extend_from_slice(name.as_bytes());
        out.push(b' ');
        out.extend_from_slice(&encoded);
        out.push(b'\n');
    }
    out.extend_from_slice(b"K ");
    out.extend_from_slice(ticket_id);
    out.extend_from_slice(b"\nU ");
    out.extend_from_slice(&crate::token_codec::fossilize(actor).map_err(TicketError::Token)?);
    out.push(b'\n');
    let checksum = crate::content_hash::hex(&crate::artifact_checksum::md5(&out));
    out.extend_from_slice(b"Z ");
    out.extend_from_slice(checksum.as_bytes());
    out.push(b'\n');
    debug_assert_eq!(out.len(), total);
    Ok(out)
}

/// Literal schema field bytes or a borrowed canonical model JSON value.
enum FieldValue<'a> {
    /// Raw fixed bytes, such as the kind name, which need only the existing token codec.
    Raw(&'a [u8]),
    /// A model value, which the emitter serializes without building an owned JSON tree.
    Json(&'a CanonicalValue<'a>),
}
impl FieldValue<'_> {
    /// Count token-escaped bytes without retaining any JSON output.
    /// # Arguments
    /// * `limit` - Maximum encoded size in bytes.
    /// # Returns
    /// Encoded byte count.
    /// # Errors
    /// Returns a size error on overflow or a JSON serialization error.
    fn encoded_size(&self, limit: usize) -> Result<usize, TicketError> {
        match self {
            Self::Raw(bytes) => {
                crate::token_codec::fossilized_size(bytes, limit).map_err(|_| TicketError::Size)
            }
            Self::Json(value) => {
                let mut counter = EncodedCounter {
                    size: 0,
                    limit,
                    exceeded: false,
                };
                let result = serde_json::to_writer(&mut counter, value);
                if counter.exceeded {
                    return Err(TicketError::Size);
                }
                result.map_err(TicketError::Json)?;
                Ok(counter.size)
            }
        }
    }
}
/// A streaming size counter that adds the token codec's encoded size of each JSON fragment.
struct EncodedCounter {
    /// Encoded bytes seen so far.
    size: usize,
    /// Maximum encoded bytes allowed.
    limit: usize,
    /// Preserve a size refusal through serde's IO error.
    exceeded: bool,
}
impl io::Write for EncodedCounter {
    /// Count escaped bytes without retaining the supplied JSON fragment.
    ///
    /// # Errors
    /// Returns an IO error if the encoded size exceeds the remaining limit.
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        match crate::token_codec::fossilized_size(bytes, self.limit - self.size) {
            Ok(size) => {
                self.size += size;
                Ok(bytes.len())
            }
            Err(_) => {
                self.exceeded = true;
                Err(io::Error::other("ticket output limit"))
            }
        }
    }
    /// Finish the in-memory sink without additional work.
    ///
    /// # Errors
    /// This implementation always succeeds.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Fallible byte sink with a caller-selected maximum.
struct Buffer {
    /// Bytes already serialized.
    bytes: Vec<u8>,
    /// Maximum retained JSON bytes.
    limit: usize,
    /// Original boundary or reservation failure, before IO conversion.
    failure: Option<TicketError>,
}
impl io::Write for Buffer {
    /// Append a JSON fragment within the retained-byte limit.
    ///
    /// # Errors
    /// Returns an IO error while retaining the size or allocation failure.
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        let failure = if self
            .bytes
            .len()
            .checked_add(input.len())
            .is_none_or(|n| n > self.limit)
        {
            Some(TicketError::Size)
        } else {
            self.bytes
                .try_reserve_exact(input.len())
                .err()
                .map(TicketError::Allocation)
        };
        if let Some(error) = failure {
            self.failure = Some(error);
            return Err(io::Error::other("ticket JSON output limit"));
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }
    /// Finish the memory buffer without additional work.
    ///
    /// # Errors
    /// This implementation always succeeds.
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Bound JSON serialization before each append, retaining the original failure.
/// # Arguments
/// * `value` - Borrowed value to serialize.
/// * `limit` - Maximum retained JSON byte count.
/// # Returns
/// Serialized JSON bytes.
/// # Errors
/// Returns a size, allocation or JSON serialization error.
fn bounded_json(value: &impl serde::Serialize, limit: usize) -> Result<Vec<u8>, TicketError> {
    let mut buffer = Buffer {
        bytes: Vec::new(),
        limit,
        failure: None,
    };
    let result = serde_json::to_writer(&mut buffer, value);
    if let Some(error) = buffer.failure {
        return Err(error);
    }
    result.map_err(TicketError::Json)?;
    Ok(buffer.bytes)
}

/// Verify error causes and bounded serialization sink contracts.
#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::io::Write;

    /// Preserve each public error description and its optional underlying cause.
    #[test]
    fn error_descriptions_and_sources() {
        let allocation = Vec::<u8>::new().try_reserve(usize::MAX).unwrap_err();
        let json = serde_json::from_str::<bool>("invalid").unwrap_err();
        let errors = [
            (TicketError::TicketId, false),
            (TicketError::Size, false),
            (TicketError::Allocation(allocation), true),
            (
                TicketError::Token(crate::token_codec::TokenError::Size),
                true,
            ),
            (TicketError::Json(json), true),
        ];
        for (error, has_source) in errors {
            assert_eq!(error.source().is_some(), has_source);
            let text = error.to_string();
            match &error {
                TicketError::TicketId => assert_eq!(text, "invalid native ticket identity"),
                TicketError::Size => assert_eq!(text, "ticket output limit"),
                _ => assert_eq!(text, error.source().unwrap().to_string()),
            }
        }
    }

    /// Check sink flush semantics and retained size failures without allocating large inputs.
    #[test]
    fn writer_contracts_and_size_failure() {
        let mut counter = EncodedCounter {
            size: 0,
            limit: 3,
            exceeded: false,
        };
        assert_eq!(counter.write(b" ").unwrap(), 1);
        counter.flush().unwrap();
        assert_eq!(counter.size, 2);
        assert!(counter.write(b" ").is_err());
        assert!(counter.exceeded);
        assert_eq!(counter.size, 2);
        let mut buffer = Buffer {
            bytes: Vec::new(),
            limit: 3,
            failure: None,
        };
        buffer.write_all(b"abc").unwrap();
        buffer.flush().unwrap();
        assert!(buffer.write(b"d").is_err());
        assert_eq!(buffer.bytes, b"abc");
        assert!(matches!(buffer.failure, Some(TicketError::Size)));
        assert_eq!(bounded_json(&"a", 3).unwrap(), b"\"a\"");
        assert!(matches!(bounded_json(&"a", 2), Err(TicketError::Size)));
    }

    /// Return a serializer failure so the helper must preserve its JSON error cause.
    struct Fails;
    impl serde::Serialize for Fails {
        /// Refuse serialization with a stable sentinel diagnostic.
        ///
        /// # Errors
        /// Always returns the sentinel serializer error.
        fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("sentinel serialization failure"))
        }
    }
    /// Preserve a serializer failure separately from a writer size refusal.
    #[test]
    fn serializer_failure_retains_cause() {
        let error = bounded_json(&Fails, 100).unwrap_err();
        assert!(matches!(error, TicketError::Json(_)));
        assert_eq!(
            error.source().unwrap().to_string(),
            "sentinel serialization failure"
        );
    }
}
