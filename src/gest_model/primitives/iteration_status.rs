//! The lifecycle status of an iteration, which is active, completed or cancelled.
//!
//! Ported from `src/store/model/primitives/iteration_status.rs` at the Gest pin.

use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

/// The lifecycle status of an iteration.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Primitive {
    /// In-progress iteration accepting work (the default).
    #[default]
    Active,
    /// Terminal: iteration was abandoned before completion.
    Cancelled,
    /// Terminal: iteration finished successfully.
    Completed,
}

impl Primitive {
    /// Whether this status represents a terminal (final) state.
    ///
    /// # Returns
    ///
    /// `true` for `Completed` or `Cancelled`.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Completed)
    }
}

impl Display for Primitive {
    /// Write `active`, `cancelled` or `completed`.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Active => f.write_str("active"),
            Self::Cancelled => f.write_str("cancelled"),
            Self::Completed => f.write_str("completed"),
        }
    }
}

impl FromStr for Primitive {
    /// The message that names the text that is not a status.
    type Err = String;

    /// Parse `active`, `cancelled` or `completed` in any case.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "active" => Ok(Self::Active),
            "cancelled" => Ok(Self::Cancelled),
            "completed" => Ok(Self::Completed),
            other => Err(format!("invalid iteration status: {other}")),
        }
    }
}
