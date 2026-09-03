use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "livechat_session_outcome", rename_all = "snake_case")]
pub enum LivechatSessionOutcome {
    NoFailure,
    NoAnswer,
    NoAgent,
    Escalated,
}

impl std::fmt::Display for LivechatSessionOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoFailure => write!(f, "no_failure"),
            Self::NoAnswer => write!(f, "no_answer"),
            Self::NoAgent => write!(f, "no_agent"),
            Self::Escalated => write!(f, "escalated"),
        }
    }
}

impl FromStr for LivechatSessionOutcome {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "no_failure" => Ok(Self::NoFailure),
            "no_answer" => Ok(Self::NoAnswer),
            "no_agent" => Ok(Self::NoAgent),
            "escalated" => Ok(Self::Escalated),
            _ => Err(format!("Unknown LivechatSessionOutcome variant: {}", s)),
        }
    }
}

impl Default for LivechatSessionOutcome {
    fn default() -> Self {
        Self::NoFailure
    }
}
