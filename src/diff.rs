//! Compute bounded line edit scripts ported from Fossil's `src/diff.c`.
//!
//! This module supplies the normalized-byte line layer of `text_diff`. The
//! callers must perform `blob_to_utf8_no_bom` conversion separately. Bytes need
//! not be valid UTF-8. No BOM removal, UTF-16 conversion, token diff, renderer,
//! regular expression filter, filesystem or repository access occurs here.
//!
//! Derived from Fossil b3a1c8efb2731dffbd5cc38c58ac349746de6da6.
//! Copyright (c) 2007 D. Richard Hipp. Distributed under the 2-Clause BSD license.
//!
//! # Recommendations
//! Normalize inputs before calling this layer and select finite caller limits.

/// Whitespace comparison used when hashing and comparing lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Whitespace {
    /// Compare all bytes except the line terminator.
    #[default]
    Exact,
    /// Ignore Fossil's trailing whitespace bytes (8, 9, 10, 12, 13, 32).
    IgnoreEol,
    /// Ignore Fossil's whitespace bytes throughout each line.
    IgnoreAll,
}

/// Options supported by the raw, line-based portion of Fossil `text_diff`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineDiffOptions {
    /// Select exact, trailing-space or all-space comparison.
    pub whitespace: Whitespace,
    /// Remove a trailing carriage return, including on an unterminated line.
    pub strip_cr: bool,
    /// Exchange FROM and TO before computing the script.
    pub invert: bool,
    /// Apply native readability shifts (`DIFF_NOOPT` disables this).
    pub optimize: bool,
    /// Reject scripts with more than 10,000 inserted plus deleted lines.
    pub not_too_big: bool,
}
impl Default for LineDiffOptions {
    /// Use exact comparison and native readability optimization.
    fn default() -> Self {
        Self {
            whitespace: Whitespace::Exact,
            strip_cr: false,
            invert: false,
            optimize: true,
            not_too_big: false,
        }
    }
}

/// Caller-selected bounds on admitted input and algorithm work.
#[derive(Clone, Copy, Debug)]
pub struct DiffLimits {
    /// Maximum sum of the two input byte lengths.
    pub max_input_bytes: usize,
    /// Maximum sum of the two input line counts.
    pub max_lines: usize,
    /// Maximum number of triples, including the terminating zero triple.
    pub max_edits: usize,
    /// Maximum charged byte inspections, comparisons and search iterations.
    pub max_work: u64,
}
impl Default for DiffLimits {
    /// Provide finite defaults suitable for moderate source files.
    fn default() -> Self {
        Self {
            max_input_bytes: 16 * 1024 * 1024,
            max_lines: 200_000,
            max_edits: 200_001,
            max_work: 100_000_000,
        }
    }
}

/// One COPY/DELETE/INSERT triple, in numbers of lines.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Edit {
    /// Common lines consumed from both inputs.
    pub copy: usize,
    /// Lines consumed only from FROM.
    pub delete: usize,
    /// Lines consumed only from TO.
    pub insert: usize,
}

/// Native rejection or a caller resource boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffError {
    /// Embedded NUL or a line longer than 32767 bytes.
    Binary,
    /// Whitespace mode produced the native unchanged-script rejection.
    WhitespaceOnly,
    /// `not_too_big` rejected more than 10,000 changed lines.
    TooManyChanges,
    /// A caller bound, checked arithmetic or allocation failed.
    Limit,
}

/// Remaining charged work for one invocation.
struct Work(u64);
impl Work {
    /// Charge a bounded amount before performing work.
    ///
    /// # Arguments
    /// * `amount` - Work units to subtract from the remaining budget.
    ///
    /// # Returns
    /// `Ok(())` after subtracting the charge.
    ///
    /// # Errors
    /// Returns [`DiffError::Limit`] when the charge exceeds the remaining work.
    fn charge(&mut self, amount: usize) -> Result<(), DiffError> {
        self.0 = self
            .0
            .checked_sub(u64::try_from(amount).map_err(|_| DiffError::Limit)?)
            .ok_or(DiffError::Limit)?;
        Ok(())
    }
}

/// Native DLine data, borrowing the caller's normalized byte buffer.
struct Line<'a> {
    /// Bytes excluding LF and, optionally, trailing CR.
    text: &'a [u8],
    /// Native length-bearing hash.
    hash: u64,
    /// First non-space position used by indentation heuristics.
    indent: usize,
    /// Native trimmed length, including its all-space edge case.
    nw: usize,
    /// One-based next hash chain element.
    next: usize,
    /// One-based bucket head, shared with this line's array slot.
    bucket: usize,
}

/// Match the exact native whitespace table (which includes backspace).
///
/// # Arguments
/// * `c` - Byte to classify.
///
/// # Returns
/// Whether the byte is one of 8, 9, 10, 12, 13 or 32.
fn space(c: u8) -> bool {
    matches!(c, 8 | 9 | 10 | 12 | 13 | 32)
}

/// Allocate a vector with its maximum capacity checked before use.
///
/// # Arguments
/// * `n` - Number of elements for which to reserve capacity.
///
/// # Returns
/// An empty vector with capacity for at least `n` elements.
///
/// # Errors
/// Returns [`DiffError::Limit`] when the capacity cannot be reserved.
fn reserved<T>(n: usize) -> Result<Vec<T>, DiffError> {
    let mut result = Vec::new();
    result.try_reserve_exact(n).map_err(|_| DiffError::Limit)?;
    Ok(result)
}

/// Split normalized bytes using the native hash and length rules.
///
/// # Arguments
/// * `input` - Normalized bytes to split into lines.
/// * `options` - Whitespace and trailing-CR rules used to build line metadata.
/// * `work` - Remaining budget charged for byte inspections.
///
/// # Returns
/// Borrowed lines with native hashes, whitespace metadata and hash chains.
///
/// # Errors
/// Returns [`DiffError::Binary`] for NUL or an overlong line, or
/// [`DiffError::Limit`] when work or allocation bounds fail.
fn lines<'a>(
    input: &'a [u8],
    options: LineDiffOptions,
    work: &mut Work,
) -> Result<Vec<Line<'a>>, DiffError> {
    work.charge(input.len())?;
    if input.contains(&0) {
        return Err(DiffError::Binary);
    }
    let count = input.iter().filter(|&&c| c == b'\n').count()
        + usize::from(!input.is_empty() && input.last() != Some(&b'\n'));
    let mut result: Vec<Line<'a>> = reserved(count)?;
    for raw in input.split_inclusive(|&c| c == b'\n') {
        let raw = raw.strip_suffix(b"\n").unwrap_or(raw);
        if raw.len() > 32767 {
            return Err(DiffError::Binary);
        }
        let text = if options.strip_cr {
            raw.strip_suffix(b"\r").unwrap_or(raw)
        } else {
            raw
        };
        let mut k = text.len();
        if options.whitespace != Whitespace::Exact {
            while k > 0 && space(text[k - 1]) {
                k -= 1;
            }
        }
        let mut indent = 0;
        let mut nw = 0;
        let mut hash = 0u64;
        let length = if options.whitespace == Whitespace::IgnoreAll {
            // Fossil's plain char is signed on supported macOS/Linux targets.
            while indent < k && (text[indent] as i8) <= 32 {
                indent += 1;
            }
            nw = k - indent;
            let mut numws = 0;
            for &c in &text[indent..k] {
                if space(c) {
                    numws += 1;
                } else {
                    hash = (hash ^ (i64::from(c as i8) as u64))
                        .wrapping_mul(9_000_000_000_000_000_041);
                }
            }
            k - numws - indent
        } else {
            let (chunks, remainder) = text[..k].as_chunks::<8>();
            for chunk in chunks {
                hash = (hash ^ u64::from_ne_bytes(*chunk)).wrapping_mul(9_000_000_000_000_000_041);
            }
            let mut tail = [0; 8];
            tail[..remainder.len()].copy_from_slice(remainder);
            hash ^= u64::from_ne_bytes(tail);
            k
        };
        work.charge(text.len())?;
        hash = ((hash % 281_474_976_710_597) << 15) | length as u64;
        result.push(Line {
            text,
            hash,
            indent,
            nw,
            next: 0,
            bucket: 0,
        });
    }
    for i in 0..result.len() {
        let bucket = (result[i].hash % result.len() as u64) as usize;
        result[i].next = result[bucket].bucket;
        result[bucket].bucket = i + 1;
    }
    Ok(result)
}

/// All mutable algorithm state for a single diff.
struct Context<'a> {
    /// FROM lines.
    from: Vec<Line<'a>>,
    /// TO lines and native bucket chains.
    to: Vec<Line<'a>>,
    /// Output triples, including a final sentinel after `all`.
    edits: Vec<Edit>,
    /// Whether the all-space comparator is active.
    all_ws: bool,
    /// Remaining work.
    work: Work,
    /// Output capacity bound.
    max_edits: usize,
}
impl Context<'_> {
    /// Compare arbitrary lines with native hash and whitespace semantics.
    ///
    /// # Arguments
    /// * `a` - Select FROM for the first line when true, otherwise TO.
    /// * `i` - Index of the first line in the selected input.
    /// * `b` - Select FROM for the second line when true, otherwise TO.
    /// * `j` - Index of the second line in the selected input.
    ///
    /// # Returns
    /// Whether the selected lines compare equal under the active whitespace mode.
    ///
    /// # Errors
    /// Returns [`DiffError::Limit`] when comparison work exceeds the budget.
    fn equal(&mut self, a: bool, i: usize, b: bool, j: usize) -> Result<bool, DiffError> {
        let x = if a { &self.from[i] } else { &self.to[i] };
        let y = if b { &self.from[j] } else { &self.to[j] };
        self.work.charge(1)?;
        if x.hash != y.hash {
            return Ok(false);
        }
        let n = (x.hash & 32767) as usize;
        self.work.charge(n)?;
        if x.text[..n] == y.text[..n] {
            return Ok(true);
        }
        if !self.all_ws {
            return Ok(false);
        }
        self.work.charge(x.text.len() + y.text.len())?;
        let (mut ai, mut bi) = (x.indent, y.indent);
        while ai < x.text.len() || bi < y.text.len() {
            // Equal non-space counts keep both cursors in bounds together.
            if x.text[ai] != y.text[bi] {
                return Ok(false);
            }
            ai += 1;
            bi += 1;
            while ai < x.text.len() && space(x.text[ai]) {
                ai += 1;
            }
            while bi < y.text.len() && space(y.text[bi]) {
                bi += 1;
            }
            // Equal IgnoreAll hashes encode equal non-space counts.
        }
        Ok(x.text.len() - ai == y.text.len() - bi)
    }

    /// Coalesce triples emitted by `all` in native order.
    ///
    /// Positive copies separate changes, so only a copy-only tail can merge.
    ///
    /// # Arguments
    /// * `copy` - Common lines consumed from both inputs.
    /// * `delete` - Lines consumed only from FROM.
    /// * `insert` - Lines consumed only from TO.
    ///
    /// # Returns
    /// `Ok(())` after merging the counts into the tail or appending a triple.
    ///
    /// # Errors
    /// Returns [`DiffError::Limit`] when work or edit capacity is exhausted.
    fn append(&mut self, copy: usize, delete: usize, insert: usize) -> Result<(), DiffError> {
        self.work.charge(1)?;
        if let Some(last) = self.edits.last_mut()
            && last.insert == 0
            && last.delete == 0
        {
            last.copy += copy;
            last.delete += delete;
            last.insert += insert;
            return Ok(());
        }
        if self.edits.len() >= self.max_edits {
            return Err(DiffError::Limit);
        }
        self.edits.push(Edit {
            copy,
            delete,
            insert,
        });
        Ok(())
    }

    /// Native exhaustive fallback for spans with product below 2500.
    ///
    /// # Arguments
    /// * `s1` - Inclusive start of the FROM search span.
    /// * `e1` - Exclusive end of the FROM search span.
    /// * `s2` - Inclusive start of the TO search span.
    /// * `e2` - Exclusive end of the TO search span.
    ///
    /// # Returns
    /// `(from_start, from_end, to_start, to_end)` bounds of the longest common
    /// contiguous match, with exclusive ends; both ranges are empty if none exists.
    ///
    /// # Errors
    /// Returns [`DiffError::Limit`] when search work exceeds the budget.
    fn optimal(
        &mut self,
        s1: usize,
        e1: usize,
        s2: usize,
        e2: usize,
    ) -> Result<(usize, usize, usize, usize), DiffError> {
        let (mut sx, mut sy, mut length) = (s1, s2, 0);
        let mut i = s1;
        while i < e1 - length {
            let mut j = s2;
            while j < e2 - length {
                if self.equal(true, i, false, j)?
                    && (length == 0 || self.equal(true, i + length, false, j + length)?)
                {
                    let mut k = 1;
                    while i + k < e1 && j + k < e2 && self.equal(true, i + k, false, j + k)? {
                        k += 1;
                    }
                    if k > length {
                        sx = i;
                        sy = j;
                        length = k;
                    }
                }
                j += 1;
            }
            i += 1;
        }
        Ok((sx, sx + length, sy, sy + length))
    }

    /// Native hash-chain heuristic, score and strict tie-breaking.
    ///
    /// # Arguments
    /// * `s1` - Inclusive start of the FROM search span.
    /// * `e1` - Exclusive end of the FROM search span.
    /// * `s2` - Inclusive start of the TO search span.
    /// * `e2` - Exclusive end of the TO search span.
    ///
    /// # Returns
    /// `(from_start, from_end, to_start, to_end)` bounds of the selected common
    /// contiguous match, with exclusive ends; both ranges are empty if none exists.
    ///
    /// # Errors
    /// Returns [`DiffError::Limit`] when search work exceeds the budget.
    fn longest(
        &mut self,
        s1: usize,
        e1: usize,
        s2: usize,
        e2: usize,
    ) -> Result<(usize, usize, usize, usize), DiffError> {
        let span = (e1 - s1 + e2 - s2) as i64;
        let mut best_score = -9_223_300_000_000_000_000i64;
        let mut best = (s1, s1, s2, s2);
        let mut previous = best;
        let mid = (e1 + s1) / 2;
        let mut cutoff = 4;
        loop {
            let mut next_cutoff = 0;
            for i in s1..e1 {
                self.work.charge(1)?;
                let bucket = (self.from[i].hash % self.to.len() as u64) as usize;
                let mut j = self.to[bucket].bucket;
                let mut limit = 0;
                while j > 0 && (j - 1 < s2 || j >= e2 || !self.equal(true, i, false, j - 1)?) {
                    self.work.charge(1)?;
                    if limit > cutoff {
                        j = 0;
                        next_cutoff = cutoff * 4;
                        break;
                    }
                    limit += 1;
                    j = self.to[j - 1].next;
                }
                if j == 0 {
                    continue;
                }
                if i < best.1 && j >= best.2 && j < best.3 {
                    continue;
                }
                if i < previous.1 && j >= previous.2 && j < previous.3 {
                    continue;
                }
                let (mut sx, mut sy, mut ex, mut ey) = (i, j - 1, i + 1, j);
                while sx > s1 && sy > s2 && self.equal(true, sx - 1, false, sy - 1)? {
                    sx -= 1;
                    sy -= 1;
                }
                while ex < e1 && ey < e2 && self.equal(true, ex, false, ey)? {
                    ex += 1;
                    ey += 1;
                }
                let skew = (sx - s1).abs_diff(sy - s2);
                let distance = ((sx + ex) / 2).abs_diff(mid);
                let score = (ex - sx) as i64 * span - (skew + distance) as i64;
                if score > best_score {
                    best_score = score;
                    best = (sx, ex, sy, ey);
                } else if ex > previous.1 {
                    previous = (sx, ex, sy, ey);
                }
            }
            if best.0 != best.1 || next_cutoff == 0 || next_cutoff > 64 {
                break;
            }
            cutoff = next_cutoff;
        }
        if best.0 == best.1 && (e1 - s1) * (e2 - s2) < 2500 {
            self.optimal(s1, e1, s2, e2)
        } else {
            Ok(best)
        }
    }

    /// Compute cached native indentation and trimmed width.
    ///
    /// # Arguments
    /// * `i` - Index of the FROM line whose cached metadata is needed.
    ///
    /// # Returns
    /// `Ok(())` after filling missing metadata or retaining an existing cache.
    ///
    /// # Errors
    /// Returns [`DiffError::Limit`] when inspection work exceeds the budget.
    fn trim(&mut self, i: usize) -> Result<(), DiffError> {
        let line = &mut self.from[i];
        if line.nw == 0 && !line.text.is_empty() {
            self.work.charge(line.text.len())?;
            let mut ii = 0;
            while ii < line.text.len() && space(line.text[ii]) {
                ii += 1;
            }
            let mut jj = line.text.len() - 1;
            while jj > ii && space(line.text[jj]) {
                jj -= 1;
            }
            line.indent = ii;
            line.nw = (jj + 1) - ii;
        }
        Ok(())
    }

    /// Reject the same small indentation artifacts as native Fossil.
    ///
    /// # Arguments
    /// * `s` - Inclusive start of the surrounding FROM span.
    /// * `sx` - Inclusive start of the candidate match in FROM.
    /// * `ex` - Exclusive end of the candidate match in FROM.
    /// * `e` - Exclusive end of the surrounding FROM span.
    ///
    /// # Returns
    /// Whether the candidate passes the native size and indentation heuristics.
    ///
    /// # Errors
    /// Returns [`DiffError::Limit`] when inspection or comparison work exceeds the budget.
    fn valid(&mut self, s: usize, sx: usize, ex: usize, e: usize) -> Result<bool, DiffError> {
        if (ex - sx) * 7 >= e - s {
            return Ok(true);
        }
        let mut n = 0;
        for i in sx..ex {
            self.trim(i)?;
            if self.from[i].nw > 1 {
                n += 1;
            }
        }
        if n == 0 {
            return Ok(false);
        }
        for i in s..e {
            if i < sx || i >= ex {
                self.trim(i)?;
            }
        }
        for i in sx..ex {
            let nw = self.from[i].nw;
            if nw <= 1 {
                continue;
            }
            let mut unique = true;
            for j in (s..sx).chain(ex..e) {
                self.work.charge(1)?;
                if self.from[j].nw != nw {
                    continue;
                }
                self.work.charge(nw)?;
                if self.from[j].text[self.from[j].indent..][..nw]
                    == self.from[i].text[self.from[i].indent..][..nw]
                {
                    unique = false;
                    break;
                }
            }
            if unique {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Compute divide-and-conquer edits with an explicit bounded stack.
    ///
    /// # Returns
    /// `Ok(())` after constructing the edit script and its terminating zero triple.
    ///
    /// # Errors
    /// Returns [`DiffError::Limit`] when work, edit capacity or stack allocation fails.
    fn all(&mut self) -> Result<(), DiffError> {
        let (mut e1, mut e2) = (self.from.len(), self.to.len());
        while e1 > 0 && e2 > 0 && self.equal(true, e1 - 1, false, e2 - 1)? {
            e1 -= 1;
            e2 -= 1;
        }
        let mut s = 0;
        while s < e1.min(e2) && self.equal(true, s, false, s)? {
            s += 1;
        }
        if s > 0 {
            self.append(s, 0, 0)?;
        }
        // Each pending range or copy corresponds to an admitted input line.
        let mut stack = reserved(self.from.len() + self.to.len() + 1)?;
        stack.push((s, e1, s, e2, false));
        while let Some((s1, e1, s2, e2, copy)) = stack.pop() {
            self.work.charge(1)?;
            if copy {
                self.append(e1 - s1, 0, 0)?;
                continue;
            }
            if e1 <= s1 {
                if e2 > s2 {
                    self.append(0, 0, e2 - s2)?;
                }
                continue;
            }
            if e2 <= s2 {
                self.append(0, e1 - s1, 0)?;
                continue;
            }
            let (sx, ex, sy, ey) = self.longest(s1, e1, s2, e2)?;
            if ex > sx + 5 || (ex > sx && self.valid(s1, sx, ex, e1)?) {
                stack.push((ex, e1, ey, e2, false));
                stack.push((sx, ex, sy, ey, true));
                stack.push((s1, sx, s2, sy, false));
            } else {
                self.append(0, e1 - s1, e2 - s2)?;
            }
        }
        if e1 < self.from.len() {
            self.append(self.from.len() - e1, 0, 0)?;
        }
        if self.edits.len() >= self.max_edits {
            return Err(DiffError::Limit);
        }
        self.edits.push(Edit::default());
        Ok(())
    }

    /// Shift pure insertions and deletions using the native length inequalities.
    ///
    /// # Returns
    /// `Ok(())` after applying readability shifts to the edit script.
    ///
    /// # Errors
    /// Returns [`DiffError::Limit`] when inspection or comparison work exceeds the budget.
    fn optimize(&mut self) -> Result<(), DiffError> {
        let (mut from, mut to) = (0, 0);
        for r in 0..self.edits.len() {
            self.work.charge(1)?;
            let Edit {
                mut copy,
                delete,
                insert,
            } = self.edits[r];
            from += copy;
            to += copy;
            while copy > 0 && delete == 0 && insert > 0 {
                if !self.equal(true, from - 1, false, to + insert - 1)? {
                    break;
                }
                if self.from[from].text.len() + self.to[to + insert - 1].text.len()
                    <= self.from[from - 1].text.len() + self.to[to + insert - 2].text.len()
                {
                    break;
                }
                from -= 1;
                to -= 1;
                self.edits[r].copy -= 1;
                self.edits[r + 1].copy += 1;
                copy -= 1;
            }
            while r + 1 < self.edits.len()
                && self.edits[r + 1].copy > 0
                && delete == 0
                && insert > 0
            {
                if !self.equal(false, to, false, to + insert)? {
                    break;
                }
                if self.to[to].text.len() + self.to[to + insert - 1].text.len()
                    <= self.to[to + 1].text.len() + self.to[to + insert].text.len()
                {
                    break;
                }
                from += 1;
                to += 1;
                self.edits[r].copy += 1;
                self.edits[r + 1].copy -= 1;
                copy += 1;
            }
            while copy > 0 && delete > 0 && insert == 0 {
                if !self.equal(true, from - 1, true, from + delete - 1)? {
                    break;
                }
                if self.from[from].text.len() + self.from[from + delete - 1].text.len()
                    <= self.from[from - 1].text.len() + self.from[from + delete - 2].text.len()
                {
                    break;
                }
                from -= 1;
                to -= 1;
                self.edits[r].copy -= 1;
                self.edits[r + 1].copy += 1;
                copy -= 1;
            }
            while r + 1 < self.edits.len()
                && self.edits[r + 1].copy > 0
                && delete > 0
                && insert == 0
            {
                if !self.equal(true, from, true, from + delete)? {
                    break;
                }
                // Preserve the native deletion inequality, including pTop on both sides.
                if self.from[from].text.len() + self.from[from + delete - 1].text.len()
                    <= self.from[from].text.len() + self.from[from + delete].text.len()
                {
                    break;
                }
                from += 1;
                to += 1;
                self.edits[r].copy += 1;
                self.edits[r + 1].copy -= 1;
            }
            from += delete;
            to += insert;
        }
        Ok(())
    }
}

/// Compute native line COPY/DELETE/INSERT triples for normalized byte text.
///
/// The result includes the native terminating zero triple. LF is excluded from
/// comparison, so adding or removing the final LF does not change a line. This
/// API does not perform the BOM or UTF-16 conversion of `blob_to_utf8_no_bom`.
///
/// # Arguments
/// * `from` - Already-normalized FROM bytes.
/// * `to` - Already-normalized TO bytes.
/// * `options` - Supported line comparison and edit options.
/// * `limits` - Finite resource bounds for this invocation.
/// # Returns
/// Native triples ending in a zero triple.
/// # Errors
/// Returns [`DiffError::Binary`] for an embedded NUL or a line longer than 32,767
/// bytes. Returns [`DiffError::WhitespaceOnly`] when a whitespace mode produces
/// one copy-only triple followed by the zero triple, and [`DiffError::TooManyChanges`]
/// when `not_too_big` rejects more than 10,000 inserted plus deleted lines.
/// Returns [`DiffError::Limit`] for caller bounds, fixed input caps, checked
/// arithmetic or allocation failure. No partial script is returned on failure.
/// # Examples
/// ```
/// use gossil::diff::{line_diff, DiffLimits, Edit, LineDiffOptions};
/// let edits = line_diff(
///     b"old\n", b"new\n", LineDiffOptions::default(), DiffLimits::default(),
/// ).unwrap();
/// assert_eq!(edits, vec![
///     Edit { copy: 0, delete: 1, insert: 1 },
///     Edit::default(),
/// ]);
/// ```
pub fn line_diff(
    from: &[u8],
    to: &[u8],
    options: LineDiffOptions,
    limits: DiffLimits,
) -> Result<Vec<Edit>, DiffError> {
    let bytes = from.len().checked_add(to.len()).ok_or(DiffError::Limit)?;
    // Native scoring uses signed integers; these caps also bound all counts and
    // products used by the port, regardless of caller-supplied larger bounds.
    if bytes > limits.max_input_bytes || bytes > i32::MAX as usize {
        return Err(DiffError::Limit);
    }
    let (from, to) = if options.invert {
        (to, from)
    } else {
        (from, to)
    };
    let mut work = Work(limits.max_work);
    work.charge(bytes)?;
    let line_count = |input: &[u8]| {
        input.iter().filter(|&&c| c == b'\n').count()
            + usize::from(!input.is_empty() && input.last() != Some(&b'\n'))
    };
    let admitted_lines = line_count(from)
        .checked_add(line_count(to))
        .ok_or(DiffError::Limit)?;
    if admitted_lines > limits.max_lines || admitted_lines > 1_000_000 {
        return Err(DiffError::Limit);
    }
    let from = lines(from, options, &mut work)?;
    let to = lines(to, options, &mut work)?;
    let n = from.len().checked_add(to.len()).ok_or(DiffError::Limit)?;
    // Successful splitting preserves the line counts admitted above.
    let capacity = n
        .checked_add(1)
        .ok_or(DiffError::Limit)?
        .min(limits.max_edits);
    let mut context = Context {
        from,
        to,
        edits: reserved(capacity)?,
        all_ws: options.whitespace == Whitespace::IgnoreAll,
        work,
        max_edits: capacity,
    };
    context.all()?;
    if options.whitespace != Whitespace::Exact
        && context.edits.len() == 2
        && context.edits[0].delete == 0
        && context.edits[0].insert == 0
    {
        return Err(DiffError::WhitespaceOnly);
    }
    if options.not_too_big
        && context
            .edits
            .iter()
            .map(|e| e.delete + e.insert)
            .sum::<usize>()
            > 10000
    {
        return Err(DiffError::TooManyChanges);
    }
    if options.optimize {
        context.optimize()?;
    }
    Ok(context.edits)
}
