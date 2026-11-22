//! The `artifacts` table: a project's documents, such as specs and ADRs.
//!
//! [`Model`] is one row. [`New`], [`Patch`] and [`Filter`] carry the input of a create, an update and a list.
//! Ported from `src/store/model/artifact.rs` at the Gest pin.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::primitives::Id;
use crate::gest_artifact_values::ArtifactValues;

/// A persistent document within a project, such as a spec, an ADR or a design document.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Model {
    /// When the artifact was archived, or `None` if still active.
    archived_at: Option<DateTime<Utc>>,
    /// Markdown body. Serialization skips it, because Gest stores and syncs it separately.
    #[serde(skip)]
    body: String,
    /// When the artifact was first inserted.
    created_at: DateTime<Utc>,
    /// Stable identifier assigned at creation.
    id: Id,
    /// Free-form JSON object for user-defined metadata.
    metadata: Value,
    /// Project this artifact belongs to.
    project_id: Id,
    /// Short human-readable title.
    title: String,
    /// When the artifact was last modified.
    updated_at: DateTime<Utc>,
}

impl Model {
    /// When the artifact was archived.
    ///
    /// # Returns
    ///
    /// The archive instant in UTC, or `None` while the artifact is active.
    pub fn archived_at(&self) -> &Option<DateTime<Utc>> {
        &self.archived_at
    }

    /// The Markdown body.
    ///
    /// # Returns
    ///
    /// The exact body text. Serde skips this field, so a deserialized model has an empty body.
    pub fn body(&self) -> &String {
        &self.body
    }

    /// When the artifact was first inserted.
    ///
    /// # Returns
    ///
    /// The creation instant in UTC.
    pub fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }

    /// The stable identifier of the artifact.
    ///
    /// # Returns
    ///
    /// The full ID.
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// The user-defined metadata.
    ///
    /// # Returns
    ///
    /// Any JSON value, including null.
    pub fn metadata(&self) -> &Value {
        &self.metadata
    }

    /// The project that the artifact belongs to.
    ///
    /// # Returns
    ///
    /// The full project ID.
    pub fn project_id(&self) -> &Id {
        &self.project_id
    }

    /// The short title.
    ///
    /// # Returns
    ///
    /// The exact title text, which may be empty.
    pub fn title(&self) -> &String {
        &self.title
    }

    /// When the artifact was last modified.
    ///
    /// # Returns
    ///
    /// The modification instant in UTC.
    pub fn updated_at(&self) -> &DateTime<Utc> {
        &self.updated_at
    }

    /// Whether this artifact is archived.
    ///
    /// # Returns
    ///
    /// `true` when `archived_at` is set.
    pub fn is_archived(&self) -> bool {
        self.archived_at.is_some()
    }
}

/// Builds an artifact from a record that [`crate::gest_artifact_values::artifact_from_values`] validated.
///
/// This takes the place of the upstream `TryFrom<libsql::Row>`. The converter
/// has checked every field, so the conversion cannot fail.
impl From<ArtifactValues> for Model {
    /// Move each validated field, including the complete body, into the model.
    fn from(values: ArtifactValues) -> Self {
        Self {
            archived_at: values.archived_at,
            body: values.body,
            created_at: values.created_at,
            id: values.gest_id,
            metadata: values.metadata,
            project_id: values.project_id,
            title: values.title,
            updated_at: values.updated_at,
        }
    }
}

/// Parameters for creating a new artifact.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct New {
    /// Markdown body content.
    pub body: String,
    /// Optional user-defined metadata merged into the new row.
    pub metadata: Option<Value>,
    /// Short human-readable title.
    pub title: String,
}

/// Optional fields for updating an existing artifact.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Patch {
    /// Replacement markdown body.
    pub body: Option<String>,
    /// Replacement metadata object. When set, it overwrites the stored value.
    pub metadata: Option<Value>,
    /// Replacement title.
    pub title: Option<String>,
}

/// Criteria for filtering artifacts.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    /// Include archived artifacts alongside active ones.
    pub all: bool,
    /// Restrict results to archived artifacts only.
    pub only_archived: bool,
    /// Restrict to artifacts carrying this tag label.
    pub tag: Option<String>,
}
