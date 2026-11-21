//! Bounded owner-local lookup assertions and exclusive complete-file publication.
//!
//! ## Recommendations
//! Prepare bounded bytes before database mutation; publish only after confirmed commit.
//! Native and Gest callers must independently admit their complete binding before publication.
/// Publication state is separate from the database commit boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Phase {
    /// No database mutation has been authorized by this locator operation.
    Preflight,
    /// Database state is committed; no destination was published by this attempt.
    CommittedUnpublished,
    /// A locator is present but its directory durability was not confirmed.
    DurabilityUnconfirmed,
    /// Locator directory is synced but owned temporary cleanup failed.
    Published,
}
/// A bounded locator diagnostic retaining its publication phase.
#[derive(Debug)]
pub(crate) struct Error {
    /// Internal I/O or configuration category; the product CLI still returns status 1.
    code: u8,
    /// Safe outer diagnostic without rendering retained provider contents.
    message: String,
    /// Publication progress, kept separate from durable database state.
    pub(crate) phase: Phase,
    /// Original typed cause or a contextual error that retains its cause chain.
    source: Option<Box<dyn std::error::Error>>,
}
impl Error {
    /// Construct a safe categorized preflight diagnostic without an underlying cause.
    /// # Arguments
    /// * `code` - Internal diagnostic category, independent of the product process exit status.
    /// * `message` - Safe outer diagnostic that does not render provider contents.
    /// # Returns
    /// A preflight diagnostic without an underlying source.
    fn new(code: u8, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            phase: Phase::Preflight,
            source: None,
        }
    }
    /// Construct a configuration-category preflight diagnostic without an underlying cause.
    /// # Arguments
    /// * `message` - Safe configuration diagnostic.
    /// # Returns
    /// A configuration-category preflight error without an underlying source.
    pub(crate) fn config(message: impl Into<String>) -> Self {
        Self::new(78, message)
    }
    /// Retain the original typed cause under a safe outer diagnostic.
    /// # Arguments
    /// * `code` - Internal I/O or configuration category.
    /// * `message` - Safe outer description of the failed boundary.
    /// * `source` - Original typed failure retained for inspection.
    /// # Returns
    /// A preflight error containing the original typed source.
    fn caused(
        code: u8,
        message: impl Into<String>,
        source: impl std::error::Error + 'static,
    ) -> Self {
        let mut error = Self::new(code, message);
        error.source = Some(Box::new(source));
        error
    }
    /// Retain a configuration failure cause without exposing its contents in the diagnostic.
    /// # Arguments
    /// * `message` - Safe configuration diagnostic.
    /// * `source` - Original typed configuration or serialization failure.
    /// # Returns
    /// A configuration-category preflight error containing the typed source.
    pub(crate) fn config_source(
        message: impl Into<String>,
        source: impl std::error::Error + 'static,
    ) -> Self {
        Self::caused(78, message, source)
    }
    /// Retain the typed underlying failure without rendering its potentially sensitive contents.
    /// # Arguments
    /// * `message` - Safe replacement description for this wrapping boundary.
    /// # Returns
    /// A contextual error that preserves the category, phase and original error chain.
    fn context(self, message: impl Into<String>) -> Self {
        let code = self.code;
        let phase = self.phase;
        Self::caused(code, message, self).at(phase)
    }
    /// Set publication progress while retaining the diagnostic and typed cause.
    /// # Arguments
    /// * `phase` - Publication progress established by the caller.
    /// # Returns
    /// The same diagnostic and cause with its phase updated.
    fn at(mut self, phase: Phase) -> Self {
        self.phase = phase;
        self
    }
}
impl std::fmt::Display for Error {
    /// Write only the publication phase and safe outer diagnostic.
    /// # Errors
    /// Return the formatter error if writing the safe phase and message fails.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.phase, self.message)
    }
}
impl std::error::Error for Error {
    /// Expose the original typed failure or the next retained contextual error.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_deref()
    }
}
impl From<std::io::Error> for Error {
    /// Retain an I/O failure under a safe filesystem diagnostic.
    fn from(error: std::io::Error) -> Self {
        Self::caused(74, "filesystem operation failed", error)
    }
}
impl From<serde_json::Error> for Error {
    /// Retain a JSON failure under a safe locator diagnostic.
    fn from(error: serde_json::Error) -> Self {
        Self::config_source("locator serialization failed", error)
    }
}
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
};
/// Bound the complete serialized locator and the bytes read before parsing.
const MAX: usize = 65536;
/// Describe an owner-local database lookup without granting repository authority.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Locator {
    /// Locator contract version.
    version: u32,
    /// Actual absolute Unix repository path bytes encoded without loss.
    repository_path_hex: String,
    /// Admitted native project-code and storage repository ID.
    pub repository_id: String,
    /// Actual owner-local checkout registration identity.
    pub checkout_id: String,
}
/// Reject unsafe marker or locator metadata without changing permissions.
/// # Arguments
/// * `path` - Marker directory or locator pathname to observe without following a symlink.
/// * `directory` - Whether the required object is a marker directory rather than a private regular file.
/// # Returns
/// Observed metadata after effective-user ownership, object type and permission checks.
/// # Errors
/// Reject wrong effective-user ownership, symlinks, wrong object types or unsafe permissions; preserve underlying metadata I/O causes.
fn metadata(path: &Path, directory: bool) -> Result<fs::Metadata, Error> {
    let m = fs::symlink_metadata(path)?;
    // SAFETY: geteuid has no pointer arguments or side effects.
    let uid = unsafe { libc::geteuid() };
    if m.uid() != uid
        || m.file_type().is_symlink()
        || (directory && (!m.is_dir() || m.mode() & 0o022 != 0))
        || (!directory && (!m.is_file() || m.mode() & 0o077 != 0))
    {
        return Err(Error::config(
            "marker/locator has unsafe ownership, type or permissions",
        ));
    }
    Ok(m)
}
/// Check that the selected marker namespace is private before any database mutation.
/// # Arguments
/// * `path` - Selected marker pathname to create or validate before database mutation.
/// # Returns
/// Success after observing an acceptable existing directory or creating and validating a mode-0700 directory.
/// # Errors
/// Return creation or metadata I/O failures and reject unsafe existing marker ownership, type or permissions without repair.
pub(crate) fn prepare_directory(path: &Path) -> Result<(), Error> {
    match fs::symlink_metadata(path) {
        Ok(_) => {
            metadata(path, true)?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            builder.create(path)?;
            metadata(path, true)?;
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
/// Validate an existing nearest marker directory without creating it.
/// # Arguments
/// * `path` - Existing nearest marker directory to validate without creating it.
/// # Returns
/// Success when existing marker metadata meets the ownership, type and permission contract.
/// # Errors
/// Return metadata I/O failures or reject a marker with unsafe ownership, type or permissions.
pub(crate) fn directory(path: &Path) -> Result<(), Error> {
    metadata(path, true).map(|_| ())
}
/// Validate native-byte path representability before database mutation.
/// # Arguments
/// * `path` - Native repository pathname to encode in a locator.
/// # Returns
/// Success for an absolute, NUL-free native filename within the 16384-byte ceiling.
/// # Errors
/// Reject nonabsolute, empty, NUL-containing or over-16384-byte native repository filenames.
pub(crate) fn validate_path(path: &Path) -> Result<(), Error> {
    let bytes = path.as_os_str().as_bytes();
    if !path.is_absolute() || bytes.is_empty() || bytes.len() > 16384 || bytes.contains(&0) {
        return Err(Error::config(
            "repository path is not a bounded absolute native filename",
        ));
    }
    Ok(())
}
impl Locator {
    /// Construct complete lookup bytes only from actual admitted identities.
    /// # Arguments
    /// * `path` - Actual bounded absolute native repository filename.
    /// * `repository_id` - Admitted lowercase repository identity.
    /// * `checkout_id` - Exact persisted checkout registration identity.
    /// # Returns
    /// A validated version-1 lookup assertion with the repository path encoded without native-byte loss.
    /// # Errors
    /// Reject invalid repository paths, identities or complete serialized size.
    pub(crate) fn new(
        path: &Path,
        repository_id: String,
        checkout_id: String,
    ) -> Result<Self, Error> {
        validate_path(path)?;
        let value = Self {
            version: 1,
            repository_path_hex: path
                .as_os_str()
                .as_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            repository_id,
            checkout_id,
        };
        value.validate()?;
        Ok(value)
    }
    /// Validate identity, encoding and complete serialized bounds.
    /// # Returns
    /// Success only when version, identities, path encoding and complete serialized size satisfy the locator contract.
    /// # Errors
    /// Reject unsupported version, malformed identities/path encoding or a complete payload beyond 65536 bytes; preserve serialization failures.
    fn validate(&self) -> Result<(), Error> {
        if self.version != 1
            || self.repository_id.len() != 40
            || !self
                .repository_id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.checkout_id.is_empty()
            || self.checkout_id.len() > 8192
            || self.checkout_id.contains('\0')
        {
            return Err(Error::config("invalid locator version or identity"));
        }
        self.path()?;
        if serde_json::to_vec(self)?.len() + 1 > MAX {
            return Err(Error::config("locator exceeds serialized bound"));
        }
        Ok(())
    }
    /// Recover exact native filename bytes from the locator encoding.
    /// # Returns
    /// The decoded bounded absolute native repository pathname; this path alone grants no storage authority.
    /// # Errors
    /// Reject empty, odd-length, oversized or non-lowercase hexadecimal encoding.
    /// Reject decoded paths outside the native filename contract, including relative paths, URLs and paths containing NUL.
    pub(crate) fn path(&self) -> Result<PathBuf, Error> {
        let bytes = self.repository_path_hex.as_bytes();
        if bytes.is_empty()
            || bytes.len() > 32768
            || !bytes.len().is_multiple_of(2)
            || !bytes
                .iter()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
        {
            return Err(Error::config("invalid locator path hex"));
        }
        let nibble = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
        let decoded = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| (nibble(p[0]) << 4) | nibble(p[1]))
            .collect::<Vec<_>>();
        let path = PathBuf::from(std::ffi::OsString::from_vec(decoded));
        validate_path(&path)?;
        Ok(path)
    }
    /// Deterministic complete representation with one trailing newline.
    /// # Returns
    /// Complete validated JSON bytes followed by one newline, within the serialized ceiling.
    /// # Errors
    /// Return locator validation or serialization failures before any publication.
    fn bytes(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        let mut bytes = serde_json::to_vec(self)?;
        bytes.push(b'\n');
        Ok(bytes)
    }
}
/// Bounded native-file read before parsing; no symlink or permissive locator admission.
/// # Arguments
/// * `path` - Existing private locator file to read through a bounded nofollow open.
/// # Returns
/// A strictly parsed and validated lookup assertion after opened-file identity checks.
/// # Errors
/// Reject unsafe metadata, changed opened-file identity, excessive bytes, invalid JSON or invalid locator fields; retain typed I/O/JSON causes.
pub(crate) fn read(path: &Path) -> Result<Locator, Error> {
    let before = metadata(path, false)?;
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let actual = file.metadata()?;
    same_file(&before, &actual)?;
    let mut bytes = Vec::with_capacity(MAX + 1);
    Read::by_ref(&mut file)
        .take((MAX + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX {
        return Err(Error::config("locator exceeds read bound"));
    }
    let value: Locator = serde_json::from_slice(&bytes)
        .map_err(|error| Error::config_source("invalid or incomplete checkout locator", error))?;
    value.validate()?;
    Ok(value)
}
/// Compare the observed pathname object with the actual opened file, without following a replacement.
/// # Arguments
/// * `before` - Metadata observed at the pathname before opening it.
/// * `actual` - Metadata obtained from the actual opened file.
/// # Returns
/// Success when both observations have the same device and inode.
/// # Errors
/// Reject a device/inode difference between pathname observation and the opened file.
fn same_file(before: &fs::Metadata, actual: &fs::Metadata) -> Result<(), Error> {
    if (before.dev(), before.ino()) != (actual.dev(), actual.ino()) {
        return Err(Error::config("locator identity changed while opening"));
    }
    Ok(())
}
/// Add commit context without erasing a later, more precise publication phase.
/// # Arguments
/// * `error` - Failure from a locator operation on an already committed binding.
/// # Returns
/// The error marked committed-unpublished when it was preflight, otherwise with its later phase preserved.
fn after_commit(error: Error) -> Error {
    if error.phase == Phase::Preflight {
        error.at(Phase::CommittedUnpublished)
    } else {
        error
    }
}
/// Owned temporary pathname; cleanup only its observed device/inode.
struct Temporary {
    /// Exclusively created pathname owned by this preparation attempt.
    path: PathBuf,
    /// Observed device and inode required before cleanup removes the pathname.
    identity: (u64, u64),
}
impl Temporary {
    /// Refuse to remove a replacement, and never inspect or delete unrelated crash leftovers.
    /// # Returns
    /// Success after removing only the matching owned file, or when that pathname is already absent.
    /// # Errors
    /// Reject replaced pathname identity or wrong file type; return metadata/removal I/O causes. Missing owned temporaries need no removal.
    fn cleanup(&self) -> Result<(), Error> {
        match fs::symlink_metadata(&self.path) {
            Ok(m) if (m.dev(), m.ino()) == self.identity && m.is_file() => {
                fs::remove_file(&self.path)?;
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Ok(_) => Err(Error::new(
                74,
                "owned locator temporary identity changed; preserved",
            )),
            Err(error) => Err(Error::caused(
                74,
                "owned locator temporary identity changed; preserved",
                error,
            )),
        }
    }
}
impl Drop for Temporary {
    /// Best-effort identity-matching cleanup also applies to early I/O failure.
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}
/// Publish only after known commit and actual binding validation; never replace destination.
/// # Arguments
/// * `marker` - Validated marker directory for an already committed and admitted binding.
/// * `value` - Complete locator assertion reconstructed from that binding.
/// # Returns
/// Success after exclusive publication or identical-locator validation, directory sync and any owned temporary cleanup.
/// # Errors
/// Return committed-unpublished conflicts or I/O failures, durability-unconfirmed directory-sync failures, or published cleanup failures.
/// Preserve the original typed cause chain and any existing destination.
pub(crate) fn publish(marker: &Path, value: &Locator) -> Result<(), Error> {
    publish_committed(marker, value).map_err(after_commit)
}
/// Existing committed binding retry, including directory sync for an identical locator.
/// # Arguments
/// * `marker` - Existing marker directory selected for the committed binding.
/// * `value` - Admitted locator value to publish or compare with an existing destination.
/// # Returns
/// Success after an identical destination is synced, or a newly prepared file is published and cleaned up.
/// # Errors
/// Return invalid marker/payload or destination conflicts, preparation/link failures and directory-sync or cleanup errors.
fn publish_committed(marker: &Path, value: &Locator) -> Result<(), Error> {
    directory(marker)?;
    value.validate()?;
    match fs::symlink_metadata(marker.join("checkout.json")) {
        Ok(_) => {
            let existing = read(&marker.join("checkout.json"))
                .map_err(|error| error.context("committed; existing locator validation failed"))?;
            if existing != *value {
                return Err(Error::config(
                    "committed; existing locator identity conflict",
                ));
            }
            return sync_present(marker);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(Error::caused(
                74,
                "committed; locator presence unavailable",
                error,
            ));
        }
    }
    Prepared::new(marker, value)
        .map_err(|error| error.context("committed; locator preparation failed"))?
        .publish()
}
/// Confirm durability only for a successfully linked or validated identical present locator.
/// # Arguments
/// * `marker` - Directory containing the successfully linked or validated identical locator.
/// # Returns
/// Success after opening and syncing the marker directory.
/// # Errors
/// Return the original directory-open/sync cause with [`Phase::DurabilityUnconfirmed`], preserving the present locator.
fn sync_present(marker: &Path) -> Result<(), Error> {
    File::open(marker)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| {
            Error::caused(
                74,
                "committed; locator present; publication durability unconfirmed",
                error,
            )
            .at(Phase::DurabilityUnconfirmed)
        })
}
/// Write complete bytes and sync the actual file before it can be linked.
/// File flush preserves ordering; this makes no claim of fallible flush behavior for `std::fs::File`.
/// # Arguments
/// * `file` - Exclusively created temporary file owned by this attempt.
/// * `bytes` - Complete bounded serialized locator payload.
/// # Returns
/// Success after writing all bytes, flushing the file and syncing it before publication.
/// # Errors
/// Return the original write or file-sync cause with preparation context, before destination publication.
fn prepare_file(file: &mut File, bytes: &[u8]) -> Result<(), Error> {
    (|| -> Result<(), Error> {
        file.write_all(bytes)?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    })()
    .map_err(|error| error.context("locator preparation failed"))
}
/// Classify a real cleanup result only after publication and directory sync have succeeded.
/// # Arguments
/// * `result` - Actual owned temporary cleanup result after link and directory sync succeeded.
/// # Returns
/// Success unchanged, or the retained cleanup failure classified as published.
/// # Errors
/// Return the original cleanup failure with [`Phase::Published`] after successful link and directory sync.
fn complete_cleanup(result: Result<(), Error>) -> Result<(), Error> {
    result.map_err(|error| {
        error
            .context("committed; locator present and directory synced; temporary cleanup failed")
            .at(Phase::Published)
    })
}
/// Complete privately synced temporary bytes, separate from the exclusive publication boundary.
pub(crate) struct Prepared {
    /// Selected validated marker directory used for exclusive publication.
    marker: PathBuf,
    /// Complete bounded lookup assertion prepared from admitted identities.
    value: Locator,
    /// Synced complete-file temporary with identity-limited cleanup ownership.
    temporary: Temporary,
}
impl Prepared {
    /// Prepare complete bytes in the same selected namespace, without destination replacement.
    /// # Arguments
    /// * `marker` - Validated same-directory marker namespace for exclusive temporary creation.
    /// * `value` - Complete locator assertion whose bytes must be ready before registration commit.
    /// # Returns
    /// An owned preparation with complete synced bytes and identity-limited temporary cleanup.
    /// # Errors
    /// Reject unsafe marker metadata, invalid payloads, exhausted temporary names or file preparation I/O failures.
    pub(crate) fn new(marker: &Path, value: &Locator) -> Result<Self, Error> {
        directory(marker)?;
        let bytes = value.bytes()?;
        let mut opened = None;
        for counter in 0..128 {
            let path = marker.join(format!(".checkout-{}-{counter}.tmp", std::process::id()));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(file) => {
                    opened = Some((path, file));
                    break;
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => {
                    return Err(Error::caused(74, "locator preparation failed", e));
                }
            }
        }
        let (path, mut file) = opened.ok_or_else(|| {
            Error::new(
                74,
                "locator preparation failed: exclusive temporary names unavailable",
            )
        })?;
        let m = metadata(&path, false)?;
        let temporary = Temporary {
            path,
            identity: (m.dev(), m.ino()),
        };
        prepare_file(&mut file, &bytes)?;
        drop(file);
        Ok(Self {
            marker: marker.to_owned(),
            value: value.clone(),
            temporary,
        })
    }
    /// Publish only a complete prepared file and confirm directory durability.
    /// # Returns
    /// Success after the prepared file is linked or an identical destination validated, the directory synced and owned temporary cleaned up.
    /// # Errors
    /// Return committed-unpublished conflicts or I/O failures, durability-unconfirmed directory-sync failures, or published cleanup failures.
    /// Preserve the original typed cause chain and any existing destination.
    pub(crate) fn publish(self) -> Result<(), Error> {
        self.publish_inner().map_err(after_commit)
    }
    /// Link without replacing a destination after the caller has confirmed its commit.
    /// # Returns
    /// Success after exclusive link or identical-destination validation, directory sync and matching temporary cleanup.
    /// # Errors
    /// Reject conflicting existing locators or link failures without overwriting; report directory-sync and owned-cleanup failures at their later phases.
    fn publish_inner(self) -> Result<(), Error> {
        let destination = self.marker.join("checkout.json");
        match fs::hard_link(&self.temporary.path, &destination) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = read(&destination)
                    .map_err(|error| error.context("committed; existing locator conflict"))?;
                if existing != self.value {
                    return Err(Error::config(
                        "committed; existing locator identity conflict",
                    ));
                }
            }
            Err(e) => {
                return Err(Error::caused(74, "committed; locator unpublished", e));
            }
        }
        sync_present(&self.marker)?;
        complete_cleanup(self.temporary.cleanup())?;
        Ok(())
    }
}
/// Pure parser and actual filesystem controls, without fabricated repository authority.
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    /// A helper-level locator fixture makes no claim of admitted repository state.
    /// # Panics
    /// Panics if fixture setup, child I/O or an expected repository or filesystem operation fails.
    fn value() -> Locator {
        Locator::new(
            Path::new("/tmp/native-repository"),
            "a".repeat(40),
            "checkout".into(),
        )
        .unwrap()
    }
    /// Encoding roundtrips native bytes; strict JSON/identity/size checks refuse malformed files.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn strict_file_and_native_path_boundaries() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("checkout.json");
        let good = value();
        let mut native = b"/tmp/native-".to_vec();
        native.push(255);
        let native = PathBuf::from(std::ffi::OsString::from_vec(native));
        assert_eq!(
            Locator::new(&native, "a".repeat(40), "checkout".into())
                .unwrap()
                .path()
                .unwrap(),
            native
        );
        for bytes in [b"{}".to_vec(),b"\xef\xbb\xbf{}".to_vec(),vec![b' ';65537],good.bytes().unwrap().into_iter().chain(b"x".iter().copied()).collect(),format!("{{\"version\":1,\"version\":1,\"repository_path_hex\":\"2f61\",\"repository_id\":\"{}\",\"checkout_id\":\"c\"}}","a".repeat(40)).into_bytes()] {
            fs::write(&path,bytes).unwrap();fs::set_permissions(&path,fs::Permissions::from_mode(0o600)).unwrap();assert!(read(&path).is_err());
        }
        fs::write(&path, good.bytes().unwrap()).unwrap();
        assert_eq!(read(&path).unwrap(), good);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(read(&path).is_err());
    }
    /// Real exclusive hard links preserve a competing destination and permit exact idempotence.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn exclusive_publication_never_overwrites_and_syncs_identical_locator() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join(".gossil");
        prepare_directory(&marker).unwrap();
        assert_eq!(fs::metadata(&marker).unwrap().mode() & 0o777, 0o700);
        let good = value();
        publish(&marker, &good).unwrap();
        let path = marker.join("checkout.json");
        let before = fs::read(&path).unwrap();
        let identity = fs::metadata(&path).unwrap().ino();
        publish(&marker, &good).unwrap();
        assert_eq!(fs::metadata(&path).unwrap().ino(), identity);
        let other = Locator::new(
            Path::new("/tmp/different"),
            "a".repeat(40),
            "checkout".into(),
        )
        .unwrap();
        assert!(publish(&marker, &other).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
        let leftovers = marker.join(".checkout-crash-leftover.tmp");
        fs::write(&leftovers, b"retained").unwrap();
        publish(&marker, &good).unwrap();
        assert_eq!(fs::read(&leftovers).unwrap(), b"retained");
    }
}

/// Additional strict boundary and pathname-ownership controls remain locator-only evidence.
#[cfg(test)]
mod boundary_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    /// The exact serialized read ceiling accepts whitespace padding and refuses one extra byte.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn exact_file_and_identity_limits_reject_every_invalid_shape() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("checkout.json");
        let good = Locator::new(Path::new("/tmp/repo"), "a".repeat(40), "checkout".into()).unwrap();
        let mut bytes = good.bytes().unwrap();
        bytes.resize(MAX, b' ');
        fs::write(&path, &bytes).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(read(&path).unwrap(), good);
        bytes.push(b' ');
        fs::write(&path, bytes).unwrap();
        assert!(read(&path).is_err());
        for (key, value) in [
            ("unknown", serde_json::json!(true)),
            ("version", serde_json::json!(2)),
            ("version", serde_json::json!(1.0)),
            ("version", serde_json::json!("1")),
            ("repository_id", serde_json::json!("A".repeat(40))),
            ("checkout_id", serde_json::json!("")),
            ("checkout_id", serde_json::json!("x".repeat(8193))),
            ("checkout_id", serde_json::json!("nul\u{0}")),
            ("repository_path_hex", serde_json::json!("2F61")),
            ("repository_path_hex", serde_json::json!("2f6")),
            ("repository_path_hex", serde_json::json!("72656c6174697665")),
            ("repository_path_hex", serde_json::json!("2f610062")),
        ] {
            let mut value_json = serde_json::to_value(&good).unwrap();
            value_json[key] = value;
            fs::write(&path, serde_json::to_vec(&value_json).unwrap()).unwrap();
            assert!(read(&path).is_err(), "{key}");
        }
        assert!(Locator::new(Path::new("/tmp/repo"), "a".repeat(40), "x".repeat(8192)).is_ok());
        let maximum = PathBuf::from(format!("/{}", "x".repeat(16383)));
        assert!(validate_path(&maximum).is_ok());
        assert!(validate_path(&PathBuf::from(format!("/{}", "x".repeat(16384)))).is_err());
    }
    /// Marker conflicts are not chmod/replaced; cleanup refuses a replaced temporary pathname.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn unsafe_marker_and_replaced_temporary_remain_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join(".gossil");
        fs::write(&marker, b"native preference file").unwrap();
        assert!(prepare_directory(&marker).is_err());
        assert_eq!(fs::read(&marker).unwrap(), b"native preference file");
        fs::remove_file(&marker).unwrap();
        prepare_directory(&marker).unwrap();
        fs::set_permissions(&marker, fs::Permissions::from_mode(0o777)).unwrap();
        assert!(prepare_directory(&marker).is_err());
        assert_eq!(fs::metadata(&marker).unwrap().mode() & 0o777, 0o777);
        fs::set_permissions(&marker, fs::Permissions::from_mode(0o700)).unwrap();
        let path = marker.join("owned.tmp");
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        let m = file.metadata().unwrap();
        let temporary = Temporary {
            path: path.clone(),
            identity: (m.dev(), m.ino()),
        };
        let original = marker.join("retained-original");
        fs::rename(&path, &original).unwrap();
        fs::write(&path, b"replacement").unwrap();
        assert!(temporary.cleanup().is_err());
        drop(temporary);
        assert_eq!(fs::read(&path).unwrap(), b"replacement");
        assert!(original.is_file());
        drop(file);
    }
}

/// Independent filesystem and payload limits without fabricated repository admission.
#[cfg(test)]
mod additional_boundary_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    /// JSON escaping can exceed the complete-file ceiling while each individual identity stays bounded.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn escaped_identity_and_maximum_native_path_exceed_complete_payload_bound() {
        let path = PathBuf::from(format!("/{}", "x".repeat(16383)));
        let error = Locator::new(&path, "a".repeat(40), "\u{1}".repeat(8192)).unwrap_err();
        assert!(error.to_string().contains("serialized bound"));
    }
    /// Existing names exhaust the bounded exclusive-create attempts and are never removed.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn real_temporary_name_exhaustion_preserves_all_competing_entries() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join(".gossil");
        prepare_directory(&marker).unwrap();
        for counter in 0..128 {
            fs::write(
                marker.join(format!(".checkout-{}-{counter}.tmp", std::process::id())),
                b"retained",
            )
            .unwrap();
        }
        let value = Locator::new(
            Path::new("/tmp/repository"),
            "a".repeat(40),
            "checkout".into(),
        )
        .unwrap();
        let error = Prepared::new(&marker, &value).err().unwrap();
        assert_eq!(error.phase, Phase::Preflight);
        assert!(
            error
                .to_string()
                .contains("exclusive temporary names unavailable")
        );
        let error = publish(&marker, &value).unwrap_err();
        assert_eq!(error.phase, Phase::CommittedUnpublished);
        assert!(!marker.join("checkout.json").exists());
        assert_eq!(fs::read_dir(&marker).unwrap().count(), 128);
        for entry in fs::read_dir(&marker).unwrap() {
            assert_eq!(fs::read(entry.unwrap().path()).unwrap(), b"retained");
        }
    }
    /// Actual different open objects cannot pass the pathname/open-file identity comparison.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn observed_open_file_identity_and_real_write_refusal_are_distinct_preflight_errors() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        fs::write(&first, b"first").unwrap();
        fs::write(&second, b"second").unwrap();
        let before = fs::metadata(&first).unwrap();
        assert!(same_file(&before, &File::open(&first).unwrap().metadata().unwrap()).is_ok());
        let error =
            same_file(&before, &File::open(&second).unwrap().metadata().unwrap()).unwrap_err();
        assert!(error.to_string().contains("identity changed"));
        let mut read_only = File::open(&first).unwrap();
        let error = prepare_file(&mut read_only, b"new").unwrap_err();
        assert_eq!(error.phase, Phase::Preflight);
        assert!(error.to_string().contains("locator preparation failed"));
        assert_eq!(fs::read(&first).unwrap(), b"first");
    }
    /// Existing malformed/private-type conflicts survive both early retry and actual EEXIST publication.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn malformed_and_nonprivate_destinations_are_preserved_before_and_after_preparation() {
        for private in [true, false] {
            let temp = tempfile::tempdir().unwrap();
            let marker = temp.path().join(".gossil");
            prepare_directory(&marker).unwrap();
            let value = Locator::new(
                Path::new("/tmp/repository"),
                "a".repeat(40),
                "checkout".into(),
            )
            .unwrap();
            let prepared = Prepared::new(&marker, &value).unwrap();
            let path = marker.join("checkout.json");
            fs::write(&path, b"conflicting destination").unwrap();
            fs::set_permissions(
                &path,
                fs::Permissions::from_mode(if private { 0o600 } else { 0o644 }),
            )
            .unwrap();
            assert_eq!(
                publish(&marker, &value).unwrap_err().phase,
                Phase::CommittedUnpublished
            );
            assert_eq!(
                prepared.publish().unwrap_err().phase,
                Phase::CommittedUnpublished
            );
            assert_eq!(fs::read(&path).unwrap(), b"conflicting destination");
        }
    }
    /// Real owner permission failures retain their precommit/committed/durability distinction.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn actual_namespace_and_temporary_create_refusals_keep_precise_phase() {
        // SAFETY: geteuid observes this process without pointer arguments.
        assert_ne!(
            unsafe { libc::geteuid() },
            0,
            "permission fixture requires unprivileged owner"
        );
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("blocked");
        fs::create_dir(&parent).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o000)).unwrap();
        let result = prepare_directory(&parent.join(".gossil"));
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(result.unwrap_err().phase, Phase::Preflight);
        let marker = parent.join(".gossil");
        prepare_directory(&marker).unwrap();
        let value = Locator::new(
            Path::new("/tmp/repository"),
            "a".repeat(40),
            "checkout".into(),
        )
        .unwrap();
        fs::set_permissions(&marker, fs::Permissions::from_mode(0o500)).unwrap();
        let prepared = Prepared::new(&marker, &value);
        fs::set_permissions(&marker, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(prepared.err().unwrap().phase, Phase::Preflight);
        fs::set_permissions(&marker, fs::Permissions::from_mode(0o600)).unwrap();
        let presence = publish(&marker, &value);
        fs::set_permissions(&marker, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(
            presence
                .unwrap_err()
                .to_string()
                .contains("locator presence unavailable")
        );
        fs::set_permissions(&marker, fs::Permissions::from_mode(0o100)).unwrap();
        let error = after_commit(sync_present(&marker).unwrap_err());
        fs::set_permissions(&marker, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(error.phase, Phase::DurabilityUnconfirmed);
    }
}

/// Actual cleanup metadata failure after real link/sync is helper evidence, not a native COMMIT/fsync fault.
#[cfg(test)]
mod cleanup_boundary_tests {
    use super::*;
    use std::error::Error as _;
    #[test]
    /// Retain the real cleanup ENOTDIR cause after link and directory sync, preserving the destination.
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn real_enotdir_cleanup_retains_cause_and_post_sync_classification() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join(".gossil");
        prepare_directory(&marker).unwrap();
        let value = Locator::new(
            Path::new("/tmp/repository"),
            "a".repeat(40),
            "checkout".into(),
        )
        .unwrap();
        let prepared = Prepared::new(&marker, &value).unwrap();
        let destination = marker.join("checkout.json");
        fs::hard_link(&prepared.temporary.path, &destination).unwrap();
        sync_present(&marker).unwrap();
        let bytes = fs::read(&destination).unwrap();
        // Preserve the actual linked file while the old temporary pathname gains a regular-file ancestor.
        let retained = temp.path().join("retained-marker");
        fs::rename(&marker, &retained).unwrap();
        fs::write(&marker, b"new regular ancestor").unwrap();
        let error = prepared.temporary.cleanup().unwrap_err();
        assert_eq!(error.code, 74);
        let cause = error
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap();
        assert_eq!(cause.raw_os_error(), Some(libc::ENOTDIR));
        assert_eq!(error.phase, Phase::Preflight);
        let error = complete_cleanup(Err(error)).unwrap_err();
        assert_eq!(error.phase, Phase::Published);
        assert!(
            error
                .to_string()
                .contains("locator present and directory synced")
        );
        let cause = error
            .source()
            .unwrap()
            .source()
            .unwrap()
            .downcast_ref::<std::io::Error>()
            .unwrap();
        assert_eq!(cause.raw_os_error(), Some(libc::ENOTDIR));
        assert_eq!(fs::read(retained.join("checkout.json")).unwrap(), bytes);
        assert_eq!(fs::read(&marker).unwrap(), b"new regular ancestor");
    }
}

/// Preserve typed failures for callers while keeping application diagnostics independent of provider input.
#[cfg(test)]
mod cause_regression_tests {
    use super::*;
    use std::error::Error as _;
    use std::os::unix::fs::PermissionsExt;
    /// Real I/O and JSON failures keep discoverable typed sources through conversion and publication context.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn typed_causes_survive_conversion_parser_and_publication_wrappers() {
        let temp = tempfile::tempdir().unwrap();
        let missing = temp.path().join("absent");
        let error = Error::from(fs::read(&missing).unwrap_err());
        assert!(
            error
                .source()
                .and_then(|source| source.downcast_ref::<std::io::Error>())
                .is_some(),
            "I/O conversion must retain the actual typed cause"
        );
        let secret = "private-provider-token";
        let malformed = format!("{{\"{secret}\":true}}");
        let error =
            Error::from(serde_json::from_slice::<Locator>(malformed.as_bytes()).unwrap_err());
        assert!(
            error
                .source()
                .and_then(|source| source.downcast_ref::<serde_json::Error>())
                .is_some()
        );
        assert!(!error.to_string().contains(secret));
        let path = temp.path().join("checkout.json");
        fs::write(&path, malformed.as_bytes()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let error = read(&path).unwrap_err();
        assert!(
            error
                .source()
                .and_then(|source| source.downcast_ref::<serde_json::Error>())
                .is_some()
        );
        assert!(!error.to_string().contains(secret));
        let marker = temp.path().join(".gossil");
        prepare_directory(&marker).unwrap();
        fs::write(marker.join("checkout.json"), malformed.as_bytes()).unwrap();
        fs::set_permissions(
            marker.join("checkout.json"),
            fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        let value = Locator::new(
            Path::new("/tmp/repository"),
            "a".repeat(40),
            "checkout".into(),
        )
        .unwrap();
        let error = publish(&marker, &value).unwrap_err();
        assert_eq!(error.phase, Phase::CommittedUnpublished);
        assert!(!error.to_string().contains(secret));
        let mut source = error.source();
        let mut found = false;
        while let Some(cause) = source {
            found |= cause.downcast_ref::<serde_json::Error>().is_some();
            source = cause.source();
        }
        assert!(
            found,
            "publication context must retain the actual parser cause"
        );
        let error = sync_present(&temp.path().join("missing-directory")).unwrap_err();
        assert_eq!(error.phase, Phase::DurabilityUnconfirmed);
        assert!(
            error
                .source()
                .and_then(|source| source.downcast_ref::<std::io::Error>())
                .is_some()
        );
    }
}

/// A second actual complete-file publisher races only after first preparation, without test hooks.
#[cfg(test)]
mod race_tests {
    use super::*;
    /// Preserve the hard-link EEXIST identity check after the early idempotent retry optimization.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn competing_complete_destination_after_preparation_is_never_overwritten() {
        for identical in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let marker = temp.path().join(".gossil");
            prepare_directory(&marker).unwrap();
            let first =
                Locator::new(Path::new("/tmp/first"), "a".repeat(40), "checkout".into()).unwrap();
            let prepared = Prepared::new(&marker, &first).unwrap();
            let second = if identical {
                first.clone()
            } else {
                Locator::new(
                    Path::new("/tmp/second"),
                    "b".repeat(40),
                    "other-checkout".into(),
                )
                .unwrap()
            };
            Prepared::new(&marker, &second).unwrap().publish().unwrap();
            let path = marker.join("checkout.json");
            let before = fs::read(&path).unwrap();
            let inode = fs::metadata(&path).unwrap().ino();
            let result = prepared.publish();
            assert_eq!(result.is_ok(), identical);
            assert_eq!(fs::read(&path).unwrap(), before);
            assert_eq!(fs::metadata(&path).unwrap().ino(), inode);
        }
    }
}

/// Exercise native commits and filesystem refusals at distinct publisher boundaries.
#[cfg(test)]
mod native_phase_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// Initialize a project-free native repository and prepare locator bytes before registration.
    /// # Panics
    /// Panics if fixture setup, child I/O or an expected repository or filesystem operation fails.
    async fn prepared_native(
        root: &Path,
        repository: &Path,
        marker: &Path,
    ) -> (crate::storage::LocalOwnerRepository, Locator, Prepared) {
        let limits = crate::storage::AdmissionLimits {
            content: crate::storage::Limits {
                max_stored_bytes: 1048576,
                max_output_bytes: 1048576,
                max_chain_depth: 128,
                max_total_stored_bytes: 67108864,
                max_total_decoded_bytes: 67108864,
            },
            parser: crate::artifact::ParseLimits {
                input_bytes: 1048576,
                cards: 4096,
                owned_bytes: 8388608,
                entries: 4096,
            },
        };
        let owner = crate::storage::LocalOwnerRepository::initialize(
            repository,
            crate::storage::InitializeOptions {
                repository_id: "0123456789abcdef0123456789abcdef01234567",
                owner_id: "publication-owner",
                date: "2026-11-20 12:34:56",
                user: b"native owner",
                password: "secret",
                sha1: false,
                content_limits: limits.content,
                parse_limits: limits.parser,
            },
        )
        .await
        .unwrap();
        prepare_directory(marker).unwrap();
        let value = Locator::new(
            &repository.canonicalize().unwrap(),
            "0123456789abcdef0123456789abcdef01234567".into(),
            "native".into(),
        )
        .unwrap();
        let prepared = Prepared::new(marker, &value).unwrap();
        let _ = root;
        (owner, value, prepared)
    }
    /// Commit the authentic registration only after complete temporary bytes have been synced.
    /// # Panics
    /// Panics if fixture setup, child I/O or an expected repository or filesystem operation fails.
    async fn committed(
        root: &Path,
        repository: &Path,
        marker: &Path,
    ) -> (crate::storage::LocalOwnerRepository, Locator, Prepared) {
        let (mut owner, value, prepared) = prepared_native(root, repository, marker).await;
        let view = owner
            .register_checkout("native", root, crate::vcs_open::limits())
            .await
            .unwrap();
        assert_eq!(view.repository_id, value.repository_id);
        assert_eq!(view.checkout_id, value.checkout_id);
        (owner, value, prepared)
    }
    /// An actual hard-link permission error occurs after the native commit and preserves registration.
    #[tokio::test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    async fn real_link_refusal_is_committed_unpublished_then_retry_uses_same_ids() {
        // SAFETY: geteuid takes no pointers and observes this process's effective user.
        assert_ne!(
            unsafe { libc::geteuid() },
            0,
            "permission fixture requires unprivileged owner"
        );
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let marker = root.join(".gossil");
        let repository = root.join("gossil.db");
        let (owner, value, prepared) = committed(&root, &repository, &marker).await;
        std::fs::set_permissions(&marker, std::fs::Permissions::from_mode(0o500)).unwrap();
        let failure = prepared.publish();
        std::fs::set_permissions(&marker, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(failure.unwrap_err().phase, Phase::CommittedUnpublished);
        assert!(!marker.join("checkout.json").exists());
        drop(owner);
        let mut reopened =
            crate::storage::LocalOwnerRepository::open(&repository, crate::vcs_open::limits())
                .await
                .unwrap();
        let view = reopened
            .checkout_view_for_root(&root, crate::vcs_open::limits())
            .await
            .unwrap();
        assert_eq!(view.checkout_id, value.checkout_id);
        publish(&marker, &value).unwrap();
        assert_eq!(read(&marker.join("checkout.json")).unwrap(), value);
    }
    /// A real rejected registration transaction cannot publish its precommit temporary bytes.
    #[tokio::test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    async fn failed_registration_drops_prepared_bytes_without_publication() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let marker = root.join(".gossil");
        let repository = root.join("gossil.db");
        let (mut owner, first, initial) = committed(&root, &repository, &marker).await;
        initial.publish().unwrap();
        let destination = fs::read(marker.join("checkout.json")).unwrap();
        let other_root = root.join("other");
        fs::create_dir(&other_root).unwrap();
        let other_marker = other_root.join(".gossil");
        prepare_directory(&other_marker).unwrap();
        let other =
            Locator::new(&repository, first.repository_id.clone(), "second".into()).unwrap();
        let prepared = Prepared::new(&other_marker, &other).unwrap();
        let temporary = prepared.temporary.path.clone();
        assert!(
            owner
                .register_checkout("second", &other_root, crate::vcs_open::limits())
                .await
                .is_err()
        );
        drop(prepared);
        assert!(!temporary.exists());
        assert!(!other_marker.join("checkout.json").exists());
        assert_eq!(fs::read(marker.join("checkout.json")).unwrap(), destination);
        drop(owner);
        let mut reopened =
            crate::storage::LocalOwnerRepository::open(&repository, crate::vcs_open::limits())
                .await
                .unwrap();
        let view = reopened
            .checkout_view_for_root(&root, crate::vcs_open::limits())
            .await
            .unwrap();
        assert_eq!(view.checkout_id, first.checkout_id);
    }
    /// Real directory-open refusal after a real link retains the present locator and reports uncertainty.
    #[tokio::test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    async fn present_locator_directory_sync_refusal_is_distinct_from_link_failure() {
        // SAFETY: geteuid takes no pointers and observes this process's effective user.
        assert_ne!(
            unsafe { libc::geteuid() },
            0,
            "permission fixture requires unprivileged owner"
        );
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let marker = root.join(".gossil");
        let repository = root.join("gossil.db");
        let (_owner, value, prepared) = committed(&root, &repository, &marker).await;
        // Exercise the actual link and sync boundary helpers, without fabricating a commit or error.
        fs::hard_link(&prepared.temporary.path, marker.join("checkout.json")).unwrap();
        std::fs::set_permissions(&marker, std::fs::Permissions::from_mode(0o100)).unwrap();
        let failure = sync_present(&marker);
        std::fs::set_permissions(&marker, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(failure.unwrap_err().phase, Phase::DurabilityUnconfirmed);
        assert_eq!(read(&marker.join("checkout.json")).unwrap(), value);
        // Retry validates identical bytes and performs the required directory sync.
        publish(&marker, &value).unwrap();
    }
    /// Reap an interrupted fixture even when its parent assertion fails.
    struct FixtureChild(
        /// Own the child until it has been killed and reaped on guard destruction.
        std::process::Child,
    );
    impl Drop for FixtureChild {
        /// Kill and reap the owned fixture child without propagating cleanup errors.
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    /// Read every durable application table independently of locator authority.
    /// # Panics
    /// Panics if fixture setup, child I/O or an expected repository or filesystem operation fails.
    async fn state(repository: &Path) -> Vec<(String, Vec<Vec<libsql::Value>>)> {
        let db = libsql::Builder::new_local(repository)
            .flags(libsql::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .build()
            .await
            .unwrap();
        let connection = db.connect().unwrap();
        let mut names = connection
            .query(
                "SELECT name FROM sqlite_schema WHERE type='table' ORDER BY name",
                (),
            )
            .await
            .unwrap();
        let mut state = Vec::new();
        while let Some(row) = names.next().await.unwrap() {
            let name: String = row.get(0).unwrap();
            let mut cursor = connection
                .query(
                    &format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")),
                    (),
                )
                .await
                .unwrap();
            let mut rows = Vec::new();
            while let Some(row) = cursor.next().await.unwrap() {
                rows.push(
                    (0..row.column_count())
                        .map(|i| row.get_value(i).unwrap())
                        .collect::<Vec<_>>(),
                );
            }
            rows.sort_by_cached_key(|row| format!("{row:?}"));
            state.push((name, rows));
        }
        state
    }
    /// Start the exact current test executable; all protocol inputs are private test-only fixture state.
    /// # Panics
    /// Panics if the current test executable cannot be resolved by `child_with_keys`.
    fn child(root: &Path, repository: &Path, phase: &str) -> std::process::Command {
        child_with_keys(
            root,
            repository,
            phase,
            std::env::vars_os().map(|(key, _)| key),
        )
    }
    /// Build an isolated child protocol command and remove exactly the supplied inherited Gest keys.
    /// # Panics
    /// Panics if the current test executable cannot be resolved.
    fn child_with_keys(
        root: &Path,
        repository: &Path,
        phase: &str,
        keys: impl IntoIterator<Item = std::ffi::OsString>,
    ) -> std::process::Command {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "checkout_locator::native_phase_tests::crash_child_protocol",
                "--nocapture",
            ])
            .current_dir(root)
            .env("GOSSIL_LOCATOR_TEST_PHASE", phase)
            .env("GOSSIL_LOCATOR_TEST_REPOSITORY", repository)
            .env("HOME", root)
            .env("XDG_CONFIG_HOME", root)
            .env("XDG_DATA_HOME", root);
        for key in keys {
            if key.to_str().is_some_and(|key| key.starts_with("GEST_")) {
                command.env_remove(key);
            }
        }
        command
    }
    /// Drain real protocol output, distinguishing normal EOF before any readiness claim.
    /// # Panics
    /// Panics if reading the fixture protocol fails; the readiness callback may also panic.
    fn drain_ready(mut output: impl std::io::BufRead, mut ready: impl FnMut()) -> bool {
        let mut observed = false;
        loop {
            let mut line = String::new();
            if output.read_line(&mut line).unwrap() == 0 {
                return observed;
            }
            if !observed && line.trim() == "GOSSIL_LOCATOR_PHASE_READY" {
                observed = true;
                ready();
            }
        }
    }
    /// Notify readiness once, or report false only after actual EOF without a ready token.
    /// # Panics
    /// Panics if reading the fixture protocol fails; the readiness callback may also panic.
    fn notify_ready(output: impl std::io::BufRead, sender: std::sync::mpsc::Sender<bool>) {
        let observed = drain_ready(output, || {
            let _ = sender.send(true);
        });
        if !observed {
            let _ = sender.send(false);
        }
    }
    /// Produce a real closed peer whose bytes are drained through the same child protocol reader.
    /// # Panics
    /// Panics if the actual pipe cannot be created or its fixture bytes cannot be written.
    fn protocol_pipe(bytes: &[u8]) -> std::io::BufReader<File> {
        use std::os::fd::FromRawFd;
        let mut descriptors = [0; 2];
        // SAFETY: pipe writes exactly two descriptors into this writable array.
        assert_eq!(unsafe { libc::pipe(descriptors.as_mut_ptr()) }, 0);
        // SAFETY: each new descriptor is adopted exactly once.
        let reader = unsafe { File::from_raw_fd(descriptors[0]) };
        // SAFETY: this distinct new descriptor is adopted once.
        let mut writer = unsafe { File::from_raw_fd(descriptors[1]) };
        writer.write_all(bytes).unwrap();
        drop(writer);
        std::io::BufReader::new(reader)
    }
    #[test]
    /// Check child environment removals and distinguish actual pipe EOF from observed readiness.
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn child_inventory_removals_and_real_pipe_eof_do_not_certify_a_ready_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let command = child_with_keys(
            temp.path(),
            &temp.path().join("repo"),
            "unused-fixture",
            ["GEST_CONFIG".into(), "OTHER".into()],
        );
        assert!(
            command
                .get_envs()
                .any(|(key, value)| key == "GEST_CONFIG" && value.is_none())
        );
        assert!(!command.get_envs().any(|(key, _)| key == "OTHER"));
        for expected in [false, true] {
            let bytes = if expected {
                b"GOSSIL_LOCATOR_PHASE_READY\n".as_slice()
            } else {
                b"ordinary child output\n".as_slice()
            };
            let mut claimed = false;
            assert_eq!(
                drain_ready(protocol_pipe(bytes), || {
                    claimed = true;
                }),
                expected
            );
            assert_eq!(claimed, expected);
            let (sender, receiver) = std::sync::mpsc::channel();
            notify_ready(protocol_pipe(bytes), sender);
            assert_eq!(receiver.recv().unwrap(), expected);
        }
    }
    /// Drive one real child termination after an observed exact publication boundary.
    /// # Panics
    /// Panics if fixture setup, child I/O or an expected repository or filesystem operation fails.
    fn crash_reopen(phase: &str) {
        use std::io::BufReader;
        use std::os::unix::process::ExitStatusExt;
        use std::process::Stdio;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let repository = temp.path().join("gossil.db");
        let normal_exit = phase.starts_with("finish-");
        let boundary = phase.strip_prefix("finish-").unwrap_or(phase);
        let mut process = FixtureChild(
            child(&root, &repository, phase)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let output = BufReader::new(process.0.stdout.take().unwrap());
        let (sender, receiver) = std::sync::mpsc::channel();
        let reader = std::thread::spawn(move || notify_ready(output, sender));
        assert!(
            receiver
                .recv_timeout(std::time::Duration::from_secs(30))
                .unwrap(),
            "child exited before the requested real boundary"
        );
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let before = runtime.block_on(state(&repository));
        let marker = root.join(".gossil");
        let mut entries = fs::read_dir(&marker)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        entries.sort();
        let path = marker.join("checkout.json");
        assert_eq!(path.exists(), boundary == "linked");
        let linked_bytes = if boundary == "linked" {
            Some(fs::read(&path).unwrap())
        } else {
            None
        };
        if normal_exit {
            use std::io::Write;
            process.0.stdin.as_mut().unwrap().write_all(&[1]).unwrap();
            assert!(process.0.wait().unwrap().success());
        } else {
            process.0.kill().unwrap();
            assert_eq!(process.0.wait().unwrap().signal(), Some(libc::SIGKILL));
        }
        reader.join().unwrap();
        assert_eq!(runtime.block_on(state(&repository)), before);
        let result = child(&root, &repository, "recover").output().unwrap();
        let diagnostic = String::from_utf8_lossy(&result.stderr);
        assert!(result.status.success(), "{diagnostic}");
        assert_eq!(runtime.block_on(state(&repository)), before);
        if boundary == "precommit" {
            assert!(!path.exists());
        } else {
            let value = read(&path).unwrap();
            assert_eq!(
                value.repository_id,
                "0123456789abcdef0123456789abcdef01234567"
            );
            assert_eq!(value.checkout_id, "native");
            assert_eq!(value.path().unwrap(), repository.canonicalize().unwrap());
        }
        if let Some(bytes) = linked_bytes {
            assert_eq!(fs::read(&path).unwrap(), bytes);
        }
        // Crash leftovers survive. Directory enumeration here is a test observation, never authority.
        if !normal_exit {
            for entry in entries {
                assert!(marker.join(entry).exists());
            }
        }
    }
    /// Kill after synced preparation but before registration; reopen cannot allocate through incompleteness.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn process_crash_before_registration_rejects_recovery_without_mutation() {
        crash_reopen("precommit");
    }
    /// Kill after authentic registration COMMIT but before link; reopen reconstructs the same registration.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn process_crash_after_commit_reconstructs_exact_existing_registration() {
        crash_reopen("committed");
    }
    /// Kill after real link but before directory sync; fresh retry validates identical bytes then syncs.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn process_crash_after_link_retries_present_locator_without_database_mutation() {
        crash_reopen("linked");
    }
    /// Normal exits flush coverage for authentic child operations, separately from the retained real kill controls.
    #[test]
    /// # Panics
    /// Panics if fixture setup fails or an asserted identity, failure phase, cause or preserved-state result differs.
    fn normal_exit_protocol_runs_each_real_boundary_and_recovery() {
        for phase in ["finish-precommit", "finish-committed", "finish-linked"] {
            crash_reopen(phase);
        }
    }
    /// Child protocol helper; the three parent tests supply every tested phase, without production hooks.
    #[test]
    /// # Panics
    /// Panics if fixture I/O or setup fails, or if the parent releases a protocol that requires process termination.
    fn crash_child_protocol() {
        use std::io::{Read, Write};
        let Some(phase) = std::env::var_os("GOSSIL_LOCATOR_TEST_PHASE") else {
            return;
        };
        let (phase, normal_exit) = if let Some(suffix) = phase
            .to_str()
            .and_then(|phase| phase.strip_prefix("finish-"))
        {
            (std::ffi::OsString::from(suffix), true)
        } else {
            (phase, false)
        };
        let repository = PathBuf::from(std::env::var_os("GOSSIL_LOCATOR_TEST_REPOSITORY").unwrap());
        let root = std::env::current_dir().unwrap().canonicalize().unwrap();
        let marker = root.join(".gossil");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        if phase == "recover" {
            let registered = runtime.block_on(async {
                let mut owner = crate::storage::LocalOwnerRepository::open(
                    &repository,
                    crate::vcs_open::limits(),
                )
                .await
                .unwrap();
                let count = owner
                    .checkout_view_for_root(&root, crate::vcs_open::limits())
                    .await;
                count.is_ok()
            });
            let args = [
                std::ffi::OsString::from("--repository"),
                repository.into_os_string(),
                std::ffi::OsString::from("--checkout"),
                std::ffi::OsString::from("native"),
            ];
            // Same production parser/adapter as product::entry; executable aliases have separate integration controls.
            assert_eq!(crate::vcs_open::run(&args).is_ok(), registered);
            if !registered {
                let mut registration = args.to_vec();
                registration.extend([std::ffi::OsString::from("--root"), root.into_os_string()]);
                assert!(crate::vcs_open::run(&registration).is_err());
            }
            return;
        }
        let (_owner, _value, _prepared) = runtime.block_on(async {
            let (owner, value, prepared) = if phase == "precommit" {
                prepared_native(&root, &repository, &marker).await
            } else {
                committed(&root, &repository, &marker).await
            };
            if phase == "linked" {
                fs::hard_link(&prepared.temporary.path, marker.join("checkout.json")).unwrap();
            }
            (owner, value, prepared)
        });
        writeln!(std::io::stdout().lock(), "\nGOSSIL_LOCATOR_PHASE_READY").unwrap();
        std::io::stdout().lock().flush().unwrap();
        let mut release = [0];
        std::io::stdin().read_exact(&mut release).unwrap();
        assert!(
            normal_exit,
            "parent must kill crash-mode children rather than release them"
        );
    }
}
