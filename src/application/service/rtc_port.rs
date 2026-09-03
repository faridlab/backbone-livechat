//! The realtime (RTC) carrier port (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! MOUNTED NOWHERE at this pin — no RTC route exists in the module.
//! The port exists to carry the JOIN-NOT-START LAW when a transport
//! lands: visitors may JOIN an existing session call, never start
//! one. The guard holds BY CONSTRUCTION — the trait exposes exactly
//! one method (`join_existing_call`); NO start verb exists anywhere
//! in the module, so there is no shadow controller to bypass and no
//! route to mount. When a transport composes, this is the seam it
//! implements (the host composes the adapter; a call id is minted by
//! the FIRST OPERATOR-side action, never by a visitor).

use async_trait::async_trait;
use uuid::Uuid;

use super::livechat_error::LivechatError;

/// The realtime seam: join an EXISTING call only.
#[async_trait]
pub trait LivechatRtcCarrier: Send + Sync {
    /// Attach `visitor_key` to the named existing call. There is no
    /// start arm — by construction, not by gating.
    async fn join_existing_call(
        &self,
        visitor_key: &str,
        call_id: Uuid,
    ) -> Result<(), LivechatError>;
}

/// The refusing default (the unmounted arm): a join attempt without
/// a composed carrier parks loudly.
pub struct RefusingRtcCarrier;

#[async_trait]
impl LivechatRtcCarrier for RefusingRtcCarrier {
    async fn join_existing_call(
        &self,
        _visitor_key: &str,
        _call_id: Uuid,
    ) -> Result<(), LivechatError> {
        Err(LivechatError::CarrierNotComposed)
    }
}
