//! The website-chat-request repository (hand-written; user-owned;
//! see `metaphor.codegen.yaml`): the operator-initiated invite
//! lifecycle over the website bridge columns.
//!
//! The invite IS a pending session (`is_pending_request` with the
//! visitor's own country/timezone frozen on). SINGLE-VISITOR BY
//! DESIGN: each row binds its own visitor and its own operator
//! ledger row — a batch is repeated audited calls, so the upstream
//! loop leakage cannot reappear. The pending invite is INVISIBLE to
//! the visitor until the operator's first message (the `has_message`
//! gate = `message_count > 0`); it then surfaces in the availability
//! answer with a short-TTL accept capability. No untraced destroy:
//! cancel/decline/expiry clear the flag with audits; rows survive.

use sqlx::PgPool;
use uuid::Uuid;

// The typed multi-row read twins live only in the legacy `company_scope` module. Their
// connection discipline is what this repository needs — request-dedicated connection when
// the composing service bound one, plain pool otherwise. The helper's legacy task-local
// branch is never taken: this module sets no legacy scope of its own (ADR-0029).
use backbone_orm::company_scope::fetch_optional_scoped;

use crate::application::service::livechat_error::LivechatError;

use super::selection_repository::audit_tx;
use super::session_command_repository::SessionRow;
use super::relay_ambient_scope;

/// The website's bound channel, projected for the availability
/// answer and the invite verbs.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ChannelSummary {
    pub id: Uuid,
    pub name: String,
    pub button_text: Option<String>,
    pub welcome_message: Option<String>,
}

/// The channel rule a Referer matched (display config ONLY — the
/// match never mutates anything).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RuleMatch {
    pub action: String,
    pub auto_popup_timer: i32,
    pub chatbot_script_id: Option<Uuid>,
    pub chatbot_enabled_condition: String,
}

pub struct WebsiteRequestRepository {
    pool: PgPool,
}

impl WebsiteRequestRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// The website's bound ACTIVE channel (one channel per website;
    /// a miss is the typed 404 family).
    pub async fn active_channel_for_website(
        &self,
        website_id: Uuid,
    ) -> Result<Option<ChannelSummary>, LivechatError> {
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, ChannelSummary>(
                r#"SELECT id, name, button_text, welcome_message
                     FROM livechat.channels
                    WHERE website_id = $1 AND is_active
                      AND metadata ->> 'deleted_at' IS NULL
                    ORDER BY name LIMIT 1"#,
            )
            .bind(website_id),
        )
        .await?;
        Ok(row)
    }

    /// The TWO-PASS rule match against the Referer (display config
    /// only): pass 1 — non-catch-all rules whose `regex_url` matches
    /// the Referer, `sequence` ascending; pass 2 — the `.*`
    /// catch-all rules. `None` when no rule matches (and when the
    /// Referer is absent, pass 1 cannot match).
    pub async fn matched_rule(
        &self,
        channel_id: Uuid,
        referer: Option<&str>,
    ) -> Result<Option<RuleMatch>, LivechatError> {
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, RuleMatch>(
                r#"SELECT action::text, auto_popup_timer, chatbot_script_id,
                          chatbot_enabled_condition::text
                     FROM livechat.channel_rules
                    WHERE channel_id = $1
                      AND metadata ->> 'deleted_at' IS NULL
                      AND (regex_url = '.*'
                           OR ($2::text IS NOT NULL AND $2 ~ regex_url))
                    ORDER BY (regex_url = '.*'), sequence,
                             (metadata ->> 'created_at') NULLS LAST
                    LIMIT 1"#,
            )
            .bind(channel_id)
            .bind(referer),
        )
        .await?;
        Ok(row)
    }

    /// The visitor's PENDING invite session on the channel (ANY
    /// pending — the invisible-until-delivered gate does not apply
    /// here; the visitor's own open cancels it either way).
    pub async fn pending_for_visitor(
        &self,
        channel_id: Uuid,
        visitor_key: &str,
    ) -> Result<Option<SessionRow>, LivechatError> {
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, SessionRow>(
                r#"SELECT id, channel_id, title, status::text, failure::text,
                          outcome::text, close_reason::text, closed_at, operator_user_id,
                          chatbot_current_step_id, expertise_names, website_visitor_id,
                          visitor_country_code, visitor_timezone, is_pending_request,
                          crm_lead_id, visitor_language, message_count, first_response_at,
                          last_interest_at,
                          last_visitor_message_at, last_operator_message_at, is_test,
                          error_detail
                     FROM livechat.sessions
                    WHERE channel_id = $1
                      AND is_pending_request
                      AND closed_at IS NULL
                      AND EXISTS (SELECT 1 FROM livechat.member_histories h
                                   WHERE h.session_id = sessions.id
                                     AND h.persona = 'visitor' AND h.visitor_key = $2)
                    ORDER BY last_interest_at DESC LIMIT 1"#,
            )
            .bind(channel_id)
            .bind(visitor_key),
        )
        .await?;
        Ok(row)
    }

    /// The visitor's PENDING invite session on the website's channel
    /// (the availability answer's `pending_invite` arm — only when
    /// the operator's first message landed, i.e. `message_count > 0`).
    pub async fn visible_pending_for_visitor(
        &self,
        channel_id: Uuid,
        visitor_key: &str,
    ) -> Result<Option<SessionRow>, LivechatError> {
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, SessionRow>(
                r#"SELECT id, channel_id, title, status::text, failure::text,
                          outcome::text, close_reason::text, closed_at, operator_user_id,
                          chatbot_current_step_id, expertise_names, website_visitor_id,
                          visitor_country_code, visitor_timezone, is_pending_request,
                          crm_lead_id, visitor_language, message_count, first_response_at,
                          last_interest_at,
                          last_visitor_message_at, last_operator_message_at, is_test,
                          error_detail
                     FROM livechat.sessions
                    WHERE channel_id = $1
                      AND is_pending_request
                      AND closed_at IS NULL
                      AND message_count > 0
                      AND EXISTS (SELECT 1 FROM livechat.member_histories h
                                   WHERE h.session_id = sessions.id
                                     AND h.persona = 'visitor' AND h.visitor_key = $2)
                    ORDER BY last_interest_at DESC LIMIT 1"#,
            )
            .bind(channel_id)
            .bind(visitor_key),
        )
        .await?;
        Ok(row)
    }

    /// Accept an invite (the `livechat-invite-accept` capability
    /// path): clear the pending flag, bind the visitor ledger row,
    /// audit `invite_accepted`.
    pub async fn accept(
        &self,
        session_id: Uuid,
        visitor_key: &str,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        let mut tx = self.pool.begin().await?;
        relay_ambient_scope(&mut tx).await?;
        let updated: Option<SessionRow> = sqlx::query_as::<_, SessionRow>(
            r#"UPDATE livechat.sessions
                  SET is_pending_request = FALSE, status = 'in_progress'
                WHERE id = $1 AND is_pending_request AND closed_at IS NULL
                RETURNING id, channel_id, title, status::text, failure::text,
                          outcome::text, close_reason::text, closed_at, operator_user_id,
                          chatbot_current_step_id, expertise_names, website_visitor_id,
                          visitor_country_code, visitor_timezone, is_pending_request,
                          crm_lead_id, visitor_language, message_count, first_response_at,
                          last_interest_at,
                          last_visitor_message_at, last_operator_message_at, is_test,
                          error_detail"#,
        )
        .bind(session_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = updated else {
            tx.rollback().await?;
            return Err(LivechatError::SessionNotFound);
        };
        sqlx::query(
            r#"INSERT INTO livechat.member_histories
                   (session_id, persona, visitor_key, expertise_names)
               VALUES ($1, 'visitor', $2, '{}')
               ON CONFLICT (session_id, visitor_key)
               WHERE persona = 'visitor' AND visitor_key IS NOT NULL
               DO UPDATE SET left_at = NULL, joined_at = now()"#,
        )
        .bind(session_id)
        .bind(visitor_key)
        .execute(&mut *tx)
        .await?;
        audit_tx(
            &mut tx,
            "invite_accepted",
            actor,
            "session",
            session_id,
            serde_json::json!({ "visitor_key": visitor_key }),
        )
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// Cancel an invite (both sides notified through the notifier
    /// port by the caller; audited here). `by_visitor` selects the
    /// audit reason; the session closes with the matching reason.
    pub async fn cancel(
        &self,
        session_id: Uuid,
        by_visitor: bool,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        let reason = if by_visitor {
            "cancelled"
        } else {
            "request_declined"
        };
        let mut tx = self.pool.begin().await?;
        relay_ambient_scope(&mut tx).await?;
        let updated: Option<SessionRow> = sqlx::query_as::<_, SessionRow>(
            r#"UPDATE livechat.sessions
                  SET is_pending_request = FALSE, closed_at = now(),
                      close_reason = $2::livechat_close_reason, status = NULL
                WHERE id = $1 AND is_pending_request AND closed_at IS NULL
                RETURNING id, channel_id, title, status::text, failure::text,
                          outcome::text, close_reason::text, closed_at, operator_user_id,
                          chatbot_current_step_id, expertise_names, website_visitor_id,
                          visitor_country_code, visitor_timezone, is_pending_request,
                          crm_lead_id, visitor_language, message_count, first_response_at,
                          last_interest_at,
                          last_visitor_message_at, last_operator_message_at, is_test,
                          error_detail"#,
        )
        .bind(session_id)
        .bind(reason)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = updated else {
            tx.rollback().await?;
            return Err(LivechatError::SessionNotFound);
        };
        audit_tx(
            &mut tx,
            "invite_cancelled",
            actor,
            "session",
            session_id,
            serde_json::json!({ "by_visitor": by_visitor, "close_reason": reason }),
        )
        .await?;
        audit_tx(
            &mut tx,
            "session_closed",
            actor,
            "session",
            session_id,
            serde_json::json!({ "close_reason": reason, "via": "invite_cancel" }),
        )
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// The operator's first message on a pending session delivers
    /// the invite (the has-message gate opens): audit
    /// `invite_delivered` once.
    pub async fn audit_delivered_if_pending(
        &self,
        session_id: Uuid,
        actor: Option<Uuid>,
    ) -> Result<bool, LivechatError> {
        let mut tx = self.pool.begin().await?;
        relay_ambient_scope(&mut tx).await?;
        // The gate opens ONCE, atomically: the claim stamps the
        // delivery marker under the row lock, so exactly one caller
        // ever audits `invite_delivered` — the pending flag itself
        // stays set until the visitor ACCEPTS (or the sweep expires
        // the invite); visibility of a delivered invite rides on the
        // message gate, not on the pending flag.
        let claimed = sqlx::query(
            r#"UPDATE livechat.sessions
                  SET metadata = jsonb_set(metadata, '{invite_delivered_at}', to_jsonb(now()))
                WHERE id = $1
                  AND is_pending_request
                  AND NOT (metadata ? 'invite_delivered_at')"#,
        )
        .bind(session_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if claimed == 0 {
            tx.rollback().await?;
            return Ok(false);
        }
        audit_tx(
            &mut tx,
            "invite_delivered",
            actor,
            "session",
            session_id,
            serde_json::json!({ "gate": "has_message" }),
        )
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    /// The merge-relink verb (the visitor→partner merge composed with
    /// the website engine): rebind `sessions.website_visitor_id` and
    /// the ledger visitor keys to the surviving visitor, audit
    /// `visitor_relinked`. Sessions survive cookie loss and merges.
    pub async fn relink_website_visitor(
        &self,
        from_visitor_id: Uuid,
        to_visitor_id: Uuid,
        to_visitor_key: &str,
        actor: Option<Uuid>,
    ) -> Result<u64, LivechatError> {
        let mut tx = self.pool.begin().await?;
        relay_ambient_scope(&mut tx).await?;
        let rebound = sqlx::query(
            "UPDATE livechat.sessions SET website_visitor_id = $2 WHERE website_visitor_id = $1",
        )
        .bind(from_visitor_id)
        .bind(to_visitor_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        sqlx::query(
            "UPDATE livechat.member_histories SET visitor_key = $1 \
             WHERE persona = 'visitor' AND visitor_key = ANY( \
                 SELECT DISTINCT h2.visitor_key FROM livechat.member_histories h2 \
                  WHERE h2.persona = 'visitor' AND h2.visitor_key IS NOT NULL \
                    AND EXISTS (SELECT 1 FROM livechat.sessions s \
                                 WHERE s.id = h2.session_id \
                                   AND s.website_visitor_id = $2))",
        )
        .bind(to_visitor_key)
        .bind(to_visitor_id)
        .execute(&mut *tx)
        .await?;
        if rebound > 0 {
            audit_tx(
                &mut tx,
                "visitor_relinked",
                actor,
                "visitor",
                to_visitor_id,
                serde_json::json!({
                    "from_website_visitor_id": from_visitor_id,
                    "to_website_visitor_id": to_visitor_id,
                    "sessions_rebound": rebound,
                }),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(rebound)
    }
}
