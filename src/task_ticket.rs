//! Emit bounded Fossil task ticket bytes for task creation.
//!
//! See the [task ticket guide](https://rahuldave.com/gossil/task-ticket.html)
//! for the format, caller responsibilities and a library example.
//!
//! Card order and token/checksum construction follow Fossil tkt.c:1837-1874
//! (b3a1c8efb2731dffbd5cc38c58ac349746de6da6). Fossil is Copyright D. Richard
//! Hipp and contributors; its BSD two-clause license is preserved in the frozen
//! source evidence. This module performs no storage, identity generation or admission.
use crate::{
    gest_task_values::{TaskValues, canonical_task_fields},
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
            Self::Size => f.write_str("task ticket output limit"),
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
/// Emit complete uncompressed Fossil ticket artifact bytes for a validated task creation.
///
/// The caller supplies the K card's ticket identity, actor bytes and a validated creation date.
/// K is forty lowercase ASCII hexadecimal bytes, independent of the 32-character
/// Gest ID in the task. Actor bytes need not be UTF-8 or a Gest author ID.
/// [`crate::gest_task_values`] supplies the normalized status spelling and UTC timestamp strings.
///
/// All eleven model fields use JSON followed by native token escaping. The three
/// additional J fields are `gossil_deleted=false`, raw `gossil_kind=task` and raw
/// `gossil_schema=1`. The fourteen J fields are sorted by name.
/// No append or parent card is emitted. The caller manages history and enforces
/// any requirement that artifact creation dates increase.
///
/// `max_output` bounds complete encoded bytes, including the final checksum and
/// newline, with an effective ceiling of `isize::MAX`. It is not a heap budget.
/// The emitter counts token-escaped JSON bytes before retaining serialized JSON
/// or reserving output bytes.
///
/// # Arguments
/// * `task` - Complete validated task snapshot.
/// * `ticket_id` - Independent ticket identity bytes for the K card.
/// * `actor` - Actor bytes for the U card.
/// * `date` - Validated creation instant for the D card.
/// * `max_output` - Maximum complete encoded artifact length in bytes.
/// # Returns
/// Complete uncompressed creation artifact bytes.
/// # Examples
/// Construct `task` with [`crate::gest_task_values::task_from_values`]. The
/// [task values example](https://rahuldave.com/gossil/gest-task-values.html#use-the-library)
/// shows all eleven input fields.
/// ```
/// use gossil::{gest_task_values::TaskValues, native_date::NativeDate,
///     task_ticket::{task_creation_bytes, TicketError}};
/// fn emit(task: &TaskValues) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
///     let date = NativeDate::parse(b"2026-11-18T12:00:00Z")?;
///     Ok(task_creation_bytes(task,
///         b"0123456789abcdef0123456789abcdef01234567", b"actor", date, 65_536)?)
/// }
/// ```
/// # Errors
/// Returns an error for malformed K, byte limits, allocation, token encoding
/// or JSON serialization failure.
pub fn task_creation_bytes(
    task: &TaskValues,
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
    let fields = canonical_task_fields(task);
    let deleted_json = serde_json::Value::Bool(false);
    let deleted = crate::gest_task_values::CanonicalTaskValue::Json(&deleted_json);
    let mut values = Vec::new();
    values
        .try_reserve_exact(14)
        .map_err(TicketError::Allocation)?;
    for (name, value) in &fields {
        values.push((*name, FieldValue::Json(value)));
    }
    values.push(("gossil_deleted", FieldValue::Json(&deleted)));
    values.push(("gossil_kind", FieldValue::Raw(b"task")));
    values.push(("gossil_schema", FieldValue::Raw(b"1")));
    values.sort_unstable_by_key(|v| v.0);
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
    /// A raw discriminator needs only the existing token codec.
    Raw(&'a [u8]),
    /// Model values are serialized without constructing owned JSON trees.
    Json(&'a crate::gest_task_values::CanonicalTaskValue<'a>),
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
/// Streaming planner reuses the codec's additive byte expansion calculation.
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
                Err(io::Error::other("task ticket output limit"))
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
            return Err(io::Error::other("task ticket JSON output limit"));
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
                TicketError::Size => assert_eq!(text, "task ticket output limit"),
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
