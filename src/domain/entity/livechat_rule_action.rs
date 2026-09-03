use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "livechat_rule_action", rename_all = "snake_case")]
pub enum LivechatRuleAction {
    DisplayButton,
    DisplayButtonAndText,
    AutoPopup,
    HideButton,
}

impl std::fmt::Display for LivechatRuleAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DisplayButton => write!(f, "display_button"),
            Self::DisplayButtonAndText => write!(f, "display_button_and_text"),
            Self::AutoPopup => write!(f, "auto_popup"),
            Self::HideButton => write!(f, "hide_button"),
        }
    }
}

impl FromStr for LivechatRuleAction {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "display_button" => Ok(Self::DisplayButton),
            "display_button_and_text" => Ok(Self::DisplayButtonAndText),
            "auto_popup" => Ok(Self::AutoPopup),
            "hide_button" => Ok(Self::HideButton),
            _ => Err(format!("Unknown LivechatRuleAction variant: {}", s)),
        }
    }
}

impl Default for LivechatRuleAction {
    fn default() -> Self {
        Self::DisplayButton
    }
}
