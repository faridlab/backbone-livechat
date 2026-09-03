//! The transcript mailer port (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! The admin transcript verb mails a session's transcript through
//! this seam. The refusing default parks loudly (the typed 503 at
//! the verb); the host composes the real mailer.

use async_trait::async_trait;
use uuid::Uuid;

use super::livechat_error::LivechatError;

/// One transcript request: the session whose transcript is mailed,
/// the optional explicit recipient (an operator-supplied email;
/// `None` = the session's visitor identity through the bridge), and
/// the acting officer for the audit row.
#[derive(Debug, Clone)]
pub struct TranscriptRequest {
    pub session_id: Uuid,
    pub email: Option<String>,
    pub actor: Option<Uuid>,
}

/// The transcript mail seam.
#[async_trait]
pub trait LivechatTranscriptMailer: Send + Sync {
    async fn send_transcript(&self, request: &TranscriptRequest) -> Result<(), LivechatError>;
}

/// The refusing default: the mailer is not composed (the unwired
/// host arm).
pub struct RefusingTranscriptMailer;

#[async_trait]
impl LivechatTranscriptMailer for RefusingTranscriptMailer {
    async fn send_transcript(&self, _request: &TranscriptRequest) -> Result<(), LivechatError> {
        Err(LivechatError::TranscriptNotComposed)
    }
}
