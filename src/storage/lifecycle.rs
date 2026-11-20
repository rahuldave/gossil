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
    /// Existing opens follow a stable symlink; fresh ownership never follows it.
    follow: bool,
    /// Exclusive-created or initially observed device and inode.
    identity: (u64, u64),
}
impl FileIdentity {
    /// Retain regular-file metadata from an exclusive descriptor or an existing pathname.
    /// # Arguments
    /// * `path` - Native filename retained for subsequent observations.
    /// * `follow` - Whether subsequent metadata observations follow the final symlink.
    /// * `metadata` - Exclusive descriptor metadata or an earlier pathname observation.
    /// # Returns
    /// A saved regular-file device and inode expectation.
    /// # Errors
    /// Rejects metadata that does not describe a regular file.
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
        Ok(Self {
            path: path.into(),
            follow,
            identity: (metadata.dev(), metadata.ino()),
        })
    }
    /// Observe the saved regular-file identity; observations are not atomic binding.
    /// # Returns
    /// Success when the current regular-file device and inode match the saved expectation.
    /// # Errors
    /// Returns OS metadata failures or a regular-file identity disagreement.
    pub(super) fn observe(&self) -> Result<(), StorageError> {
        let metadata = Self::metadata(&self.path, self.follow)?;
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
        let identity =
            FileIdentity::from_metadata(first.path(), false, &second.as_file().metadata().unwrap())
                .unwrap();
        assert_eq!(
            identity.observe().unwrap_err().operation,
            "local repository pathname identity disagreement"
        );
        assert!(first.path().is_file());
        assert!(second.path().is_file());
    }
}
