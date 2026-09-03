use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "livechat_max_sessions_mode", rename_all = "snake_case")]
pub enum LivechatMaxSessionsMode {
    Unlimited,
    Limited,
}

impl std::fmt::Display for LivechatMaxSessionsMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unlimited => write!(f, "unlimited"),
            Self::Limited => write!(f, "limited"),
        }
    }
}

impl FromStr for LivechatMaxSessionsMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "unlimited" => Ok(Self::Unlimited),
            "limited" => Ok(Self::Limited),
            _ => Err(format!("Unknown LivechatMaxSessionsMode variant: {}", s)),
        }
    }
}

impl Default for LivechatMaxSessionsMode {
    fn default() -> Self {
        Self::Unlimited
    }
}
