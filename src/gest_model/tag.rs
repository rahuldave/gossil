//! The `tags` table: labels, stored once each, that the `entity_tags` table attaches to entities.
//!
//! Ported from `src/store/model/tag.rs` at the Gest pin.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::primitives::Id;
use crate::gest_tag_values::TagValues;

/// A deduplicated label that can be attached to any entity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Model {
    /// When the tag was first inserted.
    created_at: DateTime<Utc>,
    /// Stable identifier assigned at creation.
    id: Id,
    /// Unique case-sensitive label text.
    label: String,
    /// When the tag row was last modified.
    updated_at: DateTime<Utc>,
}

impl Model {
    /// Create a new tag with a fresh [`Id`] and timestamps set to now.
    ///
    /// # Arguments
    ///
    /// * `label` - the tag's text, stored as given
    ///
    /// # Returns
    ///
    /// The tag, not yet saved.
    ///
    /// # Panics
    ///
    /// Panics when the system clock is before 1970 or the operating system
    /// gives no random bytes, as upstream does.
    pub fn new(label: impl Into<String>) -> Self {
        let now = super::now();
        Self {
            created_at: now,
            id: Id::new(),
            label: label.into(),
            updated_at: now,
        }
    }

    /// When the tag was first inserted.
    ///
    /// # Returns
    ///
    /// The creation instant in UTC.
    pub fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }

    /// The stable identifier of the tag.
    ///
    /// # Returns
    ///
    /// The full ID.
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// The label text.
    ///
    /// # Returns
    ///
    /// The exact, case-sensitive label.
    pub fn label(&self) -> &String {
        &self.label
    }

    /// When the tag was last modified.
    ///
    /// # Returns
    ///
    /// The modification instant in UTC.
    pub fn updated_at(&self) -> &DateTime<Utc> {
        &self.updated_at
    }
}

/// Builds a tag from a record that [`crate::gest_tag_values::tag_from_values`] validated.
///
/// This takes the place of the upstream `TryFrom<libsql::Row>`. The converter
/// has checked every field, so the conversion cannot fail.
impl From<TagValues> for Model {
    /// Move each validated field into the model.
    fn from(values: TagValues) -> Self {
        Self {
            created_at: values.created_at,
            id: values.gest_id,
            label: values.label,
            updated_at: values.updated_at,
        }
    }
}
