//! Whether an author is a human or an automated agent.
//!
//! Ported from `src/store/model/primitives/author_type.rs` at the Gest pin.

use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

/// The type of author that performed an action.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Primitive {
    /// An automated agent such as an LLM or CI bot.
    Agent,
    /// A human author (the default).
    #[default]
    Human,
}

impl Display for Primitive {
    /// Write `agent` or `human`.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Agent => f.write_str("agent"),
            Self::Human => f.write_str("human"),
        }
    }
}

impl FromStr for Primitive {
    /// The message that names the text that is not an author type.
    type Err = String;

    /// Parse `agent` or `human` in any case.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "agent" => Ok(Self::Agent),
            "human" => Ok(Self::Human),
            other => Err(format!("invalid author type: {other}")),
        }
    }
}
