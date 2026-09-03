//! The rating verb (hand-written; user-owned; see
//! `metaphor.codegen.yaml`) + the generated CRUD alias that keeps the
//! module's generated wiring compiling (the file is user-owned, so
//! the generator skips it wholesale — the alias lives on here).
//!
//! ONE rating per session — the DB `UNIQUE(session_id)` is the wall
//! (a repeat submit is the typed 409, audited `rating_refused`);
//! the 1/5/10 scale is validated at the verb AND carried by a CHECK;
//! attribution is the LEDGER's: an agent rating names the session's
//! operator (the operator path), a bot rating names the bot row's
//! script.

use std::sync::Arc;

use sqlx::PgPool;

use backbone_core::GenericCrudService;

use crate::domain::entity::Rating;
use crate::infrastructure::persistence::RatingRepository;
use crate::presentation::dto::{CreateRatingDto, UpdateRatingDto};

/// Application service for Rating entities (the generated CRUD
/// alias — the module wiring's type).
pub type RatingService =
    GenericCrudService<Rating, CreateRatingDto, UpdateRatingDto, RatingRepository>;

use super::livechat_error::LivechatError;
use super::notifier_port::LivechatNotifier;
use crate::infrastructure::persistence::{RatingCommandRepository, RatingRow};
use uuid::Uuid;

/// The public rating verb's service.
pub struct RatingSubmitService {
    ratings: RatingCommandRepository,
    #[allow(dead_code)]
    notifier: Arc<dyn LivechatNotifier>,
}

impl RatingSubmitService {
    pub fn new(pool: PgPool, notifier: Arc<dyn LivechatNotifier>) -> Self {
        Self {
            ratings: RatingCommandRepository::new(pool),
            notifier,
        }
    }

    /// Submit the session's rating (validated, once-only, audited).
    pub async fn submit(
        &self,
        session_id: Uuid,
        value: i32,
        rated_persona: &str,
        comment: Option<&str>,
        actor: Option<Uuid>,
    ) -> Result<RatingRow, LivechatError> {
        if !matches!(rated_persona, "agent" | "bot") {
            return Err(LivechatError::Validation(
                "rated_persona must be 'agent' or 'bot'".into(),
            ));
        }
        self.ratings
            .submit(session_id, value, rated_persona, comment, actor)
            .await
    }

    /// The session's rating, if any (the public projection).
    pub async fn find_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Option<RatingRow>, LivechatError> {
        self.ratings.find_for_session(session_id).await
    }
}
