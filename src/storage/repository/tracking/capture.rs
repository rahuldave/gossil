//! Bound complete regular-file captures to the registered checkout descriptor.
//!
//! ## Recommendations
//! Capture and recheck the complete explicit batch before publishing native tracking rows.
use super::{StorageError, TrackingLimits};
use std::{fs::File, os::unix::fs::MetadataExt, path::Path};
mod unix;
use unix::Facts;
pub(super) use unix::OpenedRoot;

/// Exact bytes and native facts, retained only until the transaction ends.
pub(super) struct Captured {
    /// Working bytes used only for bounded in-transaction stability checks.
    pub(super) bytes: Vec<u8>,
    /// Native executable flag for the new tracking row.
    pub(super) mode: i64,
    /// Modification time observed during initial enrollment.
    pub(super) mtime: i64,
    /// Descriptor identity and timestamps compared by the final recheck.
    facts: Facts,
}

/// Refuse path reinterpretation, platform aliases and internal locator paths.
/// # Arguments
/// * `path` - Exact relative UTF-8 tracking pathname to validate.
/// * `limits` - Nonzero structural bounds and pathname ceilings.
/// # Returns
/// Success when the path has the supported spelling and fits the bounds.
/// # Errors
/// Refuses zero structural bounds, invalid or reserved paths and unqualified non-ASCII paths on macOS.
pub(super) fn checked_path(path: &str, limits: TrackingLimits) -> Result<(), StorageError> {
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
pub(super) fn open_root(path: &Path, device: u64, inode: u64) -> Result<OpenedRoot, StorageError> {
    OpenedRoot::open(path, device, inode)
}

/// Capture a regular descriptor without adopting neighboring or special files.
/// # Arguments
/// * `root` - Retained registered root descriptor and ancestry.
/// * `path` - Exact validated relative file pathname.
/// * `protected` - Database and sidecar names derived from the retained provider identity.
/// * `limits` - Finite pathname, component and read-buffer bounds.
/// * `remaining` - Remaining aggregate working-byte allowance for this batch.
/// # Returns
/// Transient working bytes, executable flag, modification time and descriptor facts.
/// # Errors
/// Refuses invalid paths, protected identities, nonregular files, replaced ancestry or leaves, read races, exhausted bounds and filesystem failures.
pub(super) fn capture(
    root: &OpenedRoot,
    path: &str,
    protected: &[&Path],
    limits: TrackingLimits,
    remaining: u64,
) -> Result<Captured, StorageError> {
    checked_path(path, limits)?;
    let parent = root.parent(path)?;
    let name = unix::name(path.rsplit('/').next().unwrap().as_bytes())?;
    let mut file = parent.regular(&name)?;
    reject_protected(&file, protected)?;
    let (bytes, facts) = unix::read(&mut file, remaining, limits.io_buffer_bytes)?;
    parent.recheck(root)?;
    let live = parent.regular(&name)?;
    require_same_leaf(&facts, &live)?;
    Ok(Captured {
        mode: i64::from(facts.mode & 0o111 != 0),
        mtime: facts.mtime,
        bytes,
        facts,
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

/// Repeat bounded capture and compare exact bytes and descriptor metadata.
/// # Arguments
/// * `root` - Retained registered root descriptor and ancestry.
/// * `path` - Exact validated relative file pathname.
/// * `before` - Earlier transient bytes and descriptor facts to compare.
/// * `protected` - Database and sidecar names derived from the retained provider identity.
/// * `limits` - Finite pathname, component and read-buffer bounds.
/// # Returns
/// Success when a fresh bounded capture exactly matches the earlier bytes and facts.
/// # Errors
/// Propagates capture refusals and rejects any byte or metadata change.
pub(super) fn recheck(
    root: &OpenedRoot,
    path: &str,
    before: &Captured,
    protected: &[&Path],
    limits: TrackingLimits,
) -> Result<(), StorageError> {
    let after = capture(root, path, protected, limits, before.bytes.len() as u64)?;
    if before.facts != after.facts || before.bytes != after.bytes {
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
            let captured = capture(&opened, "file", &[], limits(), 1024).unwrap();
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
            assert_eq!(std::fs::read(&leaf).unwrap(), bytes);
            assert_eq!(std::fs::metadata(&leaf).unwrap().mode(), mode);
        }
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
