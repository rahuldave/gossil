//! Bound complete regular-file captures to the registered checkout descriptor.
//!
//! ## Recommendations
//! Capture and recheck the complete explicit batch before publishing native tracking rows.
use super::{StorageError, TrackingLimits};
use std::{fs::File, os::unix::fs::MetadataExt, path::Path};
mod unix;
use unix::Facts;
pub(in crate::storage::repository) use unix::OpenedRoot;

/// A capture holds exact bytes, native facts and parent descriptors until [`Captured::observed`].
pub(in crate::storage::repository) struct Captured {
    /// Working bytes used only for bounded in-transaction stability checks.
    pub(in crate::storage::repository) bytes: Vec<u8>,
    /// Native executable flag for the new tracking row.
    pub(in crate::storage::repository) mode: i64,
    /// Modification time observed during initial enrollment.
    pub(in crate::storage::repository) mtime: i64,
    /// Descriptor identity and timestamps compared by the final recheck.
    facts: Facts,
    /// Original directory descriptors and edges, retained until the capture is observed.
    parent: unix::Parent,
}

/// An observed capture keeps the facts of one capture after a command has used its bytes.
///
/// It holds the facts of the leaf and of each directory on its path, with no bytes and
/// no open descriptor. So a command that captures many files holds the bytes of one
/// file at a time.
pub(in crate::storage::repository) struct Observed {
    /// Descriptor identity and timestamps of the leaf, compared by the final recheck.
    facts: Facts,
    /// The identity of each directory below the root on the path, in order.
    edges: Vec<Facts>,
}

impl Captured {
    /// Drop the bytes and the descriptors, and keep the facts for the final recheck.
    /// # Returns
    /// The facts of the leaf and of each directory on its path.
    pub(in crate::storage::repository) fn observed(self) -> Observed {
        Observed {
            edges: self.parent.identities(),
            facts: self.facts,
        }
    }
}

/// Refuse path reinterpretation, platform aliases and internal locator paths.
/// # Arguments
/// * `path` - Exact relative UTF-8 tracking pathname to validate.
/// * `limits` - Nonzero structural bounds and pathname ceilings.
/// # Returns
/// Success when the path has the supported spelling and fits the bounds.
/// # Errors
/// Refuses zero structural bounds, invalid or reserved paths and unqualified non-ASCII paths on macOS.
pub(in crate::storage::repository) fn checked_path(
    path: &str,
    limits: TrackingLimits,
) -> Result<(), StorageError> {
    if limits.paths == 0
        || limits.path_bytes == 0
        || limits.components == 0
        || limits.io_buffer_bytes == 0
    {
        return Err(StorageError::state("tracking limits must be nonzero"));
    }
    if path.is_empty()
        || path.len() > limits.path_bytes
        || path.contains(['\0', '\\'])
        || path.starts_with('/')
        || path.split('/').count() > limits.components
        || path
            .split('/')
            .any(|part| part.is_empty() || part == "." || part == "..")
        || path
            .split('/')
            .next()
            .is_some_and(|part| part.eq_ignore_ascii_case(".gossil"))
    {
        return Err(StorageError::state("invalid tracking path"));
    }
    #[cfg(target_os = "macos")]
    if !path.is_ascii() {
        return Err(StorageError::state(
            "non-ASCII platform collision semantics are not qualified",
        ));
    }
    Ok(())
}

/// Open the exact registration identity; no symlink component is followed.
/// # Arguments
/// * `path` - Canonical absolute root pathname.
/// * `device` - Registered filesystem device number.
/// * `inode` - Registered directory inode number.
/// # Returns
/// Owned no-follow root descriptor with its retained ancestry.
/// # Errors
/// Refuses traversal failures, spelling mismatches and a changed root identity.
pub(in crate::storage::repository) fn open_root(
    path: &Path,
    device: u64,
    inode: u64,
) -> Result<OpenedRoot, StorageError> {
    OpenedRoot::open(path, device, inode)
}

/// Open a recording root after charging descriptor ancestry and pathname buffers.
/// # Arguments
/// * `path` - Canonical registered root pathname.
/// * `device` - Registered native device.
/// * `inode` - Registered native inode.
/// * `work` - Borrowed whole-operation work ledger.
/// # Returns
/// Retained root and ancestry descriptors.
/// # Errors
/// Refuses exhausted work before traversal, invalid spelling or changed identity.
pub(in crate::storage::repository) fn open_root_budget(
    path: &Path,
    device: u64,
    inode: u64,
    work: &mut crate::storage::content::Budget,
) -> Result<OpenedRoot, StorageError> {
    let charge = path
        .components()
        .count()
        .checked_add(1)
        .and_then(|n| n.checked_mul((libc::PATH_MAX as usize).saturating_add(1024)))
        .and_then(|n| n.checked_mul(4))
        .and_then(|n| n.checked_add(path.as_os_str().as_encoded_bytes().len()))
        .ok_or_else(|| StorageError::state("recording root work overflow"))?;
    work.charge_decoded(charge)?;
    open_root(path, device, inode)
}

/// Capture a regular descriptor without adopting neighboring or special files.
/// # Arguments
/// * `root` - Retained registered root descriptor and ancestry.
/// * `path` - Exact validated relative file pathname.
/// * `protected` - Database and sidecar names derived from the retained provider identity.
/// * `limits` - Finite pathname, component and read-buffer bounds.
/// * `remaining` - Largest working file that this capture reads.
/// # Returns
/// Transient working bytes, executable flag, modification time, descriptor facts and retained parent edges.
/// # Errors
/// Refuses invalid paths, protected identities, nonregular files, replaced ancestry or leaves, read races, exhausted bounds and filesystem failures.
#[cfg(test)]
pub(super) fn capture(
    root: &OpenedRoot,
    path: &str,
    protected: &[&Path],
    limits: TrackingLimits,
    remaining: u64,
) -> Result<Captured, StorageError> {
    capture_inner(root, path, protected, limits, remaining, None)
}

/// Capture using the whole recording operation's borrowed work ledger.
/// # Arguments
/// * `root` - Retained registered root descriptor.
/// * `path` - Exact relative tracked path.
/// * `protected` - Retained provider and sidecar paths.
/// * `limits` - Finite capture bounds.
/// * `remaining` - Largest working file that this capture reads.
/// * `work` - Shared operation ledger, charged before reading or allocation.
/// # Returns
/// Complete bytes, unchanged descriptor facts and retained parent edges.
/// # Errors
/// Refuses exhausted work, invalid paths, identity changes and I/O failures.
pub(in crate::storage::repository) fn capture_budget(
    root: &OpenedRoot,
    path: &str,
    protected: &[&Path],
    limits: TrackingLimits,
    remaining: u64,
    work: &mut crate::storage::content::Budget,
) -> Result<Captured, StorageError> {
    checked_path(path, limits)?;
    work.charge_decoded(root.capture_work_bytes(path, protected.len())?)?;
    capture_inner(root, path, protected, limits, remaining, Some(work))
}

/// Share descriptor validation while optionally charging recording work.
/// # Arguments
/// * `root` - Retained registered root descriptor.
/// * `path` - Exact relative tracked path.
/// * `protected` - Retained provider and sidecar paths.
/// * `limits` - Finite capture bounds.
/// * `remaining` - Largest working file that this capture reads.
/// * `work` - Optional shared recording ledger.
/// # Returns
/// Complete bytes, stable descriptor facts and retained parent edges.
/// # Errors
/// Refuses capture, identity, allocation and work-limit failures.
fn capture_inner(
    root: &OpenedRoot,
    path: &str,
    protected: &[&Path],
    limits: TrackingLimits,
    remaining: u64,
    work: Option<&mut crate::storage::content::Budget>,
) -> Result<Captured, StorageError> {
    checked_path(path, limits)?;
    let parent = root.parent(path)?;
    let name = unix::name(path.rsplit('/').next().unwrap().as_bytes())?;
    let mut file = parent.regular(&name)?;
    reject_protected(&file, protected)?;
    let (bytes, facts) = match work {
        Some(work) => unix::read_budget(&mut file, path, remaining, limits.io_buffer_bytes, work)?,
        None => unix::read(&mut file, path, remaining, limits.io_buffer_bytes)?,
    };
    parent.recheck(root)?;
    let live = parent.regular(&name)?;
    require_same_leaf(&facts, &live)?;
    Ok(Captured {
        mode: i64::from(facts.mode & 0o111 != 0),
        mtime: facts.mtime,
        bytes,
        facts,
        parent,
    })
}

/// Compare the final opened leaf with captured descriptor facts.
/// # Arguments
/// * `before` - Facts retained from the complete read.
/// * `live` - Freshly opened exact leaf descriptor.
/// # Returns
/// Success when native identity and all metadata facts agree.
/// # Errors
/// Refuses replacement, metadata changes and descriptor metadata errors.
fn require_same_leaf(before: &Facts, live: &File) -> Result<(), StorageError> {
    if &Facts::read(live)? != before {
        return Err(StorageError::state("tracking leaf replaced"));
    }
    Ok(())
}

/// Reject selected database and sidecar hard links by native identity.
/// The caller supplies names only from the store's retained resolved namespace;
/// appending sidecars to an unresolved symlink spelling is not sufficient.
/// # Arguments
/// * `file` - Open candidate descriptor whose device and inode are compared.
/// * `protected` - Resolved provider and sidecar names supplied by retained private identity.
/// # Returns
/// Success when no existing protected object shares the candidate identity.
/// # Errors
/// Refuses matching protected identities and metadata failures other than absent protected paths.
fn reject_protected(file: &File, protected: &[&Path]) -> Result<(), StorageError> {
    let actual = file
        .metadata()
        .map_err(|e| StorageError::caused("tracking descriptor metadata", e))?;
    for path in protected {
        match std::fs::metadata(path) {
            Ok(value) if (value.dev(), value.ino()) == (actual.dev(), actual.ino()) => {
                return Err(StorageError::state("selected repository path is reserved"));
            }
            Ok(_) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(StorageError::caused("tracking protected metadata", e)),
        }
    }
    Ok(())
}

/// Repeat bounded capture and verify retained parent edges, exact bytes and descriptor metadata.
/// # Arguments
/// * `root` - Retained registered root descriptor and ancestry.
/// * `path` - Exact validated relative file pathname.
/// * `before` - Earlier bytes, descriptor facts and retained parent edges to verify.
/// * `protected` - Database and sidecar names derived from the retained provider identity.
/// * `limits` - Finite pathname, component and read-buffer bounds.
/// # Returns
/// Success when retained parent edges still match and a fresh capture matches the earlier bytes and facts.
/// # Errors
/// Propagates capture refusals and rejects replaced parent edges, filesystem failures and byte or metadata changes.
#[cfg(test)]
pub(super) fn recheck(
    root: &OpenedRoot,
    path: &str,
    before: &Captured,
    protected: &[&Path],
    limits: TrackingLimits,
) -> Result<(), StorageError> {
    let after = capture(root, path, protected, limits, before.bytes.len() as u64)?;
    before.parent.recheck(root)?;
    if before.facts != after.facts || before.bytes != after.bytes {
        return Err(StorageError::state("tracking file changed before commit"));
    }
    Ok(())
}

/// Check that a file is still the one that its capture read, without reading it again.
///
/// The check opens the path again from the registered root, and compares the
/// identity of each directory on it and every fact of the leaf: device, inode, mode,
/// size and the modification and change times. A write to the file changes its
/// change time, so the check finds it without a second read of the bytes. On a
/// filesystem with coarse times, a write of the same size in the same tick as the
/// capture keeps every fact, and the check does not find it.
/// # Arguments
/// * `root` - Retained registered root descriptor.
/// * `path` - Exact relative tracked path.
/// * `before` - The facts that the capture saw.
/// * `protected` - Retained provider and sidecar paths.
/// # Returns
/// Success when the root, each directory and the leaf are unchanged.
/// # Errors
/// Refuses a replaced root, ancestor or directory, a changed or replaced leaf, a
/// protected leaf and filesystem failures.
pub(in crate::storage::repository) fn recheck_observed(
    root: &OpenedRoot,
    path: &str,
    before: &Observed,
    protected: &[&Path],
) -> Result<(), StorageError> {
    let parent = root.parent(path)?;
    if !parent.same_identities(&before.edges) {
        return Err(StorageError::state("checkout parent replaced"));
    }
    let name = unix::name(path.rsplit('/').next().unwrap_or(path).as_bytes())?;
    let file = parent.regular(&name)?;
    reject_protected(&file, protected)?;
    if Facts::read(&file)? != before.facts {
        return Err(StorageError::state("tracking file changed before commit"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Exercise descriptor stability, protected objects and finite capture bounds.
    use super::*;
    use std::{
        os::unix::{
            ffi::OsStrExt,
            fs::{PermissionsExt, symlink},
        },
        path::PathBuf,
    };

    /// Explicit finite bounds for deterministic descriptor controls.
    fn limits() -> TrackingLimits {
        TrackingLimits {
            paths: 16,
            path_bytes: 4096,
            components: 32,
            retained_bytes: 1048576,
            io_buffer_bytes: 1024,
        }
    }
    /// Open an existing canonical fixture root through its actual native identity.
    fn opened(path: &Path) -> OpenedRoot {
        let metadata = std::fs::metadata(path).unwrap();
        open_root(path, metadata.dev(), metadata.ino()).unwrap()
    }

    /// Captured bytes, mode and leaf identity are all rechecked through real descriptors.
    #[test]
    fn recheck_refuses_byte_mode_and_leaf_changes() {
        /// Select the change applied after the initial capture.
        enum Change {
            /// Replace the file contents in place.
            Bytes,
            /// Change the executable permission bits.
            Mode,
            /// Replace the pathname with another file.
            Leaf,
        }
        for change in [Change::Bytes, Change::Mode, Change::Leaf] {
            let temp = tempfile::tempdir().unwrap();
            let root = std::fs::canonicalize(temp.path()).unwrap();
            let leaf = root.join("file");
            std::fs::write(&leaf, b"original").unwrap();
            std::fs::set_permissions(&leaf, std::fs::Permissions::from_mode(0o600)).unwrap();
            let opened = opened(&root);
            let mut work = crate::storage::content::Budget::new(crate::storage::Limits {
                max_stored_bytes: 128,
                max_output_bytes: 128,
                max_chain_depth: 4,
                max_total_stored_bytes: 128,
                max_total_decoded_bytes: 16 << 20,
            })
            .unwrap();
            let captured = capture_budget(&opened, "file", &[], limits(), 1024, &mut work).unwrap();
            let seen = capture_budget(&opened, "file", &[], limits(), 1024, &mut work)
                .unwrap()
                .observed();
            recheck_observed(&opened, "file", &seen, &[]).unwrap();
            match change {
                Change::Bytes => std::fs::write(&leaf, b"changed!").unwrap(),
                Change::Mode => {
                    std::fs::set_permissions(&leaf, std::fs::Permissions::from_mode(0o700)).unwrap()
                }
                Change::Leaf => {
                    std::fs::rename(&leaf, root.join("saved")).unwrap();
                    std::fs::write(&leaf, b"replacement").unwrap();
                }
            }
            let bytes = std::fs::read(&leaf).unwrap();
            let mode = std::fs::metadata(&leaf).unwrap().mode();
            assert!(
                recheck(&opened, "file", &captured, &[], limits()).is_err(),
                "accepted changed capture"
            );
            assert!(
                recheck_observed(&opened, "file", &seen, &[]).is_err(),
                "recheck of facts accepted changed capture"
            );
            assert_eq!(std::fs::read(&leaf).unwrap(), bytes);
            assert_eq!(std::fs::metadata(&leaf).unwrap().mode(), mode);
        }
    }

    /// Capture and final recheck consume one ledger across files and byte comparisons.
    #[test]
    fn recording_capture_uses_one_budget_before_file_reads() {
        use crate::storage::{Limits, content::Budget};
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        std::fs::write(root.join("one"), b"abc").unwrap();
        std::fs::write(root.join("two"), b"def").unwrap();
        let root = opened(&root);
        let budget = |decoded| {
            Budget::new(Limits {
                max_stored_bytes: 128,
                max_output_bytes: 128,
                max_chain_depth: 4,
                max_total_stored_bytes: 128,
                max_total_decoded_bytes: decoded,
            })
            .unwrap()
        };
        // Descriptor/root traversal is charged in addition to file storage/copy work.
        let one = root.capture_work_bytes("one", 0).unwrap() as u64 + 10;
        let mut work = budget(1);
        assert!(capture_budget(&root, "one", &[], limits(), 3, &mut work).is_err());
        let mut work = budget(one * 2 - 1);
        let first = capture_budget(&root, "one", &[], limits(), 3, &mut work).unwrap();
        assert_eq!(first.bytes, b"abc");
        assert_eq!(
            capture_budget(&root, "two", &[], limits(), 3, &mut work)
                .err()
                .unwrap()
                .operation,
            "total decoded content limit"
        );
        // The final recheck compares facts, so it reads no byte and charges nothing.
        let mut work = budget(one * 2);
        let first = capture_budget(&root, "one", &[], limits(), 3, &mut work).unwrap();
        let charged = work.decoded();
        recheck_observed(&root, "one", &first.observed(), &[]).unwrap();
        assert_eq!(work.decoded(), charged);
        let mut work = budget(one);
        work.charge_decoded(1).unwrap();
        assert!(capture_budget(&root, "one", &[], limits(), 3, &mut work).is_err());
    }

    /// Retained parent edges, the registered root and its ancestors refuse replacement.
    #[test]
    fn retained_parent_root_and_ancestor_replacements_are_refused() {
        /// Select the retained directory replaced before the recheck.
        enum Change {
            /// Replace the file's immediate parent.
            Parent,
            /// Replace the registered checkout root.
            Root,
            /// Replace an ancestor above the registered root.
            Ancestor,
        }
        for change in [Change::Parent, Change::Root, Change::Ancestor] {
            let temp = tempfile::tempdir().unwrap();
            let base = std::fs::canonicalize(temp.path()).unwrap();
            let ancestor = base.join("ancestor");
            let root = ancestor.join("root");
            std::fs::create_dir_all(root.join("dir")).unwrap();
            std::fs::write(root.join("dir/file"), b"original").unwrap();
            let opened = opened(&root);
            let parent = opened.parent("dir/file").unwrap();
            let captured = capture(&opened, "dir/file", &[], limits(), 1024).unwrap();
            let moved = match change {
                Change::Parent => root.join("dir"),
                Change::Root => root.clone(),
                Change::Ancestor => ancestor.clone(),
            };
            std::fs::rename(&moved, base.join("saved")).unwrap();
            std::fs::create_dir_all(root.join("dir")).unwrap();
            std::fs::write(root.join("dir/file"), b"outside replacement").unwrap();
            assert!(
                parent.recheck(&opened).is_err(),
                "accepted replaced ancestry"
            );
            assert!(recheck(&opened, "dir/file", &captured, &[], limits()).is_err());
            assert_eq!(
                std::fs::read(root.join("dir/file")).unwrap(),
                b"outside replacement"
            );
        }
    }

    /// The final recheck rejects a replaced parent even when a pre-existing hard link preserves the leaf.
    /// # Panics
    /// Panics if fixture setup fails, the unchanged capture is refused or the replaced parent is accepted.
    #[test]
    fn recheck_refuses_replaced_parent_with_same_leaf() {
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        std::fs::create_dir(root.join("dir")).unwrap();
        std::fs::create_dir(root.join("spare")).unwrap();
        std::fs::write(root.join("dir/file"), b"same leaf").unwrap();
        std::fs::hard_link(root.join("dir/file"), root.join("spare/file")).unwrap();
        let opened = opened(&root);
        let mut work = crate::storage::content::Budget::new(crate::storage::Limits {
            max_stored_bytes: 128,
            max_output_bytes: 128,
            max_chain_depth: 4,
            max_total_stored_bytes: 128,
            max_total_decoded_bytes: 16 << 20,
        })
        .unwrap();
        let captured = capture_budget(&opened, "dir/file", &[], limits(), 1024, &mut work).unwrap();
        assert!(recheck(&opened, "dir/file", &captured, &[], limits()).is_ok());
        let seen = capture_budget(&opened, "dir/file", &[], limits(), 1024, &mut work)
            .unwrap()
            .observed();
        recheck_observed(&opened, "dir/file", &seen, &[]).unwrap();
        std::fs::rename(root.join("dir"), root.join("saved")).unwrap();
        std::fs::rename(root.join("spare"), root.join("dir")).unwrap();
        let live = File::open(root.join("dir/file")).unwrap();
        assert_eq!(Facts::read(&live).unwrap(), captured.facts);
        assert!(
            recheck(&opened, "dir/file", &captured, &[], limits()).is_err(),
            "accepted changed parent with unchanged leaf"
        );
        assert_eq!(
            recheck_observed(&opened, "dir/file", &seen, &[])
                .unwrap_err()
                .operation,
            "checkout parent replaced"
        );
        assert_eq!(std::fs::read(root.join("dir/file")).unwrap(), b"same leaf");
        assert_eq!(
            std::fs::read(root.join("saved/file")).unwrap(),
            b"same leaf"
        );
    }

    /// A parent symlink and a nonblocking FIFO cannot become regular capture descriptors.
    #[test]
    fn parent_symlink_and_special_leaf_are_refused() {
        let temp = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(temp.path()).unwrap();
        let root = base.join("root");
        let outside = base.join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("file"), b"outside").unwrap();
        symlink(&outside, root.join("linked")).unwrap();
        let fifo = unix::name(root.join("fifo").as_os_str().as_bytes()).unwrap();
        // SAFETY: the live CString supplies a NUL-terminated path; mkfifo creates only this fixture FIFO.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        let opened = opened(&root);
        assert!(capture(&opened, "linked/file", &[], limits(), 1024).is_err());
        let error = capture(&opened, "fifo", &[], limits(), 1024).err().unwrap();
        assert_eq!(error.operation, "expected regular checkout leaf");
        assert_eq!(std::fs::read(outside.join("file")).unwrap(), b"outside");
        assert!(
            std::fs::symlink_metadata(root.join("linked"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    /// Actual retained rollback journal paths and hardlinks reach private inode protection.
    /// This bypasses producer write-lock acquisition so lock refusal cannot satisfy the control.
    #[tokio::test]
    async fn live_rollback_journal_and_hardlink_are_protected() {
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        let path = root.join("repo");
        std::fs::write(&path, b"").unwrap();
        let identity = crate::storage::FileIdentity::from_metadata(
            &path,
            false,
            &std::fs::metadata(&path).unwrap(),
        )
        .unwrap();
        let db = libsql::Builder::new_local(&path).build().await.unwrap();
        let connection = db.connect().unwrap();
        let mut mode = connection
            .query("PRAGMA journal_mode=DELETE", ())
            .await
            .unwrap();
        assert_eq!(
            mode.next()
                .await
                .unwrap()
                .unwrap()
                .get::<String>(0)
                .unwrap()
                .to_ascii_lowercase(),
            "delete"
        );
        drop(mode);
        connection
            .execute("CREATE TABLE fixture(value BLOB)", ())
            .await
            .unwrap();
        connection
            .execute("INSERT INTO fixture VALUES(zeroblob(8192))", ())
            .await
            .unwrap();
        connection.execute("BEGIN IMMEDIATE", ()).await.unwrap();
        connection
            .execute("UPDATE fixture SET value=randomblob(8192)", ())
            .await
            .unwrap();
        let journal = root.join("repo-journal");
        assert!(std::fs::metadata(&journal).unwrap().len() > 0);
        std::fs::hard_link(&journal, root.join("journal-alias")).unwrap();
        let before = std::fs::read(&journal).unwrap();
        let names = identity.protected_paths().unwrap();
        let protected: Vec<&Path> = names.iter().map(PathBuf::as_path).collect();
        let opened = opened(&root);
        for name in ["repo-journal", "journal-alias"] {
            let error = capture(&opened, name, &protected, limits(), 1048576)
                .err()
                .unwrap();
            assert_eq!(error.operation, "selected repository path is reserved");
        }
        assert_eq!(std::fs::read(&journal).unwrap(), before);
        connection.execute("ROLLBACK", ()).await.unwrap();
    }
    /// Refuse zero bounds, unqualified spelling, unreadable protected paths and replaced leaves.
    /// # Panics
    /// Panics if fixture setup fails or a bound, protected-path or replacement check fails.
    #[test]
    fn finite_path_limits_and_protected_observation_fail_closed() {
        for bound in [
            TrackingLimits {
                paths: 0,
                ..limits()
            },
            TrackingLimits {
                path_bytes: 0,
                ..limits()
            },
            TrackingLimits {
                components: 0,
                ..limits()
            },
            TrackingLimits {
                io_buffer_bytes: 0,
                ..limits()
            },
        ] {
            assert_eq!(
                checked_path("file", bound).unwrap_err().operation,
                "tracking limits must be nonzero"
            );
        }
        #[cfg(target_os = "macos")]
        assert_eq!(
            checked_path("é", limits()).unwrap_err().operation,
            "non-ASCII platform collision semantics are not qualified"
        );
        let temp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(temp.path()).unwrap();
        let path = root.join("file");
        std::fs::write(&path, b"original").unwrap();
        let file = File::open(&path).unwrap();
        let before = Facts::read(&file).unwrap();
        let protected = path.join("impossible-child");
        assert_eq!(
            reject_protected(&file, &[&protected])
                .unwrap_err()
                .operation,
            "tracking protected metadata"
        );
        std::fs::rename(&path, root.join("saved")).unwrap();
        std::fs::write(&path, b"replacement").unwrap();
        let replacement = File::open(&path).unwrap();
        assert_eq!(
            require_same_leaf(&before, &replacement)
                .unwrap_err()
                .operation,
            "tracking leaf replaced"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"replacement");
        assert_eq!(std::fs::read(root.join("saved")).unwrap(), b"original");
    }
}
