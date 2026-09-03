//! The module's ADMIN route surface (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! The module DOES NOT SELF-MOUNT and DOES NOT SELF-GATE: it
//! exports [`livechat_admin_routes`], a plain `axum::Router` the
//! host nests under the schema name BEHIND `company_auth`, with
//! `ModuleWriteGate::new(pool, "livechat")` as the INNERMOST
//! `route_layer` (the events/website pattern: write gate innermost,
//! company_auth outside). Every write verb resolves authority
//! through the host gate (`write:livechat` and above); the module
//! adds NO authority logic of its own.
//!
//! The acting OPERATOR id arrives through the [`LivechatActor`]
//! request extension (the host's company_auth bridge inserts it);
//! without it the write verbs refuse with the typed 403 rather than
//! run as an unidentified principal.
//!
//! Route table (exhaustive — the declared admin surface):
//! - GET/POST /admin/channels                     list / create
//! - GET/PATCH/DELETE /admin/channels/:id         read / typed patch / soft delete
//! - POST /admin/channels/:id/operators           add (gated)
//! - DELETE /admin/channels/:id/operators/:user_id remove (gated IDENTICALLY)
//! - POST /admin/channels/from-website            the wizard (binds EVERY channel)
//! - POST /admin/channels/leave-all               the operator-removal cascade
//! - GET/PUT /admin/operator-profiles/:user_id    self or officer profile
//! - POST /admin/operator-profiles/heartbeat      presence
//! - GET/POST /admin/conversation-tags + /:id     CRUD (canonical unique)
//! - GET/POST /admin/expertise-tags + /:id        CRUD (canonical unique)
//! - GET/POST /admin/channel-rules + /:id         CRUD (save-time regex law)
//! - GET/POST /admin/chatbot-scripts + /:id       CRUD (canonical unique)
//! - GET/POST /admin/chatbot-scripts/:id/steps    the steps (+answers) of a script
//! - DELETE /admin/chatbot-steps/:id              pointer-fenced delete
//! - GET/POST /admin/chatbot-steps/:id/answers    the answers of a question step
//! - DELETE /admin/chatbot-answers/:id
//! - GET/POST /admin/chatbot-triggers             forward-only at save
//! - DELETE /admin/chatbot-triggers/:id
//! - POST /admin/chatbot-scripts/:id/test-sessions  the test verb (a REAL
//!   is_test session driving the real surface)
//! - GET /admin/sessions                          list (filters; closed needs bounds)
//! - GET /admin/sessions/:id
//! - POST /admin/sessions/:id/take|close|need-help|resolve-need-help|forward
//! - GET/POST /admin/sessions/:id/messages        the operator side
//! - POST /admin/sessions/:id/chatbot-restart|tags|transcript|cancel-request
//! - POST /admin/website-chat-requests            the operator-initiated invite
//! - GET /admin/report/session-summary            the bounded report window

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::Extensions,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::application::service::chatbot_service::ChatbotService;
use crate::application::service::livechat_error::LivechatError;
use crate::application::service::mail_port::{LivechatMailCarrier, MessageAuthor};
use crate::application::service::notifier_port::LivechatNotifier;
use crate::application::service::rating_service::RatingSubmitService;
use crate::application::service::report_service::ReportService;
use crate::application::service::session_service::SessionCommandService;
use crate::application::service::transcript_port::{LivechatTranscriptMailer, TranscriptRequest};
use crate::application::service::website_request_service::WebsiteRequestService;
use crate::infrastructure::persistence::{
    AdminConfigRepository, AnswerInput, ChannelPatch, CloseOutcome, OpenSessionInput, RuleInput,
    SessionListFilter, StepInput,
};

/// The request extension carrying the acting operator id (the host's
/// company_auth bridge inserts it after authentication).
#[derive(Debug, Clone, Copy)]
pub struct LivechatActor(pub Uuid);

fn actor_of(extensions: &Extensions) -> Option<Uuid> {
    extensions
        .get::<LivechatActor>()
        .map(|LivechatActor(id)| *id)
}

/// The admin state — the hand services over one pool.
#[derive(Clone)]
pub struct LivechatAdminState {
    pub config: Arc<AdminConfigRepository>,
    pub sessions: Arc<SessionCommandService>,
    pub chatbot: Arc<ChatbotService>,
    pub ratings: Arc<RatingSubmitService>,
    pub reports: Arc<ReportService>,
    pub website_requests: Arc<WebsiteRequestService>,
}

impl LivechatAdminState {
    pub fn new(
        pool: sqlx::PgPool,
        carrier: Arc<dyn LivechatMailCarrier>,
        notifier: Arc<dyn LivechatNotifier>,
        transcript: Arc<dyn LivechatTranscriptMailer>,
    ) -> Self {
        Self::with_bridge(
            pool,
            Arc::new(crate::application::service::website_bridge::RefusingLivechatWebsiteBridge),
            carrier,
            notifier,
            transcript,
        )
    }

    /// [`Self::new`] with the website bridge composed (the host
    /// installs the real adapter over backbone-website — the invite
    /// verbs read the visitor's geo through it).
    pub fn with_bridge(
        pool: sqlx::PgPool,
        bridge: Arc<dyn crate::application::service::website_bridge::LivechatWebsiteBridge>,
        carrier: Arc<dyn LivechatMailCarrier>,
        notifier: Arc<dyn LivechatNotifier>,
        transcript: Arc<dyn LivechatTranscriptMailer>,
    ) -> Self {
        Self {
            config: Arc::new(AdminConfigRepository::new(pool.clone())),
            sessions: Arc::new(SessionCommandService::new(
                pool.clone(),
                carrier.clone(),
                notifier.clone(),
                transcript,
            )),
            chatbot: Arc::new(ChatbotService::new(pool.clone(), carrier)),
            ratings: Arc::new(RatingSubmitService::new(pool.clone(), notifier.clone())),
            reports: Arc::new(ReportService::new(pool.clone())),
            website_requests: Arc::new(WebsiteRequestService::new(pool, bridge, notifier)),
        }
    }
}

/// The exported admin router — the table above, exhaustive. The host
/// nests it BEHIND `company_auth` with the module write gate as the
/// INNERMOST route_layer.
pub fn livechat_admin_routes(state: LivechatAdminState) -> Router {
    Router::new()
        // channels
        .route("/admin/channels", get(channels_list).post(channels_create))
        .route(
            "/admin/channels/:id",
            get(channels_get)
                .patch(channels_patch)
                .delete(channels_delete),
        )
        .route("/admin/channels/:id/operators", post(channel_operator_add))
        .route(
            "/admin/channels/:id/operators/:user_id",
            delete(channel_operator_remove),
        )
        .route("/admin/channels/from-website", post(channel_from_website))
        .route("/admin/channels/leave-all", post(channel_leave_all))
        // operator profiles
        .route(
            "/admin/operator-profiles/:user_id",
            get(profile_get).put(profile_put),
        )
        .route(
            "/admin/operator-profiles/heartbeat",
            post(profile_heartbeat),
        )
        // tags (conversation + expertise — one shape each)
        .route(
            "/admin/conversation-tags",
            get(conversation_tags_list).post(conversation_tags_create),
        )
        .route(
            "/admin/conversation-tags/:id",
            axum::routing::patch(conversation_tag_patch).delete(conversation_tag_delete),
        )
        .route(
            "/admin/expertise-tags",
            get(expertise_tags_list).post(expertise_tags_create),
        )
        .route(
            "/admin/expertise-tags/:id",
            axum::routing::patch(expertise_tag_patch).delete(expertise_tag_delete),
        )
        // channel rules
        .route("/admin/channel-rules", get(rules_list).post(rules_create))
        .route(
            "/admin/channel-rules/:id",
            get(rules_get).patch(rules_replace).delete(rules_delete),
        )
        // chatbot scripts
        .route(
            "/admin/chatbot-scripts",
            get(scripts_list).post(scripts_create),
        )
        .route(
            "/admin/chatbot-scripts/:id",
            get(scripts_get).patch(scripts_patch).delete(scripts_delete),
        )
        .route(
            "/admin/chatbot-scripts/:id/steps",
            get(steps_list).post(steps_create),
        )
        .route(
            "/admin/chatbot-scripts/:id/test-sessions",
            post(test_session_create),
        )
        // steps / answers / triggers
        .route("/admin/chatbot-steps/:id", delete(step_delete))
        .route(
            "/admin/chatbot-steps/:id/answers",
            get(answers_list).post(answers_create),
        )
        .route("/admin/chatbot-answers/:id", delete(answer_delete))
        .route(
            "/admin/chatbot-triggers",
            get(triggers_list).post(triggers_create),
        )
        .route("/admin/chatbot-triggers/:id", delete(trigger_delete))
        // sessions
        .route("/admin/sessions", get(sessions_list))
        .route("/admin/sessions/:id", get(sessions_get))
        .route("/admin/sessions/:id/take", post(session_take))
        .route("/admin/sessions/:id/close", post(session_close))
        .route("/admin/sessions/:id/need-help", post(session_need_help))
        .route(
            "/admin/sessions/:id/resolve-need-help",
            post(session_resolve_need_help),
        )
        .route("/admin/sessions/:id/forward", post(session_forward))
        .route(
            "/admin/sessions/:id/messages",
            get(session_messages).post(session_post_message),
        )
        .route(
            "/admin/sessions/:id/chatbot-restart",
            post(session_chatbot_restart),
        )
        .route("/admin/sessions/:id/tags", post(session_tags))
        .route("/admin/sessions/:id/transcript", post(session_transcript))
        .route(
            "/admin/sessions/:id/cancel-request",
            post(session_cancel_request),
        )
        // website chat requests + report
        .route("/admin/website-chat-requests", post(website_chat_request))
        .route("/admin/report/session-summary", get(report_session_summary))
        .with_state(state)
}

// ── Request bodies ─────────────────────────────────────────────────

#[derive(Deserialize)]
struct ChannelCreateBody {
    name: String,
    website_id: Option<Uuid>,
    button_text: Option<String>,
    welcome_message: Option<String>,
    #[serde(default = "default_mode")]
    max_sessions_mode: String,
    #[serde(default = "default_one")]
    max_sessions: i32,
    #[serde(default)]
    block_assignment_during_call: bool,
    review_link: Option<String>,
    #[serde(default = "default_true")]
    is_active: bool,
}
fn default_mode() -> String {
    "unlimited".into()
}
fn default_one() -> i32 {
    1
}
fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
struct ChannelPatchBody {
    name: Option<String>,
    button_text: Option<Option<String>>,
    welcome_message: Option<Option<String>>,
    max_sessions_mode: Option<String>,
    max_sessions: Option<i32>,
    block_assignment_during_call: Option<bool>,
    review_link: Option<Option<String>>,
    is_active: Option<bool>,
}

#[derive(Deserialize)]
struct FromWebsiteBody {
    website_id: Uuid,
    name: String,
    button_text: Option<String>,
    welcome_message: Option<String>,
}

#[derive(Deserialize)]
struct LeaveAllBody {
    user_id: Uuid,
}

#[derive(Deserialize)]
struct OperatorBody {
    user_id: Uuid,
}

#[derive(Deserialize)]
struct ProfilePutBody {
    display_name: Option<String>,
    #[serde(default)]
    languages: Vec<String>,
}

#[derive(Deserialize)]
struct TagCreateBody {
    name: String,
}

#[derive(Deserialize)]
struct TagPatchBody {
    name: String,
}

#[derive(Deserialize)]
struct RuleBody {
    channel_id: Uuid,
    regex_url: String,
    #[serde(default = "default_action")]
    action: String,
    #[serde(default)]
    auto_popup_timer: i32,
    chatbot_script_id: Option<Uuid>,
    #[serde(default = "default_condition")]
    chatbot_enabled_condition: String,
    #[serde(default)]
    country_codes: Vec<String>,
    #[serde(default = "default_one_i64")]
    sequence: i32,
}
fn default_action() -> String {
    "display_button".into()
}
fn default_condition() -> String {
    "always".into()
}
fn default_one_i64() -> i32 {
    1
}

#[derive(Deserialize)]
struct ScriptCreateBody {
    title: String,
}

#[derive(Deserialize)]
struct ScriptPatchBody {
    title: Option<String>,
    is_active: Option<bool>,
}

#[derive(Deserialize)]
struct StepBody {
    sequence: i32,
    step_type: String,
    message: Option<String>,
    #[serde(default)]
    expertise_tag_ids: Vec<Uuid>,
    #[serde(default)]
    answers: Vec<AnswerBody>,
}

#[derive(Deserialize)]
struct AnswerBody {
    #[serde(default)]
    sequence: i32,
    label: String,
    redirect_url: Option<String>,
}

#[derive(Deserialize)]
struct TriggerBody {
    answer_id: Uuid,
    target_step_id: Uuid,
}

#[derive(Deserialize)]
struct SessionsQuery {
    open: Option<bool>,
    need_help: Option<bool>,
    mine: Option<bool>,
    channel_id: Option<Uuid>,
    closed_from: Option<chrono::DateTime<chrono::Utc>>,
    closed_to: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Deserialize)]
struct TakeBody {
    #[serde(default)]
    operator_user_id: Option<Uuid>,
}

#[derive(Deserialize)]
struct CloseBody {
    reason: String,
}

#[derive(Deserialize)]
struct ForwardBody {
    visitor_label: Option<String>,
}

#[derive(Deserialize)]
struct MessageBody {
    body: String,
}

#[derive(Deserialize)]
struct MessagesQuery {
    after: Option<String>,
}

#[derive(Deserialize)]
struct RestartBody {
    #[serde(default)]
    reset_failure: bool,
}

#[derive(Deserialize)]
struct TagsBody {
    #[serde(default)]
    tag_ids: Vec<Uuid>,
}

#[derive(Deserialize)]
struct TranscriptBody {
    email: Option<String>,
}

#[derive(Deserialize)]
struct WebsiteChatRequestBody {
    website_id: Uuid,
    website_visitor_id: Uuid,
}

#[derive(Deserialize)]
struct ReportQuery {
    from: chrono::DateTime<chrono::Utc>,
    to: chrono::DateTime<chrono::Utc>,
    bucket: Option<String>,
    week_start: Option<i16>,
}

// ── Channels ───────────────────────────────────────────────────────

async fn channels_list(State(state): State<LivechatAdminState>) -> Response {
    answer(
        state
            .config
            .channel_list()
            .await
            .map(|rows| json!({ "channels": rows })),
    )
}

async fn channels_create(
    State(state): State<LivechatAdminState>,
    Json(body): Json<ChannelCreateBody>,
) -> Response {
    answer_created(
        state
            .config
            .channel_create(
                &body.name,
                body.website_id,
                body.button_text.as_deref(),
                body.welcome_message.as_deref(),
                &body.max_sessions_mode,
                body.max_sessions,
                body.block_assignment_during_call,
                body.review_link.as_deref(),
                body.is_active,
            )
            .await,
    )
}

async fn channels_get(State(state): State<LivechatAdminState>, Path(id): Path<Uuid>) -> Response {
    answer_opt(state.config.channel_get(id).await)
}

async fn channels_patch(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ChannelPatchBody>,
) -> Response {
    let patch = ChannelPatch {
        name: body.name,
        button_text: body.button_text,
        welcome_message: body.welcome_message,
        max_sessions_mode: body.max_sessions_mode,
        max_sessions: body.max_sessions,
        block_assignment_during_call: body.block_assignment_during_call,
        review_link: body.review_link,
        is_active: body.is_active,
    };
    answer_opt(state.config.channel_patch(id, &patch).await)
}

async fn channels_delete(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.config.channel_delete(id).await {
        Ok(true) => (axum::http::StatusCode::NO_CONTENT).into_response(),
        Ok(false) => LivechatError::ChannelNotFound.into_response(),
        Err(e) => e.into_response(),
    }
}

async fn channel_operator_add(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    extensions: Extensions,
    Json(body): Json<OperatorBody>,
) -> Response {
    if let Err(refused) = require_actor(&extensions) {
        return refused;
    }
    answer_created(state.config.channel_add_operator(id, body.user_id).await)
}

async fn channel_operator_remove(
    State(state): State<LivechatAdminState>,
    Path((id, user_id)): Path<(Uuid, Uuid)>,
    extensions: Extensions,
) -> Response {
    if let Err(refused) = require_actor(&extensions) {
        return refused;
    }
    match state.config.channel_remove_operator(id, user_id).await {
        Ok(true) => (axum::http::StatusCode::NO_CONTENT).into_response(),
        Ok(false) => LivechatError::ChannelNotFound.into_response(),
        Err(e) => e.into_response(),
    }
}

/// The from-website wizard: creates the channel ALREADY BOUND to the
/// website (every channel it creates is bound at birth) and creates
/// NO bot rule silently — the operator declares rules explicitly.
async fn channel_from_website(
    State(state): State<LivechatAdminState>,
    Json(body): Json<FromWebsiteBody>,
) -> Response {
    answer_created(
        state
            .config
            .channel_create(
                &body.name,
                Some(body.website_id),
                body.button_text.as_deref(),
                body.welcome_message.as_deref(),
                "unlimited",
                1,
                false,
                None,
                true,
            )
            .await,
    )
}

async fn channel_leave_all(
    State(state): State<LivechatAdminState>,
    Json(body): Json<LeaveAllBody>,
) -> Response {
    match state.config.channel_leave_all(body.user_id).await {
        Ok(n) => (
            axum::http::StatusCode::OK,
            Json(json!({ "channels_left": n })),
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}

// ── Operator profiles ──────────────────────────────────────────────

async fn profile_get(
    State(state): State<LivechatAdminState>,
    Path(user_id): Path<Uuid>,
) -> Response {
    answer_opt(state.config.profile_get(user_id).await)
}

async fn profile_put(
    State(state): State<LivechatAdminState>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<ProfilePutBody>,
) -> Response {
    answer(
        state
            .config
            .profile_put(user_id, body.display_name.as_deref(), &body.languages)
            .await,
    )
}

async fn profile_heartbeat(
    State(state): State<LivechatAdminState>,
    extensions: Extensions,
) -> Response {
    let Some(actor) = actor_of(&extensions) else {
        return LivechatError::ActorUnresolved.into_response();
    };
    match state.sessions.heartbeat(actor).await {
        Ok(()) => (axum::http::StatusCode::NO_CONTENT).into_response(),
        Err(e) => e.into_response(),
    }
}

// ── Tags ───────────────────────────────────────────────────────────

async fn conversation_tags_list(State(state): State<LivechatAdminState>) -> Response {
    answer(
        state
            .config
            .tag_list("conversation")
            .await
            .map(|r| json!({ "tags": r })),
    )
}

async fn conversation_tags_create(
    State(state): State<LivechatAdminState>,
    Json(body): Json<TagCreateBody>,
) -> Response {
    answer_created(state.config.tag_create("conversation", &body.name).await)
}

async fn conversation_tag_patch(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    Json(body): Json<TagPatchBody>,
) -> Response {
    answer_opt(
        state
            .config
            .tag_rename("conversation", id, &body.name)
            .await,
    )
}

async fn conversation_tag_delete(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
) -> Response {
    answer_bool(state.config.tag_delete("conversation", id).await)
}

async fn expertise_tags_list(State(state): State<LivechatAdminState>) -> Response {
    answer(
        state
            .config
            .tag_list("expertise")
            .await
            .map(|r| json!({ "tags": r })),
    )
}

async fn expertise_tags_create(
    State(state): State<LivechatAdminState>,
    Json(body): Json<TagCreateBody>,
) -> Response {
    answer_created(state.config.tag_create("expertise", &body.name).await)
}

async fn expertise_tag_patch(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    Json(body): Json<TagPatchBody>,
) -> Response {
    answer_opt(state.config.tag_rename("expertise", id, &body.name).await)
}

async fn expertise_tag_delete(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
) -> Response {
    answer_bool(state.config.tag_delete("expertise", id).await)
}

// ── Channel rules ──────────────────────────────────────────────────

async fn rules_list(
    State(state): State<LivechatAdminState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let channel_id = q.get("channel_id").and_then(|v| Uuid::parse_str(v).ok());
    answer(
        state
            .config
            .rule_list(channel_id)
            .await
            .map(|r| json!({ "rules": r })),
    )
}

async fn rules_create(
    State(state): State<LivechatAdminState>,
    Json(body): Json<RuleBody>,
) -> Response {
    let input = RuleInput {
        channel_id: body.channel_id,
        regex_url: body.regex_url,
        action: body.action,
        auto_popup_timer: body.auto_popup_timer,
        chatbot_script_id: body.chatbot_script_id,
        chatbot_enabled_condition: body.chatbot_enabled_condition,
        country_codes: body.country_codes,
        sequence: body.sequence,
    };
    answer_created(state.config.rule_create(&input).await)
}

async fn rules_get(State(state): State<LivechatAdminState>, Path(id): Path<Uuid>) -> Response {
    answer_opt(state.config.rule_get(id).await)
}

async fn rules_replace(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    Json(body): Json<RuleBody>,
) -> Response {
    let input = RuleInput {
        channel_id: body.channel_id,
        regex_url: body.regex_url,
        action: body.action,
        auto_popup_timer: body.auto_popup_timer,
        chatbot_script_id: body.chatbot_script_id,
        chatbot_enabled_condition: body.chatbot_enabled_condition,
        country_codes: body.country_codes,
        sequence: body.sequence,
    };
    answer_opt(state.config.rule_replace(id, &input).await)
}

async fn rules_delete(State(state): State<LivechatAdminState>, Path(id): Path<Uuid>) -> Response {
    answer_bool(state.config.rule_delete(id).await)
}

// ── Chatbot scripts ────────────────────────────────────────────────

async fn scripts_list(State(state): State<LivechatAdminState>) -> Response {
    answer(
        state
            .config
            .script_list()
            .await
            .map(|r| json!({ "scripts": r })),
    )
}

async fn scripts_create(
    State(state): State<LivechatAdminState>,
    Json(body): Json<ScriptCreateBody>,
) -> Response {
    answer_created(state.config.script_create(&body.title).await)
}

async fn scripts_get(State(state): State<LivechatAdminState>, Path(id): Path<Uuid>) -> Response {
    answer_opt(state.config.script_get(id).await)
}

async fn scripts_patch(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ScriptPatchBody>,
) -> Response {
    answer_opt(
        state
            .config
            .script_patch(id, body.title.as_deref(), body.is_active)
            .await,
    )
}

async fn scripts_delete(State(state): State<LivechatAdminState>, Path(id): Path<Uuid>) -> Response {
    answer_bool(state.config.script_delete(id).await)
}

// ── Steps / answers / triggers ─────────────────────────────────────

async fn steps_list(
    State(state): State<LivechatAdminState>,
    Path(script_id): Path<Uuid>,
) -> Response {
    answer(
        state
            .config
            .step_list(script_id)
            .await
            .map(|r| json!({ "steps": r })),
    )
}

async fn steps_create(
    State(state): State<LivechatAdminState>,
    Path(script_id): Path<Uuid>,
    Json(body): Json<StepBody>,
) -> Response {
    let input = StepInput {
        chatbot_script_id: script_id,
        sequence: body.sequence,
        step_type: body.step_type,
        message: body.message,
        expertise_tag_ids: body.expertise_tag_ids,
        answers: body
            .answers
            .into_iter()
            .map(|a| AnswerInput {
                sequence: a.sequence,
                label: a.label,
                redirect_url: a.redirect_url,
            })
            .collect(),
    };
    answer_created(state.config.step_create(&input).await)
}

async fn step_delete(State(state): State<LivechatAdminState>, Path(id): Path<Uuid>) -> Response {
    answer_bool(state.config.step_delete(id).await)
}

async fn answers_list(
    State(state): State<LivechatAdminState>,
    Path(step_id): Path<Uuid>,
) -> Response {
    answer(
        state
            .config
            .answer_list(step_id)
            .await
            .map(|r| json!({ "answers": r })),
    )
}

async fn answers_create(
    State(state): State<LivechatAdminState>,
    Path(step_id): Path<Uuid>,
    Json(body): Json<AnswerBody>,
) -> Response {
    answer_created(
        state
            .config
            .answer_create(
                step_id,
                &AnswerInput {
                    sequence: body.sequence,
                    label: body.label,
                    redirect_url: body.redirect_url,
                },
            )
            .await,
    )
}

async fn answer_delete(State(state): State<LivechatAdminState>, Path(id): Path<Uuid>) -> Response {
    answer_bool(state.config.answer_delete(id).await)
}

async fn triggers_list(State(state): State<LivechatAdminState>) -> Response {
    answer(
        state
            .config
            .trigger_list(None)
            .await
            .map(|r| json!({ "triggers": r })),
    )
}

async fn triggers_create(
    State(state): State<LivechatAdminState>,
    Json(body): Json<TriggerBody>,
) -> Response {
    answer_created(
        state
            .config
            .trigger_create(body.answer_id, body.target_step_id)
            .await,
    )
}

async fn trigger_delete(State(state): State<LivechatAdminState>, Path(id): Path<Uuid>) -> Response {
    answer_bool(state.config.trigger_delete(id).await)
}

// ── The test verb (a REAL is_test session on the real surface) ─────

async fn test_session_create(
    State(state): State<LivechatAdminState>,
    Path(script_id): Path<Uuid>,
    extensions: Extensions,
) -> Response {
    let Some(actor) = actor_of(&extensions) else {
        return LivechatError::ActorUnresolved.into_response();
    };
    // The script must exist under this company before a session is
    // minted (the binding itself refuses typed when the script is
    // not routable).
    match state.config.script_get(script_id).await {
        Ok(Some(_)) => {}
        Ok(None) => return LivechatError::SessionNotFound.into_response(),
        Err(e) => return e.into_response(),
    }
    // A test session needs a channel: the first active channel of
    // the company (the test drives the real surface; it never mints
    // config).
    let channels = match state.config.channel_list().await {
        Ok(rows) => rows,
        Err(e) => return e.into_response(),
    };
    let Some(channel_id) = channels
        .iter()
        .filter(|c| c.get("is_active").and_then(Value::as_bool).unwrap_or(false))
        .find_map(|c| {
            c.get("id")
                .and_then(Value::as_str)
                .and_then(|s| Uuid::parse_str(s).ok())
        })
    else {
        return LivechatError::Validation(
            "no active channel for a test session; create one first".into(),
        )
        .into_response();
    };
    let input = OpenSessionInput {
        channel_id,
        title: Some("Chatbot test".to_string()),
        visitor_key: format!("test:{actor}"),
        website_visitor_id: None,
        visitor_country_code: None,
        visitor_timezone: None,
        visitor_language: None,
        chatbot_script_id: Some(script_id),
        is_pending_request: false,
        is_test: true,
    };
    match state.sessions.open(&input, Some(actor)).await {
        Ok(row) => match state
            .chatbot
            .start_script(row.id, script_id, Some(actor))
            .await
        {
            Ok(step) => (
                axum::http::StatusCode::CREATED,
                Json(json!({
                    "session": { "id": row.id, "is_test": row.is_test, "status": row.status },
                    "first_step": step.map(|s| json!({
                        "id": s.id, "step_type": s.step_type, "message": s.message,
                    })),
                })),
            )
                .into_response(),
            Err(e) => e.into_response(),
        },
        Err(e) => e.into_response(),
    }
}

// ── Sessions ───────────────────────────────────────────────────────

async fn sessions_list(
    State(state): State<LivechatAdminState>,
    extensions: Extensions,
    Query(q): Query<SessionsQuery>,
) -> Response {
    let actor = actor_of(&extensions);
    // A closed window REQUIRES explicit bounds (the unbounded
    // every-session-ever scan has no caller).
    let closed_window = q.closed_from.is_some() || q.closed_to.is_some();
    if !q.open.unwrap_or(false)
        && closed_window
        && (q.closed_from.is_none() || q.closed_to.is_none())
    {
        return LivechatError::Validation(
            "a closed window requires both closed_from and closed_to bounds".into(),
        )
        .into_response();
    }
    let filter = SessionListFilter {
        open_only: q.open.unwrap_or(false),
        need_help_only: q.need_help.unwrap_or(false),
        mine_operator: q.mine.filter(|_| actor.is_some()).and(actor),
        channel_id: q.channel_id,
        closed_from: q.closed_from,
        closed_to: q.closed_to,
    };
    answer(
        state
            .sessions
            .list(&filter)
            .await
            .map(|r| json!({ "sessions": r })),
    )
}

async fn sessions_get(State(state): State<LivechatAdminState>, Path(id): Path<Uuid>) -> Response {
    match state.sessions.find(id).await {
        Ok(Some(row)) => {
            let preview = state.chatbot.pending_preview(&row).await;
            (
                axum::http::StatusCode::OK,
                Json(json!({ "session": row, "chatbot": preview })),
            )
                .into_response()
        }
        Ok(None) => LivechatError::SessionNotFound.into_response(),
        Err(e) => e.into_response(),
    }
}

async fn session_take(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    extensions: Extensions,
    Json(body): Json<TakeBody>,
) -> Response {
    let Some(actor) = actor_of(&extensions) else {
        return LivechatError::ActorUnresolved.into_response();
    };
    let operator = body.operator_user_id.unwrap_or(actor);
    answer_row(
        state.sessions.take(id, operator, Some(actor)).await,
        "session",
    )
}

async fn session_close(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    extensions: Extensions,
    Json(body): Json<CloseBody>,
) -> Response {
    let actor = actor_of(&extensions);
    if !matches!(
        body.reason.as_str(),
        "visitor_left"
            | "operator_closed"
            | "bot_completed"
            | "expired"
            | "cancelled"
            | "request_declined"
    ) {
        return LivechatError::Validation("undeclared close reason".into()).into_response();
    }
    match state.sessions.close(id, &body.reason, actor).await {
        Ok(CloseOutcome::Closed(row)) => (
            axum::http::StatusCode::OK,
            Json(json!({ "session": row, "outcome": "closed" })),
        )
            .into_response(),
        Ok(CloseOutcome::AlreadyClosed(row)) => (
            axum::http::StatusCode::OK,
            Json(json!({ "session": row, "outcome": "already_closed" })),
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}

async fn session_need_help(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    extensions: Extensions,
) -> Response {
    let actor = actor_of(&extensions);
    answer_row(
        state.sessions.set_need_help(id, true, actor).await,
        "session",
    )
}

async fn session_resolve_need_help(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    extensions: Extensions,
) -> Response {
    let actor = actor_of(&extensions);
    answer_row(
        state.sessions.set_need_help(id, false, actor).await,
        "session",
    )
}

async fn session_forward(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    extensions: Extensions,
    Json(body): Json<ForwardBody>,
) -> Response {
    let Some(actor) = actor_of(&extensions) else {
        return LivechatError::ActorUnresolved.into_response();
    };
    match state
        .sessions
        .forward(id, body.visitor_label.as_deref(), Some(actor))
        .await
    {
        Ok(outcome) => {
            use crate::infrastructure::persistence::AssignOutcome;
            let word = match outcome {
                AssignOutcome::Assigned { .. } => "assigned",
                AssignOutcome::Empty => "no_agent",
            };
            (axum::http::StatusCode::OK, Json(json!({ "outcome": word }))).into_response()
        }
        Err(e) => e.into_response(),
    }
}

async fn session_messages(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    Query(q): Query<MessagesQuery>,
) -> Response {
    match state.sessions.messages(id, q.after.as_deref(), 200).await {
        Ok(messages) => {
            let rows: Vec<Value> = messages
                .iter()
                .map(|m| {
                    json!({
                        "id": m.carrier_id,
                        "author": match &m.author {
                            MessageAuthor::Visitor => "visitor".to_string(),
                            MessageAuthor::Operator(op) => op.to_string(),
                            MessageAuthor::Bot => "bot".to_string(),
                        },
                        "body": m.body,
                        "created_at": m.created_at,
                    })
                })
                .collect();
            (
                axum::http::StatusCode::OK,
                Json(json!({ "messages": rows })),
            )
                .into_response()
        }
        Err(e) => e.into_response(),
    }
}

async fn session_post_message(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    extensions: Extensions,
    Json(body): Json<MessageBody>,
) -> Response {
    let Some(actor) = actor_of(&extensions) else {
        return LivechatError::ActorUnresolved.into_response();
    };
    if body.body.trim().is_empty() {
        return LivechatError::InputInvalid.into_response();
    }
    match state
        .sessions
        .post_operator_message(id, actor, &body.body)
        .await
    {
        Ok(message) => {
            // The FIRST operator message on a pending invite DELIVERS
            // it (the has-message gate opens; audited once).
            let delivered = state
                .website_requests
                .audit_delivered_if_pending(id, actor)
                .await
                .unwrap_or(false);
            (
                axum::http::StatusCode::CREATED,
                Json(json!({
                    "message": {
                        "id": message.carrier_id,
                        "author": "operator",
                        "body": message.body,
                        "created_at": message.created_at,
                    },
                    "invite_delivered": delivered,
                })),
            )
                .into_response()
        }
        Err(e) => e.into_response(),
    }
}

async fn session_chatbot_restart(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    extensions: Extensions,
    Json(body): Json<RestartBody>,
) -> Response {
    let actor = actor_of(&extensions);
    answer_row(
        state.sessions.restart(id, body.reset_failure, actor).await,
        "session",
    )
}

async fn session_tags(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    Json(body): Json<TagsBody>,
) -> Response {
    match state.sessions.set_tags(id, &body.tag_ids).await {
        Ok(()) => (axum::http::StatusCode::NO_CONTENT).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn session_transcript(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    extensions: Extensions,
    Json(body): Json<TranscriptBody>,
) -> Response {
    let actor = actor_of(&extensions);
    let request = TranscriptRequest {
        session_id: id,
        email: body.email,
        actor,
    };
    match state.sessions.send_transcript(&request).await {
        Ok(()) => (
            axum::http::StatusCode::ACCEPTED,
            Json(json!({ "queued": true })),
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}

async fn session_cancel_request(
    State(state): State<LivechatAdminState>,
    Path(id): Path<Uuid>,
    extensions: Extensions,
) -> Response {
    let Some(actor) = actor_of(&extensions) else {
        return LivechatError::ActorUnresolved.into_response();
    };
    match state.website_requests.cancel(id, false, Some(actor)).await {
        Ok(row) => (
            axum::http::StatusCode::OK,
            Json(json!({ "session": row, "cancelled": true })),
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}

// ── Website chat requests + report ─────────────────────────────────

async fn website_chat_request(
    State(state): State<LivechatAdminState>,
    extensions: Extensions,
    Json(body): Json<WebsiteChatRequestBody>,
) -> Response {
    let Some(actor) = actor_of(&extensions) else {
        return LivechatError::ActorUnresolved.into_response();
    };
    // SINGLE-VISITOR BY DESIGN: one visitor per call (a batch is
    // repeated audited calls).
    answer_created(
        state
            .website_requests
            .create_request(body.website_id, body.website_visitor_id, Some(actor))
            .await
            .map(|row| json!({ "session": row })),
    )
}

async fn report_session_summary(
    State(state): State<LivechatAdminState>,
    Query(q): Query<ReportQuery>,
) -> Response {
    answer(
        state
            .reports
            .session_summary(q.from, q.to, q.bucket.as_deref(), q.week_start)
            .await
            .map(|report| serde_json::to_value(report).unwrap_or(Value::Null)),
    )
}

// ── Shared helpers ─────────────────────────────────────────────────

fn require_actor(extensions: &Extensions) -> Result<Uuid, Response> {
    match actor_of(extensions) {
        Some(id) => Ok(id),
        None => Err(LivechatError::ActorUnresolved.into_response()),
    }
}

fn answer(result: Result<Value, LivechatError>) -> Response {
    match result {
        Ok(v) => (axum::http::StatusCode::OK, Json(v)).into_response(),
        Err(e) => e.into_response(),
    }
}

fn answer_created(result: Result<Value, LivechatError>) -> Response {
    match result {
        Ok(v) => (axum::http::StatusCode::CREATED, Json(v)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// A typed row answered under ONE key (the row itself serializes).
fn answer_row<T: serde::Serialize>(result: Result<T, LivechatError>, key: &str) -> Response {
    match result {
        Ok(row) => (axum::http::StatusCode::OK, Json(json!({ key: row }))).into_response(),
        Err(e) => e.into_response(),
    }
}

fn answer_opt(result: Result<Option<Value>, LivechatError>) -> Response {
    match result {
        Ok(Some(v)) => (axum::http::StatusCode::OK, Json(v)).into_response(),
        Ok(None) => LivechatError::SessionNotFound.into_response(),
        Err(e) => e.into_response(),
    }
}

fn answer_bool(result: Result<bool, LivechatError>) -> Response {
    match result {
        Ok(true) => (axum::http::StatusCode::NO_CONTENT).into_response(),
        Ok(false) => LivechatError::SessionNotFound.into_response(),
        Err(e) => e.into_response(),
    }
}
