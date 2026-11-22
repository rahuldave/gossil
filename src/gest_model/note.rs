//! The `notes` table: notes attached to an entity, each with an optional author.
//!
//! [`Model`] is one row. [`New`] and [`Patch`] carry the input of a create and an update.
//! Ported from `src/store/model/note.rs` at the Gest pin.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::primitives::{EntityType, Id};
use crate::gest_note_values::NoteValues;

/// A note attached to an entity.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Model {
    /// Author who wrote the note, if known.
    author_id: Option<Id>,
    /// Markdown body content.
    body: String,
    /// When the note was first created.
    created_at: DateTime<Utc>,
    /// Identifier of the entity the note is attached to.
    entity_id: Id,
    /// Which domain type [`entity_id`](Model::entity_id) refers to.
    entity_type: EntityType,
    /// Stable identifier assigned at creation.
    id: Id,
    /// When the note body was last modified.
    updated_at: DateTime<Utc>,
}

impl Model {
    /// The author who wrote the note.
    ///
    /// # Returns
    ///
    /// The author ID, or `None` when the author is not known.
    pub fn author_id(&self) -> &Option<Id> {
        &self.author_id
    }

    /// The Markdown body.
    ///
    /// # Returns
    ///
    /// The exact body text, which may be empty.
    pub fn body(&self) -> &String {
        &self.body
    }

    /// When the note was first created.
    ///
    /// # Returns
    ///
    /// The creation instant in UTC.
    pub fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }

    /// The entity that the note is attached to.
    ///
    /// # Returns
    ///
    /// The full ID of that entity.
    pub fn entity_id(&self) -> &Id {
        &self.entity_id
    }

    /// The kind of the entity that [`entity_id`](Model::entity_id) names.
    ///
    /// # Returns
    ///
    /// A copy of the entity type.
    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    /// The stable identifier of the note.
    ///
    /// # Returns
    ///
    /// The full ID.
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// When the note body was last modified.
    ///
    /// # Returns
    ///
    /// The modification instant in UTC.
    pub fn updated_at(&self) -> &DateTime<Utc> {
        &self.updated_at
    }
}

/// Builds a note from a record that [`crate::gest_note_values::note_from_values`] validated.
///
/// This takes the place of the upstream `TryFrom<libsql::Row>`. The converter
/// has checked every field, so the conversion cannot fail.
impl From<NoteValues> for Model {
    /// Move each validated field into the model.
    fn from(values: NoteValues) -> Self {
        Self {
            author_id: values.author_id,
            body: values.body,
            created_at: values.created_at,
            entity_id: values.entity_id,
            entity_type: values.entity_type,
            id: values.gest_id,
            updated_at: values.updated_at,
        }
    }
}

/// Parameters for creating a new note.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct New {
    /// Author credited with the note, if known.
    pub author_id: Option<Id>,
    /// Markdown body content.
    pub body: String,
}

/// Optional fields for updating an existing note.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Patch {
    /// Replacement markdown body.
    pub body: Option<String>,
}
