use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "livechat_chatbot_condition", rename_all = "snake_case")]
pub enum LivechatChatbotCondition {
    Always,
    OnlyIfNoOperator,
    OnlyIfOperator,
}

impl std::fmt::Display for LivechatChatbotCondition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Always => write!(f, "always"),
            Self::OnlyIfNoOperator => write!(f, "only_if_no_operator"),
            Self::OnlyIfOperator => write!(f, "only_if_operator"),
        }
    }
}

impl FromStr for LivechatChatbotCondition {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "always" => Ok(Self::Always),
            "only_if_no_operator" => Ok(Self::OnlyIfNoOperator),
            "only_if_operator" => Ok(Self::OnlyIfOperator),
            _ => Err(format!("Unknown LivechatChatbotCondition variant: {}", s)),
        }
    }
}

impl Default for LivechatChatbotCondition {
    fn default() -> Self {
        Self::Always
    }
}
