//! The chatbot command repository (hand-written; user-owned; see
//! `metaphor.codegen.yaml`): the static graph reads (script / steps
//! / answers / triggers), the FORWARD-ONLY router, and the pointer
//! writes.
//!
//! The whole runtime state is `sessions.chatbot_current_step_id`
//! (one logical pointer — no SQL FK, so step deletion is refused at
//! the verb rather than cascading) plus `chatbot_messages` rows (the
//! per-session execution log). Nothing else carries bot state.
//!
//! ROUTING (forward-only, deterministic):
//! walk the script's steps with `sequence >` the current step's, in
//! ascending order; the first step that either carries NO trigger
//! (the default flow — the no-triggering step wins) or carries a
//! trigger whose answer is among the selected answers (within-step
//! OR over its trigger rows) is the next step. No match = script
//! done. A trigger whose target's sequence is ≤ its answering
//! step's sequence is refused at SAVE time (a typed 422 — backwards
//! edges never enter the graph).

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::application::service::livechat_error::LivechatError;

const STEP_COLUMNS: &str =
    "id, chatbot_script_id, sequence, step_type::text, message, expertise_tag_ids";

/// One step of the static graph.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StepRow {
    pub id: Uuid,
    pub chatbot_script_id: Uuid,
    pub sequence: i32,
    pub step_type: String,
    pub message: Option<String>,
    pub expertise_tag_ids: Vec<Uuid>,
}

/// One answer of a question step.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AnswerRow {
    pub id: Uuid,
    pub question_step_id: Uuid,
    pub sequence: i32,
    pub label: String,
    pub redirect_url: Option<String>,
}

/// One execution-log row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ChatbotMessageRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub step_id: Option<Uuid>,
    pub carrier_message_id: Option<String>,
    pub selected_answer_id: Option<Uuid>,
    pub visitor_answer: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct ChatbotCommandRepository {
    pool: PgPool,
}

impl ChatbotCommandRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// The script's steps, ascending (the graph read; the welcome
    /// materialization walks the leading run off this).
    pub async fn steps_for_script(&self, script_id: Uuid) -> Result<Vec<StepRow>, LivechatError> {
        let rows = backbone_orm::company_scope::fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, StepRow>(&format!(
                "SELECT {STEP_COLUMNS} FROM livechat.chatbot_steps \
                 WHERE chatbot_script_id = $1 ORDER BY sequence, id"
            ))
            .bind(script_id),
        )
        .await?;
        Ok(rows)
    }

    /// One step (the pointer's read; the save-time trigger check).
    pub async fn step(&self, step_id: Uuid) -> Result<Option<StepRow>, LivechatError> {
        let row = backbone_orm::company_scope::fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, StepRow>(&format!(
                "SELECT {STEP_COLUMNS} FROM livechat.chatbot_steps WHERE id = $1"
            ))
            .bind(step_id),
        )
        .await?;
        Ok(row)
    }

    /// The script's first step (the restart reset; the lazy welcome
    /// preview).
    pub async fn first_step(&self, script_id: Uuid) -> Result<Option<StepRow>, LivechatError> {
        let row = backbone_orm::company_scope::fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, StepRow>(&format!(
                "SELECT {STEP_COLUMNS} FROM livechat.chatbot_steps \
                 WHERE chatbot_script_id = $1 ORDER BY sequence, id LIMIT 1"
            ))
            .bind(script_id),
        )
        .await?;
        Ok(row)
    }

    /// Whether the script is routable: active AND non-empty.
    pub async fn script_is_routable(&self, script_id: Uuid) -> Result<bool, LivechatError> {
        let n: i64 = backbone_orm::company_scope::fetch_one_scalar_scoped(
            &self.pool,
            sqlx::query_scalar::<_, i64>(
                r#"SELECT (SELECT count(*) FROM livechat.chatbot_scripts s
                      WHERE s.id = $1 AND s.is_active
                      AND NOT EXISTS (SELECT 1 FROM livechat.chatbot_scripts s2
                                       WHERE s2.id = $1
                                         AND s2.metadata->>'deleted_at' IS NOT NULL))
                     + (SELECT count(*) FROM livechat.chatbot_steps
                         WHERE chatbot_script_id = $1)"#,
            )
            .bind(script_id),
        )
        .await?;
        Ok(n >= 2)
    }

    /// A question step's declared options (the answer-validity wall).
    pub async fn answers_for_step(&self, step_id: Uuid) -> Result<Vec<AnswerRow>, LivechatError> {
        let rows = backbone_orm::company_scope::fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, AnswerRow>(
                "SELECT id, question_step_id, sequence, label, redirect_url \
                 FROM livechat.chatbot_answers WHERE question_step_id = $1 \
                 ORDER BY sequence, id",
            )
            .bind(step_id),
        )
        .await?;
        Ok(rows)
    }

    /// The FORWARD-ONLY router (deterministic; see the module doc).
    /// Returns `None` = script done.
    pub async fn fetch_next_step(
        &self,
        current_step_id: Uuid,
        selected_answer_ids: &[Uuid],
    ) -> Result<Option<StepRow>, LivechatError> {
        let row = backbone_orm::company_scope::fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, StepRow>(&format!(
                r#"SELECT {STEP_COLUMNS}
                     FROM livechat.chatbot_steps cs
                    WHERE cs.chatbot_script_id = (SELECT chatbot_script_id
                                                    FROM livechat.chatbot_steps
                                                   WHERE id = $1)
                      AND cs.sequence > (SELECT sequence FROM livechat.chatbot_steps WHERE id = $1)
                      AND (NOT EXISTS (SELECT 1 FROM livechat.chatbot_step_triggers t
                                        WHERE t.target_step_id = cs.id)
                           OR EXISTS (SELECT 1 FROM livechat.chatbot_step_triggers t2
                                       WHERE t2.target_step_id = cs.id
                                         AND t2.answer_id = ANY($2::uuid[])))
                 ORDER BY cs.sequence, cs.id
                    LIMIT 1"#
            ))
            .bind(current_step_id)
            .bind(selected_answer_ids),
        )
        .await?;
        Ok(row)
    }

    /// Advance the pointer (one column write; the caller audits).
    pub async fn set_pointer(
        &self,
        session_id: Uuid,
        step_id: Option<Uuid>,
    ) -> Result<(), LivechatError> {
        backbone_orm::org_scope::execute_scoped(
            &self.pool,
            sqlx::query("UPDATE livechat.sessions SET chatbot_current_step_id = $2 WHERE id = $1")
                .bind(session_id)
                .bind(step_id),
        )
        .await?;
        Ok(())
    }

    /// The session's execution log (the public projection + the
    /// transcript source).
    pub async fn messages_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Vec<ChatbotMessageRow>, LivechatError> {
        let rows = backbone_orm::company_scope::fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, ChatbotMessageRow>(
                "SELECT id, session_id, step_id, carrier_message_id, selected_answer_id, \
                 visitor_answer, created_at FROM livechat.chatbot_messages \
                 WHERE session_id = $1 ORDER BY created_at, id",
            )
            .bind(session_id),
        )
        .await?;
        Ok(rows)
    }

    /// The expertise LABELS for a forward step's tag ids (labels
    /// freeze onto the session at forward time — deterministic
    /// reporting, no translation mining).
    pub async fn expertise_labels(&self, tag_ids: &[Uuid]) -> Result<Vec<String>, LivechatError> {
        if tag_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = backbone_orm::company_scope::fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, (String,)>(
                "SELECT name FROM livechat.expertise_tags \
                 WHERE id = ANY($1::uuid[]) ORDER BY name",
            )
            .bind(tag_ids),
        )
        .await?;
        Ok(rows.into_iter().map(|(n,)| n).collect())
    }

    /// Tag the session with the frozen expertise names (the forward
    /// handoff's stamp).
    pub async fn stamp_session_expertise(
        &self,
        session_id: Uuid,
        expertise: &[String],
    ) -> Result<(), LivechatError> {
        backbone_orm::org_scope::execute_scoped(
            &self.pool,
            sqlx::query("UPDATE livechat.sessions SET expertise_names = $2::text[] WHERE id = $1")
                .bind(session_id)
                .bind(expertise),
        )
        .await?;
        Ok(())
    }

    /// The `chatbot_step_reached` audit row (each pointer advance).
    pub async fn record_step_reached(
        &self,
        session_id: Uuid,
        step_id: Uuid,
        actor: Option<Uuid>,
    ) -> Result<(), LivechatError> {
        super::selection_repository::record_audit(
            &self.pool,
            "chatbot_step_reached",
            actor,
            "session",
            session_id,
            serde_json::json!({ "step_id": step_id }),
        )
        .await;
        Ok(())
    }

    /// The `chatbot_forwarded` audit row (the forward handoff step).
    pub async fn record_forwarded(
        &self,
        session_id: Uuid,
        step_id: Uuid,
        actor: Option<Uuid>,
    ) -> Result<(), LivechatError> {
        super::selection_repository::record_audit(
            &self.pool,
            "chatbot_forwarded",
            actor,
            "session",
            session_id,
            serde_json::json!({ "step_id": step_id }),
        )
        .await;
        Ok(())
    }
}
