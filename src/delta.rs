//! Create and apply bounded Fossil deltas; application leaves checksum verification disabled.
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
    /// Encoder storage reservation failed, preserving its stage and cause.
    EncoderAllocationFailed {
        /// Output, collision table or landmark table.
        stage: &'static str,
        /// Additional elements requested.
        requested: usize,
        /// Original fallible reservation error.
        source: TryReserveError,
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
            Self::AllocationFailed { source, .. }
            | Self::EncoderAllocationFailed { source, .. } => Some(source),
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

/// Signed-char rolling hash for the selected native build's sixteen-byte window.
struct Rolling {
    /// Sum of signed bytes modulo 65536.
    a: u16,
    /// Weighted sum modulo 65536.
    b: u16,
    /// Circular buffer position of the outgoing byte.
    at: usize,
    /// Current window bytes, interpreted as signed during arithmetic.
    bytes: [u8; 16],
}
impl Rolling {
    /// Initialize weighted sums with explicitly sign-extended bytes.
    /// # Arguments
    /// * `bytes` - Slice containing at least sixteen bytes.
    /// # Returns
    /// Initialized circular hash window.
    fn new(bytes: &[u8]) -> Self {
        let mut hash = Self {
            a: 0,
            b: 0,
            at: 0,
            bytes: bytes[..16].try_into().unwrap(),
        };
        for byte in hash.bytes {
            hash.a = hash.a.wrapping_add(byte as i8 as u16);
            hash.b = hash.b.wrapping_add(hash.a);
        }
        hash
    }
    /// Advance one byte, retaining signed outgoing and incoming operands.
    /// # Arguments
    /// * `byte` - Incoming byte interpreted as a signed char.
    fn next(&mut self, byte: u8) {
        let old = self.bytes[self.at] as i8 as u16;
        self.bytes[self.at] = byte;
        self.at = (self.at + 1) & 15;
        self.a = self.a.wrapping_sub(old).wrapping_add(byte as i8 as u16);
        self.b = self
            .b
            .wrapping_sub(old.wrapping_mul(16))
            .wrapping_add(self.a);
    }
    /// Combine the two sixteen-bit sums.
    /// # Returns
    /// Low sum bits followed by high weighted-sum bits.
    fn value(&self) -> u32 {
        u32::from(self.a) | (u32::from(self.b) << 16)
    }
}

/// Sum unsigned big-endian words, padding the last short word on the right.
/// # Arguments
/// * `bytes` - Target bytes to checksum.
/// # Returns
/// Wrapping unsigned word sum.
fn checksum(bytes: &[u8]) -> u32 {
    bytes.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// Count base-64 digits with defined unsigned arithmetic even above 2^30.
/// # Arguments
/// * `value` - Unsigned integer to represent.
/// # Returns
/// Number of digits, from one through six.
fn digit_count(mut value: u32) -> usize {
    let mut count = 1;
    while value >= 64 {
        value >>= 6;
        count += 1;
    }
    count
}

/// Preserve the exact reservation cause and allocation stage for any encoder buffer.
/// # Arguments
/// * `values` - Buffer whose capacity may grow.
/// * `requested` - Additional elements required.
/// * `stage` - Buffer identity retained in errors.
/// # Returns
/// Success after capacity covers the additional elements.
/// # Errors
/// Returns the original reservation cause with its stage and requested count.
fn reserve_encoder<T>(
    values: &mut Vec<T>,
    requested: usize,
    stage: &'static str,
) -> Result<(), DeltaError> {
    values
        .try_reserve_exact(requested)
        .map_err(|source| DeltaError::EncoderAllocationFailed {
            stage,
            requested,
            source,
        })
}

/// Reserve before every append so no later extension can allocate implicitly.
/// # Arguments
/// * `output` - Accumulated delta bytes.
/// * `bytes` - Next encoded bytes to append.
/// # Returns
/// Success after appending all bytes.
/// # Errors
/// Returns a causal output reservation failure.
fn append(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), DeltaError> {
    reserve_encoder(output, bytes.len(), "output")?;
    output.extend_from_slice(bytes);
    Ok(())
}

/// Write the native integer alphabet, with a single zero digit and no leading zeros.
/// # Arguments
/// * `output` - Accumulated delta bytes.
/// * `value` - Integer to encode.
/// # Returns
/// Success after appending its digit representation.
/// # Errors
/// Returns a causal output reservation failure.
fn put_int(output: &mut Vec<u8>, mut value: u32) -> Result<(), DeltaError> {
    const DIGITS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ_abcdefghijklmnopqrstuvwxyz~";
    let mut digits = [0; 6];
    let mut at = digits.len();
    loop {
        at -= 1;
        digits[at] = DIGITS[(value & 63) as usize];
        value >>= 6;
        if value == 0 {
            break;
        }
    }
    append(output, &digits[at..])
}

/// Emit one literal, including a zero literal on the short-source branch.
/// # Arguments
/// * `output` - Accumulated delta bytes.
/// * `bytes` - Literal payload within the input-length ceiling.
/// # Returns
/// Success after appending the length, colon and payload.
/// # Errors
/// Returns a causal output reservation failure.
fn literal(output: &mut Vec<u8>, bytes: &[u8]) -> Result<(), DeltaError> {
    put_int(output, bytes.len() as u32)?;
    append(output, b":")?;
    append(output, bytes)
}

/// Allocate and initialize one hash table with a genuine fallible reservation.
/// # Arguments
/// * `length` - Number of table entries.
/// * `stage` - Table identity retained in errors.
/// # Returns
/// Table initialized with absent-entry sentinels.
/// # Errors
/// Returns a causal table reservation failure.
fn table(length: usize, stage: &'static str) -> Result<Vec<usize>, DeltaError> {
    let mut values = Vec::new();
    reserve_encoder(&mut values, length, stage)?;
    values.resize(length, usize::MAX);
    Ok(values)
}

/// Create a Fossil delta using the selected reference build's signed-char hash.
/// # Arguments
/// * `source` - Original bytes available to copy commands.
/// * `target` - Exact desired output bytes.
/// # Returns
/// Header, literal/copy commands and unsigned wrapping checksum, without a NUL.
/// # Errors
/// Returns input-length or causal allocation failures. Inputs are bounded by
/// `i32::MAX`; integer digit counting above 2^30 is safe Rust policy, since the
/// reference's signed shifts there are undefined.
/// # Examples
/// ```
/// assert_eq!(gossil::delta::create(b"", b"").unwrap(), b"0\n0:0;");
/// ```
pub fn create(source: &[u8], target: &[u8]) -> Result<Vec<u8>, DeltaError> {
    input_length("source", source.len())?;
    input_length("target", target.len())?;
    let mut output = Vec::new();
    put_int(&mut output, target.len() as u32)?;
    append(&mut output, b"\n")?;
    if source.len() <= 16 {
        literal(&mut output, target)?;
    } else {
        let n = source.len() / 16;
        let mut collide = table(n, "collision table")?;
        let mut landmark = table(n, "landmark table")?;
        for offset in (0..source.len() - 16).step_by(16) {
            let bucket = Rolling::new(&source[offset..]).value() as usize % n;
            collide[offset / 16] = landmark[bucket];
            landmark[bucket] = offset / 16;
        }
        let mut base = 0;
        while base + 16 < target.len() {
            let mut hash = Rolling::new(&target[base..]);
            let mut i = 0;
            loop {
                let mut block = landmark[hash.value() as usize % n];
                let mut best = (0, 0, 0);
                for _ in 0..250 {
                    if block == usize::MAX {
                        break;
                    }
                    let anchor = block * 16;
                    let mut forward = 0;
                    while anchor + forward < source.len()
                        && base + i + forward < target.len()
                        && source[anchor + forward] == target[base + i + forward]
                    {
                        forward += 1;
                    }
                    let mut k = 1;
                    while k < anchor && k <= i && source[anchor - k] == target[base + i - k] {
                        k += 1;
                    }
                    let back = k - 1;
                    let count = forward + back;
                    let offset = anchor - back;
                    let lits = i - back;
                    let overhead = digit_count(lits as u32)
                        + digit_count(count as u32)
                        + digit_count(offset as u32)
                        + 3;
                    if count >= overhead && count > best.0 {
                        best = (count, offset, lits);
                    }
                    block = collide[block];
                }
                if best.0 > 0 {
                    if best.2 > 0 {
                        literal(&mut output, &target[base..base + best.2])?;
                    }
                    base += best.2 + best.0;
                    put_int(&mut output, best.0 as u32)?;
                    append(&mut output, b"@")?;
                    put_int(&mut output, best.1 as u32)?;
                    append(&mut output, b",")?;
                    break;
                }
                if base + i + 16 >= target.len() {
                    literal(&mut output, &target[base..])?;
                    base = target.len();
                    break;
                }
                hash.next(target[base + i + 16]);
                i += 1;
            }
        }
        if base < target.len() {
            literal(&mut output, &target[base..])?;
        }
    }
    put_int(&mut output, checksum(target))?;
    append(&mut output, b";")?;
    Ok(output)
}

#[cfg(test)]
mod encoder_tests {
    use super::*;
    use std::error::Error;

    /// Source-derived arithmetic and allocation boundaries without giant inputs.
    #[test]
    fn arithmetic_and_resource_policy() {
        for (value, count) in [
            (0, 1),
            (63, 1),
            (64, 2),
            (4095, 2),
            (4096, 3),
            (1 << 30, 6),
            (u32::MAX, 6),
        ] {
            assert_eq!(digit_count(value), count);
            let mut output = Vec::new();
            put_int(&mut output, value).unwrap();
            assert_eq!(uint(&output, &mut 0), value);
            assert_eq!(output.len(), count);
        }
        assert_eq!(checksum(&[255; 8]), u32::MAX - 1);
        assert_eq!(checksum(&[1, 2, 3]), 0x01020300);
        let hash = Rolling::new(&[255; 16]);
        assert_eq!(hash.a, (-16i16) as u16);
        assert_eq!(hash.b, (-136i16) as u16);
        for stage in ["output", "collision table", "landmark table"] {
            let error = reserve_encoder(&mut Vec::<u8>::new(), usize::MAX, stage).unwrap_err();
            assert!(
                matches!(error,DeltaError::EncoderAllocationFailed {stage: actual,requested: usize::MAX,..} if actual==stage)
            );
            assert!(
                error
                    .source()
                    .unwrap()
                    .downcast_ref::<TryReserveError>()
                    .is_some()
            );
        }
    }

    /// Source-derived copy acceptance equality, first window and backward-zero exclusion.
    #[test]
    fn source_decision_boundaries() {
        // A first-window six-byte match equals overhead and wins immediately,
        // even though a later target window could copy sixteen bytes.
        let source = b"abcdef!!!!!!!!!!ABCDEFGHIJKLMNOP!";
        let target = b"abcdef?ABCDEFGHIJKLMNOP";
        let delta = create(source, target).unwrap();
        assert!(delta.starts_with(b"N\n6@0,1:?G@G,"), "{:?}", delta);
        // Hide source0 behind 250 colliders, then permit the source16 anchor
        // to extend backward. Its preceding sixteen bytes all match, but the
        // native k<anchor condition stops at source1.
        let mut source: Vec<u8> = (33..81).collect();
        let mut collision: Vec<u8> = (33..49).collect();
        collision[0] += 1;
        collision[1] -= 2;
        collision[2] += 1;
        for _ in 0..250 {
            source.extend_from_slice(&collision);
        }
        source.push(b'!');
        let delta = create(&source, &source[..33]).unwrap();
        assert!(delta.starts_with(b"X\n1:!W@1,"), "{delta:?}");
    }
}
