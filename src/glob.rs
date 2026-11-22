//! Bounded source port of Fossil `src/glob.c`, copyright 2011 D. Richard Hipp.
//! Original code is under the 2-Clause BSD license; see `evidence/glob/fossil-license.txt`.
//! Matching calls the same libSQL SQLite engine as the storage boundary.
//! Byte and pattern-count limits do not bound matching CPU time or stack use.
//!
//! Lists use comma and ASCII-whitespace delimiters. A leading single or double
//! quote retains delimiters until the same closing quote; an unmatched quote
//! consumes the remaining prefix. There is no quote escape syntax. Every input
//! ends at its first NUL. Matching returns the first one-based index, or zero.
//!
//! [`glob_expr`](crate::glob::glob_expr) retains a distinct extra byte advance
//! after a closing quote.
//! Its value argument is trusted SQL. Bind untrusted candidate values separately.
//! These functions perform no traversal, settings lookup or tracking mutation.
//!
//! ## Recommendations
//!
//! Use [`glob_create`](crate::glob::glob_create) for repeated matching and
//! [`glob_multi_match`](crate::glob::glob_multi_match) for one scan.
//! Next, set explicit byte/count budgets and bind candidate values separately
//! when executing SQL from [`glob_expr`](crate::glob::glob_expr).
use std::collections::TryReserveError;

/// Supply finite byte/count budgets; input byte lengths stop before the first NUL.
/// Prefix detection itself scans the original slice until NUL or its end.
/// These budgets do not impose a separate CPU-time or stack limit.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum pattern-list bytes.
    pub list_bytes: usize,
    /// Maximum candidate bytes.
    pub candidate_bytes: usize,
    /// Maximum number of parsed patterns.
    pub patterns: usize,
    /// Maximum SQL expression bytes, including the caller's value expression.
    pub expression_bytes: usize,
}
/// Reports a rejected budget or preserved allocation failure.
#[derive(Debug)]
pub enum GlobError {
    /// A named input or output budget was exceeded.
    Limit(&'static str),
    /// Capacity arithmetic overflowed.
    Overflow,
    /// Fallible reservation failed.
    Allocation(TryReserveError),
}
impl std::fmt::Display for GlobError {
    /// Format a stable budget or capacity diagnostic, or the allocation cause.
    /// # Errors
    /// Returns the destination formatter error if writing the diagnostic fails.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Limit(name) => write!(f, "glob {name} limit exceeded"),
            Self::Overflow => write!(f, "glob capacity overflow"),
            Self::Allocation(e) => write!(f, "glob allocation: {e}"),
        }
    }
}
impl std::error::Error for GlobError {
    /// Expose the original allocation error; budget and overflow errors have no cause.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Allocation(e) => Some(e),
            _ => None,
        }
    }
}
/// Patterns retain a terminal NUL for `sqlite3_strglob`.
#[derive(Debug)]
pub struct Glob {
    /// Owned native strings in source order.
    patterns: Vec<Vec<u8>>,
    /// Budgets retained for candidate matching.
    limits: Limits,
}
/// Returns the C-string prefix shared by all native inputs.
/// # Arguments
/// * `bytes` - Input bytes whose first NUL ends the string.
/// # Returns
/// The prefix before the first NUL, or all bytes when there is no NUL.
fn cbytes(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}
/// Recognizes space and bytes 9 through 13 as Fossil's ASCII whitespace.
/// # Arguments
/// * `b` - Byte to classify.
/// # Returns
/// Whether the byte is Fossil ASCII whitespace.
fn space(b: u8) -> bool {
    matches!(b, b' ' | 9..=13)
}
/// Reserve and append without an infallible growth path.
/// # Arguments
/// * `out` - Destination expression buffer.
/// * `bytes` - Bytes to append.
/// * `max` - Maximum permitted total expression bytes.
/// # Returns
/// Unit after appending all bytes within the budget.
/// # Errors
/// Returns an expression budget, capacity overflow or original allocation error.
fn append(out: &mut Vec<u8>, bytes: &[u8], max: usize) -> Result<(), GlobError> {
    let size = out
        .len()
        .checked_add(bytes.len())
        .ok_or(GlobError::Overflow)?;
    if size > max {
        return Err(GlobError::Limit("expression"));
    }
    out.try_reserve(bytes.len())
        .map_err(GlobError::Allocation)?;
    out.extend_from_slice(bytes);
    Ok(())
}
/// Copy a bounded native string with its terminating NUL.
/// # Arguments
/// * `bytes` - Input prefix to copy.
/// # Returns
/// An owned copy followed by one NUL byte.
/// # Errors
/// Returns a capacity overflow or original allocation error.
fn terminated(bytes: &[u8]) -> Result<Vec<u8>, GlobError> {
    let mut result = Vec::new();
    result
        .try_reserve_exact(bytes.len().checked_add(1).ok_or(GlobError::Overflow)?)
        .map_err(GlobError::Allocation)?;
    result.extend_from_slice(bytes);
    result.push(0);
    Ok(result)
}
/// Parse a reusable Fossil pattern list; absent or delimiter-only lists match nothing.
///
/// Only an initial quote opens a quoted term. Commas and ASCII whitespace delimit
/// unquoted terms, unmatched quotes consume the remainder, and empty quoted terms
/// remain patterns. The list ends at its first NUL; no escape syntax is applied.
/// # Examples
/// ```
/// use gossil::glob::{glob_create, Limits};
/// let limits = Limits { list_bytes: 64, candidate_bytes: 64,
///                       patterns: 8, expression_bytes: 256 };
/// let glob = glob_create(Some(b"*.rs,src/*"), limits)?;
/// assert_eq!(glob.glob_match(b"src/lib.rs")?, 1);
/// assert_eq!(glob.patterns().len(), 2);
/// # Ok::<(), gossil::glob::GlobError>(())
/// ```
/// # Errors
/// Returns [`GlobError::Limit`] when list bytes or pattern count exceed their
/// budgets, [`GlobError::Overflow`] for capacity arithmetic, or
/// [`GlobError::Allocation`] when reservation fails.
/// # Arguments
/// * `list` - Optional pattern list; `None` has the same meaning as an empty list.
/// * `limits` - Finite input, pattern and output budgets.
/// # Returns
/// Owned parsed patterns and the retained candidate budget.
pub fn glob_create(list: Option<&[u8]>, limits: Limits) -> Result<Glob, GlobError> {
    let list = cbytes(list.unwrap_or_default());
    if list.len() > limits.list_bytes {
        return Err(GlobError::Limit("list"));
    }
    let mut patterns = Vec::new();
    let mut pos = 0;
    while pos < list.len() {
        while pos < list.len() && (space(list[pos]) || list[pos] == b',') {
            pos += 1;
        }
        if pos == list.len() {
            break;
        }
        let delimiter = if matches!(list[pos], b'\'' | b'"') {
            let d = list[pos];
            pos += 1;
            d
        } else {
            b','
        };
        let start = pos;
        while pos < list.len() && list[pos] != delimiter && !(delimiter == b',' && space(list[pos]))
        {
            pos += 1;
        }
        if patterns.len() >= limits.patterns {
            return Err(GlobError::Limit("patterns"));
        }
        patterns.try_reserve(1).map_err(GlobError::Allocation)?;
        patterns.push(terminated(&list[start..pos])?);
        if pos < list.len() {
            pos += 1;
        }
    }
    Ok(Glob { patterns, limits })
}
impl Glob {
    /// Returns parsed patterns without their native terminators.
    /// # Returns
    /// An iterator over patterns in source order, without terminal NUL bytes.
    pub fn patterns(&self) -> impl ExactSizeIterator<Item = &[u8]> {
        self.patterns.iter().map(|p| &p[..p.len() - 1])
    }
    /// Return the first matching one-based pattern index, or zero.
    ///
    /// The candidate ends at its first NUL. Matching uses the selected SQLite
    /// engine; byte budgets do not bound engine CPU time or stack use.
    /// # Examples
    /// ```
    /// use gossil::glob::{glob_create, Limits};
    /// let limits = Limits { list_bytes: 64, candidate_bytes: 64,
    ///                       patterns: 8, expression_bytes: 256 };
    /// let glob = glob_create(Some(b"a*,*.rs"), limits)?;
    /// assert_eq!(glob.glob_match(b"abc.rs")?, 1);
    /// assert_eq!(glob.glob_match(b"lib.rs")?, 2);
    /// assert_eq!(glob.glob_match(b"notes.md")?, 0);
    /// # Ok::<(), gossil::glob::GlobError>(())
    /// ```
    /// # Errors
    /// Returns [`GlobError::Limit`] when candidate bytes exceed the retained
    /// budget, [`GlobError::Overflow`] for terminator capacity arithmetic, or
    /// [`GlobError::Allocation`] when copying the candidate fails.
    /// # Arguments
    /// * `candidate` - Candidate bytes, ending at the first NUL.
    /// # Returns
    /// The first matching one-based pattern index, or zero when no pattern matches.
    pub fn glob_match(&self, candidate: &[u8]) -> Result<usize, GlobError> {
        let candidate = cbytes(candidate);
        if candidate.len() > self.limits.candidate_bytes {
            return Err(GlobError::Limit("candidate"));
        }
        let candidate = terminated(candidate)?;
        for (i, p) in self.patterns.iter().enumerate() {
            // SAFETY: both buffers are live, NUL-terminated, and contain no interior NUL.
            if unsafe { libsql_ffi::sqlite3_strglob(p.as_ptr().cast(), candidate.as_ptr().cast()) }
                == 0
            {
                return Ok(i + 1);
            }
        }
        Ok(0)
    }
}
/// Match a Fossil list once and return the first one-based match index, or zero.
/// A matching term returns immediately, without checking later terms against the
/// pattern-count limit. Terms below 100 bytes use a temporary stack buffer;
/// longer terms use a fallible allocation. Neither choice bounds engine recursion.
/// # Errors
/// Returns [`GlobError::Limit`] when list bytes, candidate bytes or an inspected
/// pattern count exceed their budgets, [`GlobError::Overflow`] for terminator
/// capacity arithmetic, or [`GlobError::Allocation`] when reservation fails.
/// # Examples
/// ```
/// use gossil::glob::{glob_multi_match, Limits};
/// let limits = Limits { list_bytes: 64, candidate_bytes: 64,
///                       patterns: 1, expression_bytes: 256 };
/// // Early success does not inspect the second term against the count budget.
/// assert_eq!(glob_multi_match(Some(b"a*,b*"), b"abc", limits)?, 1);
/// assert!(glob_multi_match(Some(b"a*,b*"), b"zzz", limits).is_err());
/// # Ok::<(), gossil::glob::GlobError>(())
/// ```
/// # Arguments
/// * `list` - Optional pattern list, ending at the first NUL.
/// * `candidate` - Candidate bytes, ending at the first NUL.
/// * `limits` - Finite input, pattern and output budgets.
/// # Returns
/// The first matching one-based pattern index, or zero when no pattern matches.
pub fn glob_multi_match(
    list: Option<&[u8]>,
    candidate: &[u8],
    limits: Limits,
) -> Result<usize, GlobError> {
    let list = cbytes(list.unwrap_or_default());
    if list.len() > limits.list_bytes {
        return Err(GlobError::Limit("list"));
    }
    let candidate = cbytes(candidate);
    if candidate.len() > limits.candidate_bytes {
        return Err(GlobError::Limit("candidate"));
    }
    let candidate = terminated(candidate)?;
    let mut pos = 0;
    let mut count = 0;
    while pos < list.len() {
        while pos < list.len() && (space(list[pos]) || list[pos] == b',') {
            pos += 1;
        }
        if pos == list.len() {
            break;
        }
        let delimiter = if matches!(list[pos], b'\'' | b'"') {
            let d = list[pos];
            pos += 1;
            d
        } else {
            b','
        };
        let start = pos;
        while pos < list.len() && list[pos] != delimiter && !(delimiter == b',' && space(list[pos]))
        {
            pos += 1;
        }
        if count >= limits.patterns {
            return Err(GlobError::Limit("patterns"));
        }
        count += 1;
        let length = pos - start;
        let mut short = [0u8; 100];
        let long;
        let pattern = if length < short.len() {
            short[..length].copy_from_slice(&list[start..pos]);
            &short[..=length]
        } else {
            long = terminated(&list[start..pos])?;
            &long
        };
        // SAFETY: pattern and candidate are live NUL-terminated buffers with no interior NUL.
        if unsafe {
            libsql_ffi::sqlite3_strglob(pattern.as_ptr().cast(), candidate.as_ptr().cast())
        } == 0
        {
            return Ok(count);
        }
        if pos < list.len() {
            pos += 1;
        }
    }
    Ok(0)
}
/// Build a SQL expression from trusted SQL and Fossil patterns, with native quote advancement.
/// The caller MUST supply a trusted SQL value expression. Only pattern apostrophes
/// are escaped; `value` is neither quoted nor sanitized. Never pass untrusted SQL.
/// Use a trusted column or parameter expression such as `b"?1"` and bind candidate
/// bytes separately when executing the result through a caller-owned SQL interface.
///
/// This function executes no SQL. All input slices end at their first NUL.
///
/// Empty, absent and delimiter-only lists return `b"0"`; a quoted empty pattern
/// produces a real GLOB term. Adjacent text after a closing quote can therefore
/// differ from [`glob_create`] and [`glob_multi_match`] parsing.
/// # Errors
/// Returns [`GlobError::Limit`] when list bytes, pattern count or expression
/// bytes exceed their budgets, [`GlobError::Overflow`] for output capacity
/// arithmetic, or [`GlobError::Allocation`] when reservation fails.
/// # Examples
/// ```
/// use gossil::glob::{glob_expr, Limits};
/// let limits = Limits { list_bytes: 64, candidate_bytes: 64,
///                       patterns: 8, expression_bytes: 256 };
/// // Bind the candidate separately when executing this trusted parameter expression.
/// assert_eq!(glob_expr(b"?1", Some(b"*.rs"), limits)?, b"(?1 GLOB '*.rs')");
/// assert_eq!(glob_expr(b"?1", None, limits)?, b"0");
/// # Ok::<(), gossil::glob::GlobError>(())
/// ```
/// # Arguments
/// * `value` - Trusted SQL expression, neither quoted nor sanitized.
/// * `list` - Optional pattern list, ending at the first NUL.
/// * `limits` - Finite input, pattern and expression budgets.
/// # Returns
/// SQL expression bytes, or `b"0"` when the list has no patterns.
pub fn glob_expr(value: &[u8], list: Option<&[u8]>, limits: Limits) -> Result<Vec<u8>, GlobError> {
    let list = cbytes(list.unwrap_or_default());
    let value = cbytes(value);
    if list.len() > limits.list_bytes {
        return Err(GlobError::Limit("list"));
    }
    let mut out = Vec::new();
    let mut pos = 0;
    let mut terms = 0;
    while pos < list.len() {
        while pos < list.len() && (space(list[pos]) || list[pos] == b',') {
            pos += 1;
        }
        if pos == list.len() {
            break;
        }
        let delimiter = if matches!(list[pos], b'\'' | b'"') {
            let d = list[pos];
            pos += 1;
            d
        } else {
            b','
        };
        let start = pos;
        while pos < list.len() && list[pos] != delimiter && !(delimiter == b',' && space(list[pos]))
        {
            pos += 1;
        }
        if terms >= limits.patterns {
            return Err(GlobError::Limit("patterns"));
        }
        append(
            &mut out,
            if terms == 0 { b"(" } else { b" OR " },
            limits.expression_bytes,
        )?;
        append(&mut out, value, limits.expression_bytes)?;
        append(&mut out, b" GLOB '", limits.expression_bytes)?;
        for &b in &list[start..pos] {
            append(&mut out, &[b], limits.expression_bytes)?;
            if b == b'\'' {
                append(&mut out, b"'", limits.expression_bytes)?;
            }
        }
        append(&mut out, b"'", limits.expression_bytes)?;
        terms += 1;
        if delimiter != b',' && pos < list.len() {
            pos += 1;
        }
        if pos < list.len() {
            pos += 1;
        }
    }
    append(
        &mut out,
        if terms == 0 { b"0" } else { b")" },
        limits.expression_bytes,
    )?;
    Ok(out)
}
