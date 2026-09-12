//! The website button answer (hand-written; user-owned; see
//! `metaphor.codegen.yaml`): the availability verb's decision.
//!
//! Host → website (the bridge port; a miss is the typed 404, no
//! fallback site) → the website's bound ACTIVE channel → the
//! two-pass rule match against the Referer (DISPLAY CONFIG ONLY —
//! the match never mutates anything) → the decision:
//!
//! - a matched rule with a ROUTABLE chatbot script ⇒ the bot arm
//!   (available 24/7 — the bot takes absolute priority over every
//!   human stage);
//! - ≥1 operator passing the SAME capacity-gate pool the assignment
//!   ladder runs (the ONE window, read-only) ⇒ the human arm;
//! - both ⇒ `both`; neither ⇒ unavailable.
//!
//! The visitor's PENDING invite surfaces once the operator's
//! first message landed, as a short-TTL `livechat-invite-accept`
//! capability minted under the caller's secret.

use std::sync::Arc;

use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use super::capability::mint_invite_capability;
use super::livechat_error::LivechatError;
use super::website_bridge::LivechatWebsiteBridge;
use crate::infrastructure::persistence::{
    ChatbotCommandRepository, SelectionRepository, WebsiteRequestRepository,
};

/// The availability answer (the button's entire decision surface).
#[derive(Debug, Clone, serde::Serialize)]
pub struct AvailabilityAnswer {
    pub available: bool,
    /// `operators | chatbot | both` (the mode the widget renders).
    pub mode: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub button_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub welcome_preview: Option<String>,
    /// The matched rule's display action (the widget's popup posture).
    pub rule_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_popup_timer: Option<i32>,
    /// The visitor's pending invite (visible once the operator's
    /// first message landed), with the short-TTL accept capability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_invite: Option<PendingInvite>,
    /// The routable script the open verb will bind (the bot-first
    /// routing input), when the bot arm is on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chatbot_script_id: Option<Uuid>,
}

/// The surfaced pending invite.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PendingInvite {
    pub session_id: Uuid,
    /// The `livechat-invite-accept` capability (15-minute TTL).
    pub accept_capability: String,
}

pub struct AvailabilityService {
    bridge: Arc<dyn LivechatWebsiteBridge>,
    selection: SelectionRepository,
    website_requests: WebsiteRequestRepository,
    chatbot: ChatbotCommandRepository,
}

impl AvailabilityService {
    pub fn new(pool: PgPool, bridge: Arc<dyn LivechatWebsiteBridge>) -> Self {
        Self {
            bridge,
            selection: SelectionRepository::new(pool.clone()),
            website_requests: WebsiteRequestRepository::new(pool.clone()),
            chatbot: ChatbotCommandRepository::new(pool),
        }
    }

    /// The bot-first routing input shared by the open verb: the
    /// matched rule's script when it is ROUTABLE (active, non-deleted,
    /// at least one step); an unroutable or unmatched script leaves
    /// the human path standing. Row scoping is owned by the composing
    /// service's tenancy decorator — every statement rides the ambient
    /// org scope it installs, public surfaces included (ADR-0029).
    pub async fn routed_script(
        &self,
        channel_id: Uuid,
        referer: Option<&str>,
    ) -> Result<Option<Uuid>, LivechatError> {
        let Some(rule) = self
            .website_requests
            .matched_rule(channel_id, referer)
            .await?
        else {
            return Ok(None);
        };
        match rule.chatbot_script_id {
            Some(script_id) if self.chatbot.script_is_routable(script_id).await? => {
                Ok(Some(script_id))
            }
            _ => Ok(None),
        }
    }

    /// THE BUTTON ANSWER. `secret` mints the invite-accept capability
    /// (an empty secret simply omits the invite arm — the open verb
    /// is the fail-closed surface for secrets, not this read).
    pub async fn answer(
        &self,
        host: &str,
        referer: Option<&str>,
        visitor_key: Option<&str>,
        secret: &str,
    ) -> Result<AvailabilityAnswer, LivechatError> {
        let binding = self.bridge.resolve_website_by_host(host).await?;
        self.answer_scoped(binding.website_id, referer, visitor_key, secret)
            .await
    }

    /// The body half of [`Self::answer`] (after the Host → website
    /// resolution). Row scoping is owned by the composing service's
    /// tenancy decorator, not by this module (ADR-0029).
    async fn answer_scoped(
        &self,
        website_id: Uuid,
        referer: Option<&str>,
        visitor_key: Option<&str>,
        secret: &str,
    ) -> Result<AvailabilityAnswer, LivechatError> {
        let channel = self
            .website_requests
            .active_channel_for_website(website_id)
            .await?
            .ok_or(LivechatError::ChannelNotFound)?;

        let rule = self
            .website_requests
            .matched_rule(channel.id, referer)
            .await?;
        let rule_action = rule
            .as_ref()
            .map(|r| r.action.clone())
            .unwrap_or_else(|| "display_button".to_string());
        let auto_popup_timer = rule.as_ref().map(|r| r.auto_popup_timer);

        // The bot arm.
        let chatbot_script_id = self.routed_script(channel.id, referer).await?;

        // The human arm: the SAME pool the ladder runs (the ONE
        // window, the buffer, the capacity gate), read-only.
        let operators = self.selection.eligible_operator_count(channel.id).await?;

        let available = chatbot_script_id.is_some() || operators > 0;
        let mode = match (chatbot_script_id.is_some(), operators > 0) {
            (true, true) => "both",
            (true, false) => "chatbot",
            (false, _) => "operators",
        };

        // The pending invite: visible once the operator's
        // first message landed (the `message_count > 0` gate).
        let mut pending_invite = None;
        if let Some(key) = visitor_key.filter(|_| !secret.is_empty()) {
            if let Some(pending) = self
                .website_requests
                .visible_pending_for_visitor(channel.id, key)
                .await?
            {
                if let Ok(token) = mint_invite_capability(secret, &pending.id, key, Utc::now()) {
                    pending_invite = Some(PendingInvite {
                        session_id: pending.id,
                        accept_capability: token,
                    });
                }
            }
        }

        Ok(AvailabilityAnswer {
            available,
            mode,
            button_text: channel.button_text.clone(),
            welcome_preview: channel.welcome_message.clone(),
            rule_action,
            auto_popup_timer,
            pending_invite,
            chatbot_script_id,
        })
    }
}
