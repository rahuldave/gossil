//! The priority of a task.
//!
//! Ported from `src/store/model/primitives/priority.rs` at the Gest pin.

use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

/// Relative importance assigned to a task.
///
/// The five named levels map to `0..=4`. The task model itself stores any
/// `u8` priority, so this enum is the named surface and not the only one (GEST-02).
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum Primitive {
    /// Must be addressed immediately. It blocks other work.
    Critical = 0,
    /// Important but not a blocker.
    High = 1,
    /// Should be done eventually.
    Low = 3,
    /// Nice to have. Defer it if needed.
    Lowest = 4,
    /// Default importance for most work.
    Medium = 2,
}

impl Primitive {
    /// All priority variants in rank order, from highest to lowest.
    pub const ALL: &'static [Primitive] = &[
        Primitive::Critical,
        Primitive::High,
        Primitive::Medium,
        Primitive::Low,
        Primitive::Lowest,
    ];
}

impl Display for Primitive {
    /// Write the lowercase name, such as `critical`.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Critical => f.write_str("critical"),
            Self::High => f.write_str("high"),
            Self::Low => f.write_str("low"),
            Self::Lowest => f.write_str("lowest"),
            Self::Medium => f.write_str("medium"),
        }
    }
}

impl From<Primitive> for u8 {
    /// Return the rank, from 0 for critical to 4 for lowest.
    fn from(value: Primitive) -> Self {
        value as u8
    }
}

impl FromStr for Primitive {
    /// The message that names the text that is not a priority.
    type Err = String;

    /// Parse one of the five names in any case.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "critical" => Ok(Self::Critical),
            "high" => Ok(Self::High),
            "low" => Ok(Self::Low),
            "lowest" => Ok(Self::Lowest),
            "medium" => Ok(Self::Medium),
            other => Err(format!("invalid priority: {other}")),
        }
    }
}

impl TryFrom<u8> for Primitive {
    /// The message that names the number that is not a rank.
    type Error = String;

    /// Return the level of rank `value`, which must be from 0 through 4.
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Critical),
            1 => Ok(Self::High),
            2 => Ok(Self::Medium),
            3 => Ok(Self::Low),
            4 => Ok(Self::Lowest),
            other => Err(format!("invalid priority value: {other}")),
        }
    }
}
