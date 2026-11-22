//! The `projects` table: each repository or directory root that Gest tracks.
//!
//! Ported from `src/store/model/project.rs` at the Gest pin. The canonical
//! project record of [`crate::gest_project_values`] has no root, because the
//! root is a local checkout binding (GEST-01). So this module has no `From`
//! for that record. A caller that holds the root uses [`Model::from_synced_parts`].

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::primitives::Id;

/// A project represents a tracked repository or directory root.
///
/// Each project is uniquely identified by its [`root`](Model::root) path and
/// is assigned a stable [`Id`] at creation time.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Model {
    /// When the project was soft-archived, or `None` if it is active.
    archived_at: Option<DateTime<Utc>>,
    /// When the project was first registered.
    created_at: DateTime<Utc>,
    /// Stable identifier shared with collaborators through the synced `.gest/project.yaml`.
    id: Id,
    /// Absolute path to the local checkout root.
    root: PathBuf,
    /// When the project record was last modified.
    updated_at: DateTime<Utc>,
}

impl Model {
    /// Reconstruct a project from a synced `.gest/project.yaml` file.
    ///
    /// The id and timestamps come from the synced file (so collaborators share
    /// a stable identity), while `root` is the local checkout path.
    ///
    /// # Arguments
    ///
    /// * `id` - the id from the synced file
    /// * `root` - the local checkout path
    /// * `archived_at` - when the project was archived, or `None` if it is active
    /// * `created_at` - when the project was first registered
    /// * `updated_at` - when the project record last changed
    ///
    /// # Returns
    ///
    /// The project, built from these parts with no checks.
    pub fn from_synced_parts(
        id: Id,
        root: PathBuf,
        archived_at: Option<DateTime<Utc>>,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            archived_at,
            created_at,
            id,
            root,
            updated_at,
        }
    }

    /// Create a new project with a fresh [`Id`] and timestamps set to now.
    ///
    /// # Arguments
    ///
    /// * `root` - the path of the project root
    ///
    /// # Returns
    ///
    /// The project, active (not archived) and not yet saved.
    ///
    /// # Panics
    ///
    /// Panics when the system clock is before 1970 or the operating system
    /// gives no random bytes, as upstream does.
    pub fn new(root: PathBuf) -> Self {
        let now = super::now();
        Self {
            archived_at: None,
            created_at: now,
            id: Id::new(),
            root,
            updated_at: now,
        }
    }

    /// When the project was archived.
    ///
    /// # Returns
    ///
    /// The archive instant in UTC, or `None` while the project is active.
    pub fn archived_at(&self) -> &Option<DateTime<Utc>> {
        &self.archived_at
    }

    /// When the project was first registered.
    ///
    /// # Returns
    ///
    /// The creation instant in UTC.
    pub fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }

    /// The stable identifier of the project.
    ///
    /// # Returns
    ///
    /// The full ID.
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// The local checkout root.
    ///
    /// # Returns
    ///
    /// The path that the caller gave. Gossil never publishes it (GEST-01).
    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    /// When the project record was last modified.
    ///
    /// # Returns
    ///
    /// The modification instant in UTC.
    pub fn updated_at(&self) -> &DateTime<Utc> {
        &self.updated_at
    }
}
