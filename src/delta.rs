//! Decode Fossil deltas with wrapping unsigned integers and disabled checksum verification.
//!
//! Validation finishes before output allocation. A second pass copies checked slices.
use std::collections::TryReserveError;

/// Explicit ceiling for decoded bytes, additionally bounded by `i32::MAX`.
#[derive(Debug, Copy, Clone)]
pub struct Limits {
    /// Largest advertised output accepted by [`apply`].
    pub max_output_bytes: u32,
}
impl Default for Limits {
    /// Use the reference implementation's signed-length envelope.
    fn default() -> Self {
        Self {
            max_output_bytes: i32::MAX as u32,
        }
    }
}

/// A malformed delta, unsupported input size, or recoverable allocation failure.
#[derive(Debug)]
pub enum DeltaError {
    /// Header digits were not followed by LF.
    MissingHeaderNewline,
    /// Input lies outside the reference signed-length envelope.
    InputTooLarge {
        /// Source or delta input name.
        input: &'static str,
        /// Observed input byte length.
        length: usize,
    },
    /// Advertised size exceeds the effective ceiling.
    OutputTooLarge {
        /// Wrapped header size.
        advertised: u32,
        /// Effective caller and signed-length ceiling.
        limit: u32,
    },
    /// A numeric command has no following operator.
    TruncatedCommand {
        /// Missing operator byte index.
        at: usize,
    },
    /// Copy offset lacks a present comma delimiter.
    MissingCopyComma {
        /// Expected comma byte index.
        at: usize,
    },
    /// Operator byte has no defined meaning.
    UnknownOperator {
        /// Operator byte index.
        at: usize,
        /// Unrecognized unsigned byte.
        byte: u8,
    },
    /// Copy extends beyond source, including a zero copy beyond its end.
    SourceRange {
        /// Wrapped source offset.
        offset: u32,
        /// Wrapped copy count.
        count: u32,
        /// Available source byte length.
        source_len: usize,
    },
    /// Command would exceed advertised output.
    OutputRange {
        /// Bytes produced by earlier commands.
        produced: u64,
        /// Current command byte count.
        count: u32,
        /// Wrapped header size.
        advertised: u32,
    },
    /// Literal has fewer bytes than its count.
    TruncatedLiteral {
        /// First payload byte index.
        at: usize,
        /// Requested payload byte count.
        count: u32,
        /// Remaining encoded byte length.
        remaining: usize,
    },
    /// End or command-start NUL occurred before semicolon.
    Unterminated,
    /// Semicolon arrived before advertised size was produced.
    SizeMismatch {
        /// Wrapped header size.
        advertised: u32,
        /// Actual produced byte length.
        produced: u64,
    },
    /// Output reservation failed, preserving the genuine allocator cause.
    AllocationFailed {
        /// Exact output bytes requested.
        requested: usize,
        /// Original fallible reservation error.
        source: TryReserveError,
    },
}

impl std::fmt::Display for DeltaError {
    /// Present the typed failure without discarding its cause.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "delta decode failed: {self:?}")
    }
}
impl std::error::Error for DeltaError {
    /// Expose the original reservation failure for callers to inspect.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::AllocationFailed { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Consume the exact unsigned-byte alphabet, wrapping arithmetic modulo 2^32.
/// # Arguments
/// * `bytes` - Immutable encoded bytes.
/// * `at` - Cursor advanced past recognized digits.
/// # Returns
/// Wrapped integer, or zero for an empty field.
fn uint(bytes: &[u8], at: &mut usize) -> u32 {
    const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_abcdefghijklmnopqrstuvwxyz~";
    let mut value = 0u32;
    while let Some(byte) = bytes.get(*at) {
        let Some(digit) = DIGITS.iter().position(|digit| digit == byte) else {
            break;
        };
        value = value.wrapping_shl(6).wrapping_add(digit as u32);
        *at += 1;
    }
    value
}

/// Parse the advertised size without validating commands or applying an output limit.
/// # Arguments
/// * `delta` - Encoded delta bytes.
/// # Returns
/// The wrapped unsigned header value.
/// # Errors
/// Returns [`DeltaError::MissingHeaderNewline`] when digits are not followed by LF.
/// # Examples
/// ```
/// assert_eq!(gossil::delta::output_size(b"3\n" ).unwrap(), 3);
/// ```
pub fn output_size(delta: &[u8]) -> Result<u32, DeltaError> {
    let mut at = 0;
    let size = uint(delta, &mut at);
    if delta.get(at) != Some(&b'\n') {
        return Err(DeltaError::MissingHeaderNewline);
    }
    Ok(size)
}

/// Validate all commands and optionally emit their checked slices, without storing operations.
/// # Arguments
/// * `source` - Original bytes used by copy commands.
/// * `delta` - Immutable encoded bytes with an already checked header.
/// * `advertised` - Wrapped size from that header.
/// * `emit` - Consumer of each checked output slice.
/// # Returns
/// Success after a terminator and an exact size match.
/// # Errors
/// Returns typed command, range, truncation or final-size errors.
fn walk(
    source: &[u8],
    delta: &[u8],
    advertised: u32,
    mut emit: impl FnMut(&[u8]),
) -> Result<(), DeltaError> {
    let mut at = 0;
    uint(delta, &mut at);
    at += 1; // Header newline was checked before either pass.
    let mut produced = 0u64;
    loop {
        if matches!(delta.get(at), None | Some(0)) {
            return Err(DeltaError::Unterminated);
        }
        let count = uint(delta, &mut at);
        let operator_at = at;
        let operator = *delta.get(at).ok_or(DeltaError::TruncatedCommand { at })?;
        at += 1;
        if operator == b';' {
            return if produced == u64::from(advertised) {
                Ok(())
            } else {
                Err(DeltaError::SizeMismatch {
                    advertised,
                    produced,
                })
            };
        }
        if operator != b'@' && operator != b':' {
            return Err(DeltaError::UnknownOperator {
                at: operator_at,
                byte: operator,
            });
        }
        if produced + u64::from(count) > u64::from(advertised) {
            return Err(DeltaError::OutputRange {
                produced,
                count,
                advertised,
            });
        }
        let chunk = if operator == b'@' {
            let offset = uint(delta, &mut at);
            if delta.get(at) != Some(&b',') {
                return Err(DeltaError::MissingCopyComma { at });
            }
            at += 1;
            let end = u64::from(offset) + u64::from(count);
            if end > source.len() as u64 {
                return Err(DeltaError::SourceRange {
                    offset,
                    count,
                    source_len: source.len(),
                });
            }
            // Both values fit usize because they are bounded by an existing slice.
            source
                .get(offset as usize..end as usize)
                .ok_or(DeltaError::SourceRange {
                    offset,
                    count,
                    source_len: source.len(),
                })?
        } else {
            let remaining = delta.len() - at;
            let end = at
                .checked_add(count as usize)
                .filter(|end| *end <= delta.len())
                .ok_or(DeltaError::TruncatedLiteral {
                    at,
                    count,
                    remaining,
                })?;
            let chunk = &delta[at..end];
            at = end;
            chunk
        };
        produced += u64::from(count);
        emit(chunk);
    }
}

/// Enforce the signed input envelope independently of any platform-sized cast.
/// # Arguments
/// * `input` - Input name retained in the typed error.
/// * `length` - Byte length to check without allocating a slice.
/// # Returns
/// Success for lengths within the reference signed envelope.
/// # Errors
/// Returns [`DeltaError::InputTooLarge`] above `i32::MAX`.
fn input_length(input: &'static str, length: usize) -> Result<(), DeltaError> {
    if length > i32::MAX as usize {
        Err(DeltaError::InputTooLarge { input, length })
    } else {
        Ok(())
    }
}

/// Decode after validation, using a private reservation seam for bounded fault tests.
/// # Arguments
/// * `source` - Original bytes used by copy commands.
/// * `delta` - Immutable header and command bytes.
/// * `limits` - Explicit caller output ceiling.
/// * `reserve` - Fallible reservation operation, invoked only after validation.
/// # Returns
/// Exact decoded bytes without a trailing NUL.
/// # Errors
/// Returns the same typed failures as [`apply`], preserving reservation causes.
fn apply_with(
    source: &[u8],
    delta: &[u8],
    limits: Limits,
    reserve: impl FnOnce(&mut Vec<u8>, usize) -> Result<(), TryReserveError>,
) -> Result<Vec<u8>, DeltaError> {
    input_length("source", source.len())?;
    input_length("delta", delta.len())?;
    let advertised = output_size(delta)?;
    let limit = limits.max_output_bytes.min(i32::MAX as u32);
    if advertised > limit {
        return Err(DeltaError::OutputTooLarge { advertised, limit });
    }
    walk(source, delta, advertised, |_| {})?;
    let mut output = Vec::new();
    let requested = advertised as usize;
    if requested != 0 {
        reserve(&mut output, requested)
            .map_err(|source| DeltaError::AllocationFailed { requested, source })?;
    }
    walk(source, delta, advertised, |chunk| {
        output.extend_from_slice(chunk)
    })?;
    Ok(output)
}

/// Apply a Fossil delta with checksum verification disabled, as in the pinned reference.
/// # Arguments
/// * `source` - Original bytes available to copy commands.
/// * `delta` - Header, copy/literal commands and semicolon terminator.
/// * `limits` - Caller's explicit output ceiling.
/// # Returns
/// Exactly the advertised output, with no trailing NUL.
/// # Errors
/// Returns a typed [`DeltaError`] for malformed commands, limits or reservation failure.
/// Every command is validated before reserving output storage.
/// # Examples
/// ```
/// use gossil::delta::{apply, Limits};
/// assert_eq!(apply(b"", b"1\n1:A0;", Limits::default()).unwrap(), b"A");
/// ```
pub fn apply(source: &[u8], delta: &[u8], limits: Limits) -> Result<Vec<u8>, DeltaError> {
    apply_with(source, delta, limits, Vec::try_reserve_exact)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    /// Inject genuine capacity overflow without asking the allocator for huge storage.
    #[test]
    fn reserve_failure_and_validate_before_reserve() {
        let failure = apply_with(b"", b"1\n1:A;", Limits::default(), |_, requested| {
            assert_eq!(requested, 1);
            Vec::<u8>::new().try_reserve_exact(usize::MAX)
        })
        .unwrap_err();
        assert!(matches!(
            failure,
            DeltaError::AllocationFailed { requested: 1, .. }
        ));
        assert!(
            failure
                .source()
                .unwrap()
                .downcast_ref::<TryReserveError>()
                .is_some()
        );
        assert!(failure.to_string().contains("AllocationFailed"));
        let mut calls = 0;
        let mut reserve = |out: &mut Vec<u8>, size| {
            calls += 1;
            out.try_reserve_exact(size)
        };
        assert_eq!(
            apply_with(b"", b"1\n1:A;", Limits::default(), &mut reserve).unwrap(),
            b"A"
        );
        for delta in [b"1~~~~~\n;".as_slice(), b"200000\n;", b"1\n", b"1\n1@0,;"] {
            assert!(apply_with(b"", delta, Limits::default(), &mut reserve).is_err());
        }
        assert!(
            apply_with(
                b"",
                b"1\n1:A;",
                Limits {
                    max_output_bytes: 0
                },
                &mut reserve
            )
            .is_err()
        );
        assert_eq!(
            apply_with(b"", b"0\n;", Limits::default(), &mut reserve).unwrap(),
            b""
        );
        assert_eq!(calls, 1);
        let malformed = output_size(b"bad").unwrap_err();
        assert!(malformed.source().is_none());
        assert!(malformed.to_string().contains("MissingHeaderNewline"));
    }

    /// Test the signed input boundary with lengths, without creating giant slices.
    #[test]
    fn input_envelope() {
        assert!(input_length("delta", i32::MAX as usize).is_ok());
        assert!(
            matches!(input_length("source", i32::MAX as usize + 1), Err(DeltaError::InputTooLarge { input: "source", length }) if length == i32::MAX as usize + 1)
        );
    }
}
