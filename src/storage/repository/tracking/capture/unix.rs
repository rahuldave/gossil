//! No-follow descriptor capture extracted narrowly from the native checkout donor.
//!
//! ## Recommendations
//! Retain descriptor ancestry and recheck every captured object before the SQL commit.
use super::StorageError;
use std::{
    ffi::{CStr, CString},
    fs::File,
    io::Read,
    os::{
        fd::{AsRawFd, FromRawFd},
        unix::fs::MetadataExt,
    },
    path::{Component, Path, PathBuf},
};
/// Identity and metadata used to detect replacement and changes across a read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Facts {
    /// Actual descriptor device number.
    pub device: u64,
    /// Actual descriptor inode number.
    pub inode: u64,
    /// Complete native stat mode bits.
    pub mode: u32,
    /// Actual stat byte length checked before allocation.
    pub size: u64,
    /// Native modification time in whole seconds.
    pub mtime: i64,
    /// Modification subsecond component used to reject read races.
    pub mtime_ns: i64,
    /// Native change time in whole seconds.
    pub ctime: i64,
    /// Change subsecond component used to reject read races.
    pub ctime_ns: i64,
}
impl Facts {
    /// Read metadata from an already open descriptor.
    /// # Errors
    /// Returns the original descriptor metadata error.
    /// # Arguments
    /// * `file` - Open descriptor whose metadata or content this operation uses.
    /// # Returns
    /// Device, inode, mode, size and modification/change timestamps read from the descriptor.
    pub(super) fn read(file: &File) -> Result<Self, StorageError> {
        let m = file.metadata().map_err(io)?;
        Ok(Self {
            device: m.dev(),
            inode: m.ino(),
            mode: m.mode(),
            size: m.size(),
            mtime: m.mtime(),
            mtime_ns: m.mtime_nsec(),
            ctime: m.ctime(),
            ctime_ns: m.ctime_nsec(),
        })
    }
    /// Compare the stable object identity, independent of mutable directory timestamps.
    /// # Arguments
    /// * `other` - Metadata facts whose device, inode and filesystem type are compared.
    /// # Returns
    /// Whether device, inode and filesystem type match, ignoring mutable timestamps and other mode bits.
    fn same_object(&self, other: &Self) -> bool {
        self.device == other.device
            && self.inode == other.inode
            && u64::from(self.mode) & u64::from(libc::S_IFMT)
                == u64::from(other.mode) & u64::from(libc::S_IFMT)
    }
}
/// Retain the registered root descriptor and identities of its absolute ancestry.
pub(in crate::storage::repository) struct OpenedRoot {
    /// Owned descriptor of the exact registered directory.
    directory: File,
    /// Canonical spelling reopened during reachability checks.
    path: PathBuf,
    /// Root and ancestor identities retained from the first traversal.
    chain: Vec<Facts>,
}
/// A parent descriptor plus all open ancestry edges used to reach it.
pub(super) struct Parent {
    /// Owned root or parent directory descriptor.
    pub(super) directory: File,
    /// Each opened parent edge and child identity for reachability rechecks.
    edges: Vec<(File, CString, Facts)>,
}
/// Preserve a filesystem failure as a storage error.
/// # Arguments
/// * `error` - Original filesystem error retained as the cause.
/// # Returns
/// Storage error naming the failed checkout filesystem syscall.
fn io(error: std::io::Error) -> StorageError {
    StorageError::caused("checkout filesystem syscall failed", error)
}
/// Convert exact pathname bytes to a native syscall argument.
/// # Errors
/// Returns an error when the bytes contain NUL.
/// # Arguments
/// * `value` - Exact native pathname bytes for one syscall argument.
/// # Returns
/// NUL-terminated owned pathname preserving the input bytes.
pub(super) fn name(value: &[u8]) -> Result<CString, StorageError> {
    CString::new(value).map_err(|e| StorageError::caused("NUL in checkout path", e))
}
/// Open one exactly spelled directory edge without following its final symlink.
/// # Errors
/// Returns syscall, descriptor metadata or exact-spelling failures.
/// # Arguments
/// * `parent` - Open descriptor of the directory containing the child.
/// * `child` - Exact NUL-terminated child directory name.
/// # Returns
/// Owned read-only directory descriptor with matching basename spelling.
fn directory_at(parent: &File, child: &CStr) -> Result<File, StorageError> {
    // SAFETY: Borrowed parent and child CStr stay live through openat; DIRECTORY and NOFOLLOW
    // request a directory without following its final symlink.
    let fd = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            child.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        Err(io(std::io::Error::last_os_error()))
    } else {
        // SAFETY: openat returned a nonnegative, newly owned descriptor; this is its only
        // transfer into File, which also closes it if the spelling check fails.
        let file = unsafe { File::from_raw_fd(fd) };
        exact_spelling(&file, child)?;
        Ok(file)
    }
}
/// Traverse a bounded normalized absolute path and retain each directory identity.
/// # Errors
/// Refuses invalid paths, symlink edges, spelling mismatches and filesystem failures.
/// # Arguments
/// * `path` - Normalized absolute directory pathname below the platform path ceiling.
/// # Returns
/// Opened final directory and facts for the filesystem root and each traversed ancestor.
fn absolute(path: &Path) -> Result<(File, Vec<Facts>), StorageError> {
    if path.as_os_str().as_encoded_bytes().len() > libc::PATH_MAX as usize {
        return Err(StorageError::state(
            "registered root exceeds platform path ceiling",
        ));
    }
    if !path.is_absolute() {
        return Err(StorageError::state("registered root must be absolute"));
    }
    let mut current = File::open("/").map_err(io)?;
    let mut chain = vec![Facts::read(&current)?];
    for part in path.components() {
        match part {
            Component::RootDir => {}
            Component::Normal(value) => {
                use std::os::unix::ffi::OsStrExt;
                current = directory_at(&current, &name(value.as_bytes())?)?;
                chain.push(Facts::read(&current)?);
            }
            _ => return Err(StorageError::state("registered root is not normalized")),
        }
    }
    Ok((current, chain))
}
impl OpenedRoot {
    /// Bound complete path traversal, rechecks and platform pathname buffers.
    /// # Arguments
    /// * `path` - Validated relative path.
    /// * `protected` - Number of provider/sidecar metadata observations.
    /// # Returns
    /// Conservative retained and temporary allocation work before traversal begins.
    /// # Errors
    /// Refuses arithmetic overflow.
    pub(super) fn capture_work_bytes(
        &self,
        path: &str,
        protected: usize,
    ) -> Result<usize, StorageError> {
        self.chain
            .len()
            .checked_add(path.split('/').count())
            .and_then(|n| n.checked_add(protected))
            .and_then(|n| n.checked_add(4))
            .and_then(|n| n.checked_mul((libc::PATH_MAX as usize).saturating_add(1024)))
            .and_then(|n| n.checked_mul(8))
            .and_then(|n| n.checked_add(path.len()))
            .and_then(|n| n.checked_add(self.path.as_os_str().as_encoded_bytes().len()))
            .ok_or_else(|| StorageError::state("recording traversal work overflow"))
    }

    /// Bind an absolute root descriptor to its registered device and inode.
    /// # Errors
    /// Refuses traversal failures and a changed root identity.
    /// # Arguments
    /// * `path` - Canonical absolute root pathname.
    /// * `device` - Registered filesystem device number.
    /// * `inode` - Registered directory inode number.
    /// # Returns
    /// Root descriptor, canonical spelling and ancestry authenticated against registration.
    pub(super) fn open(path: &Path, device: u64, inode: u64) -> Result<Self, StorageError> {
        let (directory, chain) = absolute(path)?;
        let facts = Facts::read(&directory)?;
        if facts.device != device || facts.inode != inode {
            return Err(StorageError::state("registered root identity changed"));
        }
        Ok(Self {
            directory,
            path: path.to_owned(),
            chain,
        })
    }
    /// Reopen the absolute ancestry and compare every retained object identity.
    /// # Errors
    /// Refuses replaced ancestors, a replaced root and filesystem failures.
    /// # Returns
    /// Success when the reopened root and complete ancestry retain their identities.
    pub(super) fn recheck(&self) -> Result<(), StorageError> {
        let (file, chain) = absolute(&self.path)?;
        if chain.len() != self.chain.len()
            || chain
                .iter()
                .zip(&self.chain)
                .any(|(a, b)| !a.same_object(b))
            || !Facts::read(&file)?.same_object(&Facts::read(&self.directory)?)
        {
            return Err(StorageError::state("registered root or ancestor replaced"));
        }
        Ok(())
    }
    /// Retain no-follow directory edges for an already validated relative file path.
    /// # Errors
    /// Refuses root changes, invalid syscall names and parent traversal failures.
    /// # Arguments
    /// * `path` - Already validated relative file path whose final component is excluded.
    /// # Returns
    /// Opened parent descriptor and retained edges from the registered root.
    pub(super) fn parent(&self, path: &str) -> Result<Parent, StorageError> {
        self.recheck()?;
        let mut current = self.directory.try_clone().map_err(io)?;
        let mut edges = Vec::new();
        let mut parts = path.split('/').peekable();
        while let Some(part) = parts.next() {
            if parts.peek().is_none() {
                break;
            }
            let child = name(part.as_bytes())?;
            let next = directory_at(&current, &child)?;
            let facts = Facts::read(&next)?;
            edges.push((current, child, facts));
            current = next;
        }
        Ok(Parent {
            directory: current,
            edges,
        })
    }
}
impl Parent {
    /// Give the identity of each directory below the root on the path, in order.
    /// # Returns
    /// The facts of each directory edge, with no descriptor.
    pub(super) fn identities(&self) -> Vec<Facts> {
        self.edges
            .iter()
            .map(|(_, _, facts)| facts.clone())
            .collect()
    }
    /// Compare the directories of this path with identities saved earlier.
    /// # Arguments
    /// * `saved` - The identities that [`Self::identities`] gave for the same path.
    /// # Returns
    /// Whether each directory is the same object, in the same order.
    pub(super) fn same_identities(&self, saved: &[Facts]) -> bool {
        self.edges.len() == saved.len()
            && self
                .edges
                .iter()
                .zip(saved)
                .all(|((_, _, facts), old)| facts.same_object(old))
    }
    /// Verify that the registered root and every retained parent edge still match.
    /// # Errors
    /// Refuses replaced directory edges and filesystem failures.
    /// # Arguments
    /// * `root` - Registered root whose identity and ancestry must still match.
    /// # Returns
    /// Success when the root and all retained parent edges remain reachable unchanged.
    pub(super) fn recheck(&self, root: &OpenedRoot) -> Result<(), StorageError> {
        root.recheck()?;
        for (parent, name, facts) in &self.edges {
            let child = directory_at(parent, name)?;
            if !facts.same_object(&Facts::read(&child)?) {
                return Err(StorageError::state("checkout parent replaced"));
            }
        }
        Ok(())
    }
    /// Open an exactly spelled regular leaf without following a symlink.
    /// # Errors
    /// Refuses syscall failures, spelling mismatches and nonregular leaves.
    /// # Arguments
    /// * `basename` - Exact NUL-terminated leaf name relative to this parent.
    /// # Returns
    /// Owned descriptor for the exactly spelled regular file.
    pub(super) fn regular(&self, basename: &CStr) -> Result<File, StorageError> {
        let flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK;
        // SAFETY: borrowed descriptor and name remain live; openat requests only a
        // read-only existing leaf, never creation, and refuses final symbolic links.
        let fd = unsafe {
            libc::openat(
                self.directory.as_raw_fd(),
                basename.as_ptr(),
                flags,
                0o600 as libc::c_uint,
            )
        };
        if fd < 0 {
            return Err(io(std::io::Error::last_os_error()));
        }
        // SAFETY: This nonnegative openat result has no prior Rust owner; File takes ownership
        // once and closes it on every subsequent error path.
        let file = unsafe { File::from_raw_fd(fd) };
        exact_spelling(&file, basename)?;
        if u64::from(Facts::read(&file)?.mode) & u64::from(libc::S_IFMT) != u64::from(libc::S_IFREG)
        {
            return Err(StorageError::state("expected regular checkout leaf"));
        }
        Ok(file)
    }
}
/// Read a finite regular file and compare descriptor facts before and after.
/// # Errors
/// Refuses exhausted bounds, allocation or I/O failures and changes during the read.
/// # Arguments
/// * `file` - Open regular-file descriptor read from its current offset.
/// * `ceiling` - Maximum allowed byte length for this file.
/// * `buffer` - Nonzero maximum size of each read buffer.
/// # Returns
/// Complete observed bytes and descriptor facts unchanged across the read.
pub(super) fn read(
    file: &mut File,
    ceiling: u64,
    buffer: usize,
) -> Result<(Vec<u8>, Facts), StorageError> {
    read_inner(file, ceiling, buffer, None)
}

/// Charge complete file storage and read buffers before allocating either.
/// # Arguments
/// * `file` - Open regular descriptor at its initial offset.
/// * `ceiling` - Remaining retained file-byte allowance.
/// * `buffer` - Maximum temporary read-buffer size.
/// * `work` - Borrowed complete operation ledger.
/// # Returns
/// Complete bytes and stable descriptor facts.
/// # Errors
/// Refuses exhausted work, unstable descriptors, allocation and I/O errors.
pub(super) fn read_budget(
    file: &mut File,
    ceiling: u64,
    buffer: usize,
    work: &mut crate::storage::content::Budget,
) -> Result<(Vec<u8>, Facts), StorageError> {
    read_inner(file, ceiling, buffer, Some(work))
}

/// Perform the shared bounded read after optionally reserving its work.
/// # Arguments
/// * `file` - Open regular descriptor.
/// * `ceiling` - Maximum retained file bytes.
/// * `buffer` - Nonzero read-buffer ceiling.
/// * `work` - Optional complete recording ledger.
/// # Returns
/// Complete observed bytes and stable facts.
/// # Errors
/// Refuses finite limits, unstable descriptors and I/O errors.
fn read_inner(
    file: &mut File,
    ceiling: u64,
    buffer: usize,
    work: Option<&mut crate::storage::content::Budget>,
) -> Result<(Vec<u8>, Facts), StorageError> {
    let before = Facts::read(file)?;
    if before.size > ceiling || buffer == 0 {
        return Err(StorageError::state("checkout read exceeds finite limit"));
    }
    let length = usize::try_from(before.size)
        .map_err(|_| StorageError::state("checkout read size overflow"))?;
    if let Some(work) = work {
        // Retained output, byte-copy work and the temporary read buffer share one ledger.
        let charge = length
            .checked_mul(2)
            .and_then(|total| total.checked_add(buffer.min(length.saturating_add(1)).max(1)))
            .ok_or_else(|| StorageError::state("recording read work overflow"))?;
        work.charge_decoded(charge)?;
    }
    let bytes = read_bounded(file, length, buffer)?;
    require_read_facts(&before, &Facts::read(file)?, bytes.len(), length)?;
    Ok((bytes, before))
}

/// Read no more than an already authenticated descriptor length.
/// # Arguments
/// * `file` - Reader from its current offset.
/// * `length` - Previously observed finite byte length.
/// * `buffer` - Nonzero bounded chunk ceiling, checked by the calling descriptor reader.
/// # Returns
/// Read bytes within the observed length.
/// # Errors
/// Refuses growth, allocation and I/O errors.
fn read_bounded(
    file: &mut impl Read,
    length: usize,
    buffer: usize,
) -> Result<Vec<u8>, StorageError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|e| StorageError::caused("tracking read allocation", e))?;
    let mut chunk = vec![0; buffer.min(length.saturating_add(1)).max(1)];
    loop {
        let count = file.read(&mut chunk).map_err(io)?;
        if count == 0 {
            break;
        }
        if bytes.len().checked_add(count).is_none_or(|n| n > length) {
            return Err(StorageError::state("checkout leaf grew while reading"));
        }
        bytes.extend_from_slice(&chunk[..count]);
    }
    Ok(bytes)
}

/// Validate complete read length and final descriptor identity/metadata.
/// # Arguments
/// * `before` - Facts captured before reading.
/// * `after` - Facts observed after reading.
/// * `actual` - Number of bytes read.
/// * `expected` - Authenticated initial byte length.
/// # Returns
/// Success for a complete unchanged descriptor read.
/// # Errors
/// Refuses byte-length or descriptor-fact changes.
fn require_read_facts(
    before: &Facts,
    after: &Facts,
    actual: usize,
    expected: usize,
) -> Result<(), StorageError> {
    if actual != expected || before != after {
        return Err(StorageError::state("checkout leaf changed while reading"));
    }
    Ok(())
}
#[cfg(target_os = "macos")]
/// Compare the descriptor basename with the exact requested native bytes.
/// # Errors
/// Refuses pathname lookup failures and spelling or case mismatches.
/// # Arguments
/// * `file` - Live descriptor whose actual basename is inspected.
/// * `requested` - Exact expected native basename bytes.
/// # Returns
/// Success when the descriptor basename matches byte for byte.
fn exact_spelling(file: &File, requested: &CStr) -> Result<(), StorageError> {
    let mut bytes = vec![0u8; libc::PATH_MAX as usize];
    // SAFETY: The File descriptor stays live and bytes is a writable PATH_MAX buffer,
    // as F_GETPATH requires; its initialized bytes are inspected only after success.
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_GETPATH, bytes.as_mut_ptr()) } == -1 {
        return Err(io(std::io::Error::last_os_error()));
    }
    let end = bytes
        .iter()
        .position(|b| *b == 0)
        .ok_or_else(|| StorageError::state("descriptor pathname exceeds platform bound"))?;
    let actual = bytes[..end].rsplit(|b| *b == b'/').next().unwrap_or(&[]);
    if actual != requested.to_bytes() {
        return Err(StorageError::state("checkout path spelling/case mismatch"));
    }
    Ok(())
}
#[cfg(target_os = "linux")]
/// Compare the descriptor basename with the exact requested native bytes.
/// # Errors
/// Refuses pathname lookup failures and spelling or case mismatches.
/// # Arguments
/// * `file` - Live descriptor whose actual basename is inspected.
/// * `requested` - Exact expected native basename bytes.
/// # Returns
/// Success when the descriptor basename matches byte for byte.
fn exact_spelling(file: &File, requested: &CStr) -> Result<(), StorageError> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::fs::read_link(format!("/proc/self/fd/{}", file.as_raw_fd())).map_err(io)?;
    let actual = path
        .file_name()
        .ok_or_else(|| StorageError::state("descriptor has no basename"))?;
    if actual.as_bytes() != requested.to_bytes() {
        return Err(StorageError::state("checkout path spelling/case mismatch"));
    }
    Ok(())
}

#[cfg(test)]
mod boundary_controls {
    //! Exercise root spelling, descriptor facts and exact basename checks.
    use super::*;
    use std::{io::Write, os::unix::fs::PermissionsExt};
    /// Refuse oversized or noncanonical roots and mismatched registered identities.
    /// # Panics
    /// Panics if fixture setup fails or an invalid root is accepted or misreported.
    #[test]
    fn root_spelling_bounds_and_identity_are_explicit() {
        assert_eq!(
            absolute(Path::new(&format!(
                "/{}",
                "x".repeat(libc::PATH_MAX as usize)
            )))
            .err()
            .unwrap()
            .operation,
            "registered root exceeds platform path ceiling"
        );
        assert_eq!(
            absolute(Path::new("relative")).err().unwrap().operation,
            "registered root must be absolute"
        );
        assert_eq!(
            absolute(Path::new("/..")).err().unwrap().operation,
            "registered root is not normalized"
        );
        let temp = tempfile::tempdir().unwrap();
        let path = std::fs::canonicalize(temp.path()).unwrap();
        let facts = std::fs::metadata(&path).unwrap();
        assert_eq!(
            OpenedRoot::open(&path, facts.dev(), facts.ino().checked_add(1).unwrap())
                .err()
                .unwrap()
                .operation,
            "registered root identity changed"
        );
    }
    /// Detect file growth, incomplete reads and metadata changes against retained facts.
    /// # Panics
    /// Panics if fixture setup fails or changed descriptor facts or bytes pass validation.
    #[test]
    fn captured_length_and_final_facts_refuse_real_changes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("file");
        std::fs::write(&path, b"one").unwrap();
        let mut file = File::open(&path).unwrap();
        let before = Facts::read(&file).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap()
            .write_all(b"two")
            .unwrap();
        assert_eq!(
            read_bounded(&mut file, before.size as usize, 2)
                .unwrap_err()
                .operation,
            "checkout leaf grew while reading"
        );
        let after = Facts::read(&file).unwrap();
        assert_eq!(
            require_read_facts(&before, &after, 3, 3)
                .unwrap_err()
                .operation,
            "checkout leaf changed while reading"
        );
        assert_eq!(
            require_read_facts(&after, &after, 2, 6)
                .unwrap_err()
                .operation,
            "checkout leaf changed while reading"
        );
        assert!(require_read_facts(&after, &after, 6, 6).is_ok());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(require_read_facts(&after, &Facts::read(&file).unwrap(), 6, 6).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"onetwo");
    }
    /// Refuse a requested basename that differs from the descriptor's actual spelling.
    /// # Panics
    /// Panics if fixture setup fails or a spelling mismatch is accepted or misreported.
    #[test]
    fn exact_descriptor_basename_refuses_different_spelling() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("Actual");
        std::fs::write(&path, b"working").unwrap();
        let file = File::open(&path).unwrap();
        assert_eq!(
            exact_spelling(&file, &name(b"actual").unwrap())
                .unwrap_err()
                .operation,
            "checkout path spelling/case mismatch"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"working");
    }
    #[cfg(target_os = "macos")]
    /// Preserve a pathname-query failure for a descriptor without a filesystem name.
    /// # Panics
    /// Panics if socket creation fails or the unnamed descriptor passes the spelling check.
    #[test]
    fn descriptor_path_query_refuses_a_real_socket() {
        let (socket, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        let descriptor: std::os::fd::OwnedFd = socket.into();
        let file = File::from(descriptor);
        assert!(exact_spelling(&file, &name(b"socket").unwrap()).is_err());
    }
}
