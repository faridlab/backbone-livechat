//! The rating repository (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! ONE rating per session by DB constraint (`UNIQUE(session_id)` —
//! the wall that replaces read-then-create; a repeat submit is the
//! typed 409, audited). The 1/5/10 scale is validated at the verb
//! AND carried by a CHECK. Attribution is bound to the LEDGER: an
//! agent rating names the session's operator (the ONE answer — the
//! operator path); a bot rating names the bot row's script.
//!
//! This file also carries the generated CRUD newtype
//! [`RatingRepository`] (the generic-repository shape the module's
//! generated wiring composes): the file is user-owned, so the
//! generator skips it wholesale — the newtype lives here so the
//! generated service alias and lib wiring keep compiling across
//! regens. The verb layer is [`RatingCommandRepository`] below.

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use backbone_orm::company_scope;

use crate::application::service::livechat_error::LivechatError;

use super::selection_repository::audit_tx;

/// Table name for Rating entities (the generated CRUD shape).
pub const RATING_TABLE_NAME: &str = "livechat.ratings";

/// The generic CRUD repository over `livechat.ratings` (the generated
/// wiring's type; kept here because this file is user-owned).
pub struct RatingRepository(
    backbone_orm::GenericCrudRepository<crate::domain::entity::Rating, backbone_orm::SoftDelete>,
);

impl std::ops::Deref for RatingRepository {
    type Target = backbone_orm::GenericCrudRepository<
        crate::domain::entity::Rating,
        backbone_orm::SoftDelete,
    >;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl RatingRepository {
    /// Create a new CRUD repository instance.
    pub fn new(pool: PgPool) -> Self {
        Self(backbone_orm::GenericCrudRepository::new(
            pool,
            RATING_TABLE_NAME,
        ))
    }
}

backbone_core::impl_crud_repository!(RatingRepository, crate::domain::entity::Rating, soft_delete);

/// One rating row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RatingRow {
    pub id: Uuid,
    pub session_id: Uuid,
    pub value: i32,
    pub rated_persona: String,
    pub operator_user_id: Option<Uuid>,
    pub chatbot_script_id: Option<Uuid>,
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
}

pub struct RatingCommandRepository {
    pool: PgPool,
}

impl RatingCommandRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Submit the session's rating. `rated_persona = 'agent'`
    /// attributes to the session's operator at rating time (the
    /// ledger's agent row); `'bot'` attributes to the bot row's
    /// script. The DB unique is the once-wall: a second submit is
    /// the typed 409 (audited `rating_refused`).
    pub async fn submit(
        &self,
        session_id: Uuid,
        value: i32,
        rated_persona: &str,
        comment: Option<&str>,
        actor: Option<Uuid>,
    ) -> Result<RatingRow, LivechatError> {
        if !matches!(value, 1 | 5 | 10) {
            audit_and_refuse(self, session_id, actor, "value outside the 1/5/10 scale").await?;
            return Err(LivechatError::Validation(
                "rating value must be one of 1, 5, 10".into(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        company_scope::bind_current_company(&mut tx).await?;
        // The session must exist and be fence-visible; a miss is the
        // uniform 404 family.
        let company: Option<(Uuid,)> =
            sqlx::query_as("SELECT company_id FROM livechat.sessions WHERE id = $1")
                .bind(session_id)
                .fetch_optional(&mut *tx)
                .await?;
        let Some((company_id,)) = company else {
            tx.rollback().await?;
            return Err(LivechatError::SessionNotFound);
        };
        let inserted: Option<RatingRow> = sqlx::query_as::<_, RatingRow>(
            r#"INSERT INTO livechat.ratings
                   (session_id, value, rated_persona, operator_user_id, chatbot_script_id,
                    comment, company_id)
               VALUES ($1, $2, $3::livechat_rated_persona,
                       (SELECT s.operator_user_id FROM livechat.sessions s WHERE s.id = $1),
                       (SELECT h.chatbot_script_id FROM livechat.member_histories h
                          WHERE h.session_id = $1 AND h.persona = 'bot' LIMIT 1),
                       $4, $5)
               ON CONFLICT (session_id) DO NOTHING
               RETURNING id, session_id, value, rated_persona::text, operator_user_id,
                         chatbot_script_id, comment, created_at"#,
        )
        .bind(session_id)
        .bind(value)
        .bind(rated_persona)
        .bind(comment)
        .bind(company_id)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(row) = inserted else {
            audit_tx(
                &mut tx,
                "rating_refused",
                actor,
                "session",
                session_id,
                serde_json::json!({ "reason": "already_submitted", "value": value }),
            )
            .await?;
            tx.commit().await?;
            return Err(LivechatError::RatingAlreadySubmitted);
        };
        audit_tx(
            &mut tx,
            "session_rated",
            actor,
            "session",
            session_id,
            serde_json::json!({
                "value": value,
                "rated_persona": rated_persona,
                "operator_user_id": row.operator_user_id,
                "chatbot_script_id": row.chatbot_script_id,
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(row)
    }

    /// The session's rating, if any (the public projection).
    pub async fn find_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Option<RatingRow>, LivechatError> {
        let row = backbone_orm::company_scope::fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, RatingRow>(
                "SELECT id, session_id, value, rated_persona::text, operator_user_id, \
                 chatbot_script_id, comment, created_at FROM livechat.ratings \
                 WHERE session_id = $1",
            )
            .bind(session_id),
        )
        .await?;
        Ok(row)
    }
}

async fn audit_and_refuse(
    repo: &RatingCommandRepository,
    session_id: Uuid,
    actor: Option<Uuid>,
    reason: &str,
) -> Result<(), LivechatError> {
    let mut tx = repo.pool.begin().await?;
    company_scope::bind_current_company(&mut tx).await?;
    audit_tx(
        &mut tx,
        "rating_refused",
        actor,
        "session",
        session_id,
        serde_json::json!({ "reason": reason }),
    )
    .await?;
    tx.commit().await?;
    Ok(())
}
