//! The kind of entity that an id refers to, such as a task or an artifact.
//!
//! Ported from `src/store/model/primitives/entity_type.rs` at the Gest pin.

use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

/// The type of entity in the system.
///
/// Notes, tags and relationship endpoints use only these three kinds. Other
/// Gossil ticket kinds do not enlarge this enum (GEST-02).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Primitive {
    /// Persistent document such as a spec or ADR.
    Artifact,
    /// Time-boxed collection of tasks.
    Iteration,
    /// Unit of work.
    Task,
}

impl Display for Primitive {
    /// Write `artifact`, `iteration` or `task`.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Artifact => f.write_str("artifact"),
            Self::Iteration => f.write_str("iteration"),
            Self::Task => f.write_str("task"),
        }
    }
}

impl FromStr for Primitive {
    /// The message that names the text that is not an entity type.
    type Err = String;

    /// Parse `artifact`, `iteration` or `task` in any case.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "artifact" => Ok(Self::Artifact),
            "iteration" => Ok(Self::Iteration),
            "task" => Ok(Self::Task),
            other => Err(format!("invalid entity type: {other}")),
        }
    }
}
