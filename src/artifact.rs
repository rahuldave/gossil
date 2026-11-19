/*
Source-port notice for the Fossil artifact parser and legacy helpers.
Copyright 2007 D. Richard Hipp. All rights reserved.

Redistribution and use in source and binary forms, with or
without modification, are permitted provided that the
following conditions are met:

   1. Redistributions of source code must retain the above
      copyright notice, this list of conditions and the
      following disclaimer.

   2. Redistributions in binary form must reproduce the above
      copyright notice, this list of conditions and the
      following disclaimer in the documentation and/or other
      materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE AUTHORS ``AS IS'' AND ANY EXPRESS
OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
ARE DISCLAIMED. IN NO EVENT SHALL THE AUTHORS OR CONTRIBUTORS BE
LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR
BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE
OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE,
EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

The views and conclusions contained in the software and documentation
are those of the authors and contributors and should not be interpreted
as representing official policies, either expressed or implied, of anybody
else.
*/
//! Parse bounded structural artifacts with an explicit date provider.
//!
//! This module ports Fossil `manifest.c` (Copyright 2007 D. Richard Hipp, Simplified BSD).
use crate::token_codec::DecodedToken;
/// Identify one of the eight structural artifact types.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ArtifactType {
    /// File check-in manifest.
    Manifest,
    /// Sorted artifact cluster.
    Cluster,
    /// Tag control record.
    Control,
    /// Wiki page revision.
    Wiki,
    /// Ticket field update.
    Ticket,
    /// Attachment addition or deletion.
    Attachment,
    /// Timestamped technote.
    Technote,
    /// Forum post or revision.
    Forum,
}
/// Set the caller's resource policy with no universal default.
#[derive(Debug, Clone, Copy)]
pub struct ParseLimits {
    /// Maximum complete input size.
    pub input_bytes: usize,
    /// Maximum number of structural cards.
    pub cards: usize,
    /// Maximum logical owned-byte charges, including retained input and encoded tokens.
    pub owned_bytes: usize,
    /// Maximum total list entries across all cards.
    pub entries: usize,
}
/// Report the exact resource charges of one successful parse.
///
/// These counters use the same accounting as [`ParseLimits`], including conservative
/// encoded token charges rather than decoded lengths. Subtract them from an
/// operation's remaining limits before parsing another artifact. The counters report
/// logical charges against [`ParseLimits`], not every physical Rust allocator byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseUsage {
    /// Complete raw input size, including any recognized signature envelope.
    pub input_bytes: usize,
    /// Nonzero structural card markers consumed, including Z but excluding a zero sentinel.
    pub cards: usize,
    /// Total charged owned bytes, including copies, token charges and list entry sizes.
    pub owned_bytes: usize,
    /// Total charged list insertions.
    pub entries: usize,
}
/// Retain a date adapter failure separately from invalid date syntax.
#[derive(Debug)]
pub struct DateProviderError(pub Box<dyn std::error::Error + Send + Sync>);
/// Convert dates on the selected SQLite provider, possibly borrowing a transaction.
pub trait DateProvider {
    /// Evaluate SQLite `julianday` on the C-visible byte prefix and preserve SQL NULL.
    /// # Arguments
    /// * `token` - Optional raw token, truncated by the caller at its C-string boundary.
    /// # Returns
    /// A future yielding the engine Julian day, SQL NULL, or the original provider error.
    /// # Errors
    /// Returns an error when the selected engine cannot execute or decode the conversion.
    fn julianday(
        &self,
        token: Option<&[u8]>,
    ) -> impl std::future::Future<Output = Result<Option<f64>, DateProviderError>>;
}
/// Distinguish syntax, checksum, resource and provider failures.
#[derive(Debug)]
pub enum ArtifactError {
    /// Malformed structural input, with byte cursor and card number.
    Syntax {
        /// Byte cursor at rejection.
        offset: usize,
        /// Current structural card number.
        line: usize,
        /// Static syntax branch diagnostic.
        message: &'static str,
    },
    /// Exact legacy MD5 did not match the final checksum record.
    Checksum,
    /// Caller bound, integer capacity or allocation failed.
    ResourceLimit,
    /// The selected date engine failed to execute.
    Provider(DateProviderError),
}
/// Store one parsed tag whose name includes its operator prefix.
#[derive(Debug)]
pub struct Tag {
    /// Decoded name, including +, * or -.
    name: DecodedToken,
    /// `None` means self-reference.
    target: Option<Vec<u8>>,
    /// Optional decoded property value.
    value: Option<DecodedToken>,
}
/// Own an immutable parsed artifact.
#[derive(Debug)]
pub struct ParsedArtifact {
    /// Inferred and structurally checked type.
    artifact_type: ArtifactType,
    /// Original bytes, including any signature envelope.
    raw: Vec<u8>,
    /// Decoded C-card.
    comment: Option<DecodedToken>,
    /// Decoded U-card, including anonymous fallback.
    user: Option<DecodedToken>,
    /// Converted D-card Julian day.
    date: Option<f64>,
    /// Original D token before any date conversion.
    date_token: Option<Vec<u8>>,
    /// Original E date token before conversion.
    event_date_token: Option<Vec<u8>>,
    /// R-card syntax only, not aggregate checksum validation.
    repository_checksum: Option<Vec<u8>>,
    /// Parsed tags in native sort order.
    tags: Vec<Tag>,
    /// Optional delta-manifest baseline hash.
    baseline: Option<Vec<u8>>,
    /// Retained manifest files after reserved-name suppression.
    files: Vec<FileEntry>,
    /// Accumulated parent hashes, preserving source order.
    parents: Vec<Vec<u8>>,
    /// Signed manifest cherrypick references.
    cherrypicks: Vec<Cherrypick>,
    /// Sorted cluster member hashes.
    cluster_children: Vec<Vec<u8>>,
    /// Sorted ticket updates.
    ticket_fields: Vec<TicketField>,
    /// Forty-digit ticket identity.
    ticket_id: Option<Vec<u8>>,
    /// Decoded wiki page title.
    wiki_title: Option<DecodedToken>,
    /// Decoded optional MIME type.
    mimetype: Option<DecodedToken>,
    /// Exact W body bytes.
    body: Option<Vec<u8>>,
    /// Converted E-card Julian day.
    event_date: Option<f64>,
    /// Technote identity hash.
    event_id: Option<Vec<u8>>,
    /// Optional forum thread root hash.
    thread_root: Option<Vec<u8>>,
    /// Decoded forum thread title.
    thread_title: Option<DecodedToken>,
    /// Optional forum parent hash.
    in_reply_to: Option<Vec<u8>>,
    /// Attachment-specific parsed fields.
    attachment: Option<Attachment>,
}

/// Store a manifest file entry with decoded names and opaque permissions.
#[derive(Debug)]
pub struct FileEntry {
    /// Native raw or decoded key, as specified by this record.
    name: DecodedToken,
    /// Optional file content hash; absent for delta deletion.
    hash: Option<Vec<u8>>,
    /// Opaque file permission token.
    permissions: Option<Vec<u8>>,
    /// Optional decoded prior pathname.
    prior_name: Option<DecodedToken>,
}
/// Store one ticket update with its raw name and decoded value.
#[derive(Debug)]
pub struct TicketField {
    /// Native raw or decoded key, as specified by this record.
    name: Vec<u8>,
    /// Decoded record value.
    value: DecodedToken,
}
/// Store one signed cherrypick with an optional base hash.
#[derive(Debug)]
pub struct Cherrypick {
    /// Referenced artifact or attachment target.
    target: Vec<u8>,
    /// Optional cherrypick base hash.
    base: Option<Vec<u8>>,
}
/// Store an attachment whose name is reduced to the native platform basename.
#[derive(Debug)]
pub struct Attachment {
    /// Native raw or decoded key, as specified by this record.
    name: DecodedToken,
    /// Referenced artifact or attachment target.
    target: DecodedToken,
    /// Optional attachment content hash.
    source: Option<Vec<u8>>,
}
impl ParsedArtifact {
    /// Read the parsed artifact type without permitting mutation.
    /// # Returns
    /// The parsed artifact type.
    pub fn artifact_type(&self) -> ArtifactType {
        self.artifact_type
    }
    /// Read the original bytes, including any signature envelope, without permitting mutation.
    /// # Returns
    /// The original input bytes.
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
    /// Read the parsed comment without permitting mutation.
    /// # Returns
    /// The parsed comment.
    pub fn comment(&self) -> Option<&DecodedToken> {
        self.comment.as_ref()
    }
    /// Read the parsed user without permitting mutation.
    /// # Returns
    /// The parsed user.
    pub fn user(&self) -> Option<&DecodedToken> {
        self.user.as_ref()
    }
    /// Read the exact raw D token supplied to the date provider.
    /// # Returns
    /// The exact raw D token supplied to the date provider.
    pub fn date_token(&self) -> Option<&[u8]> {
        self.date_token.as_deref()
    }
    /// Read the exact raw E date token supplied to the date provider.
    /// # Returns
    /// The exact raw E date token supplied to the date provider.
    pub fn event_date_token(&self) -> Option<&[u8]> {
        self.event_date_token.as_deref()
    }
    /// Read the parsed date without permitting mutation.
    /// # Returns
    /// The parsed date.
    pub fn date(&self) -> Option<f64> {
        self.date
    }
    /// Read the parsed repository checksum without permitting mutation.
    /// # Returns
    /// The parsed repository checksum.
    pub fn repository_checksum(&self) -> Option<&[u8]> {
        self.repository_checksum.as_deref()
    }
    /// Read the parsed tags without permitting mutation.
    /// # Returns
    /// The parsed tags.
    pub fn tags(&self) -> &[Tag] {
        &self.tags
    }
    /// Read the parsed baseline without permitting mutation.
    /// # Returns
    /// The parsed baseline.
    pub fn baseline(&self) -> Option<&[u8]> {
        self.baseline.as_deref()
    }
    /// Read the parsed files without permitting mutation.
    /// # Returns
    /// The parsed files.
    pub fn files(&self) -> &[FileEntry] {
        &self.files
    }
    /// Read the parsed parents without permitting mutation.
    /// # Returns
    /// The parsed parents.
    pub fn parents(&self) -> &[Vec<u8>] {
        &self.parents
    }
    /// Read the parsed cherrypicks without permitting mutation.
    /// # Returns
    /// The parsed cherrypicks.
    pub fn cherrypicks(&self) -> &[Cherrypick] {
        &self.cherrypicks
    }
    /// Read the parsed cluster children without permitting mutation.
    /// # Returns
    /// The parsed cluster children.
    pub fn cluster_children(&self) -> &[Vec<u8>] {
        &self.cluster_children
    }
    /// Read the parsed ticket fields without permitting mutation.
    /// # Returns
    /// The parsed ticket fields.
    pub fn ticket_fields(&self) -> &[TicketField] {
        &self.ticket_fields
    }
    /// Read the parsed ticket id without permitting mutation.
    /// # Returns
    /// The parsed ticket id.
    pub fn ticket_id(&self) -> Option<&[u8]> {
        self.ticket_id.as_deref()
    }
    /// Read the parsed wiki title without permitting mutation.
    /// # Returns
    /// The parsed wiki title.
    pub fn wiki_title(&self) -> Option<&DecodedToken> {
        self.wiki_title.as_ref()
    }
    /// Read the parsed MIME type without permitting mutation.
    /// # Returns
    /// The parsed MIME type.
    pub fn mimetype(&self) -> Option<&DecodedToken> {
        self.mimetype.as_ref()
    }
    /// Read the parsed body without permitting mutation.
    /// # Returns
    /// The parsed body.
    pub fn body(&self) -> Option<&[u8]> {
        self.body.as_deref()
    }
    /// Read the parsed event date without permitting mutation.
    /// # Returns
    /// The parsed event date.
    pub fn event_date(&self) -> Option<f64> {
        self.event_date
    }
    /// Read the parsed event id without permitting mutation.
    /// # Returns
    /// The parsed event id.
    pub fn event_id(&self) -> Option<&[u8]> {
        self.event_id.as_deref()
    }
    /// Read the parsed thread root without permitting mutation.
    /// # Returns
    /// The parsed thread root.
    pub fn thread_root(&self) -> Option<&[u8]> {
        self.thread_root.as_deref()
    }
    /// Read the parsed thread title without permitting mutation.
    /// # Returns
    /// The parsed thread title.
    pub fn thread_title(&self) -> Option<&DecodedToken> {
        self.thread_title.as_ref()
    }
    /// Read the forum parent hash without permitting mutation.
    /// # Returns
    /// The forum parent hash.
    pub fn in_reply_to(&self) -> Option<&[u8]> {
        self.in_reply_to.as_deref()
    }
    /// Read the parsed attachment without permitting mutation.
    /// # Returns
    /// The parsed attachment.
    pub fn attachment(&self) -> Option<&Attachment> {
        self.attachment.as_ref()
    }
}
impl Tag {
    /// Read the parsed name without permitting mutation.
    /// # Returns
    /// The parsed name.
    pub fn name(&self) -> &DecodedToken {
        &self.name
    }
    /// Read the parsed target without permitting mutation.
    /// # Returns
    /// The parsed target.
    pub fn target(&self) -> Option<&[u8]> {
        self.target.as_deref()
    }
    /// Read the parsed value without permitting mutation.
    /// # Returns
    /// The parsed value.
    pub fn value(&self) -> Option<&DecodedToken> {
        self.value.as_ref()
    }
}
impl FileEntry {
    /// Read the parsed name without permitting mutation.
    /// # Returns
    /// The parsed name.
    pub fn name(&self) -> &DecodedToken {
        &self.name
    }
    /// Read the parsed hash without permitting mutation.
    /// # Returns
    /// The parsed hash.
    pub fn hash(&self) -> Option<&[u8]> {
        self.hash.as_deref()
    }
    /// Read the parsed permissions without permitting mutation.
    /// # Returns
    /// The parsed permissions.
    pub fn permissions(&self) -> Option<&[u8]> {
        self.permissions.as_deref()
    }
    /// Read the parsed prior name without permitting mutation.
    /// # Returns
    /// The parsed prior name.
    pub fn prior_name(&self) -> Option<&DecodedToken> {
        self.prior_name.as_ref()
    }
}
impl TicketField {
    /// Read the parsed name without permitting mutation.
    /// # Returns
    /// The parsed name.
    pub fn name(&self) -> &[u8] {
        &self.name
    }
    /// Read the parsed value without permitting mutation.
    /// # Returns
    /// The parsed value.
    pub fn value(&self) -> &DecodedToken {
        &self.value
    }
}
impl Cherrypick {
    /// Read the parsed target without permitting mutation.
    /// # Returns
    /// The parsed target.
    pub fn target(&self) -> &[u8] {
        &self.target
    }
    /// Read the parsed base without permitting mutation.
    /// # Returns
    /// The parsed base.
    pub fn base(&self) -> Option<&[u8]> {
        self.base.as_deref()
    }
}
impl Attachment {
    /// Read the parsed name without permitting mutation.
    /// # Returns
    /// The parsed name.
    pub fn name(&self) -> &DecodedToken {
        &self.name
    }
    /// Read the parsed target without permitting mutation.
    /// # Returns
    /// The parsed target.
    pub fn target(&self) -> &DecodedToken {
        &self.target
    }
    /// Read the parsed source without permitting mutation.
    /// # Returns
    /// The parsed source.
    pub fn source(&self) -> Option<&[u8]> {
        self.source.as_deref()
    }
}

impl std::fmt::Display for DateProviderError {
    /// Describe the selected provider's failure.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for DateProviderError {
    /// Retain the original provider cause.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&*self.0)
    }
}
impl std::fmt::Display for ArtifactError {
    /// Describe the rejected boundary.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Syntax {
                offset,
                line,
                message,
            } => write!(f, "card {line} at byte {offset}: {message}"),
            Self::Checksum => f.write_str("incorrect Z-card checksum"),
            Self::ResourceLimit => f.write_str("artifact resource limit or allocation failure"),
            Self::Provider(error) => write!(f, "date provider: {error}"),
        }
    }
}
impl std::error::Error for ArtifactError {
    /// Retain provider errors as causes.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        if let Self::Provider(error) = self {
            Some(error)
        } else {
            None
        }
    }
}
/// Implement native `next_card` and `next_token` cursor behavior, including raw NUL delimiters.
struct Cursor<'a> {
    /// Borrowed complete unsigned artifact.
    bytes: &'a [u8],
    /// Checked byte cursor into the unsigned artifact.
    pos: usize,
    /// Whether the previous token ended at LF.
    eol: bool,
    /// Current structural card number.
    line: usize,
}
impl<'a> Cursor<'a> {
    /// Attach a syntax diagnostic to the current card and cursor.
    /// # Arguments
    /// * `message` - Static description of the rejected syntax.
    /// # Returns
    /// A syntax error with the current byte offset and card number.
    fn syntax(&self, message: &'static str) -> ArtifactError {
        ArtifactError::Syntax {
            offset: self.pos,
            line: self.line,
            message,
        }
    }
    /// Read the next card only when all its predecessor's tokens were consumed.
    /// # Returns
    /// The next card marker, or `None` for an invalid delimiter, zero marker or end.
    fn card(&mut self) -> Option<u8> {
        if !self.eol || self.pos >= self.bytes.len() {
            return None;
        }
        let c = *self.bytes.get(self.pos)?;
        match self.bytes.get(self.pos + 1) {
            Some(b' ') => self.eol = false,
            Some(b'\n') => self.eol = true,
            _ => return None,
        }
        self.pos += 2;
        // Native next_card consumes the delimiter before returning its zero sentinel.
        // The caller exits without incrementing its card count or validating this marker.
        if c == 0 {
            return None;
        }
        self.line += 1;
        Some(c)
    }
    /// Read through space, LF or NUL, preserving token-consumer dependent NUL behavior.
    /// # Returns
    /// The next token bytes, or `None` when no delimited token is available.
    fn token(&mut self) -> Option<&'a [u8]> {
        if self.eol || self.pos >= self.bytes.len() {
            return None;
        }
        let start = self.pos;
        let n = self.bytes[start..]
            .iter()
            .position(|b| matches!(b, b' ' | b'\n' | 0))?;
        self.pos = start + n + 1;
        self.eol = self.bytes[start + n] == b'\n';
        Some(&self.bytes[start..start + n])
    }
    /// Require the next token, including an empty token when the source returns one.
    /// # Returns
    /// The next token, including an empty token.
    /// # Errors
    /// Returns a syntax error when no token is available.
    fn required(&mut self) -> Result<&'a [u8], ArtifactError> {
        self.token().ok_or_else(|| self.syntax("missing token"))
    }
}
/// Track the aggregate output budget for every owned copy and list insertion.
struct Budget {
    /// Caller-selected resource policy.
    limits: ParseLimits,
    /// Total logical owned-byte charges so far.
    bytes: usize,
    /// Total charged list entries.
    entries: usize,
}
impl Budget {
    /// Charge bytes before allocation.
    /// # Arguments
    /// * `n` - Additional owned bytes to charge.
    /// # Returns
    /// Success after increasing the aggregate byte charge.
    /// # Errors
    /// Returns `ArtifactError::ResourceLimit` on overflow or when the caller byte bound is exceeded.
    fn charge(&mut self, n: usize) -> Result<(), ArtifactError> {
        self.bytes = self
            .bytes
            .checked_add(n)
            .filter(|n| *n <= self.limits.owned_bytes && *n <= isize::MAX as usize)
            .ok_or(ArtifactError::ResourceLimit)?;
        Ok(())
    }
    /// Copy one bounded byte slice with fallible allocation.
    /// # Arguments
    /// * `b` - Bytes to retain.
    /// # Returns
    /// An owned copy of the input bytes.
    /// # Errors
    /// Returns `ArtifactError::ResourceLimit` when charging or reservation fails.
    fn copy(&mut self, b: &[u8]) -> Result<Vec<u8>, ArtifactError> {
        self.charge(b.len())?;
        let mut v = Vec::new();
        v.try_reserve_exact(b.len())
            .map_err(|_| ArtifactError::ResourceLimit)?;
        v.extend_from_slice(b);
        Ok(v)
    }
    /// Decode with the existing codec and charge its conservative input-size allocation.
    /// # Arguments
    /// * `b` - Fossil-escaped token bytes.
    /// # Returns
    /// The decoded bytes and their C-visible length.
    /// # Errors
    /// Returns `ArtifactError::ResourceLimit` when charging or decoding allocation fails.
    fn decode(&mut self, b: &[u8]) -> Result<DecodedToken, ArtifactError> {
        self.charge(b.len())?;
        crate::token_codec::defossilize(b).map_err(|_| ArtifactError::ResourceLimit)
    }
    /// Charge list capacity and reserve before inserting.
    /// # Arguments
    /// * `v` - Destination list.
    /// * `item` - Owned entry to append.
    /// # Returns
    /// Success after charging and appending one entry.
    /// # Errors
    /// Returns `ArtifactError::ResourceLimit` when entry or byte limits or reservation fail.
    fn push<T>(&mut self, v: &mut Vec<T>, item: T) -> Result<(), ArtifactError> {
        self.entries = self
            .entries
            .checked_add(1)
            .filter(|n| *n <= self.limits.entries)
            .ok_or(ArtifactError::ResourceLimit)?;
        self.charge(std::mem::size_of::<T>())?;
        v.try_reserve_exact(1)
            .map_err(|_| ArtifactError::ResourceLimit)?;
        v.push(item);
        Ok(())
    }
}
/// Read the C-visible prefix of an already decoded token.
/// # Arguments
/// * `t` - Decoded token with its recorded C-string length.
/// # Returns
/// The token prefix before its first decoded NUL.
fn visible(t: &DecodedToken) -> &[u8] {
    &t.bytes[..t.c_string_len]
}
/// Validate hexadecimal bytes with the guaranteed zero endpoint of lexer-owned tokens.
/// # Arguments
/// * `b` - Candidate hexadecimal bytes.
/// # Returns
/// Whether every byte is an ASCII hexadecimal digit; empty input is accepted.
fn hex(b: &[u8]) -> bool {
    b.iter().all(u8::is_ascii_hexdigit)
}
/// Check native SHA1/SHA3 syntax without looking up the referenced object.
/// # Arguments
/// * `b` - Candidate artifact hash.
/// # Returns
/// Whether the bytes form a 40- or 64-digit hexadecimal hash.
fn hash(b: &[u8]) -> bool {
    matches!(b.len(), 40 | 64) && hex(b)
}
/// Check native nonstrict pathnames with no Unicode or backslash restriction.
/// # Arguments
/// * `b` - Candidate filename bytes.
/// # Returns
/// Whether the nonempty path has no empty, dot or dot-dot components.
fn path(b: &[u8]) -> bool {
    !b.is_empty()
        && b.split(|b| *b == b'/')
            .all(|p| !p.is_empty() && p != b"." && p != b"..")
}
/// Check native wiki names as bytes rather than Unicode characters.
/// # Arguments
/// * `b` - Candidate wiki title bytes.
/// # Returns
/// Whether length, control-byte and spacing rules accept the title.
fn wiki_name(b: &[u8]) -> bool {
    !b.is_empty()
        && b.len() <= 100
        && b[0] > 32
        && b.last() != Some(&b' ')
        && !b.iter().any(|c| *c < 32)
        && !b.windows(2).any(|w| w == b"  ")
}
/// Suppress fixed reserved names using native suffix and directory rules.
/// # Arguments
/// * `b` - Candidate filename bytes.
/// # Returns
/// Whether the native checkout-name rules suppress this filename.
fn reserved(mut b: &[u8]) -> bool {
    if b.len() < 8 {
        return false;
    }
    let mut suffix = false;
    if b.len() >= 12 {
        if b[b.len() - 4] == b'-' {
            if !b[b.len() - 3..].eq_ignore_ascii_case(b"wal")
                && !b[b.len() - 3..].eq_ignore_ascii_case(b"shm")
            {
                return false;
            }
            b = &b[..b.len() - 4];
            suffix = true;
        } else if b.len() >= 16 && b[b.len() - 8] == b'-' {
            if !b[b.len() - 7..].eq_ignore_ascii_case(b"journal") {
                return false;
            }
            b = &b[..b.len() - 8];
            suffix = true;
        }
    }
    for name in [b"_FOSSIL_".as_slice(), b".fslckout"] {
        if b.len() >= name.len() && b[b.len() - name.len()..].eq_ignore_ascii_case(name) {
            return b.len() == name.len() || b[b.len() - name.len() - 1] == b'/' || suffix;
        }
    }
    false
}
/// Strip the recognized signature envelope without verifying signature authenticity.
/// # Arguments
/// * `input` - Complete raw artifact with an optional envelope.
/// # Returns
/// The recognized unsigned payload, or the unchanged input when no envelope matches.
fn unsigned(input: &[u8]) -> &[u8] {
    if !input.starts_with(b"-----BEGIN PGP SIGNED MESSAGE-----")
        && !input.starts_with(b"-----BEGIN SSH SIGNED MESSAGE-----")
    {
        return input;
    }
    let Some(start) = (34..input.len()).find(|i| {
        input[*i - 1] == b'\n'
            && (input[*i - 2] == b'\n' || (input[*i - 2] == b'\r' && input[*i - 3] == b'\n'))
    }) else {
        return input;
    };
    let b = &input[start..];
    let end = (0..b.len()).rev().find(|i| {
        b[*i..].starts_with(b"\n-----BEGIN PGP SIGNATURE-----")
            || b[*i..].starts_with(b"\n-----BEGIN SSH SIGNATURE-----")
    });
    &b[..end.map_or(b.len(), |i| i + 1)]
}
/// Select native allowed and required masks independently of selector inference.
/// # Arguments
/// * `kind` - Inferred structural artifact type.
/// # Returns
/// The allowed and required card-marker slices for this type.
fn matrix(kind: ArtifactType) -> (&'static [u8], &'static [u8]) {
    match kind {
        ArtifactType::Manifest => (b"BCDFNPQRTUZ", b"DZ"),
        ArtifactType::Cluster => (b"MZ", b"MZ"),
        ArtifactType::Control => (b"DTUZ", b"DTUZ"),
        ArtifactType::Wiki => (b"CDLNPUWZ", b"DLUWZ"),
        ArtifactType::Ticket => (b"DJKUZ", b"DJKUZ"),
        ArtifactType::Attachment => (b"ACDNUZ", b"ADZ"),
        ArtifactType::Technote => (b"CDENPTUWZ", b"DEWZ"),
        ArtifactType::Forum => (b"DGHINPUWZ", b"DUWZ"),
    }
}
/// Parse all eight structural artifact types with `rid=0` semantics.
///
/// This validates grammar and exact MD5, not signatures, repository closure or R contents.
/// Date conversion must use the caller's selected SQLite/libSQL engine.
/// # Arguments
/// * `input` - Immutable raw artifact bytes, with any supported signature envelope.
/// * `limits` - Explicit input, card, owned-byte and list-entry bounds.
/// * `dates` - Asynchronous provider, which may borrow an existing transaction.
/// # Returns
/// An owned result exposing read-only parsed fields and the exact original bytes.
/// Use [`parse_artifact_with_usage`] when the caller also needs resource charges.
/// # Examples
/// ```
/// use gossil::artifact::{
///     ArtifactError, DateProvider, ParseLimits, ParsedArtifact, parse_artifact,
/// };
///
/// async fn parse_for_operation(
///     bytes: &[u8],
///     dates: &impl DateProvider,
/// ) -> Result<ParsedArtifact, ArtifactError> {
///     let limits = ParseLimits {
///         input_bytes: 1_048_576,
///         cards: 10_000,
///         owned_bytes: 4_194_304,
///         entries: 10_000,
///     };
///     parse_artifact(bytes, limits, dates).await
/// }
/// ```
/// # Errors
/// Returns syntax, checksum, resource-limit or provider failure separately.
pub async fn parse_artifact(
    input: &[u8],
    limits: ParseLimits,
    dates: &impl DateProvider,
) -> Result<ParsedArtifact, ArtifactError> {
    parse_artifact_with_usage(input, limits, dates)
        .await
        .map(|(parsed, _)| parsed)
}
/// Parse an artifact and report its existing resource counters on success.
///
/// Uses the same grammar and allocation checks as [`parse_artifact`]. No usage is
/// returned on failure; an aggregate caller must stop its operation on an error.
/// # Arguments
/// * `input` - Complete raw bytes, including any supported signature envelope.
/// * `limits` - Remaining input, card, owned-byte and entry bounds for the operation.
/// * `dates` - Selected date provider, possibly borrowing the caller's transaction.
/// # Returns
/// The validated artifact and the exact logical charges against the supplied limits.
/// These charges do not measure every physical Rust allocator byte.
/// # Examples
/// Parse a cluster, which has no date card, and debit a caller-owned budget.
/// ```
/// use gossil::artifact::{
///     ArtifactError, DateProvider, DateProviderError, ParseLimits, parse_artifact_with_usage,
/// };
/// struct NoDates;
/// impl DateProvider for NoDates {
///     async fn julianday(&self, _: Option<&[u8]>) -> Result<Option<f64>, DateProviderError> {
///         Ok(None)
///     }
/// }
/// # #[tokio::main]
/// # async fn main() -> Result<(), ArtifactError> {
/// let mut input = b"M aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n".to_vec();
/// let checksum = gossil::artifact_checksum::md5(&input);
/// input.extend_from_slice(b"Z ");
/// for byte in checksum {
///     input.extend_from_slice(format!("{byte:02x}").as_bytes());
/// }
/// input.push(b'\n');
/// let mut remaining = ParseLimits {
///     input_bytes: 4096, cards: 100, owned_bytes: 8192, entries: 100,
/// };
/// let (_, usage) = parse_artifact_with_usage(&input, remaining, &NoDates).await?;
/// assert_eq!(usage.input_bytes, input.len());
/// assert_eq!(usage.cards, 2);
/// assert_eq!(usage.entries, 1);
/// remaining.input_bytes -= usage.input_bytes;
/// remaining.cards -= usage.cards;
/// remaining.owned_bytes -= usage.owned_bytes;
/// remaining.entries -= usage.entries;
/// assert_eq!(remaining.cards, 98);
/// # Ok(())
/// # }
/// ```
/// The successful charges fit the supplied limits, so these subtractions cannot
/// underflow. On an error, stop and discard the enclosing operation state.
/// # Errors
/// Returns syntax, checksum, resource-limit or provider failure separately.
pub async fn parse_artifact_with_usage(
    input: &[u8],
    limits: ParseLimits,
    dates: &impl DateProvider,
) -> Result<(ParsedArtifact, ParseUsage), ArtifactError> {
    if input.len() > limits.input_bytes || input.len() > isize::MAX as usize {
        return Err(ArtifactError::ResourceLimit);
    }
    let bytes = unsigned(input);
    let mut x = Cursor {
        bytes,
        pos: 0,
        eol: true,
        line: 0,
    };
    if input.last() != Some(&b'\n') {
        return Err(x.syntax("not terminated with LF"));
    }
    if bytes.len() < 10 || !bytes[0].is_ascii_uppercase() || bytes[1] != b' ' {
        return Err(x.syntax("line 1 not recognized"));
    }
    if bytes.len() >= 35 && bytes[bytes.len() - 35..].starts_with(b"Z ") {
        let digest = crate::artifact_checksum::md5(&bytes[..bytes.len() - 35]);
        let alphabet = b"0123456789abcdef";
        for (i, b) in digest.iter().enumerate() {
            if bytes[bytes.len() - 33 + 2 * i] != alphabet[(b >> 4) as usize]
                || bytes[bytes.len() - 32 + 2 * i] != alphabet[(b & 15) as usize]
            {
                return Err(ArtifactError::Checksum);
            }
        }
    }
    let mut budget = Budget {
        limits,
        bytes: 0,
        entries: 0,
    };
    let mut p = ParsedArtifact {
        artifact_type: ArtifactType::Control,
        raw: budget.copy(input)?,
        comment: None,
        user: None,
        date: None,
        date_token: None,
        event_date_token: None,
        repository_checksum: None,
        tags: Vec::new(),
        baseline: None,
        files: Vec::new(),
        parents: Vec::new(),
        cherrypicks: Vec::new(),
        cluster_children: Vec::new(),
        ticket_fields: Vec::new(),
        ticket_id: None,
        wiki_title: None,
        mimetype: None,
        body: None,
        event_date: None,
        event_id: None,
        thread_root: None,
        thread_title: None,
        in_reply_to: None,
        attachment: None,
    };
    let mut selected = None;
    let mut seen = 0u32;
    let mut previous = 0u8;
    while let Some(card) = x.card() {
        if x.line > limits.cards {
            return Err(ArtifactError::ResourceLimit);
        }
        if card < previous && !(card == b'N' && previous == b'P' && p.event_id.is_some()) {
            return Err(x.syntax("cards not in lexicographical order"));
        }
        if !card.is_ascii_uppercase() {
            return Err(x.syntax("bad card type"));
        }
        previous = card;
        seen |= 1u32 << (card - b'A');
        match card {
            b'A' => {
                let name = x.required()?;
                let target = x.required()?;
                let source = x.token();
                if p.attachment.is_some() {
                    return Err(x.syntax("more than one A-card"));
                }
                let mut name = budget.decode(name)?;
                let decoded_target = budget.decode(target)?;
                if !path(visible(&name)) {
                    return Err(x.syntax("invalid A filename"));
                }
                // Original raw endpoint stays zero after decode: a shortened escaped hash cannot pass raw-length validate16.
                if !(wiki_name(visible(&decoded_target))
                    || (matches!(target.len(), 40 | 64)
                        && decoded_target.bytes.len() == target.len()
                        && hash(visible(&decoded_target))))
                {
                    return Err(x.syntax("invalid A target"));
                }
                if source.is_some_and(|s| !hash(s)) {
                    return Err(x.syntax("invalid A source"));
                }
                let tail = visible(&name)
                    .iter()
                    .rposition(|b| *b == b'/' || (cfg!(windows) && *b == b'\\'))
                    .map_or(0, |i| i + 1);
                if tail > 0 {
                    name.bytes.drain(..tail);
                    name.c_string_len -= tail;
                }
                p.attachment = Some(Attachment {
                    name,
                    target: decoded_target,
                    source: source.map(|s| budget.copy(s)).transpose()?,
                });
                selected = Some(ArtifactType::Attachment);
            }
            b'B' | b'G' | b'I' | b'K' | b'R' => {
                let slot = match card {
                    b'B' => &mut p.baseline,
                    b'G' => &mut p.thread_root,
                    b'I' => &mut p.in_reply_to,
                    b'K' => &mut p.ticket_id,
                    _ => &mut p.repository_checksum,
                };
                if slot.is_some() {
                    return Err(x.syntax("duplicate hash card"));
                }
                let t = x.required()?;
                let valid = match card {
                    b'K' => t.len() == 40 && hex(t),
                    b'R' => t.len() == 32 && hex(t),
                    _ => hash(t),
                };
                if !valid {
                    return Err(x.syntax("invalid hash"));
                }
                *slot = Some(budget.copy(t)?);
                selected = Some(match card {
                    b'G' | b'I' => ArtifactType::Forum,
                    b'K' => ArtifactType::Ticket,
                    _ => ArtifactType::Manifest,
                });
            }
            b'C' | b'H' | b'L' | b'N' | b'U' => {
                let slot = match card {
                    b'C' => &mut p.comment,
                    b'H' => &mut p.thread_title,
                    b'L' => &mut p.wiki_title,
                    b'N' => &mut p.mimetype,
                    _ => &mut p.user,
                };
                if slot.is_some() {
                    return Err(x.syntax("duplicate text card"));
                }
                let token = if card == b'U' {
                    x.token().filter(|b| !b.is_empty()).unwrap_or(b"anonymous")
                } else {
                    x.required()?
                };
                let decoded = budget.decode(token)?;
                if card == b'L' && !wiki_name(visible(&decoded)) {
                    return Err(x.syntax("invalid wiki name"));
                }
                *slot = Some(decoded);
                if card == b'H' {
                    selected = Some(ArtifactType::Forum);
                } else if card == b'L' {
                    selected = Some(ArtifactType::Wiki);
                }
            }
            b'D' | b'E' => {
                if (card == b'D' && p.date.is_some_and(|day| day > 0.0))
                    || (card == b'E' && p.event_date.is_some_and(|day| day > 0.0))
                {
                    return Err(x.syntax("duplicate date card"));
                }
                let t = x.token();
                let day = dates
                    .julianday(t)
                    .await
                    .map_err(ArtifactError::Provider)?
                    .unwrap_or(0.0);
                if day <= 0.0 {
                    return Err(x.syntax("cannot parse date"));
                }
                if card == b'D' {
                    p.date = Some(day);
                    p.date_token = t.map(|b| budget.copy(b)).transpose()?;
                } else {
                    p.event_date_token = t.map(|b| budget.copy(b)).transpose()?;
                    p.event_date = Some(day);
                    let id = x.required()?;
                    if !hash(id) {
                        return Err(x.syntax("invalid E hash"));
                    }
                    p.event_id = Some(budget.copy(id)?);
                    selected = Some(ArtifactType::Technote);
                }
            }
            b'F' => {
                let name = budget.decode(x.required()?)?;
                if !path(visible(&name)) {
                    return Err(x.syntax("invalid F pathname"));
                }
                let id = x.token();
                if (p.baseline.is_none() || id.is_some()) && !id.is_some_and(hash) {
                    return Err(x.syntax("invalid F hash"));
                }
                let perm = x.token();
                let prior = x.token().map(|b| budget.decode(b)).transpose()?;
                if prior.as_ref().is_some_and(|p| !path(visible(p))) {
                    return Err(x.syntax("invalid F prior pathname"));
                }
                if p.files
                    .last()
                    .is_some_and(|f| visible(&f.name) >= visible(&name))
                {
                    return Err(x.syntax("incorrect F sort order"));
                }
                if reserved(visible(&name)) {
                    continue;
                }
                let entry = FileEntry {
                    name,
                    hash: id.map(|b| budget.copy(b)).transpose()?,
                    permissions: perm.map(|b| budget.copy(b)).transpose()?,
                    prior_name: prior,
                };
                budget.push(&mut p.files, entry)?;
                selected = Some(ArtifactType::Manifest);
            }
            b'J' => {
                let name = x.required()?;
                let value = budget.decode(x.token().unwrap_or(b""))?;
                if p.ticket_fields
                    .last()
                    .is_some_and(|f| f.name.as_slice() >= name)
                {
                    return Err(x.syntax("incorrect J sort order"));
                }
                let name = budget.copy(name)?;
                budget.push(&mut p.ticket_fields, TicketField { name, value })?;
                selected = Some(ArtifactType::Ticket);
            }
            b'M' | b'P' => loop {
                let t = if card == b'M' {
                    Some(x.required()?)
                } else {
                    x.token()
                };
                let Some(t) = t else {
                    break;
                };
                if !hash(t) {
                    return Err(x.syntax("invalid parent or cluster hash"));
                }
                let list = if card == b'M' {
                    &mut p.cluster_children
                } else {
                    &mut p.parents
                };
                if card == b'M' && list.last().is_some_and(|last| last.as_slice() >= t) {
                    return Err(x.syntax("incorrect M sort order"));
                }
                let owned = budget.copy(t)?;
                budget.push(list, owned)?;
                if card == b'M' {
                    selected = Some(ArtifactType::Cluster);
                    break;
                }
            },
            b'Q' => {
                let target = x.required()?;
                if !matches!(target.first(), Some(b'+' | b'-')) || !hash(&target[1..]) {
                    return Err(x.syntax("invalid Q target"));
                }
                let base = x.token();
                if base.is_some_and(|b| !hash(b)) {
                    return Err(x.syntax("invalid Q base"));
                }
                let item = Cherrypick {
                    target: budget.copy(target)?,
                    base: base.map(|b| budget.copy(b)).transpose()?,
                };
                budget.push(&mut p.cherrypicks, item)?;
                selected = Some(ArtifactType::Manifest);
            }
            b'T' => {
                let name = x.required()?;
                let target = x.required()?;
                let value = x.token().map(|b| budget.decode(b)).transpose()?;
                let target = if target == b"*" {
                    None
                } else if hash(target) {
                    Some(budget.copy(target)?)
                } else {
                    return Err(x.syntax("invalid T target"));
                };
                let name = budget.decode(name)?;
                let n = visible(&name);
                if !matches!(n.first(), Some(b'-' | b'+' | b'*')) || hex(&n[1..]) {
                    return Err(x.syntax("invalid T name"));
                }
                if p.tags.last().is_some_and(|t| {
                    visible(&t.name) > n
                        || (visible(&t.name) == n && t.target.as_deref() >= target.as_deref())
                }) {
                    return Err(x.syntax("incorrect T sort order"));
                }
                budget.push(
                    &mut p.tags,
                    Tag {
                        name,
                        target,
                        value,
                    },
                )?;
            }
            b'W' => {
                let size = x.required()?;
                if !x.eol {
                    return Err(x.syntax("no content after W-card"));
                }
                let mut old = 0u32;
                for &c in size.iter().take_while(|c| c.is_ascii_digit()) {
                    let new = old.wrapping_mul(10).wrapping_add(u32::from(c - b'0'));
                    if new < old {
                        return Err(x.syntax("size overflow on W-card"));
                    }
                    old = new;
                }
                if p.body.is_some() {
                    return Err(x.syntax("duplicate W-card"));
                }
                let len = usize::try_from(old).map_err(|_| ArtifactError::ResourceLimit)?;
                let end = x.pos.checked_add(len).ok_or(ArtifactError::ResourceLimit)?;
                if end.checked_add(1).ok_or(ArtifactError::ResourceLimit)? >= bytes.len() {
                    return Err(x.syntax("not enough W content"));
                }
                if bytes[end] != b'\n' {
                    return Err(x.syntax("W content has no LF"));
                }
                p.body = Some(budget.copy(&bytes[x.pos..end])?);
                x.pos = end + 1;
            }
            b'Z' => {
                let z = x.required()?;
                if z.len() != 32 || !hex(z) {
                    return Err(x.syntax("invalid Z checksum syntax"));
                }
            }
            _ => return Err(x.syntax("unrecognized card")),
        }
    }
    if x.pos < bytes.len() {
        return Err(x.syntax("extra characters at end of card"));
    }
    p.artifact_type = selected.unwrap_or(
        if p.comment.is_some() || !p.files.is_empty() || !p.parents.is_empty() {
            ArtifactType::Manifest
        } else {
            ArtifactType::Control
        },
    );
    let (allowed, required) = matrix(p.artifact_type);
    for card in b'A'..=b'Z' {
        let present = seen & (1u32 << (card - b'A')) != 0;
        if (present && !allowed.contains(&card)) || (!present && required.contains(&card)) {
            return Err(x.syntax("artifact card matrix violation"));
        }
    }
    match p.artifact_type {
        ArtifactType::Control if p.tags.iter().any(|t| t.target.is_none()) => {
            return Err(x.syntax("self-referential control tag"));
        }
        ArtifactType::Technote
            if p.tags
                .iter()
                .any(|t| t.target.is_some() || visible(&t.name).first() != Some(&b'+')) =>
        {
            return Err(x.syntax("invalid technote tag"));
        }
        ArtifactType::Forum
            if (p.thread_title.is_some() && p.in_reply_to.is_some()) || p.parents.len() > 1 =>
        {
            return Err(x.syntax("invalid forum postconditions"));
        }
        _ => {}
    }
    let usage = ParseUsage {
        input_bytes: input.len(),
        cards: x.line,
        owned_bytes: budget.bytes,
        entries: budget.entries,
    };
    Ok((p, usage))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// All 208 allowed edges, 29 required edges and 25 optional edges from the pin.
    #[test]
    fn exhaustive_native_card_matrix() {
        let rows = [
            (
                ArtifactType::Manifest,
                b"BCDFNPQRTUZ".as_slice(),
                b"DZ".as_slice(),
            ),
            (ArtifactType::Cluster, b"MZ", b"MZ"),
            (ArtifactType::Control, b"DTUZ", b"DTUZ"),
            (ArtifactType::Wiki, b"CDLNPUWZ", b"DLUWZ"),
            (ArtifactType::Ticket, b"DJKUZ", b"DJKUZ"),
            (ArtifactType::Attachment, b"ACDNUZ", b"ADZ"),
            (ArtifactType::Technote, b"CDENPTUWZ", b"DEWZ"),
            (ArtifactType::Forum, b"DGHINPUWZ", b"DUWZ"),
        ];
        let (mut edges, mut required, mut optional) = (0, 0, 0);
        for (kind, allowed, needed) in rows {
            let (a, r) = matrix(kind);
            for c in b'A'..=b'Z' {
                assert_eq!(a.contains(&c), allowed.contains(&c), "{kind:?} {c}");
                assert_eq!(r.contains(&c), needed.contains(&c), "{kind:?} {c}");
                edges += 1;
                required += usize::from(needed.contains(&c));
                optional += usize::from(allowed.contains(&c) && !needed.contains(&c));
            }
        }
        assert_eq!((edges, required, optional), (208, 29, 25));
    }
    /// Zero-terminated validate16 uses exact length and accepts an empty hex span.
    #[test]
    fn zero_endpoint_hash_and_legacy_paths() {
        assert!(hex(b""));
        assert!(!hash(b""));
        assert!(hash(&[b'F'; 64]));
        assert!(!hash(&[b'f'; 63]));
        for p in [b"a".as_slice(), b"a\\b", b"\xff", b".a/b.."] {
            assert!(path(p));
        }
        for p in [
            b"".as_slice(),
            b"/a",
            b"a/",
            b"a//b",
            b"a/../b",
            b"a/./b",
            b".",
            b"..",
        ] {
            assert!(!path(p));
        }
        for p in [
            b"_FOSSIL_".as_slice(),
            b".fslckout",
            b"a/_fossil_",
            b"_FOSSIL_-wal",
            b"prefix_FOSSIL_-journal",
        ] {
            assert!(reserved(p));
        }
        for p in [
            b"a".as_slice(),
            b"prefix_FOSSIL_",
            b"_FOSSIL_-xyz",
            b"_FOSSIL_-badbadb",
            b".fslckout-walx",
        ] {
            assert!(!reserved(p));
        }
    }
}

#[cfg(test)]
mod terminal_zero_tests {
    use super::*;
    /// Source next_card consumes space/LF before returning zero, without counting a card.
    #[test]
    fn zero_sentinel_preserves_native_cursor_and_delimiter_state() {
        for (bytes, pos, eol) in [
            (b"\0\n".as_slice(), 2, true),
            (b"\0 ", 2, false),
            (b"\0\t", 0, true),
            (b"\0\nmore\n", 2, true),
        ] {
            let mut cursor = Cursor {
                bytes,
                pos: 0,
                eol: true,
                line: 3,
            };
            assert_eq!(cursor.card(), None);
            assert_eq!(cursor.pos, pos);
            assert_eq!(cursor.eol, eol);
            assert_eq!(cursor.line, 3);
        }
        let mut cursor = Cursor {
            bytes: b"\x01\n",
            pos: 0,
            eol: true,
            line: 3,
        };
        assert_eq!(cursor.card(), Some(1));
        assert_eq!(cursor.pos, 2);
        assert_eq!(cursor.line, 4);
    }
}
