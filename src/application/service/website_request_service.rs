//! The operator-initiated website chat request lifecycle
//! (hand-written; user-owned; see `metaphor.codegen.yaml`) — the
//! website bridge's invite arm, inside this module.
//!
//! The invite IS a pending session (`is_pending_request`, the
//! visitor's own geo frozen on). SINGLE-VISITOR BY DESIGN:
//! the verb binds ONE visitor per call — each row carries its own
//! country/timezone and its own operator ledger row, so the upstream
//! batch-loop leakage cannot reappear (a batch is repeated audited
//! calls).
//!
//! Bounded lifecycle, both sides visible: the pending
//! invite is INVISIBLE to the visitor until the operator's first
//! message (the `has_message` gate = `message_count > 0`); a visitor
//! opening their own session CANCELS their pending invite ("visitor
//! wins", both sides notified through the non-blocking notifier,
//! audited); accept, decline, and expiry all clear the flag with
//! audits. No untraced destroy: rows survive every ending.

use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use super::livechat_error::LivechatError;
use super::notifier_port::{LivechatNotice, LivechatNotifier};
use super::website_bridge::LivechatWebsiteBridge;
use crate::infrastructure::persistence::{
    relay_ambient_scope, upsert_agent_ledger_tx, OpenSessionInput, SessionCommandRepository,
    SessionRow, WebsiteRequestRepository,
};

pub struct WebsiteRequestService {
    pool: PgPool,
    sessions: SessionCommandRepository,
    website_requests: WebsiteRequestRepository,
    notifier: Arc<dyn LivechatNotifier>,
    bridge: Arc<dyn LivechatWebsiteBridge>,
}

impl WebsiteRequestService {
    pub fn new(
        pool: PgPool,
        bridge: Arc<dyn LivechatWebsiteBridge>,
        notifier: Arc<dyn LivechatNotifier>,
    ) -> Self {
        Self {
            sessions: SessionCommandRepository::new(pool.clone()),
            website_requests: WebsiteRequestRepository::new(pool.clone()),
            notifier,
            bridge,
            pool,
        }
    }

    /// The website's bound ACTIVE channel (the open verb's target;
    /// a miss is the typed 404 — no fallback channel).
    pub async fn active_channel(
        &self,
        website_id: Uuid,
    ) -> Result<Option<crate::infrastructure::persistence::ChannelSummary>, LivechatError> {
        self.website_requests
            .active_channel_for_website(website_id)
            .await
    }

    /// CREATE the operator-initiated invite (one visitor per
    /// call): a pending session on the website's bound active
    /// channel, the VISITOR's own geo frozen through the bridge, the
    /// acting operator's own agent ledger row (the operator
    /// self-add), audited `invite_created`. Idempotent per visitor:
    /// an existing pending invite is returned untouched.
    pub async fn create_request(
        &self,
        website_id: Uuid,
        website_visitor_id: Uuid,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        let channel = self
            .website_requests
            .active_channel_for_website(website_id)
            .await?
            .ok_or(LivechatError::ChannelNotFound)?;

        // The visitor's own identity + geo (per-visitor binding; a
        // bridge miss opens the invite WITHOUT geo rather than
        // refusing — the invite itself is the durable fact).
        let visitor = self
            .bridge
            .visitor_by_id(website_id, website_visitor_id)
            .await
            .unwrap_or(None);

        // Idempotency: an existing pending invite for this visitor
        // stands (its ledger key binds the visitor).
        if let Some(v) = &visitor {
            if let Some(existing) = self
                .website_requests
                .pending_for_visitor(channel.id, &v.visitor_key)
                .await?
            {
                return Ok(existing);
            }
        }

        let input = OpenSessionInput {
            channel_id: channel.id,
            title: Some("Chat request".to_string()),
            visitor_key: visitor
                .as_ref()
                .map(|v| v.visitor_key.clone())
                // A visitor the bridge does not know still gets a
                // stable row-local key (the invite is operator-
                // initiated; the visitor binds at accept time).
                .unwrap_or_else(|| format!("website-visitor:{website_visitor_id}")),
            website_visitor_id: Some(website_visitor_id),
            visitor_country_code: visitor.as_ref().and_then(|v| v.country_code.clone()),
            visitor_timezone: visitor.as_ref().and_then(|v| v.timezone.clone()),
            visitor_language: None,
            chatbot_script_id: None,
            is_pending_request: true,
            is_test: false,
        };
        let row = self.sessions.open_session(&input, actor).await?;

        // The acting operator's own ledger row (the self-add is a
        // ledger row per session, not a channel-membership side
        // effect).
        if let Some(operator) = actor {
            let mut tx = self.pool.begin().await?;
            relay_ambient_scope(&mut tx).await?;
            upsert_agent_ledger_tx(&mut tx, row.id, operator).await?;
            tx.commit().await?;
        }

        crate::infrastructure::persistence::record_audit(
            &self.pool,
            "invite_created",
            actor,
            "session",
            row.id,
            serde_json::json!({ "channel_id": channel.id, "website_id": website_id }),
        )
        .await;
        Ok(row)
    }

    /// ACCEPT an invite (the `livechat-invite-accept` capability
    /// path — the route verifies the capability and passes the
    /// visitor key): clear the pending flag, bind the visitor ledger
    /// row, audit `invite_accepted`.
    pub async fn accept(
        &self,
        session_id: Uuid,
        visitor_key: &str,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        self.website_requests
            .accept(session_id, visitor_key, actor)
            .await
    }

    /// CANCEL an invite (operator decline, or the visitor's own open
    /// — `by_visitor`): close with the matching reason, audit
    /// `invite_cancelled` + the close, notify BOTH sides through the
    /// NON-blocking notifier (an unwired notifier answers
    /// `notified=false`; the cancel stands).
    pub async fn cancel(
        &self,
        session_id: Uuid,
        by_visitor: bool,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        let row = self
            .website_requests
            .cancel(session_id, by_visitor, actor)
            .await?;
        let _ = self
            .notifier
            .notify(&LivechatNotice::SessionCancelled {
                session_id,
                by_operator: !by_visitor,
            })
            .await;
        Ok(row)
    }

    /// The visitor's-own-open hook: a visitor opening their
    /// own session CANCELS their pending invite on that channel —
    /// "visitor wins", both sides notified, audited. Returns the
    /// cancelled invite when one stood.
    pub async fn cancel_pending_for_visitor(
        &self,
        channel_id: Uuid,
        visitor_key: &str,
        actor: Option<Uuid>,
    ) -> Result<Option<SessionRow>, LivechatError> {
        let Some(pending) = self
            .website_requests
            .pending_for_visitor(channel_id, visitor_key)
            .await?
        else {
            return Ok(None);
        };
        let row = self.cancel(pending.id, true, actor).await?;
        Ok(Some(row))
    }

    /// The operator's first message on a pending session DELIVERS the
    /// invite (the has-message gate opens): audit `invite_delivered`
    /// once. Returns whether the gate opened.
    pub async fn audit_delivered_if_pending(
        &self,
        session_id: Uuid,
        operator_user_id: Uuid,
    ) -> Result<bool, LivechatError> {
        self.website_requests
            .audit_delivered_if_pending(session_id, Some(operator_user_id))
            .await
    }

    /// The merge-relink verb: rebind
    /// `sessions.website_visitor_id` and the ledger's visitor keys to
    /// the surviving visitor, audit `visitor_relinked` — sessions
    /// survive cookie loss and the visitor→partner merge. The host
    /// composes website's `VisitorEngine::merge_visitor` with this.
    pub async fn relink_website_visitor(
        &self,
        from_visitor_id: Uuid,
        to_visitor_id: Uuid,
        to_visitor_key: &str,
        actor: Option<Uuid>,
    ) -> Result<u64, LivechatError> {
        self.website_requests
            .relink_website_visitor(from_visitor_id, to_visitor_id, to_visitor_key, actor)
            .await
    }

    /// The test verb's harvest fallback reads the visitor record
    /// through the bridge.
    pub async fn visitor_by_id(
        &self,
        website_id: Uuid,
        visitor_id: Uuid,
    ) -> Result<Option<super::website_bridge::VisitorIdentity>, LivechatError> {
        self.bridge.visitor_by_id(website_id, visitor_id).await
    }
}

