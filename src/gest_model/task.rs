//! The `tasks` table: units of work within a project.
//!
//! [`Model`] is one row. [`New`], [`Patch`] and [`Filter`] carry the input of a create, an update and a list.
//! Ported from `src/store/model/task.rs` at the Gest pin.
//!
//! [`Model`]: crate::gest_model::task::Model
//! [`New`]: crate::gest_model::task::New
//! [`Patch`]: crate::gest_model::task::Patch
//! [`Filter`]: crate::gest_model::task::Filter

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::primitives::{Id, TaskStatus};
use crate::gest_task_values::TaskValues;

/// A unit of work within a project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Model {
    /// Author the task is assigned to, if any.
    assigned_to: Option<Id>,
    /// When the task was first created.
    created_at: DateTime<Utc>,
    /// Longer-form description of the work required.
    description: String,
    /// Stable identifier assigned at creation.
    id: Id,
    /// Free-form JSON object for user-defined metadata.
    metadata: Value,
    /// Optional priority score. Lower numbers rank first.
    priority: Option<u8>,
    /// Project this task belongs to.
    project_id: Id,
    /// When the task entered a terminal status, or `None` if still open.
    resolved_at: Option<DateTime<Utc>>,
    /// Current lifecycle status.
    status: TaskStatus,
    /// Short human-readable title.
    title: String,
    /// When the task was last modified.
    updated_at: DateTime<Utc>,
}

impl Model {
    /// The author the task is assigned to.
    ///
    /// # Returns
    ///
    /// The author ID, or `None` when nobody is assigned.
    pub fn assigned_to(&self) -> &Option<Id> {
        &self.assigned_to
    }

    /// When the task was first created.
    ///
    /// # Returns
    ///
    /// The creation instant in UTC.
    pub fn created_at(&self) -> &DateTime<Utc> {
        &self.created_at
    }

    /// The longer description of the work.
    ///
    /// # Returns
    ///
    /// The exact description text, which may be empty.
    pub fn description(&self) -> &String {
        &self.description
    }

    /// The stable identifier of the task.
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

    /// The priority score, where a lower number ranks first.
    ///
    /// # Returns
    ///
    /// A number from 0 through 255, or `None` when the task has no priority.
    pub fn priority(&self) -> Option<u8> {
        self.priority
    }

    /// The project that the task belongs to.
    ///
    /// # Returns
    ///
    /// The full project ID.
    pub fn project_id(&self) -> &Id {
        &self.project_id
    }

    /// When the task entered a terminal status.
    ///
    /// # Returns
    ///
    /// The instant in UTC, or `None` while the task is open or in progress.
    pub fn resolved_at(&self) -> &Option<DateTime<Utc>> {
        &self.resolved_at
    }

    /// The lifecycle status.
    ///
    /// # Returns
    ///
    /// A copy of the status.
    pub fn status(&self) -> TaskStatus {
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

    /// When the task was last modified.
    ///
    /// # Returns
    ///
    /// The modification instant in UTC.
    pub fn updated_at(&self) -> &DateTime<Utc> {
        &self.updated_at
    }
}

/// Builds a task from a record that [`crate::gest_task_values::task_from_values`] validated.
///
/// This takes the place of the upstream `TryFrom<libsql::Row>`. The converter
/// has checked every field, so the conversion cannot fail.
impl From<TaskValues> for Model {
    /// Move each validated field into the model.
    fn from(values: TaskValues) -> Self {
        Self {
            assigned_to: values.assigned_to,
            created_at: values.created_at,
            description: values.description,
            id: values.gest_id,
            metadata: values.metadata,
            priority: values.priority,
            project_id: values.project_id,
            resolved_at: values.resolved_at,
            status: values.status,
            title: values.title,
            updated_at: values.updated_at,
        }
    }
}

/// Parameters for creating a new task.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct New {
    /// Author the new task should be assigned to.
    pub assigned_to: Option<Id>,
    /// Longer-form description of the work required.
    pub description: String,
    /// Optional user-defined metadata merged into the new row.
    pub metadata: Option<Value>,
    /// Optional priority score. Lower numbers rank first.
    pub priority: Option<u8>,
    /// Initial lifecycle status. When unset, it defaults to [`TaskStatus::default`].
    pub status: Option<TaskStatus>,
    /// Short human-readable title.
    pub title: String,
}

/// Optional fields for updating an existing task.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct Patch {
    /// Replacement assignee. `Some(None)` clears it.
    pub assigned_to: Option<Option<Id>>,
    /// Replacement description.
    pub description: Option<String>,
    /// Replacement metadata object. When set, it overwrites the stored value.
    pub metadata: Option<Value>,
    /// Replacement priority. `Some(None)` clears it.
    pub priority: Option<Option<u8>>,
    /// New lifecycle status. A transition to a terminal status also sets `resolved_at`.
    pub status: Option<TaskStatus>,
    /// Replacement title.
    pub title: Option<String>,
}

/// Criteria for filtering tasks.
#[derive(Clone, Debug, Default)]
pub struct Filter {
    /// Include tasks in every status, even terminal ones.
    pub all: bool,
    /// Restrict to tasks assigned to the author matching this name or id prefix.
    pub assigned_to: Option<String>,
    /// Restrict to tasks in this status.
    pub status: Option<TaskStatus>,
    /// Restrict to tasks carrying this tag label.
    pub tag: Option<String>,
}

impl Filter {
    /// Construct a filter that includes tasks in every status, including terminal ones.
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
