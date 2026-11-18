//! Pack and unpack Fossil's four-byte big-endian capacity and zlib stream.
use flate2::{Compress, Compression, Decompress, FlushCompress, FlushDecompress, Status};
use std::{collections::TryReserveError, error::Error, fmt};

/// Report a packed-blob failure, retaining allocation and backend causes.
#[derive(Debug)]
pub enum BlobError {
    /// Compression level is outside zlib's -1 or 0 through 9 range.
    Level,
    /// Input length cannot fit the format's unsigned 32-bit header.
    InputSize,
    /// Declared output capacity exceeds the caller's resource limit.
    Limit,
    /// Output exceeds its declared capacity.
    Capacity,
    /// The zlib stream ended before its checksum/end marker.
    Truncated,
    /// The allocator could not reserve the requested output storage.
    Allocation(TryReserveError),
    /// The compression backend rejected the input.
    Compress(flate2::CompressError),
    /// The decompression backend rejected the stream or its checksum.
    Decompress(flate2::DecompressError),
}
impl fmt::Display for BlobError {
    /// Describe the failed boundary without losing the original cause.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Level => f.write_str("compression level must be -1 or 0 through 9"),
            Self::InputSize => f.write_str("input exceeds packed-blob u32 capacity"),
            Self::Limit => f.write_str("declared capacity exceeds caller limit"),
            Self::Capacity => f.write_str("decoded bytes exceed declared capacity"),
            Self::Truncated => f.write_str("incomplete zlib stream"),
            Self::Allocation(e) => write!(f, "output allocation failed: {e}"),
            Self::Compress(e) => write!(f, "zlib compression failed: {e}"),
            Self::Decompress(e) => write!(f, "invalid zlib stream: {e}"),
        }
    }
}
impl Error for BlobError {
    /// Preserve the original allocator or compression failure for callers.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Allocation(e) => Some(e),
            Self::Compress(e) => Some(e),
            Self::Decompress(e) => Some(e),
            _ => None,
        }
    }
}

/// A short-input no-op is distinct from a successfully decoded empty blob.
#[derive(Debug, PartialEq, Eq)]
pub enum Unpacked {
    /// Fossil leaves the caller's output unchanged for inputs of at most four bytes.
    NoPayload,
    /// Return decoded bytes from a complete validated zlib stream, possibly empty.
    Bytes(Vec<u8>),
}

/// Append bytes using a fallible reservation before any vector growth.
/// # Arguments
/// * `output` - Accumulated output.
/// * `bytes` - Next bounded chunk.
/// # Returns
/// Success after appending the bytes.
/// # Errors
/// Retains the allocator's failure if capacity cannot be reserved.
fn append(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), BlobError> {
    output
        .try_reserve(bytes.len())
        .map_err(BlobError::Allocation)?;
    output.extend_from_slice(bytes);
    Ok(())
}

/// Check packed length before requesting exact output growth.
/// # Arguments
/// * `output` - Stored envelope buffer to extend.
/// * `bytes` - Input bytes for this operation.
/// * `max_stored` - Maximum complete stored envelope length.
/// # Returns
/// Success after bounded append.
/// # Errors
/// Returns a stored-length or allocation error before an append exceeds its ceiling.
fn append_packed(output: &mut Vec<u8>, bytes: &[u8], max_stored: usize) -> Result<(), BlobError> {
    output
        .len()
        .checked_add(bytes.len())
        .filter(|length| *length <= max_stored)
        .ok_or(BlobError::Limit)?;
    output
        .try_reserve_exact(bytes.len())
        .map_err(BlobError::Allocation)?;
    output.extend_from_slice(bytes);
    Ok(())
}

/// Pack one byte slice with zlib's default level or a level from zero to nine.
/// # Arguments
/// * `input` - Bytes to compress.
/// * `level` - -1 selects the default, zero stores bytes, 1 through 9 compress.
/// # Returns
/// A four-byte capacity header followed by a zlib stream.
/// # Errors
/// Rejects invalid levels, lengths above `u32`, allocation and backend failures.
/// # Examples
/// ```
/// let packed = gossil::compression::pack(b"abc", -1).unwrap();
/// assert_eq!(&packed[..4], &[0, 0, 0, 3]);
/// ```
pub fn pack(input: &[u8], level: i32) -> Result<Vec<u8>, BlobError> {
    pack_parts(input, &[], level, usize::MAX)
}

/// Pack bytes within a caller-selected stored-envelope ceiling.
///
/// The ceiling includes the four-byte header and is checked before every output
/// reservation. The encoder still uses its fixed 8192-byte working buffer and
/// the compression backend owns internal state; this is not a global allocator cap.
/// # Arguments
/// * `input` - Full bytes to encode.
/// * `level` - Native zlib level, -1 or zero through nine.
/// * `max_stored` - Largest returned header plus compressed stream.
/// # Returns
/// Exact legacy packed bytes when they fit the ceiling.
/// # Examples
/// ```
/// let packed = gossil::compression::pack_bounded(b"abc", -1, 64).unwrap();
/// assert_eq!(packed, gossil::compression::pack(b"abc", -1).unwrap());
/// assert!(gossil::compression::pack_bounded(b"abc", -1, 3).is_err());
/// ```
/// # Errors
/// Rejects output above the ceiling and preserves all ordinary packing errors.
pub fn pack_bounded(input: &[u8], level: i32, max_stored: usize) -> Result<Vec<u8>, BlobError> {
    pack_parts(input, &[], level, max_stored)
}

/// Pack the concatenation of two slices at Fossil's fixed compression level nine.
/// # Arguments
/// * `first` - First bytes in the concatenation.
/// * `second` - Remaining bytes in the concatenation.
/// # Returns
/// A capacity header and zlib stream for the combined content.
/// # Errors
/// Rejects lengths above `u32`, allocation and compression backend failures.
/// # Examples
/// ```
/// assert!(gossil::compression::pack_two(b"a", b"bc").is_ok());
/// ```
pub fn pack_two(first: &[u8], second: &[u8]) -> Result<Vec<u8>, BlobError> {
    pack_parts(first, second, 9, usize::MAX)
}

/// Check the format length without allocating or concatenating input.
/// # Arguments
/// * `first` - First input length.
/// * `second` - Second input length.
/// # Returns
/// The sum when it fits the unsigned 32-bit format header.
/// # Errors
/// Rejects machine-size addition overflow and sums above the format limit.
fn input_length(first: usize, second: usize) -> Result<u32, BlobError> {
    first
        .checked_add(second)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or(BlobError::InputSize)
}

/// Compress both slices without allocating a concatenated input buffer.
/// # Arguments
/// * `first` - Initial content.
/// * `second` - Additional content.
/// * `level` - Validated zlib compression level.
/// * `max_stored` - Maximum complete stored envelope length.
/// # Returns
/// Packed bytes for the concatenation.
/// # Errors
/// Returns checked length, level, allocation or backend failures.
fn pack_parts(
    first: &[u8],
    second: &[u8],
    level: i32,
    max_stored: usize,
) -> Result<Vec<u8>, BlobError> {
    let compression = match level {
        -1 => Compression::default(),
        0..=9 => Compression::new(level as u32),
        _ => return Err(BlobError::Level),
    };
    let length = input_length(first.len(), second.len())?;
    let mut output = Vec::new();
    append_packed(&mut output, &length.to_be_bytes(), max_stored)?;
    let mut stream = Compress::new(compression, true);
    for (part, finish) in [(first, false), (second, true)] {
        let mut consumed = 0;
        loop {
            let mut chunk = [0; 8192];
            let before_in = stream.total_in();
            let before_out = stream.total_out();
            let status = stream
                .compress(
                    &part[consumed..],
                    &mut chunk,
                    if finish {
                        FlushCompress::Finish
                    } else {
                        FlushCompress::None
                    },
                )
                .map_err(BlobError::Compress)?;
            consumed += (stream.total_in() - before_in) as usize;
            let produced = (stream.total_out() - before_out) as usize;
            append_packed(&mut output, &chunk[..produced], max_stored)?;
            if status == Status::StreamEnd || (!finish && consumed == part.len()) {
                break;
            }
        }
    }
    Ok(output)
}

/// Decode one packed zlib stream within a caller-selected capacity ceiling.
///
/// The header is an upper bound, so shorter actual output is accepted. Like
/// Fossil's `uncompress`, trailing bytes after the first stream are ignored.
/// A short input yields `NoPayload`; an in-place caller must retain its old
/// bytes for that outcome. Invalid data produces an error, even though Fossil's
/// diagnostic `test-uncompress` command ignores the C function's error return.
/// # Arguments
/// * `input` - Packed bytes including the capacity header.
/// * `max_output` - Maximum accepted declared capacity, checked before decoding.
/// # Returns
/// No-change outcome or the validated decompressed bytes.
/// # Errors
/// Rejects caller-limit violations, exceeded headers, invalid/truncated zlib,
/// and allocation failures. No fixed small limit is a format restriction.
/// # Examples
/// ```
/// use gossil::compression::{pack, unpack, Unpacked};
/// assert_eq!(unpack(&pack(b"abc", -1).unwrap(), 3).unwrap(), Unpacked::Bytes(b"abc".to_vec()));
/// ```
pub fn unpack(input: &[u8], max_output: usize) -> Result<Unpacked, BlobError> {
    if input.len() <= 4 {
        return Ok(Unpacked::NoPayload);
    }
    let capacity = u32::from_be_bytes(input[..4].try_into().expect("four-byte header")) as usize;
    if capacity > max_output {
        return Err(BlobError::Limit);
    }
    let mut output = Vec::new();
    let mut stream = Decompress::new(true);
    loop {
        let mut chunk = [0; 8192];
        let before_in = stream.total_in();
        let before_out = stream.total_out();
        let status = stream
            .decompress(
                &input[4 + before_in as usize..],
                &mut chunk,
                FlushDecompress::None,
            )
            .map_err(BlobError::Decompress)?;
        let produced = (stream.total_out() - before_out) as usize;
        if produced > capacity - output.len() {
            return Err(BlobError::Capacity);
        }
        append(&mut output, &chunk[..produced])?;
        if status == Status::StreamEnd {
            return Ok(Unpacked::Bytes(output));
        }
        if stream.total_in() == before_in && produced == 0 {
            return Err(BlobError::Truncated);
        }
    }
}

#[cfg(test)]
/// Check error boundaries that need impossible-sized requests or backend misuse.
mod tests {
    use super::*;

    #[test]
    /// Stored ceilings include the header and are checked before each vector growth.
    fn bounded_packing_header_and_stream_boundaries() {
        for input in [
            vec![],
            b"abc".to_vec(),
            (0..20000).map(|i| (i * 131 % 251) as u8).collect(),
        ] {
            let expected = pack(&input, 0).unwrap();
            assert_eq!(pack_bounded(&input, 0, expected.len()).unwrap(), expected);
            assert!(matches!(
                pack_bounded(&input, 0, expected.len() - 1),
                Err(BlobError::Limit)
            ));
            assert!(matches!(pack_bounded(&input, 0, 3), Err(BlobError::Limit)));
        }
    }

    #[test]
    /// Reject oversized header lengths without allocating multi-gigabyte inputs.
    fn format_length_bounds() {
        assert_eq!(input_length(u32::MAX as usize, 0).unwrap(), u32::MAX);
        assert!(matches!(
            input_length(u32::MAX as usize, 1),
            Err(BlobError::InputSize)
        ));
        assert!(matches!(
            input_length(usize::MAX, 1),
            Err(BlobError::InputSize)
        ));
    }

    #[test]
    /// Preserve genuine underlying causes and useful diagnostics for each error.
    fn error_causes_and_messages() {
        let allocation = Vec::<u8>::new().try_reserve(usize::MAX).unwrap_err();
        let mut compressor = Compress::new(Compression::default(), true);
        let mut buffer = [0; 128];
        let _ = compressor
            .compress(b"a", &mut [0; 1], FlushCompress::Finish)
            .unwrap();
        let compression = compressor
            .compress(b"b", &mut buffer, FlushCompress::None)
            .unwrap_err();
        let decompression = Decompress::new(true)
            .decompress(b"invalid", &mut buffer, FlushDecompress::None)
            .unwrap_err();
        for error in [
            BlobError::Allocation(allocation),
            BlobError::Compress(compression),
            BlobError::Decompress(decompression),
        ] {
            assert!(error.source().is_some());
            assert!(!error.to_string().is_empty());
        }
        for error in [
            BlobError::Level,
            BlobError::InputSize,
            BlobError::Limit,
            BlobError::Capacity,
            BlobError::Truncated,
        ] {
            assert!(error.source().is_none());
            assert!(!error.to_string().is_empty());
        }
    }
}
