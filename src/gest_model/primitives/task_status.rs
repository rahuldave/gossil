//! The lifecycle status of a task, which is open, in progress, done or cancelled.
//!
//! Ported from `src/store/model/primitives/task_status.rs` at the Gest pin.

use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

/// The lifecycle status of a task.
///
/// [`Display`] writes `in-progress`, the stored spelling. Serde writes
/// `inprogress`, the spelling of the Gest API. [`FromStr`] accepts both and
/// `in_progress`, in any case.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Primitive {
    /// Terminal: task was abandoned before completion.
    Cancelled,
    /// Terminal: task was completed successfully.
    Done,
    /// Actively being worked on.
    InProgress,
    /// Ready to be picked up (the default).
    #[default]
    Open,
}

impl Primitive {
    /// Whether this status represents a terminal (final) state.
    ///
    /// # Returns
    ///
    /// `true` for `Done` or `Cancelled`.
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Cancelled | Self::Done)
    }
}

impl Display for Primitive {
    /// Write the stored spelling, which is `cancelled`, `done`, `in-progress` or `open`.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("cancelled"),
            Self::Done => f.write_str("done"),
            Self::InProgress => f.write_str("in-progress"),
            Self::Open => f.write_str("open"),
        }
    }
}

impl FromStr for Primitive {
    /// The message that names the text that is not a status.
    type Err = String;

    /// Parse a status in any case, with the three spellings of in progress.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "cancelled" => Ok(Self::Cancelled),
            "done" => Ok(Self::Done),
            "in-progress" | "in_progress" | "inprogress" => Ok(Self::InProgress),
            "open" => Ok(Self::Open),
            other => Err(format!("invalid task status: {other}")),
        }
    }
}
