//! The `relationships` table: directed links, such as `blocks` or `child-of`, between two entities.
//!
//! Ported from `src/store/model/relationship.rs` at the Gest pin.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::primitives::{EntityType, Id, RelationshipType};
use crate::gest_relationship_values::RelationshipValues;

/// A directed relationship between two entities.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Model {
    /// When the relationship was first recorded.
    created_at: DateTime<Utc>,
    /// Stable identifier assigned at creation.
    id: Id,
    /// Semantic kind of relationship (e.g. blocks, parent-of).
    rel_type: RelationshipType,
    /// Identifier of the source (from-side) entity.
    source_id: Id,
    /// Which domain type [`source_id`](Model::source_id) refers to.
    source_type: EntityType,
    /// Identifier of the target (to-side) entity.
    target_id: Id,
    /// Which domain type [`target_id`](Model::target_id) refers to.
    target_type: EntityType,
    /// When the relationship row was last modified.
    updated_at: DateTime<Utc>,
}

impl Model {
    /// When the relationship was first recorded.
    ///
    /// # Returns
    ///
    /// The creation instant in UTC.
    pub fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }

    /// The stable identifier of the relationship.
    ///
    /// # Returns
    ///
    /// The full ID.
    pub fn id(&self) -> &Id {
        &self.id
    }

    /// The kind of the relationship, read from the source to the target.
    ///
    /// # Returns
    ///
    /// A copy of the relationship type.
    pub fn rel_type(&self) -> RelationshipType {
        self.rel_type
    }

    /// The entity on the source side.
    ///
    /// # Returns
    ///
    /// The full ID of the source entity.
    pub fn source_id(&self) -> &Id {
        &self.source_id
    }

    /// The kind of the source entity.
    ///
    /// # Returns
    ///
    /// A copy of the source entity type.
    pub fn source_type(&self) -> EntityType {
        self.source_type
    }

    /// The entity on the target side.
    ///
    /// # Returns
    ///
    /// The full ID of the target entity.
    pub fn target_id(&self) -> &Id {
        &self.target_id
    }

    /// The kind of the target entity.
    ///
    /// # Returns
    ///
    /// A copy of the target entity type.
    pub fn target_type(&self) -> EntityType {
        self.target_type
    }

    /// When the relationship was last modified.
    ///
    /// # Returns
    ///
    /// The modification instant in UTC.
    pub fn updated_at(&self) -> &DateTime<Utc> {
        &self.updated_at
    }
}

/// Builds a relationship from a record that [`crate::gest_relationship_values::relationship_from_values`] validated.
///
/// This takes the place of the upstream `TryFrom<libsql::Row>`. The converter
/// has checked every field, so the conversion cannot fail. The direction stays as the record gives it.
impl From<RelationshipValues> for Model {
    /// Move each validated field into the model.
    fn from(values: RelationshipValues) -> Self {
        Self {
            created_at: values.created_at,
            id: values.gest_id,
            rel_type: values.rel_type,
            source_id: values.source_id,
            source_type: values.source_type,
            target_id: values.target_id,
            target_type: values.target_type,
            updated_at: values.updated_at,
        }
    }
}
