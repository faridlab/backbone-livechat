//! Session verbs (hand-written; user-owned; see
//! `metaphor.codegen.yaml`) + the generated CRUD alias that keeps the
//! module's generated wiring compiling (the file is user-owned, so
//! the generator skips it wholesale — the alias lives on here).
//!
//! [`SessionCommandService`] is the verb layer: open, the message
//! verbs (the carrier-first posture), close (with the non-blocking
//! rating prompt), the serialized take, need-help, restart (the
//! carrier transcript cleanup parks loudly on refusal), the
//! operator-forced forward, tags, the transcript seam, presence, and
//! the list/get reads. Every write is the repository's — the service
//! composes ports and SQL-carrying repositories, holding no raw
//! sqlx itself (the DDD boundary).

use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use backbone_core::GenericCrudService;

use crate::domain::entity::Session;
use crate::infrastructure::persistence::SessionRepository;
use crate::presentation::dto::{CreateSessionDto, UpdateSessionDto};

/// Application service for Session entities (the generated CRUD
/// alias — the module wiring's type).
pub type SessionService =
    GenericCrudService<Session, CreateSessionDto, UpdateSessionDto, SessionRepository>;

use super::livechat_error::LivechatError;
use super::mail_port::{LivechatMailCarrier, MessageAuthor};
use super::notifier_port::{LivechatNotice, LivechatNotifier};
use super::transcript_port::{LivechatTranscriptMailer, TranscriptRequest};
use crate::infrastructure::persistence::{
    ChatbotCommandRepository, CloseOutcome, OpenSessionInput, SelectionRepository,
    SessionCommandRepository, SessionListFilter, SessionRow,
};

pub struct SessionCommandService {
    sessions: SessionCommandRepository,
    chatbot: ChatbotCommandRepository,
    selection: SelectionRepository,
    carrier: Arc<dyn LivechatMailCarrier>,
    notifier: Arc<dyn LivechatNotifier>,
    transcript: Arc<dyn LivechatTranscriptMailer>,
}

impl SessionCommandService {
    /// Compose with the host-installed ports (each has a refusing or
    /// unwired default so an uncomposed host gets typed failures,
    /// never silent skips).
    pub fn new(
        pool: PgPool,
        carrier: Arc<dyn LivechatMailCarrier>,
        notifier: Arc<dyn LivechatNotifier>,
        transcript: Arc<dyn LivechatTranscriptMailer>,
    ) -> Self {
        Self {
            sessions: SessionCommandRepository::new(pool.clone()),
            chatbot: ChatbotCommandRepository::new(pool.clone()),
            selection: SelectionRepository::new(pool),
            carrier,
            notifier,
            transcript,
        }
    }

    /// One row by id, company-fenced.
    pub async fn find(&self, session_id: Uuid) -> Result<Option<SessionRow>, LivechatError> {
        self.sessions.find(session_id).await
    }

    /// The admin list (filters: open / need_help / mine / channel;
    /// closed windows REQUIRE explicit date bounds).
    pub async fn list(&self, filter: &SessionListFilter) -> Result<Vec<SessionRow>, LivechatError> {
        self.sessions.list(filter).await
    }

    /// Open a session (the repository binds the fence; the caller
    /// binds the company scope of the resolved website — public — or
    /// carries the request scope — admin/test).
    pub async fn open(
        &self,
        input: &OpenSessionInput,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        self.sessions.open_session(input, actor).await
    }

    /// A visitor message: CARRIER-FIRST (the carrier is blocking for
    /// the human message verbs — a refusal answers the typed 503 and
    /// writes NOTHING), then the ONE chokepoint.
    pub async fn post_visitor_message(
        &self,
        session_id: Uuid,
        body: &str,
    ) -> Result<super::mail_port::CarrierMessage, LivechatError> {
        let carrier_id = self
            .carrier
            .post(session_id, &MessageAuthor::Visitor, body)
            .await?;
        self.sessions
            .apply_message(&post_message_command(
                session_id,
                MessageAuthor::Visitor,
                body,
                Some(carrier_id.clone()),
                None,
            ))
            .await?;
        Ok(super::mail_port::CarrierMessage {
            carrier_id,
            author: MessageAuthor::Visitor,
            body: body.to_string(),
            created_at: chrono::Utc::now(),
        })
    }

    /// An operator message: carrier-first, then the chokepoint. The
    /// first operator message on a pending invite also DELIVERS the
    /// invite (the has-message gate) — the route layer drives that
    /// audit through the website-request service.
    pub async fn post_operator_message(
        &self,
        session_id: Uuid,
        operator_user_id: Uuid,
        body: &str,
    ) -> Result<super::mail_port::CarrierMessage, LivechatError> {
        let author = MessageAuthor::Operator(operator_user_id);
        let carrier_id = self.carrier.post(session_id, &author, body).await?;
        self.sessions
            .apply_message(&post_message_command(
                session_id,
                author.clone(),
                body,
                Some(carrier_id.clone()),
                None,
            ))
            .await?;
        Ok(super::mail_port::CarrierMessage {
            carrier_id,
            author,
            body: body.to_string(),
            created_at: chrono::Utc::now(),
        })
    }

    /// The cursor poll (`?after=<carrier_message_id>`), ascending and
    /// bounded.
    pub async fn messages(
        &self,
        session_id: Uuid,
        after: Option<&str>,
        limit: i64,
    ) -> Result<Vec<super::mail_port::CarrierMessage>, LivechatError> {
        self.carrier.fetch(session_id, after, limit).await
    }

    /// Close a session (idempotent, audited). The rating PROMPT rides
    /// the notifier — NON-blocking: an unwired notifier answers
    /// `notified=false` and the close stands.
    pub async fn close(
        &self,
        session_id: Uuid,
        reason: &str,
        actor: Option<Uuid>,
    ) -> Result<CloseOutcome, LivechatError> {
        let outcome = self.sessions.close(session_id, reason, actor).await?;
        if let CloseOutcome::Closed(_) = &outcome {
            let _ = self
                .notifier
                .notify(&LivechatNotice::RatingPrompt { session_id })
                .await;
        }
        Ok(outcome)
    }

    /// The serialized first-wins take; the loser of the race answers
    /// the typed 409.
    pub async fn take(
        &self,
        session_id: Uuid,
        operator_user_id: Uuid,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        self.sessions
            .take(session_id, operator_user_id, actor)
            .await
    }

    /// The need-help verbs, audited both directions.
    pub async fn set_need_help(
        &self,
        session_id: Uuid,
        on: bool,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        self.sessions.set_need_help(session_id, on, actor).await
    }

    /// The operator-forced forward handoff: the ladder with
    /// stickiness off, the current operator counting as nobody; on
    /// nobody — `no_agent` and the session stands. (The
    /// chatbot-triggered forward — with the step's expertise stamp
    /// and the script continuation — lives in the chatbot service.)
    pub async fn forward(
        &self,
        session_id: Uuid,
        visitor_label: Option<&str>,
        actor: Option<Uuid>,
    ) -> Result<crate::infrastructure::persistence::AssignOutcome, LivechatError> {
        let session = self
            .sessions
            .find(session_id)
            .await?
            .ok_or(LivechatError::SessionNotFound)?;
        let outcome = self
            .selection
            .assign_forward(&crate::infrastructure::persistence::ForwardAssignInput {
                session_id,
                channel_id: session.channel_id,
                current_operator: session.operator_user_id,
                visitor_label: visitor_label.map(str::to_string),
                visitor_language: session.visitor_language.clone(),
                expertise: Vec::new(),
                visitor_country: session.visitor_country_code.clone(),
                stamp_expertise: Vec::new(),
                actor,
            })
            .await?;
        if matches!(
            outcome,
            crate::infrastructure::persistence::AssignOutcome::Empty
        ) {
            self.sessions.mark_no_agent(session_id).await?;
        }
        Ok(outcome)
    }

    /// The restart verb: the carrier transcript cleanup FIRST (a
    /// refusal parks on `sessions.error_detail` — the restart still
    /// proceeds; the parked step is retried on the next interaction),
    /// then the repository's reopen + pointer reset + log clear.
    pub async fn restart(
        &self,
        session_id: Uuid,
        reset_failure: bool,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        if let Err(e) = self.carrier.remove(session_id).await {
            self.sessions
                .park_carrier(
                    session_id,
                    &format!("transcript cleanup refused at restart: {}", e.code()),
                    actor,
                )
                .await?;
        }
        let first = match self.sessions.find(session_id).await? {
            None => return Err(LivechatError::SessionNotFound),
            Some(row) => {
                let script = match row.chatbot_current_step_id {
                    Some(step_id) => self
                        .chatbot
                        .step(step_id)
                        .await?
                        .map(|s| s.chatbot_script_id),
                    None => None,
                };
                match script {
                    Some(script_id) => self.chatbot.first_step(script_id).await?,
                    None => None,
                }
            }
        };
        self.sessions
            .restart(session_id, first.map(|s| s.id), reset_failure, actor)
            .await
    }

    /// Replace the session's conversation tags.
    pub async fn set_tags(&self, session_id: Uuid, tag_ids: &[Uuid]) -> Result<(), LivechatError> {
        self.sessions.set_tags(session_id, tag_ids).await
    }

    /// The transcript seam: the mailer port (refusing default = the
    /// typed 503 — loud, never a silent skip). The audit-event
    /// vocabulary carries no transcript arm; the delivery itself is
    /// the host mailer's record.
    pub async fn send_transcript(&self, request: &TranscriptRequest) -> Result<(), LivechatError> {
        self.transcript.send_transcript(request).await
    }

    /// The operator's presence heartbeat (presence is a heartbeat,
    /// never a session row).
    pub async fn heartbeat(&self, user_id: Uuid) -> Result<(), LivechatError> {
        self.sessions.heartbeat(user_id).await
    }

    /// The public session view's operator projection: a display name
    /// ONLY (never an id on the public surface).
    pub async fn operator_display_name(
        &self,
        operator_user_id: Uuid,
    ) -> Result<Option<String>, LivechatError> {
        self.sessions.operator_display_name(operator_user_id).await
    }

    /// The visitor's open session on a channel (the resume check).
    pub async fn find_open_by_visitor(
        &self,
        channel_id: Uuid,
        visitor_key: &str,
    ) -> Result<Option<SessionRow>, LivechatError> {
        self.sessions
            .find_open_by_visitor(channel_id, visitor_key)
            .await
    }
}

/// The chokepoint command factory (one shape, three authors).
fn post_message_command(
    session_id: Uuid,
    author: MessageAuthor,
    body: &str,
    carrier_message_id: Option<String>,
    chatbot: Option<crate::infrastructure::persistence::ChatbotMessageAppend>,
) -> crate::infrastructure::persistence::PostMessageCommand {
    crate::infrastructure::persistence::PostMessageCommand {
        session_id,
        author,
        body: body.to_string(),
        carrier_message_id,
        chatbot,
    }
}
