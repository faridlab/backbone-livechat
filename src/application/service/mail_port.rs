//! The mail carrier port (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! The session's messages live behind this port — the ported
//! mail/discuss hosting posture: message storage belongs to the
//! composed carrier (the host adapter over backbone-mail's
//! `message_post` + read-back), never to this crate. The refusing
//! default parks loudly: the message verbs answer the typed 503 and
//! write nothing; chatbot steps park on `sessions.error_detail` +
//! an audit row instead (retried by the sweep or the next visitor
//! interaction).
//!
//! The module's own `livechat.chatbot_messages` rows are the bot's
//! execution log, linked to the carrier's message id through the
//! partial unique on `carrier_message_id`.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::livechat_error::LivechatError;

/// Who authored a carrier message.
#[derive(Debug, Clone)]
pub enum MessageAuthor {
    Visitor,
    Operator(Uuid),
    Bot,
}

/// One carrier message, in ascending carrier order.
#[derive(Debug, Clone)]
pub struct CarrierMessage {
    pub carrier_id: String,
    pub author: MessageAuthor,
    pub body: String,
    pub created_at: DateTime<Utc>,
}

/// The message transport seam.
#[async_trait]
pub trait LivechatMailCarrier: Send + Sync {
    /// Post one message; returns the carrier's message id (the
    /// monotonic cursor clients poll with).
    async fn post(
        &self,
        session_id: Uuid,
        author: &MessageAuthor,
        body: &str,
    ) -> Result<String, LivechatError>;

    /// Fetch the session's messages AFTER the cursor (`None` = from
    /// the beginning), ascending, bounded.
    async fn fetch(
        &self,
        session_id: Uuid,
        after: Option<&str>,
        limit: i64,
    ) -> Result<Vec<CarrierMessage>, LivechatError>;

    /// Remove the session's messages (the restart verb's transcript
    /// cleanup — requested through the port; a refusal parks, never
    /// a silent skip). Returns the removed count.
    async fn remove(&self, session_id: Uuid) -> Result<u64, LivechatError>;
}

/// The refusing default: the carrier is not composed (the unwired
/// host arm).
pub struct RefusingMailCarrier;

#[async_trait]
impl LivechatMailCarrier for RefusingMailCarrier {
    async fn post(
        &self,
        _session_id: Uuid,
        _author: &MessageAuthor,
        _body: &str,
    ) -> Result<String, LivechatError> {
        Err(LivechatError::CarrierNotComposed)
    }

    async fn fetch(
        &self,
        _session_id: Uuid,
        _after: Option<&str>,
        _limit: i64,
    ) -> Result<Vec<CarrierMessage>, LivechatError> {
        Err(LivechatError::CarrierNotComposed)
    }

    async fn remove(&self, _session_id: Uuid) -> Result<u64, LivechatError> {
        Err(LivechatError::CarrierNotComposed)
    }
}
