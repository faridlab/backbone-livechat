use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "livechat_failure", rename_all = "snake_case")]
pub enum LivechatFailure {
    NoAnswer,
    NoAgent,
    NoFailure,
}

impl std::fmt::Display for LivechatFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoAnswer => write!(f, "no_answer"),
            Self::NoAgent => write!(f, "no_agent"),
            Self::NoFailure => write!(f, "no_failure"),
        }
    }
}

impl FromStr for LivechatFailure {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "no_answer" => Ok(Self::NoAnswer),
            "no_agent" => Ok(Self::NoAgent),
            "no_failure" => Ok(Self::NoFailure),
            _ => Err(format!("Unknown LivechatFailure variant: {}", s)),
        }
    }
}

impl Default for LivechatFailure {
    fn default() -> Self {
        Self::NoFailure
    }
}
