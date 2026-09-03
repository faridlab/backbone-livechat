use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "livechat_step_type", rename_all = "snake_case")]
pub enum LivechatStepType {
    Text,
    QuestionSelection,
    QuestionEmail,
    QuestionPhone,
    ForwardOperator,
    FreeInputSingle,
    FreeInputMulti,
}

impl std::fmt::Display for LivechatStepType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::QuestionSelection => write!(f, "question_selection"),
            Self::QuestionEmail => write!(f, "question_email"),
            Self::QuestionPhone => write!(f, "question_phone"),
            Self::ForwardOperator => write!(f, "forward_operator"),
            Self::FreeInputSingle => write!(f, "free_input_single"),
            Self::FreeInputMulti => write!(f, "free_input_multi"),
        }
    }
}

impl FromStr for LivechatStepType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "question_selection" => Ok(Self::QuestionSelection),
            "question_email" => Ok(Self::QuestionEmail),
            "question_phone" => Ok(Self::QuestionPhone),
            "forward_operator" => Ok(Self::ForwardOperator),
            "free_input_single" => Ok(Self::FreeInputSingle),
            "free_input_multi" => Ok(Self::FreeInputMulti),
            _ => Err(format!("Unknown LivechatStepType variant: {}", s)),
        }
    }
}

impl Default for LivechatStepType {
    fn default() -> Self {
        Self::Text
    }
}
