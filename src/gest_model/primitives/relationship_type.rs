//! The kinds of relationship between two entities, and the inverse of each.
//!
//! Ported from `src/store/model/primitives/relationship_type.rs` at the Gest pin.

use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

/// The type of relationship between two entities.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Primitive {
    /// Source cannot progress until the target is resolved.
    BlockedBy,
    /// Source prevents the target from progressing.
    Blocks,
    /// Source is a child of the target in a parent/child hierarchy.
    ChildOf,
    /// Source is a parent of the target in a parent/child hierarchy.
    ParentOf,
    /// Symmetric, loose association between source and target.
    RelatesTo,
}

impl Primitive {
    /// Returns the inverse relationship type.
    ///
    /// # Returns
    ///
    /// `blocks` and `blocked-by` swap, `parent-of` and `child-of` swap, and
    /// `relates-to` stays `relates-to`.
    pub fn inverse(self) -> Self {
        match self {
            Self::BlockedBy => Self::Blocks,
            Self::Blocks => Self::BlockedBy,
            Self::ChildOf => Self::ParentOf,
            Self::ParentOf => Self::ChildOf,
            Self::RelatesTo => Self::RelatesTo,
        }
    }
}

impl Display for Primitive {
    /// Write the hyphenated spelling, such as `blocked-by`.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::BlockedBy => f.write_str("blocked-by"),
            Self::Blocks => f.write_str("blocks"),
            Self::ChildOf => f.write_str("child-of"),
            Self::ParentOf => f.write_str("parent-of"),
            Self::RelatesTo => f.write_str("relates-to"),
        }
    }
}

impl FromStr for Primitive {
    /// The message that names the text that is not a relationship type.
    type Err = String;

    /// Parse one of the five hyphenated spellings in any case.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "blocked-by" => Ok(Self::BlockedBy),
            "blocks" => Ok(Self::Blocks),
            "child-of" => Ok(Self::ChildOf),
            "parent-of" => Ok(Self::ParentOf),
            "relates-to" => Ok(Self::RelatesTo),
            other => Err(format!("invalid relationship type: {other}")),
        }
    }
}
