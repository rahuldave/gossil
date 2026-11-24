//! The `entity_tags` join table, which attaches a tag to a task, an artifact or an iteration.
//!
//! Ported from `src/store/model/entity_tag.rs` at the Gest pin.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::primitives::{EntityType, Id};
use crate::gest_entity_tag_values::EntityTagValues;

/// A join between an entity and a tag.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Model {
    /// When the association was created.
    created_at: DateTime<Utc>,
    /// Identifier of the tagged entity.
    entity_id: Id,
    /// Which domain type [`entity_id`](Model::entity_id) refers to.
    entity_type: EntityType,
    /// Tag being attached.
    tag_id: Id,
}

impl Model {
    /// Create a new entity-tag association with `created_at` set to now.
    ///
    /// # Arguments
    ///
    /// * `entity_type` - the kind of entity being tagged
    /// * `entity_id` - the id of the tagged entity
    /// * `tag_id` - the id of the tag to attach
    ///
    /// # Returns
    ///
    /// The association, not yet saved.
    ///
    /// # Panics
    ///
    /// Panics when the system clock is before 1970, as upstream does.
    pub fn new(entity_type: EntityType, entity_id: Id, tag_id: Id) -> Self {
        Self {
            created_at: super::now(),
            entity_id,
            entity_type,
            tag_id,
        }
    }

    /// When the association was created.
    ///
    /// # Returns
    ///
    /// The creation instant in UTC.
    pub fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }

    /// The tagged entity.
    ///
    /// # Returns
    ///
    /// The full ID of the entity.
    pub fn entity_id(&self) -> &Id {
        &self.entity_id
    }

    /// The kind of the tagged entity.
    ///
    /// # Returns
    ///
    /// A copy of the entity type.
    pub fn entity_type(&self) -> EntityType {
        self.entity_type
    }

    /// The attached tag.
    ///
    /// # Returns
    ///
    /// The full ID of the tag.
    pub fn tag_id(&self) -> &Id {
        &self.tag_id
    }
}

/// Builds an association from a record that [`crate::gest_entity_tag_values::entity_tag_from_values`] validated.
///
/// The converter has checked every field, so the conversion cannot fail.
impl From<EntityTagValues> for Model {
    /// Move each validated field into the model.
    fn from(values: EntityTagValues) -> Self {
        Self {
            created_at: values.created_at,
            entity_id: values.entity_id,
            entity_type: values.entity_type,
            tag_id: values.tag_id,
        }
    }
}
