//! The notifier port (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! The NON-blocking seam: cancel notices (an invite cancelled by the
//! visitor's own open — both sides learn) and the rating prompt at
//! close. The unwired default WARNs and answers `notified=false` —
//! the write is NEVER refused by the port (the website notifier
//! posture): a missing notice channel degrades the experience, not
//! the data.

use async_trait::async_trait;
use tracing::warn;
use uuid::Uuid;

use super::livechat_error::LivechatError;

/// The notice family.
#[derive(Debug, Clone)]
pub enum LivechatNotice {
    /// An operator-initiated invite was cancelled (the visitor opened
    /// their own session — visitor wins) or declined.
    SessionCancelled { session_id: Uuid, by_operator: bool },
    /// A session closed; the visitor is prompted to rate it.
    RatingPrompt { session_id: Uuid },
}

/// The notice seam. `Ok(true)` = delivered; `Ok(false)` = unwired
/// (the caller records `notified=false` and moves on).
#[async_trait]
pub trait LivechatNotifier: Send + Sync {
    async fn notify(&self, notice: &LivechatNotice) -> Result<bool, LivechatError>;
}

/// The unwired default: WARN + `notified=false`.
pub struct UnwiredNotifier;

#[async_trait]
impl LivechatNotifier for UnwiredNotifier {
    async fn notify(&self, notice: &LivechatNotice) -> Result<bool, LivechatError> {
        warn!(
            notice = ?notice,
            "livechat notifier not composed: notice dropped (notified=false, non-blocking)"
        );
        Ok(false)
    }
}
