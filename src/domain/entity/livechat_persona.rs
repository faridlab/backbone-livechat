use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "livechat_persona", rename_all = "snake_case")]
pub enum LivechatPersona {
    Agent,
    Visitor,
    Bot,
}

impl std::fmt::Display for LivechatPersona {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Agent => write!(f, "agent"),
            Self::Visitor => write!(f, "visitor"),
            Self::Bot => write!(f, "bot"),
        }
    }
}

impl FromStr for LivechatPersona {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "agent" => Ok(Self::Agent),
            "visitor" => Ok(Self::Visitor),
            "bot" => Ok(Self::Bot),
            _ => Err(format!("Unknown LivechatPersona variant: {}", s)),
        }
    }
}

impl Default for LivechatPersona {
    fn default() -> Self {
        Self::Agent
    }
}
