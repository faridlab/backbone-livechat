//! The digest queue port (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! The operator digest (the closed-sessions-of-the-day summary)
//! enqueues through this seam. Blocking at the digest verb only —
//! the typed 503; every KPI read computes from the report view and
//! never needs the queue.

use async_trait::async_trait;
use uuid::Uuid;

use super::livechat_error::LivechatError;

/// One digest entry for one operator.
#[derive(Debug, Clone)]
pub struct DigestEntry {
    pub operator_user_id: Uuid,
    pub subject: String,
    pub body: String,
}

/// The digest seam.
#[async_trait]
pub trait LivechatDigestQueue: Send + Sync {
    async fn enqueue(&self, entry: &DigestEntry) -> Result<(), LivechatError>;
}

/// The refusing default: the queue is not composed (the unwired host
/// arm).
pub struct RefusingDigestQueue;

#[async_trait]
impl LivechatDigestQueue for RefusingDigestQueue {
    async fn enqueue(&self, _entry: &DigestEntry) -> Result<(), LivechatError> {
        Err(LivechatError::DigestNotComposed)
    }
}
