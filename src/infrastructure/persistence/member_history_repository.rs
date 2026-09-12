//! The member-history ledger repository (hand-written; user-owned;
//! see `metaphor.codegen.yaml`).
//!
//! The ledger is dual-duty: the per-participant reporting snapshot
//! AND the table the ladder's ongoing counts run against. Three DB
//! partial uniques (one per persona) + the strict persona
//! trichotomy — landed in the hardening migration — are the wall
//! this repository writes against: rejoins re-point through ON
//! CONFLICT upserts, never duplicate; persona is frozen at create
//! (never rewritten); a row without a persona identity is refused by
//! the CHECK, not by vigilance.
//!
//! The ledger is READ-ONLY over HTTP (system-only writes): these
//! methods are the system's only arms.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::application::service::livechat_error::LivechatError;
use super::relay_ambient_scope;

/// Table name for MemberHistory entities (the generated CRUD shape).
pub const MEMBER_HISTORY_TABLE_NAME: &str = "livechat.member_histories";

/// The generic CRUD repository over `livechat.member_histories` (the
/// generated wiring's type; kept here because this file is
/// user-owned — the generator skips it wholesale on regen, so the
/// generated service alias and lib wiring keep compiling). The verb
/// layer is [`MemberHistoryLedgerRepository`] below.
pub struct MemberHistoryRepository(
    backbone_orm::GenericCrudRepository<
        crate::domain::entity::MemberHistory,
        backbone_orm::SoftDelete,
    >,
);

impl std::ops::Deref for MemberHistoryRepository {
    type Target = backbone_orm::GenericCrudRepository<
        crate::domain::entity::MemberHistory,
        backbone_orm::SoftDelete,
    >;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl MemberHistoryRepository {
    /// Create a new CRUD repository instance.
    pub fn new(pool: PgPool) -> Self {
        Self(backbone_orm::GenericCrudRepository::new(
            pool,
            MEMBER_HISTORY_TABLE_NAME,
        ))
    }
}

backbone_core::impl_crud_repository!(
    MemberHistoryRepository,
    crate::domain::entity::MemberHistory,
    soft_delete
);

const HISTORY_COLUMNS: &str =
    "id, session_id, persona::text, operator_user_id, visitor_key, chatbot_script_id, \
     joined_at, left_at, message_count, response_time_secs, expertise_names";

/// One ledger row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct MemberHistoryRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub persona: String,
    pub operator_user_id: Option<Uuid>,
    pub visitor_key: Option<String>,
    pub chatbot_script_id: Option<Uuid>,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
    pub message_count: i32,
    pub response_time_secs: Option<i32>,
    pub expertise_names: Vec<String>,
}

// The typed multi-row read twins live only in the legacy `company_scope` module. Their
// connection discipline is what this repository needs — request-dedicated connection when
// the composing service bound one, plain pool otherwise. The helper's legacy task-local
// branch is never taken: this module sets no legacy scope of its own (ADR-0029).
pub struct MemberHistoryLedgerRepository {
    pool: PgPool,
}

impl MemberHistoryLedgerRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// The session's ledger rows (the reporting read; the escalation
    /// derive keys off the agent subset).
    pub async fn list_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Vec<MemberHistoryRow>, LivechatError> {
        let rows = backbone_orm::company_scope::fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, MemberHistoryRow>(&format!(
                "SELECT {HISTORY_COLUMNS} FROM livechat.member_histories \
                 WHERE session_id = $1 ORDER BY joined_at, id"
            ))
            .bind(session_id),
        )
        .await?;
        Ok(rows)
    }

    /// The bot row's lifecycle: upsert at script start (the bot
    /// joins), `left_at` at the forward handoff (the bot unfollows —
    /// never a delete).
    pub async fn upsert_bot_row(
        &self,
        session_id: Uuid,
        chatbot_script_id: Uuid,
    ) -> Result<(), LivechatError> {
        let mut tx = self.pool.begin().await?;
        relay_ambient_scope(&mut tx).await?;
        sqlx::query(
            r#"INSERT INTO livechat.member_histories
                   (session_id, persona, chatbot_script_id, expertise_names)
               VALUES ($1, 'bot', $2, '{}')
               ON CONFLICT (session_id, chatbot_script_id)
               WHERE persona = 'bot' AND chatbot_script_id IS NOT NULL
               DO UPDATE SET left_at = NULL, joined_at = now()"#,
        )
        .bind(session_id)
        .bind(chatbot_script_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// The forward handoff's bot unfollow (the bot row's `left_at`).
    pub async fn leave_bot_row(&self, session_id: Uuid) -> Result<(), LivechatError> {
        backbone_orm::org_scope::execute_scoped(
            &self.pool,
            sqlx::query(
                "UPDATE livechat.member_histories SET left_at = now() \
                 WHERE session_id = $1 AND persona = 'bot' AND left_at IS NULL",
            )
            .bind(session_id),
        )
        .await?;
        Ok(())
    }

    /// The operator of the visitor's PREVIOUS session on a channel
    /// (the stickiness arm's input) — the most recent closed or open
    /// session carrying this visitor with an assigned operator.
    pub async fn previous_operator_for_visitor(
        &self,
        channel_id: Uuid,
        visitor_key: &str,
    ) -> Result<Option<Uuid>, LivechatError> {
        let op = backbone_orm::company_scope::fetch_optional_scalar_scoped(
            &self.pool,
            sqlx::query_scalar::<_, Uuid>(
                r#"SELECT s.operator_user_id
                     FROM livechat.sessions s
                     JOIN livechat.member_histories h ON h.session_id = s.id
                    WHERE s.channel_id = $1
                      AND h.persona = 'visitor' AND h.visitor_key = $2
                      AND s.operator_user_id IS NOT NULL
                 ORDER BY s.last_interest_at DESC
                    LIMIT 1"#,
            )
            .bind(channel_id)
            .bind(visitor_key),
        )
        .await?;
        Ok(op)
    }
}
