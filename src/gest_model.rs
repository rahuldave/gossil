//! The Gest domain model, with one record type for each Gest entity and the primitive values that they use.
//!
//! This module ports `src/store/model` of Gest at commit
//! `eaf821b19c3cf75b99d22e1cc70e0eff4a3562a8`, under the MIT licence in
//! `src/gest_model/gest-LICENSE.txt`. Each submodule keeps the upstream name
//! of its `Model` struct and of its `New`, `Patch` and `Filter` inputs.
//!
//! The port differs from upstream in these ways:
//!
//! - There is no `TryFrom<libsql::Row>`. Gossil does not store Gest SQL rows.
//!   A model comes from a record that a converter in `crate::gest_*_values`
//!   has validated, through `From`, so the converters stay the one place
//!   that checks a decoded field. The models also derive `Deserialize`, as
//!   upstream does.
//! - The `project_workspace` and `transaction` models are not here. They are
//!   local bookkeeping, and the storage tasks own them.
//! - Getters are written by hand, with the names and return types that the
//!   `getset` derives give upstream.
//! - The clock comes from [`std::time::SystemTime`], as `chrono::Utc::now`
//!   does, so `chrono` needs no `clock` feature.

use chrono::{DateTime, Utc};
use std::time::{SystemTime, UNIX_EPOCH};

/// Represent a project document, such as a spec or an ADR.
pub mod artifact;
/// Represent the humans and agents who write notes and other records.
pub mod author;
/// Represent the attachment of a tag to a task, an artifact or an iteration.
pub mod entity_tag;
/// Represent a time-boxed group of tasks.
pub mod iteration;
/// Represent a note attached to an entity.
pub mod note;
/// Hold the primitive values that the Gest records use.
///
/// Ported from `src/store/model/primitives.rs` at the Gest pin. Each type is
/// named `Primitive` in its own module and exported here under its domain name.
/// The module is inline here, and not in its own file, because a file of
/// declarations only has no line that the coverage report can count.
pub mod primitives {
    mod author_type;
    mod entity_type;
    mod id;
    mod iteration_status;
    mod priority;
    mod relationship_type;
    mod task_status;

    /// Distinguishes human authors from automated agents.
    pub use author_type::Primitive as AuthorType;
    /// Tags a model value with the domain entity kind it refers to.
    pub use entity_type::Primitive as EntityType;
    /// Represent the 128-bit identifier that every domain model shares.
    pub use id::Primitive as Id;
    /// Represent the lifecycle status of each iteration.
    pub use iteration_status::Primitive as IterationStatus;
    /// Represent the relative importance of a task.
    pub use priority::Primitive as Priority;
    /// Name the kind of relationship between two entities.
    pub use relationship_type::Primitive as RelationshipType;
    /// Represent the lifecycle status of each task.
    pub use task_status::Primitive as TaskStatus;
}
/// Represent a tracked project.
pub mod project;
/// Represent a directed link between two entities.
pub mod relationship;
/// Represent a label that is stored once and attached to entities.
pub mod tag;
/// Represent a unit of work within a project.
pub mod task;

/// Represent an author who creates notes, events and other actions.
pub use author::Model as Author;
/// Represent a tracked project and its root directory.
pub use project::Model as Project;
/// Represent a deduplicated label that any entity can carry.
pub use tag::Model as Tag;

/// Read the system clock as a UTC instant, as `chrono::Utc::now` does.
///
/// # Returns
///
/// The current time, with the nanoseconds that the system clock gives.
///
/// # Panics
///
/// Panics when the system clock is before 1970, as `chrono::Utc::now` does.
fn now() -> DateTime<Utc> {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before Unix epoch");
    let seconds = i64::try_from(since.as_secs()).expect("system time after year 292277026596");
    DateTime::from_timestamp(seconds, since.subsec_nanos())
        .expect("system time outside the range of chrono")
}
