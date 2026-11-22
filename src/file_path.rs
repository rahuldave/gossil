//! Simplify filename bytes in a bounded port of the lexical rules of Fossil `src/file.c`.
//!
//! The original code is copyright 2006 D. Richard Hipp, under the 2-Clause BSD
//! license. The license text is in `tests/fixtures/file-path/COPYRIGHT-BSD2.txt`.
//!
//! The module ports the Unix branch and the Windows/Cygwin branch of
//! `file_simplify_name` and `backup_dir` from Fossil check-in `7e8ecf0ce4`.
//! It does not read the filesystem, resolve symlinks, authorize paths or make
//! names absolute. The tests of the source branches do not show that the code runs
//! correctly on a native Windows or Cygwin host. No test has run on such a host.
//! The input is a complete byte name with no NUL, and an empty input stays empty.
//!
//! # Recommendations
//! Set explicit limits for the application, and treat the result as lexical bytes.
//! Next, apply the filesystem and containment policy of the caller in a separate step.
use std::{collections::TryReserveError, error::Error, fmt};

/// `Limits` holds the bounds of the caller on the input bytes and the returned bytes.
///
/// Neither bound counts a C terminator.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Sets the largest number of input bytes that a call accepts.
    pub max_input_bytes: usize,
    /// Sets the largest number of bytes that a call returns.
    pub max_output_bytes: usize,
}

/// `SourcePlatform` names the branch of the native source to apply, whatever the host.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourcePlatform {
    /// Keeps backslashes and extended prefixes as ordinary bytes.
    Unix,
    /// Converts backslashes and removes extended prefixes, as native C does.
    WindowsCygwin,
}

/// `SimplifiedName` holds the span that the native code reports.
///
/// The span can contain an early C terminator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SimplifiedName {
    /// Holds the bytes that the native return value counts, without its final terminator.
    span: Vec<u8>,
}
impl SimplifiedName {
    /// Borrow exactly the bytes that the native return value counts.
    /// # Returns
    /// The reported span. On Windows/Cygwin, the input `//?/` gives the one-byte span `[0]`.
    pub fn reported_span(&self) -> &[u8] {
        &self.span
    }

    /// Borrow the bytes of the diagnostic C string, which end before the first NUL.
    /// # Returns
    /// The visible filename bytes, which can be shorter than [`Self::reported_span`].
    pub fn visible_bytes(&self) -> &[u8] {
        let end = self
            .span
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(self.span.len());
        &self.span[..end]
    }
}

/// `PathError` reports a rejected input, an exceeded bound or a failed allocation.
#[derive(Debug)]
pub enum PathError {
    /// The input exceeds the bound of the caller or the largest buffer that memory can address.
    InputLimit,
    /// The simplified output exceeds the bound of the caller.
    OutputLimit,
    /// The input contains a NUL, which has no accepted meaning in a filename.
    EmbeddedNul,
    /// The native simplification would write its terminator before the start of the buffer.
    TerminalCancellation,
    /// The reservation of the buffer failed before the input was copied.
    Allocation(TryReserveError),
}
impl fmt::Display for PathError {
    /// Describe the failed boundary.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputLimit => f.write_str("filename input exceeds its limit"),
            Self::OutputLimit => f.write_str("filename output exceeds its limit"),
            Self::EmbeddedNul => f.write_str("filename contains NUL"),
            Self::TerminalCancellation => {
                f.write_str("relative terminal cancellation has no safe native result")
            }
            Self::Allocation(error) => write!(f, "filename allocation failed: {error}"),
        }
    }
}
impl Error for PathError {
    /// Return the original failure of the reservation as the source.
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Allocation(error) => Some(error),
            _ => None,
        }
    }
}

/// Back out the previous component exactly as native `backup_dir` does.
/// # Arguments
/// * `bytes` - The working bytes that the loops have already written.
/// * `end` - The signed output cursor. A value of minus one suppresses a following slash.
/// # Returns
/// Whether the function could remove the final component.
fn backup_dir(bytes: &[u8], end: &mut isize) -> bool {
    if *end <= 0 {
        return false;
    }
    let j = *end as usize;
    let mut i = j - 1;
    while i > 0 && bytes[i - 1] != b'/' {
        i -= 1;
    }
    if bytes[i] == b'.' && i + 2 == j && bytes[i + 1] == b'.' {
        return false;
    }
    *end = i as isize - 1;
    true
}

/// Simplify a filename with no NUL by the Unix lexical rules of Fossil.
///
/// Duplicate separators collapse to one, but an initial double slash stays.
/// The function removes a leading `./`, each internal `/./` and each
/// `/component/..` that can cancel. Unresolved `..` components stay, even above
/// an absolute root. As in native Fossil, a nonempty name whose simplified bytes
/// are empty becomes `/`.
///
/// A relative name that cancels past its start, such as `a/..`, is a relative
/// terminal cancellation. It leaves native C with a negative cursor and a
/// terminator outside the buffer, and this function rejects it.
///
/// The input buffer holds at most `max_input_bytes` plus one sentinel byte. The
/// output bound applies to the actual length of the simplified name.
/// # Arguments
/// * `input` - The complete filename bytes, with no embedded NUL.
/// * `keep_trailing_slash` - Keep a trailing separator where the native rules keep it.
/// * `limits` - The bounds of the caller on the input bytes and the output bytes.
/// # Returns
/// The owned simplified bytes, without a terminating NUL.
/// # Errors
/// Returns [`PathError`] for an exceeded bound, a NUL, a failed allocation or a
/// relative terminal cancellation.
/// # Examples
/// ```
/// use gossil::file_path::{Limits, simplify_name};
/// let bounds = Limits { max_input_bytes: 128, max_output_bytes: 128 };
/// assert_eq!(simplify_name(b"a/b/../c/", false, bounds).unwrap(), b"a/c");
/// ```
pub fn simplify_name(
    input: &[u8],
    keep_trailing_slash: bool,
    limits: Limits,
) -> Result<Vec<u8>, PathError> {
    simplify_name_for_source(input, keep_trailing_slash, limits, SourcePlatform::Unix)
        .map(|name| name.span)
}

/// Simplify a filename by the branch of the Fossil source that the caller selects.
///
/// The Windows/Cygwin branch converts every backslash to a slash. Then it
/// removes `//?/` and compares the next three bytes with `UNC`, by the ASCII
/// case map of SQLite. The function does not check drives, the names after the
/// `UNC` prefix or whether a name is valid. For the input `//?/`, native C copies the terminator of the input
/// to byte zero but reports a length of one. So the result keeps that NUL in its
/// reported span, and its visible C string is empty.
///
/// The input buffer holds at most the length of the input plus one sentinel
/// byte. The output bound counts the reported span, with any NUL inside that
/// span. After the loops compact the name, the native lookahead can still read
/// the original bytes at the end of the buffer. A relative terminal cancellation
/// returns the same error as [`simplify_name`].
/// # Arguments
/// * `input` - The complete filename bytes, with no NUL.
/// * `keep_trailing_slash` - The native option to keep a trailing slash.
/// * `limits` - The bounds on the input and on the reported output span.
/// * `platform` - The source branch to apply, whatever the host.
/// # Returns
/// The bytes that the native code reports, with a separate accessor for the visible C string.
/// # Errors
/// Returns [`PathError`] for an exceeded bound, a NUL, a failed allocation or a
/// relative terminal cancellation.
/// # Examples
/// ```
/// use gossil::file_path::{Limits, SourcePlatform, simplify_name_for_source};
/// let bounds = Limits { max_input_bytes: 128, max_output_bytes: 128 };
/// let windows = SourcePlatform::WindowsCygwin;
/// let unc = simplify_name_for_source(b"\\\\?\\UNC\\a\\b", false, bounds, windows).unwrap();
/// assert_eq!(unc.visible_bytes(), b"//a/b");
/// let prefix = simplify_name_for_source(b"//?/", false, bounds, windows).unwrap();
/// assert_eq!(prefix.reported_span(), b"\0");
/// assert!(prefix.visible_bytes().is_empty());
/// ```
pub fn simplify_name_for_source(
    input: &[u8],
    keep_trailing_slash: bool,
    limits: Limits,
    platform: SourcePlatform,
) -> Result<SimplifiedName, PathError> {
    let capacity = input
        .len()
        .checked_add(1)
        .filter(|size| *size <= isize::MAX as usize)
        .filter(|_| input.len() <= limits.max_input_bytes)
        .ok_or(PathError::InputLimit)?;
    if input.contains(&0) {
        return Err(PathError::EmbeddedNul);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity)
        .map_err(PathError::Allocation)?;
    bytes.extend_from_slice(input);
    bytes.push(0);
    let mut n = input.len();
    if n == 0 {
        bytes.clear();
        return Ok(SimplifiedName { span: bytes });
    }
    let mut i = 1;
    if platform == SourcePlatform::WindowsCygwin {
        for byte in &mut bytes[..n] {
            if *byte == b'\\' {
                *byte = b'/';
            }
        }
        if n > 3 && &bytes[..4] == b"//?/" {
            // fossil_strnicmp delegates to sqlite3_strnicmp. ASCII `UNC` has no NUL,
            // so a short input mismatches at its sentinel. Never read beyond that
            // sentinel, because the native comparison stops at the first mismatch.
            let unc = bytes[4..]
                .iter()
                .take(3)
                .map(u8::to_ascii_lowercase)
                .eq(b"unc".iter().copied());
            if unc {
                i += 6;
                bytes[0] = b'/';
            } else {
                i += 4;
                bytes[0] = bytes[4];
            }
        }
    }
    // file_simplify_name: trim trailing slashes before collapsing duplicates.
    if !keep_trailing_slash {
        while n > 1 && bytes[n - 1] == b'/' {
            n -= 1;
        }
    }
    let mut j = 1;
    while i < n {
        bytes[j] = bytes[i];
        j += 1;
        while bytes[i] == b'/' && i < n - 1 && bytes[i + 1] == b'/' {
            i += 1;
        }
        i += 1;
    }
    n = j;
    // file.c:1229 changes only n. Preserve the original tail, because the /./
    // lookahead at file.c:1238 can read beyond n, within the input buffer.
    i = 0;
    while i < n - 1 && bytes[i] == b'.' && bytes[i + 1] == b'/' {
        i += 2;
    }
    let mut end: isize = 0;
    while i < n {
        if bytes[i] == b'/' {
            if bytes[i + 1] == b'.' && (i + 2 == n || bytes[i + 2] == b'/') {
                i += 2;
                continue;
            }
            if bytes[i + 1] == b'.'
                && i + 2 < n
                && bytes[i + 2] == b'.'
                && (i + 3 == n || bytes[i + 3] == b'/')
                && backup_dir(&bytes, &mut end)
            {
                i += 3;
                continue;
            }
        }
        if end >= 0 {
            bytes[end as usize] = bytes[i];
        }
        end += 1;
        i += 1;
    }
    if end == 0 {
        bytes[0] = b'/';
        end = 1;
    }
    if end < 0 {
        return Err(PathError::TerminalCancellation);
    }
    let length = end as usize;
    if length > limits.max_output_bytes {
        return Err(PathError::OutputLimit);
    }
    bytes.truncate(length);
    Ok(SimplifiedName { span: bytes })
}
