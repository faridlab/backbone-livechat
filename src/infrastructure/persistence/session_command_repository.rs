//! The session command repository (hand-written; user-owned; see
//! `metaphor.codegen.yaml`): open/resume, the state verbs, the ONE
//! message chokepoint (its four duties, transactionally), close, and
//! the per-record outcome recompute entry points.
//!
//! RLS LAW: every transactional method binds the company scope
//! immediately after `begin()`; direct-pool reads go through the
//! `company_scope` `*_scoped` helpers.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use backbone_orm::company_scope;

use crate::application::service::livechat_error::LivechatError;
use crate::application::service::mail_port::MessageAuthor;

use super::selection_repository::{audit_tx, recompute_outcome_tx};

/// The session columns every read projects (one list, one order).
pub(crate) const SESSION_COLUMNS: &str = "id, channel_id, title, status::text, failure::text, \
     outcome::text, close_reason::text, closed_at, operator_user_id, chatbot_current_step_id, \
     expertise_names, website_visitor_id, visitor_country_code, visitor_timezone, \
     is_pending_request, crm_lead_id, visitor_language, message_count, first_response_at, \
     last_interest_at, last_visitor_message_at, last_operator_message_at, is_test, error_detail, \
     company_id";

/// A session row (enum columns projected as text).
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct SessionRow {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub title: Option<String>,
    pub status: Option<String>,
    pub failure: String,
    pub outcome: Option<String>,
    pub close_reason: Option<String>,
    pub closed_at: Option<DateTime<Utc>>,
    pub operator_user_id: Option<Uuid>,
    pub chatbot_current_step_id: Option<Uuid>,
    pub expertise_names: Vec<String>,
    pub website_visitor_id: Option<Uuid>,
    pub visitor_country_code: Option<String>,
    pub visitor_timezone: Option<String>,
    pub is_pending_request: bool,
    /// The CRM lead minted from this conversation (NULL = none yet);
    /// stamped only by the CRM bridge's first-wins link verb.
    pub crm_lead_id: Option<Uuid>,
    pub visitor_language: Option<String>,
    pub message_count: i32,
    pub first_response_at: Option<DateTime<Utc>>,
    pub last_interest_at: DateTime<Utc>,
    pub last_visitor_message_at: Option<DateTime<Utc>>,
    pub last_operator_message_at: Option<DateTime<Utc>>,
    pub is_test: bool,
    pub error_detail: Option<String>,
    pub company_id: Uuid,
}

/// The open verb's input.
#[derive(Debug, Clone)]
pub struct OpenSessionInput {
    pub channel_id: Uuid,
    pub title: Option<String>,
    /// The visitor's website digest (the ledger's visitor key).
    pub visitor_key: String,
    pub website_visitor_id: Option<Uuid>,
    pub visitor_country_code: Option<String>,
    pub visitor_timezone: Option<String>,
    pub visitor_language: Option<String>,
    /// The chatbot script the matched rule routed to (bot-first);
    /// `None` = the human path (failure born `no_answer`).
    pub chatbot_script_id: Option<Uuid>,
    /// The operator-initiated pending-request arm: the session
    /// is born `is_pending_request` with the visitor's geo.
    pub is_pending_request: bool,
    pub is_test: bool,
}

/// The message chokepoint's DB command.
#[derive(Debug, Clone)]
pub struct PostMessageCommand {
    pub session_id: Uuid,
    pub author: MessageAuthor,
    pub body: String,
    /// The carrier's message id (the poll cursor anchor), when the
    /// message went through the carrier.
    pub carrier_message_id: Option<String>,
    /// The chatbot execution-log append (bot messages and answered
    /// steps only).
    pub chatbot: Option<ChatbotMessageAppend>,
}

/// One `livechat.chatbot_messages` row to append inside the
/// chokepoint.
#[derive(Debug, Clone)]
pub struct ChatbotMessageAppend {
    pub step_id: Option<Uuid>,
    pub selected_answer_id: Option<Uuid>,
    pub visitor_answer: Option<String>,
}

/// What the chokepoint changed (the caller's response projection).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MessageDeltas {
    pub message_count: i32,
    pub status: Option<String>,
    pub first_response_at: Option<DateTime<Utc>>,
}

/// The close verb's outcome (idempotent).
#[derive(Debug, Clone)]
pub enum CloseOutcome {
    Closed(SessionRow),
    AlreadyClosed(SessionRow),
}

pub struct SessionCommandRepository {
    pool: PgPool,
}

impl SessionCommandRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Open a session: the row (born `waiting`; failure born
    /// `no_answer` on the human path / `no_failure` on the bot path),
    /// the visitor ledger row, and the audit row — one transaction.
    /// NO message rows are minted here (the welcome is a preview the
    /// widget renders; it materializes on the visitor's first
    /// interaction).
    pub async fn open_session(
        &self,
        input: &OpenSessionInput,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        let mut tx = self.pool.begin().await?;
        company_scope::bind_current_company(&mut tx).await?;
        // The company the fence bound: the public routes bind the
        // resolved website's company before calling in; the admin and
        // test verbs carry the request scope. An unbound scope is a
        // typed refusal — no row is minted (fail-closed).
        let company = company_scope::current_company()
            .ok_or_else(|| LivechatError::Database("no company scope bound at open".into()))?;
        let id = Uuid::new_v4();
        let row = sqlx::query_as::<_, SessionRow>(&format!(
            r#"INSERT INTO livechat.sessions
                   (id, channel_id, title, status, failure, expertise_names,
                    website_visitor_id, visitor_country_code, visitor_timezone,
                    is_pending_request, visitor_language, is_test, company_id)
               VALUES ($1, $2, $3, 'waiting',
                       (CASE WHEN $4::uuid IS NULL THEN 'no_answer' ELSE 'no_failure' END)::livechat_failure,
                       '{{}}', $5, $6, $7, $8, $9, $10, $11)
               RETURNING {SESSION_COLUMNS}"#
        ))
        .bind(id)
        .bind(input.channel_id)
        .bind(&input.title)
        .bind(input.chatbot_script_id)
        .bind(input.website_visitor_id)
        .bind(&input.visitor_country_code)
        .bind(&input.visitor_timezone)
        .bind(input.is_pending_request)
        .bind(&input.visitor_language)
        .bind(input.is_test)
        .bind(company)
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            r#"INSERT INTO livechat.member_histories
                   (session_id, persona, visitor_key, expertise_names, company_id)
               VALUES ($1, 'visitor', $2, '{}', $3)
               ON CONFLICT (session_id, visitor_key)
               WHERE persona = 'visitor' AND visitor_key IS NOT NULL
               DO UPDATE SET left_at = NULL, joined_at = now()"#,
        )
        .bind(row.id)
        .bind(&input.visitor_key)
        .bind(company)
        .execute(&mut *tx)
        .await?;
        audit_tx(
            &mut tx,
            if input.is_test {
                "test_session_opened"
            } else {
                "session_opened"
            },
            actor,
            "session",
            row.id,
            serde_json::json!({
                "channel_id": input.channel_id,
                "chatbot_script_id": input.chatbot_script_id,
                "is_pending_request": input.is_pending_request,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// One row by id, company-fenced (a cross-company id reads as
    /// missing — the uniform 404 family).
    pub async fn find(&self, session_id: Uuid) -> Result<Option<SessionRow>, LivechatError> {
        let row = backbone_orm::company_scope::fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, SessionRow>(&format!(
                "SELECT {SESSION_COLUMNS} FROM livechat.sessions WHERE id = $1"
            ))
            .bind(session_id),
        )
        .await?;
        Ok(row)
    }

    /// The visitor's OPEN session on a channel (the resume check at
    /// open; the pending-invite cancel check).
    pub async fn find_open_by_visitor(
        &self,
        channel_id: Uuid,
        visitor_key: &str,
    ) -> Result<Option<SessionRow>, LivechatError> {
        let row = backbone_orm::company_scope::fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, SessionRow>(&format!(
                r#"SELECT {SESSION_COLUMNS} FROM livechat.sessions
                    WHERE channel_id = $1
                      AND closed_at IS NULL
                      AND EXISTS (SELECT 1 FROM livechat.member_histories h
                                   WHERE h.session_id = sessions.id
                                     AND h.persona = 'visitor' AND h.visitor_key = $2)
                 ORDER BY last_interest_at DESC LIMIT 1"#
            ))
            .bind(channel_id)
            .bind(visitor_key),
        )
        .await?;
        Ok(row)
    }

    /// THE MESSAGE CHOKEPOINT, transactionally: every message —
    /// visitor, operator, and bot alike — lands through here.
    /// The four duties (the upstream hook duties, relocated):
    ///   1. the session's `message_count++` + the interest stamp
    ///      (+ the author's own last-message stamp);
    ///   2. `response_time_secs` written ONCE for the agent row
    ///      (join-to-first-response seconds, only while NULL);
    ///   3. an agent post clears `no_answer` (and stamps
    ///      `first_response_at` once); a visitor post flips
    ///      `waiting -> in_progress`;
    ///   4. the chatbot-message factory append (while present).
    /// The per-record outcome recompute rides the same transaction
    /// (a cleared failure changes the derive's inputs).
    pub async fn apply_message(
        &self,
        command: &PostMessageCommand,
    ) -> Result<MessageDeltas, LivechatError> {
        let mut tx = self.pool.begin().await?;
        company_scope::bind_current_company(&mut tx).await?;

        let deltas = apply_message_tx(&mut tx, command).await?;
        recompute_outcome_tx(&mut tx, command.session_id).await?;
        tx.commit().await?;
        Ok(deltas)
    }

    /// Close a session (idempotent): `closed_at` + the close reason +
    /// status cleared (ended ⇒ no status), the per-record outcome
    /// recompute, and the `session_closed` audit. A second close is
    /// a no-op returning the current state.
    pub async fn close(
        &self,
        session_id: Uuid,
        reason: &str,
        actor: Option<Uuid>,
    ) -> Result<CloseOutcome, LivechatError> {
        let mut tx = self.pool.begin().await?;
        company_scope::bind_current_company(&mut tx).await?;
        let updated: Option<SessionRow> = sqlx::query_as::<_, SessionRow>(&format!(
            r#"UPDATE livechat.sessions
                  SET closed_at = now(), close_reason = $2::livechat_close_reason, status = NULL
                WHERE id = $1 AND closed_at IS NULL
                RETURNING {SESSION_COLUMNS}"#
        ))
        .bind(session_id)
        .bind(reason)
        .fetch_optional(&mut *tx)
        .await?;
        let outcome = match updated {
            Some(row) => {
                recompute_outcome_tx(&mut tx, session_id).await?;
                audit_tx(
                    &mut tx,
                    "session_closed",
                    actor,
                    "session",
                    session_id,
                    serde_json::json!({ "close_reason": reason }),
                )
                .await?;
                let final_row = find_in_tx(&mut tx, session_id).await?;
                tx.commit().await?;
                CloseOutcome::Closed(final_row.unwrap_or(row))
            }
            None => {
                tx.rollback().await?;
                // Idempotent: return the current state untouched.
                let row = self
                    .find(session_id)
                    .await?
                    .ok_or(LivechatError::SessionNotFound)?;
                CloseOutcome::AlreadyClosed(row)
            }
        };
        Ok(outcome)
    }

    /// The serialized take: the conditional UPDATE is the
    /// serialization point (`operator_user_id IS NULL AND closed_at IS
    /// NULL`); the loser of the race gets the typed 409. The agent
    /// ledger row and the buffer stamp ride the same transaction.
    pub async fn take(
        &self,
        session_id: Uuid,
        operator_user_id: Uuid,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        let mut tx = self.pool.begin().await?;
        company_scope::bind_current_company(&mut tx).await?;
        let won: Option<SessionRow> = sqlx::query_as::<_, SessionRow>(&format!(
            r#"UPDATE livechat.sessions
                  SET operator_user_id = $2, status = 'in_progress'
                WHERE id = $1 AND operator_user_id IS NULL AND closed_at IS NULL
                RETURNING {SESSION_COLUMNS}"#
        ))
        .bind(session_id)
        .bind(operator_user_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = won else {
            audit_tx(
                &mut tx,
                "operator_busy",
                actor,
                "session",
                session_id,
                serde_json::json!({ "operator_user_id": operator_user_id }),
            )
            .await?;
            tx.commit().await?;
            return Err(LivechatError::OperatorBusy);
        };
        upsert_agent_ledger_tx(&mut tx, session_id, operator_user_id, row.company_id).await?;
        sqlx::query(
            "UPDATE livechat.operator_profiles SET last_assigned_at = now() \
             WHERE user_id = $1 AND company_id = $2",
        )
        .bind(operator_user_id)
        .bind(row.company_id)
        .execute(&mut *tx)
        .await?;
        recompute_outcome_tx(&mut tx, session_id).await?;
        audit_tx(
            &mut tx,
            "operator_assigned",
            actor,
            "session",
            session_id,
            serde_json::json!({
                "operator_user_id": operator_user_id,
                "rung": null,
                "candidates_considered": null,
                "previous_operator_considered": null,
                "buffer_applied": false,
                "via": "take",
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// The need-help verbs (explicit, audited both directions).
    pub async fn set_need_help(
        &self,
        session_id: Uuid,
        on: bool,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        let mut tx = self.pool.begin().await?;
        company_scope::bind_current_company(&mut tx).await?;
        let row: SessionRow = sqlx::query_as::<_, SessionRow>(&format!(
            r#"UPDATE livechat.sessions
                  SET status = CASE
                        WHEN $2 THEN 'need_help'
                        WHEN status = 'need_help' THEN 'in_progress'
                        ELSE status END
                WHERE id = $1 AND closed_at IS NULL
                RETURNING {SESSION_COLUMNS}"#
        ))
        .bind(session_id)
        .bind(on)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| map_zero_rows(e, LivechatError::SessionNotFound))?;
        audit_tx(
            &mut tx,
            if on {
                "help_requested"
            } else {
                "help_resolved"
            },
            actor,
            "session",
            session_id,
            serde_json::json!({ "on": on }),
        )
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// The restart verb's DB half: reopen (`closed_at = NULL`,
    /// status `waiting`, audited `session_reopened`), reset the
    /// pointer to the script's first step, clear the module's
    /// execution-log rows, and reset-or-preserve the failure
    /// EXPLICITLY (the flag rides the audit row either way). The
    /// carrier transcript cleanup is the service's port call; a
    /// carrier refusal parks on `error_detail` there, never here.
    pub async fn restart(
        &self,
        session_id: Uuid,
        first_step_id: Option<Uuid>,
        reset_failure: bool,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        let mut tx = self.pool.begin().await?;
        company_scope::bind_current_company(&mut tx).await?;
        let reopened: Option<SessionRow> = sqlx::query_as::<_, SessionRow>(&format!(
            r#"UPDATE livechat.sessions
                  SET closed_at = NULL, close_reason = NULL, status = 'waiting',
                      chatbot_current_step_id = $2,
                      failure = CASE WHEN $3 THEN 'no_answer' ELSE failure END,
                      outcome = NULL, error_detail = NULL
                WHERE id = $1
                RETURNING {SESSION_COLUMNS}"#
        ))
        .bind(session_id)
        .bind(first_step_id)
        .bind(reset_failure)
        .fetch_optional(&mut *tx)
        .await?;
        let row = reopened.ok_or(LivechatError::SessionNotFound)?;
        sqlx::query("DELETE FROM livechat.chatbot_messages WHERE session_id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
        audit_tx(
            &mut tx,
            "session_reopened",
            actor,
            "session",
            session_id,
            serde_json::json!({ "via": "chatbot_restart" }),
        )
        .await?;
        audit_tx(
            &mut tx,
            "chatbot_restarted",
            actor,
            "session",
            session_id,
            serde_json::json!({ "reset_failure": reset_failure }),
        )
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// Park a carrier refusal on the session row (the loud parking
    /// lot) + the `carrier_parked` audit row.
    pub async fn park_carrier(
        &self,
        session_id: Uuid,
        detail: &str,
        actor: Option<Uuid>,
    ) -> Result<(), LivechatError> {
        let mut tx = self.pool.begin().await?;
        company_scope::bind_current_company(&mut tx).await?;
        sqlx::query("UPDATE livechat.sessions SET error_detail = $2 WHERE id = $1")
            .bind(session_id)
            .bind(detail)
            .execute(&mut *tx)
            .await?;
        audit_tx(
            &mut tx,
            "carrier_parked",
            actor,
            "session",
            session_id,
            serde_json::json!({ "detail": detail }),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Clear the parking lot (the retried step landed).
    pub async fn clear_error_detail(&self, session_id: Uuid) -> Result<(), LivechatError> {
        backbone_orm::company_scope::execute_scoped(
            &self.pool,
            sqlx::query("UPDATE livechat.sessions SET error_detail = NULL WHERE id = $1")
                .bind(session_id),
        )
        .await?;
        Ok(())
    }

    /// Stamp a heartbeat on the operator's presence row.
    pub async fn heartbeat(&self, user_id: Uuid) -> Result<(), LivechatError> {
        let n = backbone_orm::company_scope::execute_scoped(
            &self.pool,
            sqlx::query(
                "UPDATE livechat.operator_profiles SET last_heartbeat_at = now() \
                 WHERE user_id = $1",
            )
            .bind(user_id),
        )
        .await?
        .rows_affected();
        if n == 0 {
            return Err(LivechatError::OperatorProfileNotFound);
        }
        Ok(())
    }

    /// An operator's display name (the public session view projects
    /// the operator as a display name ONLY — never an id, never an
    /// email).
    pub async fn operator_display_name(
        &self,
        operator_user_id: Uuid,
    ) -> Result<Option<String>, LivechatError> {
        let name = backbone_orm::company_scope::fetch_optional_scalar_scoped(
            &self.pool,
            sqlx::query_scalar::<_, String>(
                "SELECT display_name FROM livechat.operator_profiles WHERE user_id = $1",
            )
            .bind(operator_user_id),
        )
        .await?;
        Ok(name)
    }

    /// The nobody-found failure stamp of the forward handoff (the
    /// `assignment_empty` audit row is already committed by the
    /// selection write): `no_agent` + the per-record recompute.
    pub async fn mark_no_agent(&self, session_id: Uuid) -> Result<(), LivechatError> {
        let mut tx = self.pool.begin().await?;
        company_scope::bind_current_company(&mut tx).await?;
        sqlx::query(
            "UPDATE livechat.sessions SET failure = 'no_agent' \
             WHERE id = $1 AND closed_at IS NULL",
        )
        .bind(session_id)
        .execute(&mut *tx)
        .await?;
        recompute_outcome_tx(&mut tx, session_id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Replace the session's conversation tags (`tag_ids` must name
    /// the company's own non-deleted tags — a miss is the typed 422).
    /// The tag rows ARE the record; the audit-event vocabulary carries
    /// no tagging arm, so none is minted here.
    pub async fn set_tags(&self, session_id: Uuid, tag_ids: &[Uuid]) -> Result<(), LivechatError> {
        let mut tx = self.pool.begin().await?;
        company_scope::bind_current_company(&mut tx).await?;
        let session: Option<(Uuid,)> =
            sqlx::query_as("SELECT company_id FROM livechat.sessions WHERE id = $1")
                .bind(session_id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((company_id,)) = session else {
            tx.rollback().await?;
            return Err(LivechatError::SessionNotFound);
        };
        let known: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM livechat.conversation_tags \
             WHERE id = ANY($1::uuid[]) AND metadata ->> 'deleted_at' IS NULL",
        )
        .bind(tag_ids)
        .fetch_one(&mut *tx)
        .await?;
        if known != tag_ids.len() as i64 {
            tx.rollback().await?;
            return Err(LivechatError::Validation(
                "one or more tag ids are unknown in this company".into(),
            ));
        }
        sqlx::query("DELETE FROM livechat.session_tags WHERE session_id = $1")
            .bind(session_id)
            .execute(&mut *tx)
            .await?;
        if !tag_ids.is_empty() {
            sqlx::query(
                r#"INSERT INTO livechat.session_tags (session_id, tag_id, company_id)
                   SELECT $1, t.id, $2
                     FROM livechat.conversation_tags t
                    WHERE t.id = ANY($3::uuid[])"#,
            )
            .bind(session_id)
            .bind(company_id)
            .bind(tag_ids)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The admin list filter (open / need_help / mine / channel;
    /// closed windows REQUIRE explicit date bounds).
    pub async fn list(&self, filter: &SessionListFilter) -> Result<Vec<SessionRow>, LivechatError> {
        let rows = backbone_orm::company_scope::fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, SessionRow>(&format!(
                r#"SELECT {SESSION_COLUMNS} FROM livechat.sessions
                    WHERE ($1::bool OR closed_at IS NULL)
                      AND ($2::bool OR status IS DISTINCT FROM 'need_help')
                      AND ($3::uuid IS NULL OR operator_user_id = $3)
                      AND ($4::uuid IS NULL OR channel_id = $4)
                      AND ($5::timestamptz IS NULL OR closed_at >= $5)
                      AND ($6::timestamptz IS NULL OR closed_at < $6)
                    ORDER BY last_interest_at DESC
                    LIMIT 200"#
            ))
            .bind(!filter.open_only)
            .bind(!filter.need_help_only)
            .bind(filter.mine_operator)
            .bind(filter.channel_id)
            .bind(filter.closed_from)
            .bind(filter.closed_to),
        )
        .await?;
        Ok(rows)
    }
}

/// The admin list filter.
#[derive(Debug, Clone, Default)]
pub struct SessionListFilter {
    pub open_only: bool,
    pub need_help_only: bool,
    pub mine_operator: Option<Uuid>,
    pub channel_id: Option<Uuid>,
    pub closed_from: Option<DateTime<Utc>>,
    pub closed_to: Option<DateTime<Utc>>,
}

/// The chokepoint's transactional core (shared with the chatbot
/// service's step posting — ONE chokepoint, not two).
pub async fn apply_message_tx(
    tx: &mut sqlx::PgConnection,
    command: &PostMessageCommand,
) -> Result<MessageDeltas, LivechatError> {
    let (visitor_stamp, operator_stamp, visitor_flip, operator_clear) = match &command.author {
        MessageAuthor::Visitor => (true, false, true, false),
        MessageAuthor::Operator(_) => (false, true, true, true),
        MessageAuthor::Bot => (false, false, false, false),
    };
    let deltas = sqlx::query_as::<_, MessageDeltas>(
        r#"UPDATE livechat.sessions
              SET message_count = message_count + 1,
                  last_interest_at = now(),
                  last_visitor_message_at = CASE WHEN $2 THEN now()
                                                 ELSE last_visitor_message_at END,
                  last_operator_message_at = CASE WHEN $3 THEN now()
                                                  ELSE last_operator_message_at END,
                  status = CASE WHEN $4 AND status = 'waiting'
                                THEN 'in_progress' ELSE status END,
                  failure = CASE WHEN $5 AND failure = 'no_answer'
                                 THEN 'no_failure' ELSE failure END,
                  first_response_at = CASE WHEN $3 AND first_response_at IS NULL
                                            THEN now() ELSE first_response_at END
            WHERE id = $1
            RETURNING message_count, status::text, first_response_at"#,
    )
    .bind(command.session_id)
    .bind(visitor_stamp)
    .bind(operator_stamp)
    .bind(visitor_flip || operator_stamp)
    .bind(operator_clear)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(LivechatError::SessionNotFound)?;

    // The author's ledger row (upsert + the once-only response time).
    match &command.author {
        MessageAuthor::Visitor => {
            let visitor_key = sqlx::query_scalar::<_, String>(
                "SELECT visitor_key FROM livechat.member_histories \
                 WHERE session_id = $1 AND persona = 'visitor' LIMIT 1",
            )
            .bind(command.session_id)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(key) = visitor_key {
                sqlx::query(
                    r#"UPDATE livechat.member_histories
                          SET message_count = message_count + 1, left_at = NULL
                        WHERE session_id = $1 AND persona = 'visitor' AND visitor_key = $2"#,
                )
                .bind(command.session_id)
                .bind(&key)
                .execute(&mut *tx)
                .await?;
            }
        }
        MessageAuthor::Operator(operator_user_id) => {
            let company = sqlx::query_scalar::<_, Uuid>(
                "SELECT company_id FROM livechat.sessions WHERE id = $1",
            )
            .bind(command.session_id)
            .fetch_one(&mut *tx)
            .await?;
            upsert_agent_ledger_tx(tx, command.session_id, *operator_user_id, company).await?;
            // The once-only first response seconds (join → first
            // response), written only while NULL.
            sqlx::query(
                r#"UPDATE livechat.member_histories
                      SET message_count = message_count + 1,
                          response_time_secs = COALESCE(
                              response_time_secs,
                              EXTRACT(EPOCH FROM (now() - joined_at))::int)
                    WHERE session_id = $1 AND persona = 'agent'
                      AND operator_user_id = $2"#,
            )
            .bind(command.session_id)
            .bind(operator_user_id)
            .execute(&mut *tx)
            .await?;
        }
        MessageAuthor::Bot => {
            sqlx::query(
                r#"UPDATE livechat.member_histories
                      SET message_count = message_count + 1, left_at = NULL
                    WHERE session_id = $1 AND persona = 'bot'"#,
            )
            .bind(command.session_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    // The chatbot execution-log append, when present.
    if let Some(append) = &command.chatbot {
        let company =
            sqlx::query_scalar::<_, Uuid>("SELECT company_id FROM livechat.sessions WHERE id = $1")
                .bind(command.session_id)
                .fetch_one(&mut *tx)
                .await?;
        sqlx::query(
            r#"INSERT INTO livechat.chatbot_messages
                   (session_id, step_id, carrier_message_id, selected_answer_id,
                    visitor_answer, company_id)
               VALUES ($1, $2, $3, $4, $5, $6)"#,
        )
        .bind(command.session_id)
        .bind(append.step_id)
        .bind(&command.carrier_message_id)
        .bind(append.selected_answer_id)
        .bind(&append.visitor_answer)
        .bind(company)
        .execute(&mut *tx)
        .await?;
    }
    Ok(deltas)
}

/// The agent ledger upsert (rejoin re-points, never duplicates).
pub async fn upsert_agent_ledger_tx(
    tx: &mut sqlx::PgConnection,
    session_id: Uuid,
    operator_user_id: Uuid,
    company_id: Uuid,
) -> Result<(), LivechatError> {
    sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, operator_user_id, expertise_names, company_id)
           VALUES ($1, 'agent', $2,
                   COALESCE((SELECT ARRAY(SELECT t.name
                                            FROM livechat.operator_expertise oe
                                            JOIN livechat.expertise_tags t
                                              ON t.id = oe.expertise_tag_id
                                           WHERE oe.operator_profile_id = p.id)
                      FROM livechat.operator_profiles p
                      WHERE p.user_id = $2 AND p.company_id = $3), '{}'),
                   $3)
           ON CONFLICT (session_id, operator_user_id)
           WHERE persona = 'agent' AND operator_user_id IS NOT NULL
           DO UPDATE SET left_at = NULL, joined_at = now()"#,
    )
    .bind(session_id)
    .bind(operator_user_id)
    .bind(company_id)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

async fn find_in_tx(
    tx: &mut sqlx::PgConnection,
    session_id: Uuid,
) -> Result<Option<SessionRow>, LivechatError> {
    let row = sqlx::query_as::<_, SessionRow>(&format!(
        "SELECT {SESSION_COLUMNS} FROM livechat.sessions WHERE id = $1"
    ))
    .bind(session_id)
    .fetch_optional(&mut *tx)
    .await?;
    Ok(row)
}

fn map_zero_rows(e: sqlx::Error, fallback: LivechatError) -> LivechatError {
    match e {
        sqlx::Error::RowNotFound => fallback,
        other => LivechatError::Database(other.to_string()),
    }
}
