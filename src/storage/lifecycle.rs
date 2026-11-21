//! This module observes cooperative pathnames separately from SQLite handle identity.
//!
//! ## Recommendations
//! Keep one stable native namespace through actual engine close and recovery.
use super::StorageError;
use std::os::unix::fs::MetadataExt;
use std::{
    fs::Metadata,
    path::{Path, PathBuf},
};

/// This identity records the expected regular file at separate pathname observation boundaries.
#[derive(Clone)]
pub(super) struct FileIdentity {
    /// Native filesystem filename; never a serialized SQLite URI.
    path: PathBuf,
    /// Observed resolved namespace used by the bundled Unix provider, retained before open.
    resolved_provider: PathBuf,
    /// Existing opens follow a stable symlink; fresh ownership never follows it.
    follow: bool,
    /// Exclusive-created or initially observed device and inode.
    identity: (u64, u64),
}
impl FileIdentity {
    /// Return the retained resolved provider namespace and its SQLite sidecars.
    /// These cooperative observations do not establish atomic SQLite-handle binding.
    /// # Returns
    /// Retained resolved database path and its -journal, -wal and -shm neighbor names.
    /// # Errors
    /// Refuses selected-file identity changes, resolved-namespace changes and observation failures.
    pub(super) fn protected_paths(&self) -> Result<Vec<PathBuf>, StorageError> {
        self.observe()?;
        let mut paths = Vec::new();
        for suffix in ["", "-journal", "-wal", "-shm"] {
            let mut value = self.resolved_provider.as_os_str().to_owned();
            value.push(suffix);
            paths.push(PathBuf::from(value));
        }
        Ok(paths)
    }

    /// Retain regular-file metadata from an exclusive descriptor or an existing pathname.
    /// # Arguments
    /// * `path` - Native filename retained for subsequent observations.
    /// * `follow` - Whether subsequent metadata observations follow the final symlink.
    /// * `metadata` - Exclusive descriptor metadata or an earlier pathname observation.
    /// # Returns
    /// Saved regular-file identity and the resolved provider namespace.
    /// # Errors
    /// Rejects nonregular metadata, namespace lookup failures and disagreement
    /// between the supplied identity and the resolved provider file.
    pub(super) fn from_metadata(
        path: &Path,
        follow: bool,
        metadata: &Metadata,
    ) -> Result<Self, StorageError> {
        if !metadata.is_file() {
            return Err(StorageError::state(
                "local repository is not a regular file",
            ));
        }
        let resolved_provider = std::fs::canonicalize(path).map_err(|error| {
            StorageError::caused("resolved provider namespace unavailable", error)
        })?;
        let resolved = Self::metadata(&resolved_provider, false)?;
        if !Self::matches(
            (metadata.dev(), metadata.ino()),
            resolved.is_file(),
            (resolved.dev(), resolved.ino()),
        ) {
            return Err(StorageError::state(
                "resolved provider identity disagreement",
            ));
        }
        Ok(Self {
            path: path.into(),
            resolved_provider,
            follow,
            identity: (metadata.dev(), metadata.ino()),
        })
    }
    /// Observe the saved regular-file identity; observations are not atomic binding.
    /// # Returns
    /// Success when the selected file, resolved namespace and provider identity match their saved expectations.
    /// # Errors
    /// Returns metadata or namespace lookup failures, namespace changes or regular-file identity disagreements.
    pub(super) fn observe(&self) -> Result<(), StorageError> {
        let metadata = Self::metadata(&self.path, self.follow)?;
        let resolved = std::fs::canonicalize(&self.path).map_err(|error| {
            StorageError::caused("resolved provider namespace unavailable", error)
        })?;
        if resolved != self.resolved_provider {
            return Err(StorageError::state("resolved provider namespace changed"));
        }
        let provider = Self::metadata(&self.resolved_provider, false)?;
        if !Self::matches(
            self.identity,
            provider.is_file(),
            (provider.dev(), provider.ino()),
        ) {
            return Err(StorageError::state(
                "resolved provider identity disagreement",
            ));
        }
        if !Self::matches(
            self.identity,
            metadata.is_file(),
            (metadata.dev(), metadata.ino()),
        ) {
            return Err(StorageError::state(
                "local repository pathname identity disagreement",
            ));
        }
        Ok(())
    }
    /// Compare ordinary metadata records without any namespace mutation.
    /// # Arguments
    /// * `expected` - Saved device and inode.
    /// * `regular` - Whether the observed entry is a regular file.
    /// * `actual` - Observed device and inode.
    /// # Returns
    /// Whether regular kind, device and inode all agree.
    pub(super) fn matches(expected: (u64, u64), regular: bool, actual: (u64, u64)) -> bool {
        regular && expected == actual
    }
    /// Obtain following or no-follow metadata without raw `File` open/close.
    /// # Arguments
    /// * `path` - Native filename to observe without opening a raw descriptor.
    /// * `follow` - Select following metadata instead of no-follow final-entry metadata.
    /// # Returns
    /// The observed OS metadata.
    /// # Errors
    /// Returns the original metadata IO failure through the repository error boundary.
    fn metadata(path: &Path, follow: bool) -> Result<Metadata, StorageError> {
        (if follow {
            std::fs::metadata(path)
        } else {
            std::fs::symlink_metadata(path)
        })
        .map_err(|error| StorageError::caused("repository identity metadata unavailable", error))
    }
}

#[cfg(test)]
/// Check regular-file identity refusals using stable owned filesystem entries.
mod tests {
    use super::*;
    /// Directory metadata cannot establish regular-file authority.
    #[test]
    fn descriptor_identity_refuses_nonregular_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let error = FileIdentity::from_metadata(
            temp.path(),
            false,
            &std::fs::metadata(temp.path()).unwrap(),
        )
        .err()
        .unwrap();
        assert_eq!(error.operation, "local repository is not a regular file");
    }
    /// Metadata from another owned descriptor cannot authenticate an unchanged pathname.
    #[test]
    fn descriptor_identity_refuses_other_owned_file_without_namespace_changes() {
        let temp = tempfile::tempdir().unwrap();
        let first = tempfile::NamedTempFile::new_in(temp.path()).unwrap();
        let second = tempfile::NamedTempFile::new_in(temp.path()).unwrap();
        let error =
            FileIdentity::from_metadata(first.path(), false, &second.as_file().metadata().unwrap())
                .err()
                .unwrap();
        assert_eq!(error.operation, "resolved provider identity disagreement");
        assert!(first.path().is_file());
        assert!(second.path().is_file());
    }
    /// Retained provider namespace refuses same-inode retargeting and resolved replacement.
    #[test]
    fn protected_namespace_is_retained_and_reobserved() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let other = temp.path().join("other");
        let selected = temp.path().join("selected");
        std::fs::write(&first, b"database").unwrap();
        std::fs::hard_link(&first, &other).unwrap();
        symlink(&first, &selected).unwrap();
        let identity =
            FileIdentity::from_metadata(&selected, true, &std::fs::metadata(&selected).unwrap())
                .unwrap();
        let resolved = std::fs::canonicalize(&first).unwrap();
        assert_eq!(
            identity.protected_paths().unwrap(),
            ["", "-journal", "-wal", "-shm"].map(|suffix| {
                let mut path = resolved.as_os_str().to_owned();
                path.push(suffix);
                PathBuf::from(path)
            })
        );
        std::fs::remove_file(&selected).unwrap();
        symlink(&other, &selected).unwrap();
        assert_eq!(
            identity.protected_paths().unwrap_err().operation,
            "resolved provider namespace changed"
        );
        std::fs::remove_file(&selected).unwrap();
        symlink(&first, &selected).unwrap();
        std::fs::remove_file(&first).unwrap();
        std::fs::write(&first, b"replacement").unwrap();
        assert!(identity.protected_paths().is_err());
    }
    /// Preserve namespace lookup failures during identity capture and later observation.
    /// # Panics
    /// Panics if fixture setup fails or namespace failures are accepted or misreported.
    #[test]
    fn unavailable_resolved_namespace_is_an_error_at_both_boundaries() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repo");
        std::fs::write(&path, b"original").unwrap();
        let metadata = std::fs::metadata(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            FileIdentity::from_metadata(&path, false, &metadata)
                .err()
                .unwrap()
                .operation,
            "resolved provider namespace unavailable"
        );
        std::fs::write(&path, b"original").unwrap();
        let identity =
            FileIdentity::from_metadata(&path, false, &std::fs::metadata(&path).unwrap()).unwrap();
        std::fs::rename(&path, temp.path().join("saved")).unwrap();
        symlink(temp.path().join("missing"), &path).unwrap();
        assert_eq!(
            identity.observe().unwrap_err().operation,
            "resolved provider namespace unavailable"
        );
        assert_eq!(
            std::fs::read(temp.path().join("saved")).unwrap(),
            b"original"
        );
    }
}
