//! Encode and decode Fossil token bytes without text conversion.
use std::{collections::TryReserveError, error::Error, fmt};

/// A size boundary or allocation failure in a safe token operation.
#[derive(Debug)]
pub enum TokenError {
    /// The exact encoded length cannot fit Rust vector capacity.
    Size,
    /// The allocator could not reserve output storage.
    Allocation(TryReserveError),
}
impl fmt::Display for TokenError {
    /// Describe the failed boundary while preserving allocation causes.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Size => f.write_str("token output exceeds Rust vector capacity"),
            Self::Allocation(error) => write!(f, "token allocation failed: {error}"),
        }
    }
}
impl Error for TokenError {
    /// Expose the original allocation failure.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Size => None,
            Self::Allocation(error) => Some(error),
        }
    }
}

/// Complete decoded output and the prefix visible to C-string consumers.
#[derive(Debug, PartialEq, Eq)]
pub struct DecodedToken {
    /// All emitted bytes, including bytes following a decoded NUL.
    pub bytes: Vec<u8>,
    /// Index of the first decoded NUL, or the output length.
    pub c_string_len: usize,
}

/// Identify the escape letter for a byte, without text classification.
/// # Arguments
/// * `byte` - Byte to classify.
/// # Returns
/// Its escape letter, or none when it passes through unchanged.
fn escape(byte: u8) -> Option<u8> {
    match byte {
        0 => Some(b'0'),
        b' ' => Some(b's'),
        b'\t' => Some(b't'),
        b'\n' => Some(b'n'),
        b'\r' => Some(b'r'),
        11 => Some(b'v'),
        12 => Some(b'f'),
        b'\\' => Some(b'\\'),
        _ => None,
    }
}

/// Check the exact output size before reserving or emitting bytes.
/// # Arguments
/// * `input` - Number of source bytes.
/// * `escapes` - Number of bytes requiring an extra slash.
/// * `limit` - Maximum output size, also bounded by Rust vector capacity.
/// # Returns
/// Exact output size within Rust vector capacity.
/// # Errors
/// Returns [`TokenError::Size`] on overflow or lengths above the output bound.
fn encoded_size(input: usize, escapes: usize, limit: usize) -> Result<usize, TokenError> {
    input
        .checked_add(escapes)
        .filter(|size| *size <= limit.min(isize::MAX as usize))
        .ok_or(TokenError::Size)
}

/// Reserve output storage once, retaining the allocator's cause.
/// # Arguments
/// * `size` - Required byte capacity.
/// # Returns
/// An empty vector with sufficient capacity.
/// # Errors
/// Returns [`TokenError::Allocation`] with the original reservation error.
fn output_buffer(size: usize) -> Result<Vec<u8>, TokenError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(TokenError::Allocation)?;
    Ok(bytes)
}

/// Encode every supplied byte, including embedded NUL.
/// # Arguments
/// * `input` - Arbitrary bytes, without UTF-8 requirements.
/// # Returns
/// Encoded bytes without a C terminator.
/// # Errors
/// Returns [`TokenError::Size`] on encoded-size overflow or lengths above
/// `isize::MAX`, or [`TokenError::Allocation`] if reservation fails.
/// # Examples
/// ```
/// assert_eq!(gossil::token_codec::fossilize(b"a b\0").unwrap(), b"a\\sb\\0");
/// ```
pub fn fossilize(input: &[u8]) -> Result<Vec<u8>, TokenError> {
    fossilize_with_limit(input, isize::MAX as usize)
}

/// Encode with an output bound, checking expansion before reservation.
/// # Arguments
/// * `input` - Arbitrary bytes, without UTF-8 requirements.
/// * `limit` - Maximum encoded length, capped at Rust vector capacity.
/// # Returns
/// Encoded bytes without a C terminator.
/// # Errors
/// Returns [`TokenError::Size`] when expansion exceeds the bound, or
/// [`TokenError::Allocation`] when output reservation fails.
fn fossilize_with_limit(input: &[u8], limit: usize) -> Result<Vec<u8>, TokenError> {
    let size = encoded_size(
        input.len(),
        input.iter().filter(|byte| escape(**byte).is_some()).count(),
        limit,
    )?;
    let mut output = output_buffer(size)?;
    for &byte in input {
        if let Some(letter) = escape(byte) {
            output.extend_from_slice(&[b'\\', letter]);
        } else {
            output.push(byte);
        }
    }
    Ok(output)
}

/// Encode bytes preceding the first NUL, or the full slice when absent.
/// # Arguments
/// * `input` - Bounded C-string contents; a terminator is optional.
/// # Returns
/// Encoded prefix without a C terminator.
/// # Errors
/// Returns [`TokenError::Size`] on encoded-size overflow or lengths above
/// `isize::MAX`, or [`TokenError::Allocation`] if reservation fails.
/// # Examples
/// ```
/// assert_eq!(gossil::token_codec::fossilize_c_string(b"a\0b").unwrap(), b"a");
/// ```
pub fn fossilize_c_string(input: &[u8]) -> Result<Vec<u8>, TokenError> {
    fossilize(
        &input[..input
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(input.len())],
    )
}

/// Decode bytes before the first source NUL or the slice end.
/// Preserve all emitted bytes, including bytes after a decoded NUL.
/// Unknown escapes discard the slash; a terminal slash survives.
/// # Arguments
/// * `input` - Arbitrary bounded token bytes; a terminator is optional.
/// # Returns
/// Complete emitted bytes and their C-string prefix length.
/// # Errors
/// Returns [`TokenError::Allocation`] if reservation fails before output is emitted.
/// # Examples
/// ```
/// let decoded = gossil::token_codec::defossilize(b"a\\0b").unwrap();
/// assert_eq!(decoded.bytes, b"a\0b");
/// assert_eq!(decoded.c_string_len, 1);
/// ```
pub fn defossilize(input: &[u8]) -> Result<DecodedToken, TokenError> {
    let input = &input[..input
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(input.len())];
    let mut bytes = output_buffer(input.len())?;
    let mut source = input.iter().copied();
    while let Some(mut byte) = source.next() {
        if byte == b'\\'
            && let Some(letter) = source.next()
        {
            byte = match letter {
                b'n' => b'\n',
                b's' => b' ',
                b't' => b'\t',
                b'r' => b'\r',
                b'v' => 11,
                b'f' => 12,
                b'0' => 0,
                other => other,
            };
        }
        bytes.push(byte);
    }
    let c_string_len = bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(bytes.len());
    Ok(DecodedToken {
        bytes,
        c_string_len,
    })
}

#[cfg(test)]
/// Exercise resource boundaries without allocating enormous slices.
mod tests {
    use super::*;

    #[test]
    /// Reject expansion beyond the output bound before allocating or emitting.
    fn encoder_propagates_output_size_errors() {
        assert_eq!(fossilize_with_limit(b"a b", 4).unwrap(), b"a\\sb");
        assert_eq!(fossilize_with_limit(b"abc", 3).unwrap(), b"abc");
        assert_eq!(fossilize_with_limit(b"", 0).unwrap(), b"");
        for (input, limit) in [(b"a b".as_slice(), 3), (b"abc", 2), (b" ", 0)] {
            let error = fossilize_with_limit(input, limit).unwrap_err();
            assert!(matches!(error, TokenError::Size));
            assert!(error.source().is_none());
        }
    }

    #[test]
    /// Check exact size arithmetic and deterministic reservation failure causes.
    fn resource_errors_preserve_causes() {
        assert_eq!(encoded_size(3, 2, usize::MAX).unwrap(), 5);
        assert_eq!(
            encoded_size(isize::MAX as usize, 0, usize::MAX).unwrap(),
            isize::MAX as usize
        );
        for (input, escapes) in [(usize::MAX, 1), (isize::MAX as usize, 1)] {
            let error = encoded_size(input, escapes, usize::MAX).unwrap_err();
            assert!(matches!(error, TokenError::Size));
            assert!(error.source().is_none());
            assert!(error.to_string().contains("capacity"));
        }
        let error = output_buffer(usize::MAX).unwrap_err();
        assert!(matches!(error, TokenError::Allocation(_)));
        assert!(error.source().is_some());
        assert!(error.to_string().contains("allocation"));
    }
}
