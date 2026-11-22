//! The `iterations` table: time-boxed groups of tasks within a project.
//!
//! [`Model`] is one row. [`New`], [`Patch`] and [`Filter`] carry the input of a create, an update and a list.
//! Ported from `src/store/model/iteration.rs` at the Gest pin.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::primitives::{Id, IterationStatus};
use crate::gest_iteration_values::IterationValues;

/// A time-boxed collection of tasks within a project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Model {
    /// When the iteration entered a terminal status, or `None` while still active.
    completed_at: Option<DateTime<Utc>>,
    /// When the iteration was first created.
    created_at: DateTime<Utc>,
    /// Longer-form description of the iteration's goals.
    description: String,
    /// Stable identifier assigned at creation.
    id: Id,
    /// Free-form JSON object for user-defined metadata.
    metadata: Value,
    /// Project this iteration belongs to.
    project_id: Id,
    /// Current lifecycle status.
    status: IterationStatus,
    /// Short human-readable title.
    title: String,
    /// When the iteration was last modified.
    updated_at: DateTime<Utc>,
}

impl Model {
    /// When the iteration entered a terminal status.
    ///
    /// # Returns
    ///
    /// The instant in UTC, or `None` while the iteration is active.
    pub fn completed_at(&self) -> &Option<DateTime<Utc>> {
        &self.completed_at
    }

    /// When the iteration was first created.
    ///
    /// # Returns
    ///
    /// The creation instant in UTC.
    pub fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }

    /// The longer description of the goals of the iteration.
    ///
    /// # Returns
    ///
    /// The exact description text, which may be empty.
    pub fn description(&self) -> &String {
        &self.description
    }

    /// The stable identifier of the iteration.
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

    /// The project that the iteration belongs to.
    ///
    /// # Returns
    ///
    /// The full project ID.
    pub fn project_id(&self) -> &Id {
        &self.project_id
    }

    /// The lifecycle status.
    ///
    /// # Returns
    ///
    /// A copy of the status.
    pub fn status(&self) -> IterationStatus {
        self.status
    }

    /// The short title.
    ///
    /// # Returns
    ///
    /// The exact title text, which may be empty.
    pub fn title(&self) -> &String {
        &self.title
    }

    /// When the iteration was last modified.
    ///
    /// # Returns
    ///
    /// The modification instant in UTC.
    pub fn updated_at(&self) -> &DateTime<Utc> {
        &self.updated_at
    }
}

/// Builds an iteration from a record that [`crate::gest_iteration_values::iteration_from_values`] validated.
///
/// This takes the place of the upstream `TryFrom<libsql::Row>`. The converter
/// has checked every field, so the conversion cannot fail.
impl From<IterationValues> for Model {
    /// Move each validated field into the model.
    fn from(values: IterationValues) -> Self {
        Self {
            completed_at: values.completed_at,
            created_at: values.created_at,
            description: values.description,
            id: values.gest_id,
            metadata: values.metadata,
            project_id: values.project_id,
            status: values.status,
            title: values.title,
            updated_at: values.updated_at,
        }
    }
}

/// Parameters for creating a new iteration.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct New {
    /// Longer-form description of the iteration's goals.
    pub description: String,
    /// Optional user-defined metadata merged into the new row.
    pub metadata: Option<Value>,
    /// Short human-readable title.
    pub title: String,
}

/// Optional fields for updating an existing iteration.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Patch {
    /// Replacement description.
    pub description: Option<String>,
    /// Replacement metadata object. When set, it overwrites the stored value.
    pub metadata: Option<Value>,
    /// New lifecycle status. A transition to a terminal status also sets `completed_at`.
    pub status: Option<IterationStatus>,
    /// Replacement title.
    pub title: Option<String>,
}

/// Criteria for filtering iterations.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    /// Include iterations in every status, even terminal ones.
    pub all: bool,
    /// Restrict to iterations that still contain at least one open task.
    pub has_available: bool,
    /// Restrict to iterations in this status.
    pub status: Option<IterationStatus>,
    /// Restrict to iterations carrying this tag label.
    pub tag: Option<String>,
}

impl Filter {
    /// Construct a filter that includes iterations in every status, including terminal ones.
    ///
    /// # Returns
    ///
    /// A filter with `all` set and every other criterion unset.
    pub fn all() -> Self {
        Self {
            all: true,
            ..Self::default()
        }
    }
}
