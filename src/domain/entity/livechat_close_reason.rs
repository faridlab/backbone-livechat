use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "livechat_close_reason", rename_all = "snake_case")]
pub enum LivechatCloseReason {
    VisitorLeft,
    OperatorClosed,
    BotCompleted,
    Expired,
    Cancelled,
    RequestDeclined,
}

impl std::fmt::Display for LivechatCloseReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::VisitorLeft => write!(f, "visitor_left"),
            Self::OperatorClosed => write!(f, "operator_closed"),
            Self::BotCompleted => write!(f, "bot_completed"),
            Self::Expired => write!(f, "expired"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::RequestDeclined => write!(f, "request_declined"),
        }
    }
}

impl FromStr for LivechatCloseReason {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "visitor_left" => Ok(Self::VisitorLeft),
            "operator_closed" => Ok(Self::OperatorClosed),
            "bot_completed" => Ok(Self::BotCompleted),
            "expired" => Ok(Self::Expired),
            "cancelled" => Ok(Self::Cancelled),
            "request_declined" => Ok(Self::RequestDeclined),
            _ => Err(format!("Unknown LivechatCloseReason variant: {}", s)),
        }
    }
}

impl Default for LivechatCloseReason {
    fn default() -> Self {
        Self::VisitorLeft
    }
}
