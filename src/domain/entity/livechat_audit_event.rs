use serde::{Deserialize, Serialize};
use sqlx::Type;
use std::str::FromStr;
#[cfg(feature = "openapi")]
use utoipa::ToSchema;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[cfg_attr(feature = "openapi", derive(ToSchema))]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "livechat_audit_event", rename_all = "snake_case")]
pub enum LivechatAuditEvent {
    SessionOpened,
    OperatorAssigned,
    OperatorBusy,
    AssignmentEmpty,
    SessionClosed,
    SessionReopened,
    SessionRated,
    RatingRefused,
    ChatbotStepReached,
    ChatbotRestarted,
    ChatbotForwarded,
    HelpRequested,
    HelpResolved,
    InviteCreated,
    InviteDelivered,
    InviteCancelled,
    InviteAccepted,
    InviteExpired,
    VisitorRelinked,
    CarrierParked,
    CapabilityRefused,
    Throttled,
    TestSessionOpened,
}

impl std::fmt::Display for LivechatAuditEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SessionOpened => write!(f, "session_opened"),
            Self::OperatorAssigned => write!(f, "operator_assigned"),
            Self::OperatorBusy => write!(f, "operator_busy"),
            Self::AssignmentEmpty => write!(f, "assignment_empty"),
            Self::SessionClosed => write!(f, "session_closed"),
            Self::SessionReopened => write!(f, "session_reopened"),
            Self::SessionRated => write!(f, "session_rated"),
            Self::RatingRefused => write!(f, "rating_refused"),
            Self::ChatbotStepReached => write!(f, "chatbot_step_reached"),
            Self::ChatbotRestarted => write!(f, "chatbot_restarted"),
            Self::ChatbotForwarded => write!(f, "chatbot_forwarded"),
            Self::HelpRequested => write!(f, "help_requested"),
            Self::HelpResolved => write!(f, "help_resolved"),
            Self::InviteCreated => write!(f, "invite_created"),
            Self::InviteDelivered => write!(f, "invite_delivered"),
            Self::InviteCancelled => write!(f, "invite_cancelled"),
            Self::InviteAccepted => write!(f, "invite_accepted"),
            Self::InviteExpired => write!(f, "invite_expired"),
            Self::VisitorRelinked => write!(f, "visitor_relinked"),
            Self::CarrierParked => write!(f, "carrier_parked"),
            Self::CapabilityRefused => write!(f, "capability_refused"),
            Self::Throttled => write!(f, "throttled"),
            Self::TestSessionOpened => write!(f, "test_session_opened"),
        }
    }
}

impl FromStr for LivechatAuditEvent {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "session_opened" => Ok(Self::SessionOpened),
            "operator_assigned" => Ok(Self::OperatorAssigned),
            "operator_busy" => Ok(Self::OperatorBusy),
            "assignment_empty" => Ok(Self::AssignmentEmpty),
            "session_closed" => Ok(Self::SessionClosed),
            "session_reopened" => Ok(Self::SessionReopened),
            "session_rated" => Ok(Self::SessionRated),
            "rating_refused" => Ok(Self::RatingRefused),
            "chatbot_step_reached" => Ok(Self::ChatbotStepReached),
            "chatbot_restarted" => Ok(Self::ChatbotRestarted),
            "chatbot_forwarded" => Ok(Self::ChatbotForwarded),
            "help_requested" => Ok(Self::HelpRequested),
            "help_resolved" => Ok(Self::HelpResolved),
            "invite_created" => Ok(Self::InviteCreated),
            "invite_delivered" => Ok(Self::InviteDelivered),
            "invite_cancelled" => Ok(Self::InviteCancelled),
            "invite_accepted" => Ok(Self::InviteAccepted),
            "invite_expired" => Ok(Self::InviteExpired),
            "visitor_relinked" => Ok(Self::VisitorRelinked),
            "carrier_parked" => Ok(Self::CarrierParked),
            "capability_refused" => Ok(Self::CapabilityRefused),
            "throttled" => Ok(Self::Throttled),
            "test_session_opened" => Ok(Self::TestSessionOpened),
            _ => Err(format!("Unknown LivechatAuditEvent variant: {}", s)),
        }
    }
}

impl Default for LivechatAuditEvent {
    fn default() -> Self {
        Self::SessionOpened
    }
}
