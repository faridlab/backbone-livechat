//! The CRM bridge repository (hand-written; user-owned; see
//! `metaphor.codegen.yaml`): the session-side SQL of the
//! conversation-becomes-a-lead seam — the lead-linked read (the
//! partial-index-served lookup), the chatbot contact harvest, the
//! first-wins link stamp, and the lead-granted agent join.
//!
//! RLS LAW: every transactional method binds the company scope
//! immediately after `begin()`; direct-pool reads go through the
//! `company_scope` `*_scoped` helpers. The link column's ONLY writer
//! is [`Self::link_lead`] — a conditional UPDATE whose `crm_lead_id
//! IS NULL` arm is the anti-fabrication wall (a concurrent or replayed
//! mint loses by row atomicity and surfaces as the typed 409, audited
//! as a refusal; no client surface ever supplies the lead id).

use sqlx::PgPool;
use uuid::Uuid;

use backbone_orm::company_scope;

use crate::application::service::livechat_error::LivechatError;

use super::selection_repository::{audit_tx, recompute_outcome_tx};
use super::session_command_repository::{SessionRow, SESSION_COLUMNS};

/// The chatbot-collected contact facts of one session (the earliest
/// answered email/phone step — NULL arms when the script never asked
/// or the visitor never answered).
#[derive(Debug, Clone, Default, sqlx::FromRow)]
pub struct HarvestedContact {
    pub email: Option<String>,
    pub phone: Option<String>,
}

pub struct CrmBridgeRepository {
    pool: PgPool,
}

impl CrmBridgeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// The session a lead was minted from (the lead-owner read grant's
    /// lookup). Served by the `session_crm_lead_uq` partial index —
    /// the donor's `has_crm_lead` partial-index translation: only
    /// linked rows exist in the index, exactly the read-grant domain.
    /// Company-fenced (a cross-company lead id reads as missing — the
    /// uniform 404 family).
    pub async fn find_by_lead_id(
        &self,
        lead_id: Uuid,
    ) -> Result<Option<SessionRow>, LivechatError> {
        let row = company_scope::fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, SessionRow>(&format!(
                "SELECT {SESSION_COLUMNS} FROM livechat.sessions WHERE crm_lead_id = $1"
            ))
            .bind(lead_id),
        )
        .await?;
        Ok(row)
    }

    /// The session's chatbot-collected contact facts: the EARLIEST
    /// answered `question_email` / `question_phone` steps (sanitized
    /// answers, stored verbatim by the pointer machine). Two
    /// first-match subqueries, one round trip, company-fenced.
    pub async fn harvest_contact(
        &self,
        session_id: Uuid,
    ) -> Result<HarvestedContact, LivechatError> {
        let row = company_scope::fetch_one_scoped(
            &self.pool,
            sqlx::query_as::<_, HarvestedContact>(
                r#"SELECT
                       (SELECT m.visitor_answer FROM livechat.chatbot_messages m
                          JOIN livechat.chatbot_steps s ON s.id = m.step_id
                         WHERE m.session_id = $1
                           AND s.step_type = 'question_email'
                           AND m.visitor_answer IS NOT NULL
                         ORDER BY m.created_at ASC LIMIT 1) AS email,
                       (SELECT m.visitor_answer FROM livechat.chatbot_messages m
                          JOIN livechat.chatbot_steps s ON s.id = m.step_id
                         WHERE m.session_id = $1
                           AND s.step_type = 'question_phone'
                           AND m.visitor_answer IS NOT NULL
                         ORDER BY m.created_at ASC LIMIT 1) AS phone"#,
            )
            .bind(session_id),
        )
        .await?;
        Ok(row)
    }

    /// Stamp the minted lead onto the session — FIRST-WINS by row
    /// atomicity: the `crm_lead_id IS NULL` arm means a concurrent or
    /// replayed mint updates ZERO rows and surfaces as the typed 409
    /// (audited as a refusal). This is the column's only writer; the
    /// partial UNIQUE behind it also pins one session per lead.
    pub async fn link_lead(
        &self,
        session_id: Uuid,
        lead_id: Uuid,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        let mut tx = self.pool.begin().await?;
        company_scope::bind_current_company(&mut tx).await?;
        let updated = sqlx::query_as::<_, SessionRow>(&format!(
            r#"UPDATE livechat.sessions SET crm_lead_id = $2
                WHERE id = $1 AND crm_lead_id IS NULL
                RETURNING {SESSION_COLUMNS}"#
        ))
        .bind(session_id)
        .bind(lead_id)
        .fetch_optional(&mut *tx)
        .await?;
        let row = match updated {
            Some(row) => row,
            None => {
                // Zero rows: either the session is not readable under
                // this scope (the uniform 404), or someone else's link
                // won the row (the typed 409, audited).
                let existing = sqlx::query_as::<_, SessionRow>(&format!(
                    "SELECT {SESSION_COLUMNS} FROM livechat.sessions WHERE id = $1"
                ))
                .bind(session_id)
                .fetch_optional(&mut *tx)
                .await?;
                match existing {
                    None => {
                        tx.commit().await?;
                        return Err(LivechatError::SessionNotFound);
                    }
                    Some(row) => {
                        audit_tx(
                            &mut tx,
                            "lead_link_refused",
                            actor,
                            "session",
                            session_id,
                            serde_json::json!({
                                "reason": "already_linked",
                                "existing_lead_id": row.crm_lead_id,
                            }),
                        )
                        .await?;
                        tx.commit().await?;
                        return Err(LivechatError::SessionAlreadyHasLead);
                    }
                }
            }
        };
        audit_tx(
            &mut tx,
            "lead_linked",
            actor,
            "session",
            session_id,
            serde_json::json!({ "lead_id": lead_id }),
        )
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// The lead-granted agent join (the donor's second read-grant
    /// rule — read+create on the members of a lead-linked channel —
    /// translated to the participation ledger): resolve the session
    /// by its lead, refuse closed conversations typed, then insert or
    /// rebind the joining user's agent ledger row and recompute the
    /// outcome derive (a second agent row escalates — the join IS a
    /// handoff participant, honestly counted).
    pub async fn join_agent_for_lead(
        &self,
        lead_id: Uuid,
        user_id: Uuid,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        let mut tx = self.pool.begin().await?;
        company_scope::bind_current_company(&mut tx).await?;
        let row = match sqlx::query_as::<_, SessionRow>(&format!(
            "SELECT {SESSION_COLUMNS} FROM livechat.sessions WHERE crm_lead_id = $1"
        ))
        .bind(lead_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            Some(row) => row,
            None => {
                tx.commit().await?;
                return Err(LivechatError::SessionNotFound);
            }
        };
        if row.closed_at.is_some() {
            audit_tx(
                &mut tx,
                "lead_session_join_refused",
                actor,
                "session",
                row.id,
                serde_json::json!({ "reason": "session_closed", "lead_id": lead_id }),
            )
            .await?;
            tx.commit().await?;
            return Err(LivechatError::Validation(
                "the conversation behind this lead is closed; the read verbs serve its history"
                    .into(),
            ));
        }
        super::upsert_agent_ledger_tx(&mut tx, row.id, user_id, row.company_id).await?;
        recompute_outcome_tx(&mut tx, row.id).await?;
        audit_tx(
            &mut tx,
            "lead_session_joined",
            actor,
            "session",
            row.id,
            serde_json::json!({ "lead_id": lead_id, "joined_user_id": user_id }),
        )
        .await?;
        tx.commit().await?;
        Ok(row)
    }
}
