//! The module's PUBLIC route surface (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! The module DOES NOT SELF-MOUNT: it exports
//! [`livechat_public_routes`], a plain `axum::Router` the composing
//! host nests BARE of `company_auth` under the schema name —
//! `Router::new().nest("/api/v1/livechat", livechat_public_routes(state))`.
//! The Tier A capability + the fixed-window throttle + the
//! website-host fence are the fence; there is no session, no company
//! auth middleware, no CORS mirror here (the host composes CORS at
//! its own edge — this module never answers `*` for anything).
//!
//! The allowlist (exhaustive — the boundary probe's target):
//! - `GET  /public/availability`                    the button answer
//! - `POST /public/sessions`                        open / resume
//! - `GET  /public/sessions/:capability`            the session view
//! - `GET  /public/sessions/:capability/messages`   the cursor poll
//! - `POST /public/sessions/:capability/messages`   the visitor message
//! - `POST /public/sessions/:capability/answers`    the chatbot answer
//! - `POST /public/sessions/:capability/close`      the visitor leave
//! - `POST /public/sessions/:capability/rating`     the 1/5/10 rating
//! - `POST /public/invites/:capability/accept`      the invite handoff
//!
//! THE FENCE ON THE PUBLIC SURFACE: every handler resolves the
//! request Host through the website bridge, takes the website's
//! company off the binding, and binds that company scope around
//! every repository call — a token minted on another site verifies
//! (the HMAC secret is per-install), then reads as MISSING under the
//! fence: the uniform `livechat_session_not_found` 404, no oracle.
//!
//! The secret: `LIVECHAT_CAPABILITY_SECRET` at compose; unset = the
//! minting verbs answer the typed 503
//! `livechat_capability_secret_not_configured` (fail-closed — the
//! boot WARN is the host's; the secret is never printed).

use std::sync::Arc;

use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::application::service::availability_service::AvailabilityService;
use crate::application::service::capability::{
    capability_secret_from_env, mint_guest_capability, CapabilityClaims,
    LIVECHAT_GUEST_TOKEN_TTL_SECS, PURPOSE_GUEST_SESSION, PURPOSE_INVITE_ACCEPT,
};
use crate::application::service::chatbot_service::{AnswerPayload, ChatbotService, StepPreview};
use crate::application::service::livechat_error::LivechatError;
use crate::application::service::mail_port::{LivechatMailCarrier, MessageAuthor};
use crate::application::service::notifier_port::LivechatNotifier;
use crate::application::service::rating_service::RatingSubmitService;
use crate::application::service::session_service::SessionCommandService;
use crate::application::service::throttle::{
    caller_ip, trusted_proxy_from_env, FixedWindows, LivechatRatePolicy,
};
use crate::application::service::transcript_port::LivechatTranscriptMailer;
use crate::application::service::website_bridge::{
    LivechatWebsiteBridge, VisitFacts, WebsiteBinding,
};
use crate::application::service::website_request_service::WebsiteRequestService;
use crate::infrastructure::persistence::{OpenSessionInput, SessionRow};

/// The cursor poll's page bound (messages per answer).
const POLL_LIMIT: i64 = 200;

/// The shared public state (cheap-to-clone service handles).
#[derive(Clone)]
pub struct LivechatPublicState {
    secret: String,
    trusted_proxy: bool,
    windows: Arc<FixedWindows>,
    policy: LivechatRatePolicy,
    availability: Arc<AvailabilityService>,
    sessions: Arc<SessionCommandService>,
    chatbot: Arc<ChatbotService>,
    ratings: Arc<RatingSubmitService>,
    website_requests: Arc<WebsiteRequestService>,
    bridge: Arc<dyn LivechatWebsiteBridge>,
}

impl LivechatPublicState {
    /// Compose over one pool + the host ports; the secret comes from
    /// [`crate::application::service::capability::LIVECHAT_CAPABILITY_SECRET_ENV`]
    /// (empty = the typed 503 at the minting verbs) and the
    /// trusted-proxy posture from `LIVECHAT_TRUSTED_PROXY` (unset =
    /// direct connections — the forwarded header is client-controlled
    /// text and never read).
    pub fn compose(
        pool: sqlx::PgPool,
        bridge: Arc<dyn LivechatWebsiteBridge>,
        carrier: Arc<dyn LivechatMailCarrier>,
        notifier: Arc<dyn LivechatNotifier>,
        transcript: Arc<dyn LivechatTranscriptMailer>,
    ) -> Self {
        Self::compose_with_trusted_proxy(
            pool,
            bridge,
            carrier,
            notifier,
            transcript,
            capability_secret_from_env(),
            trusted_proxy_from_env(),
        )
    }

    /// [`Self::compose`] with the secret and trusted-proxy posture
    /// explicit (the probe entry — tests must not depend on process
    /// environment other tests mutate).
    pub fn compose_with_trusted_proxy(
        pool: sqlx::PgPool,
        bridge: Arc<dyn LivechatWebsiteBridge>,
        carrier: Arc<dyn LivechatMailCarrier>,
        notifier: Arc<dyn LivechatNotifier>,
        transcript: Arc<dyn LivechatTranscriptMailer>,
        secret: String,
        trusted_proxy: bool,
    ) -> Self {
        Self {
            secret,
            trusted_proxy,
            windows: Arc::new(FixedWindows::new()),
            policy: LivechatRatePolicy::default(),
            availability: Arc::new(AvailabilityService::new(pool.clone(), bridge.clone())),
            sessions: Arc::new(SessionCommandService::new(
                pool.clone(),
                carrier.clone(),
                notifier.clone(),
                transcript,
            )),
            chatbot: Arc::new(ChatbotService::new(pool.clone(), carrier.clone())),
            ratings: Arc::new(RatingSubmitService::new(pool.clone(), notifier.clone())),
            website_requests: Arc::new(WebsiteRequestService::new(
                pool.clone(),
                bridge.clone(),
                notifier.clone(),
            )),
            bridge,
        }
    }

    /// The compose secret is deliberately not readable (never
    /// printed, never logged); this answers ONLY whether it is set.
    pub fn secret_is_configured(&self) -> bool {
        !self.secret.is_empty()
    }

    fn ip_of(
        &self,
        headers: &HeaderMap,
        connect_info: Option<ConnectInfo<std::net::SocketAddr>>,
    ) -> String {
        caller_ip(
            headers,
            connect_info.map(|c| c.ip().to_string()).as_deref(),
            self.trusted_proxy,
        )
    }

    fn throttle(&self, key: &str, window: (u64, u64)) -> Option<Response> {
        let (max, secs) = window;
        if self.windows.allow(key, max, secs) {
            None
        } else {
            Some(
                LivechatError::Throttled {
                    retry_after_secs: secs as u32,
                }
                .into_response(),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// The public session view (the visitor's projection: display-name
// operator only, never an id; derived chatbot preview, never rows).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct PublicSessionView {
    pub id: Uuid,
    pub status: String,
    /// The failure state, opaque to the visitor (renderable label,
    /// no internals).
    pub failure: String,
    pub closed: bool,
    pub message_count: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operator_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chatbot: Option<StepPreview>,
}

async fn public_view(
    sessions: &SessionCommandService,
    chatbot: &ChatbotService,
    row: &SessionRow,
) -> PublicSessionView {
    let operator_display_name = match row.operator_user_id {
        Some(operator) => sessions
            .operator_display_name(operator)
            .await
            .ok()
            .flatten(),
        None => None,
    };
    PublicSessionView {
        id: row.id,
        status: row.status.clone().unwrap_or_else(|| "waiting".to_string()),
        failure: row.failure.clone(),
        closed: row.closed_at.is_some(),
        message_count: row.message_count,
        operator_display_name,
        chatbot: chatbot.pending_preview(row).await,
    }
}

// ---------------------------------------------------------------------------
// THE PUBLIC TREE (the exhaustive allowlist — see the module doc).
// ---------------------------------------------------------------------------

pub fn livechat_public_routes(state: LivechatPublicState) -> Router {
    Router::new()
        .route("/public/availability", get(availability_handler))
        .route("/public/sessions", post(open_handler))
        .route("/public/sessions/:capability", get(session_handler))
        .route(
            "/public/sessions/:capability/messages",
            get(poll_handler).post(message_handler),
        )
        .route("/public/sessions/:capability/answers", post(answer_handler))
        .route("/public/sessions/:capability/close", post(close_handler))
        .route("/public/sessions/:capability/rating", post(rating_handler))
        .route("/public/invites/:capability/accept", post(accept_handler))
        .with_state(state)
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// The request bodies (all fields optional; a malformed or absent
/// JSON body parses as the empty request — the verbs validate what
/// they actually need, typed).
#[derive(Debug, Default, Deserialize)]
struct OpenRequest {
    capability: Option<String>,
    visitor_language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MessageRequest {
    body: String,
}

#[derive(Debug, Default, Deserialize)]
struct AnswerRequest {
    step_id: Option<Uuid>,
    answer_id: Option<Uuid>,
    input: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RatingRequest {
    value: i32,
    rated_persona: String,
    #[serde(default)]
    comment: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct MessagesQuery {
    after: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct AvailabilityQuery {
    visitor_key: Option<String>,
}

/// GET /public/availability — the website button answer. Per-IP
/// fixed window only (the visitor has no identity yet).
async fn availability_handler(
    State(state): State<LivechatPublicState>,
    headers: HeaderMap,
    connect_info: Option<ConnectInfo<std::net::SocketAddr>>,
    Query(query): Query<AvailabilityQuery>,
) -> Response {
    let ip = state.ip_of(&headers, connect_info);
    if let Some(refused) = state.throttle(&format!("avail-ip:{ip}"), state.policy.availability_ip) {
        return refused;
    }
    let host = match host_header(&headers) {
        Some(h) => h,
        None => return LivechatError::ChannelNotFound.into_response(),
    };
    let referer = headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    match state
        .availability
        .answer(
            &host,
            referer.as_deref(),
            query.visitor_key.as_deref(),
            &state.secret,
        )
        .await
    {
        Ok(answer) => (StatusCode::OK, Json(answer)).into_response(),
        Err(e) => e.into_response(),
    }
}

/// POST /public/sessions — open/resume. Throttled per-IP AND
/// per-identity. A presented capability that verifies ⇒ continuity;
/// one that fails ⇒ `401 livechat_guest_token_invalid`, ZERO rows
/// minted; no token ⇒ first visit (the bridge mints the website
/// visitor; livechat binds the returned key).
async fn open_handler(
    State(state): State<LivechatPublicState>,
    headers: HeaderMap,
    connect_info: Option<ConnectInfo<std::net::SocketAddr>>,
    maybe_json: Result<Json<OpenRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    // Tolerant body: an absent/empty body is the no-token open.
    let payload = maybe_json.map(|Json(p)| p).unwrap_or_default();

    let ip = state.ip_of(&headers, connect_info);
    if let Some(refused) = state.throttle(&format!("open-ip:{ip}"), state.policy.open_ip) {
        return refused;
    }
    if !state.secret_is_configured() {
        return LivechatError::CapabilitySecretNotConfigured.into_response();
    }

    // The presented capability: verify BEFORE anything durable — a
    // failed verify is the typed 401 and ZERO rows.
    let presented = match &payload.capability {
        Some(token) if !token.is_empty() => match CapabilityClaims::verify(
            &state.secret,
            PURPOSE_GUEST_SESSION,
            token,
            chrono::Utc::now(),
        ) {
            Ok(claims) => Some(claims),
            Err(_) => return LivechatError::GuestTokenInvalid.into_response(),
        },
        _ => None,
    };
    // The identity arm keys the visitor digest (the presented key, or
    // the IP until a key exists — a NAT of first-visitors is the IP
    // arm's business).
    let identity = presented
        .as_ref()
        .and_then(|c| c.visitor_key().map(str::to_string))
        .unwrap_or_else(|| format!("ip:{ip}"));
    if let Some(refused) =
        state.throttle(&format!("open-id:{identity}"), state.policy.open_identity)
    {
        return refused;
    }

    // The host fence: the website the Host header names.
    let host = match host_header(&headers) {
        Some(h) => h,
        None => return LivechatError::ChannelNotFound.into_response(),
    };
    let binding = match state.bridge.resolve_website_by_host(&host).await {
        Ok(b) => b,
        Err(e) => return e.into_response(),
    };
    let referer = headers
        .get(header::REFERER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let result = backbone_orm::company_scope::with_company_scope(Some(binding.company_id), async {
        // The website's bound active channel + the matched rule's
        // script (bot-first routing; the human path stands when the
        // match routes nowhere).
        let Some(channel) = state
            .website_requests
            .active_channel(binding.website_id)
            .await?
        else {
            return Err(LivechatError::ChannelNotFound);
        };
        let routed = state
            .availability
            .routed_script(channel.id, referer.as_deref())
            .await?;

        // Continuity: a verified capability with an OPEN session on
        // this channel resumes it (a fresh capability rotates out).
        if let Some(claims) = &presented {
            if let Some(key) = claims.visitor_key() {
                if let Some(existing) = state.sessions.find_open_by_visitor(channel.id, key).await?
                {
                    return Ok(OpenOutcome {
                        row: existing,
                        visitor_key: key.to_string(),
                        fresh: false,
                    });
                }
            }
        }

        // First visit (or a closed prior session): the bridge mints
        // the website visitor; livechat binds the returned identity.
        let (visitor_key, website_visitor_id, country, timezone) = match &presented {
            Some(claims) => {
                // A verified returning key keeps ITS digest (the
                // visitor is already known to the site).
                let key = claims.visitor_key().map(str::to_string).unwrap_or_default();
                (key, None, None, None)
            }
            None => {
                let facts = VisitFacts {
                    website_id: binding.website_id,
                    ip: ip.clone(),
                    user_agent: headers
                        .get(header::USER_AGENT)
                        .and_then(|v| v.to_str().ok())
                        .map(str::to_string),
                    url: referer.clone(),
                };
                let visitor = state.bridge.ensure_visitor(&facts).await?;
                (
                    visitor.visitor_key,
                    Some(visitor.visitor_id),
                    visitor.country_code,
                    visitor.timezone,
                )
            }
        };
        if visitor_key.is_empty() {
            return Err(LivechatError::Validation(
                "visitor identity could not be established".into(),
            ));
        }

        // The visitor's own open WINS over any pending operator
        // invite on this channel (both sides notified, audited).
        let _ = state
            .website_requests
            .cancel_pending_for_visitor(channel.id, &visitor_key, None)
            .await?;

        let input = OpenSessionInput {
            channel_id: channel.id,
            title: None,
            visitor_key: visitor_key.clone(),
            website_visitor_id,
            visitor_country_code: country,
            visitor_timezone: timezone,
            visitor_language: payload.visitor_language.clone(),
            chatbot_script_id: routed,
            is_pending_request: false,
            is_test: false,
        };
        let row = state.sessions.open(&input, None).await?;
        // The lazy welcome: binding a routable script sets the pointer
        // and mints ZERO message rows (the preview is the answer).
        if let Some(script_id) = routed {
            state.chatbot.start_script(row.id, script_id, None).await?;
        }
        Ok(OpenOutcome {
            row,
            visitor_key,
            fresh: true,
        })
    })
    .await;

    let outcome = match result {
        Ok(o) => o,
        Err(e) => return e.into_response(),
    };
    let row = outcome.row;
    let capability = match mint_guest_capability(
        &state.secret,
        &row.id,
        // The identity the token binds is the LEDGER's visitor key
        // (the same digest the open bound).
        &outcome.visitor_key,
        chrono::Utc::now(),
        LIVECHAT_GUEST_TOKEN_TTL_SECS,
    ) {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };
    let view = public_view(&state.sessions, &state.chatbot, &row).await;
    let status = if outcome.fresh {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    (
        status,
        Json(serde_json::json!({
            "capability": capability,
            "resumed": !outcome.fresh,
            "session": view,
        })),
    )
        .into_response()
}

struct OpenOutcome {
    row: SessionRow,
    visitor_key: String,
    fresh: bool,
}

/// GET /public/sessions/:capability — the session view (status,
/// opaque failure state, chatbot preview, operator display name).
async fn session_handler(
    State(state): State<LivechatPublicState>,
    Path(capability): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some((session_id, _visitor_key)) = verify_guest(&state, &capability) else {
        return LivechatError::SessionNotFound.into_response();
    };
    match fenced_session(&state, &headers, session_id).await {
        Ok(row) => {
            let view = public_view(&state.sessions, &state.chatbot, &row).await;
            (StatusCode::OK, Json(view)).into_response()
        }
        Err(e) => e.into_response(),
    }
}

/// GET /public/sessions/:capability/messages — the cursor poll
/// (`?after=<carrier_message_id>`), ascending, bounded.
async fn poll_handler(
    State(state): State<LivechatPublicState>,
    Path(capability): Path<String>,
    headers: HeaderMap,
    Query(query): Query<MessagesQuery>,
) -> Response {
    let Some((session_id, visitor_key)) = verify_guest(&state, &capability) else {
        return LivechatError::SessionNotFound.into_response();
    };
    if let Some(refused) = state.throttle(
        &format!("poll-id:{visitor_key}"),
        state.policy.poll_identity,
    ) {
        return refused;
    }
    let result: Result<
        (
            SessionRow,
            Vec<crate::application::service::mail_port::CarrierMessage>,
        ),
        LivechatError,
    > = backbone_orm::company_scope::with_company_scope(
        company_of(&state, &headers).await,
        async {
            let row = state
                .sessions
                .find(session_id)
                .await?
                .ok_or(LivechatError::SessionNotFound)?;
            let messages = state
                .sessions
                .messages(session_id, query.after.as_deref(), POLL_LIMIT)
                .await?;
            Ok((row, messages))
        },
    )
    .await;
    match result {
        Ok((row, messages)) => {
            let view = public_view(&state.sessions, &state.chatbot, &row).await;
            let messages: Vec<serde_json::Value> = messages
                .iter()
                .map(|m| {
                    serde_json::json!({
                        "id": m.carrier_id,
                        "author": match &m.author {
                            MessageAuthor::Visitor => "visitor".to_string(),
                            MessageAuthor::Operator(_) => "operator".to_string(),
                            MessageAuthor::Bot => "bot".to_string(),
                        },
                        "body": m.body,
                        "created_at": m.created_at,
                    })
                })
                .collect();
            (
                StatusCode::OK,
                Json(serde_json::json!({ "messages": messages, "session": view })),
            )
                .into_response()
        }
        Err(e) => e.into_response(),
    }
}

/// POST /public/sessions/:capability/messages — the visitor message
/// through the ONE chokepoint; the heartbeat piggybacks
/// `track_visit` (chat activity IS the visitor heartbeat; a refused
/// track is a WARN, never a failed message).
async fn message_handler(
    State(state): State<LivechatPublicState>,
    Path(capability): Path<String>,
    headers: HeaderMap,
    connect_info: Option<ConnectInfo<std::net::SocketAddr>>,
    Json(payload): Json<MessageRequest>,
) -> Response {
    let Some((session_id, visitor_key)) = verify_guest(&state, &capability) else {
        return LivechatError::SessionNotFound.into_response();
    };
    let ip = state.ip_of(&headers, connect_info);
    if let Some(refused) = state
        .throttle(
            &format!("msg-id:{visitor_key}"),
            state.policy.message_identity,
        )
        .or_else(|| state.throttle(&format!("msg-ip:{ip}"), state.policy.message_ip))
    {
        return refused;
    }
    if payload.body.trim().is_empty() {
        return LivechatError::InputInvalid.into_response();
    }

    // The host fence + the visitor-facts website arm (the heartbeat
    // piggyback needs the site the session lives on).
    let binding = match binding_of(&state, &headers).await {
        Some(b) => b,
        None => return LivechatError::SessionNotFound.into_response(),
    };
    let result = backbone_orm::company_scope::with_company_scope(Some(binding.company_id), async {
        let row = state
            .sessions
            .find(session_id)
            .await?
            .ok_or(LivechatError::SessionNotFound)?;
        if row.closed_at.is_some() {
            return Err(LivechatError::Validation("session is closed".into()));
        }
        let message = state
            .sessions
            .post_visitor_message(session_id, &payload.body)
            .await?;
        // The heartbeat piggyback: non-blocking.
        if let Some(visitor_id) = row.website_visitor_id {
            let facts = VisitFacts {
                website_id: binding.website_id,
                ip: ip.clone(),
                user_agent: headers
                    .get(header::USER_AGENT)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string),
                url: headers
                    .get(header::REFERER)
                    .and_then(|v| v.to_str().ok())
                    .map(str::to_string),
            };
            if let Err(e) = state.bridge.track_visit(&facts, visitor_id).await {
                tracing::warn!(
                    error_code = e.code(),
                    "visit heartbeat refused behind a delivered message; the message stands"
                );
            }
        }
        // The engine: a bot-owned session reacts (an agent-owned one
        // halts inside).
        let _ = state.chatbot.on_visitor_interaction(session_id, None).await;
        let fresh = state
            .sessions
            .find(session_id)
            .await?
            .ok_or(LivechatError::SessionNotFound)?;
        Ok((message, fresh))
    })
    .await;
    match result {
        Ok((message, fresh)) => {
            let view = public_view(&state.sessions, &state.chatbot, &fresh).await;
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "message": {
                        "id": message.carrier_id,
                        "author": "visitor",
                        "body": message.body,
                        "created_at": message.created_at,
                    },
                    "session": view,
                })),
            )
                .into_response()
        }
        Err(e) => e.into_response(),
    }
}

/// POST /public/sessions/:capability/answers — the chatbot answer /
/// free input (the ONE input contract; the race guard refuses a
/// stale step id typed).
async fn answer_handler(
    State(state): State<LivechatPublicState>,
    Path(capability): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<AnswerRequest>,
) -> Response {
    let Some((session_id, visitor_key)) = verify_guest(&state, &capability) else {
        return LivechatError::SessionNotFound.into_response();
    };
    if let Some(refused) = state.throttle(
        &format!("ans-id:{visitor_key}"),
        state.policy.answers_identity,
    ) {
        return refused;
    }
    let parsed = match (payload.answer_id, payload.input) {
        (Some(answer_id), None) => AnswerPayload::Selection { answer_id },
        (None, Some(input)) => AnswerPayload::Text { input },
        _ => {
            return LivechatError::Validation(
                "exactly one of answer_id or input is required".into(),
            )
            .into_response()
        }
    };
    let result = backbone_orm::company_scope::with_company_scope(
        company_of(&state, &headers).await,
        async {
            let row = state
                .sessions
                .find(session_id)
                .await?
                .ok_or(LivechatError::SessionNotFound)?;
            if row.closed_at.is_some() {
                return Err(LivechatError::Validation("session is closed".into()));
            }
            state
                .chatbot
                .answer(session_id, payload.step_id, parsed, None)
                .await
        },
    )
    .await;
    match result {
        Ok(outcome) => {
            use crate::application::service::chatbot_service::EngineOutcome;
            let (state_word, step) = match outcome {
                EngineOutcome::Waiting { step } => (
                    "waiting",
                    Some(StepPreview {
                        step_type: step.step_type.clone(),
                        message: step.message.clone(),
                    }),
                ),
                EngineOutcome::Done => ("done", None),
                EngineOutcome::Parked => ("parked", None),
            };
            (
                StatusCode::OK,
                Json(serde_json::json!({ "outcome": state_word, "step": step })),
            )
                .into_response()
        }
        Err(e) => e.into_response(),
    }
}

/// POST /public/sessions/:capability/close — the visitor leave
/// (idempotent; audited; the rating prompt rides the notifier).
async fn close_handler(
    State(state): State<LivechatPublicState>,
    Path(capability): Path<String>,
    headers: HeaderMap,
) -> Response {
    let Some((session_id, _)) = verify_guest(&state, &capability) else {
        return LivechatError::SessionNotFound.into_response();
    };
    let result = backbone_orm::company_scope::with_company_scope(
        company_of(&state, &headers).await,
        async { state.sessions.close(session_id, "visitor_left", None).await },
    )
    .await;
    match result {
        Ok(outcome) => {
            let row = match outcome {
                crate::infrastructure::persistence::CloseOutcome::Closed(row) => row,
                crate::infrastructure::persistence::CloseOutcome::AlreadyClosed(row) => row,
            };
            let view = public_view(&state.sessions, &state.chatbot, &row).await;
            (StatusCode::OK, Json(view)).into_response()
        }
        Err(e) => e.into_response(),
    }
}

/// POST /public/sessions/:capability/rating — the 1/5/10 rating,
/// once per session (the DB UNIQUE is the wall).
async fn rating_handler(
    State(state): State<LivechatPublicState>,
    Path(capability): Path<String>,
    headers: HeaderMap,
    Json(payload): Json<RatingRequest>,
) -> Response {
    let Some((session_id, visitor_key)) = verify_guest(&state, &capability) else {
        return LivechatError::SessionNotFound.into_response();
    };
    if let Some(refused) = state.throttle(
        &format!("rate-id:{visitor_key}"),
        state.policy.rating_identity,
    ) {
        return refused;
    }
    let result = backbone_orm::company_scope::with_company_scope(
        company_of(&state, &headers).await,
        async {
            state
                .ratings
                .submit(
                    session_id,
                    payload.value,
                    &payload.rated_persona,
                    payload.comment.as_deref(),
                    None,
                )
                .await
        },
    )
    .await;
    match result {
        Ok(row) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "session_id": row.session_id,
                "value": row.value,
                "rated_persona": row.rated_persona,
            })),
        )
            .into_response(),
        Err(e) => e.into_response(),
    }
}

/// POST /public/invites/:capability/accept — the operator-initiated
/// invite handoff (the capability is the short-TTL
/// `livechat-invite-accept` mint from the availability answer).
async fn accept_handler(
    State(state): State<LivechatPublicState>,
    Path(capability): Path<String>,
    headers: HeaderMap,
) -> Response {
    if !state.secret_is_configured() {
        return LivechatError::CapabilitySecretNotConfigured.into_response();
    }
    let claims = match CapabilityClaims::verify(
        &state.secret,
        PURPOSE_INVITE_ACCEPT,
        &capability,
        chrono::Utc::now(),
    ) {
        Ok(c) => c,
        Err(_) => return LivechatError::SessionNotFound.into_response(),
    };
    let Some(session_id) = claims.session_id() else {
        return LivechatError::SessionNotFound.into_response();
    };
    let Some(visitor_key) = claims.visitor_key().map(str::to_string) else {
        return LivechatError::SessionNotFound.into_response();
    };
    let result = backbone_orm::company_scope::with_company_scope(
        company_of(&state, &headers).await,
        async {
            state
                .website_requests
                .accept(session_id, &visitor_key, None)
                .await
        },
    )
    .await;
    let row = match result {
        Ok(row) => row,
        Err(e) => return e.into_response(),
    };
    let capability = match mint_guest_capability(
        &state.secret,
        &row.id,
        &visitor_key,
        chrono::Utc::now(),
        LIVECHAT_GUEST_TOKEN_TTL_SECS,
    ) {
        Ok(t) => t,
        Err(e) => return e.into_response(),
    };
    let view = public_view(&state.sessions, &state.chatbot, &row).await;
    (
        StatusCode::OK,
        Json(serde_json::json!({ "capability": capability, "session": view })),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// Shared handler helpers
// ---------------------------------------------------------------------------

/// The Host header (the website binding key), lowercased.
fn host_header(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .map(|h| h.trim().to_ascii_lowercase())
        .filter(|h| !h.is_empty())
}

/// Verify a guest-session capability against the state secret; the
/// ENTIRE refusal family is the uniform 404 (handled by the caller).
fn verify_guest(state: &LivechatPublicState, token: &str) -> Option<(Uuid, String)> {
    let claims = CapabilityClaims::verify(
        &state.secret,
        PURPOSE_GUEST_SESSION,
        token,
        chrono::Utc::now(),
    )
    .ok()?;
    let session_id = claims.session_id()?;
    let visitor_key = claims.visitor_key()?.to_string();
    Some((session_id, visitor_key))
}

/// Resolve the request Host to the bound website (`None` when the
/// Host does not name a bound site — the scoped reads then see
/// nothing, the uniform 404).
async fn binding_of(state: &LivechatPublicState, headers: &HeaderMap) -> Option<WebsiteBinding> {
    let host = host_header(headers)?;
    state.bridge.resolve_website_by_host(&host).await.ok()
}

/// The company whose fence owns the session (see [`binding_of`]).
async fn company_of(state: &LivechatPublicState, headers: &HeaderMap) -> Option<Uuid> {
    binding_of(state, headers).await.map(|b| b.company_id)
}

/// A session read under the Host-derived fence (a token minted on
/// another site verifies, then reads as missing — no oracle).
async fn fenced_session(
    state: &LivechatPublicState,
    headers: &HeaderMap,
    session_id: Uuid,
) -> Result<SessionRow, LivechatError> {
    backbone_orm::company_scope::with_company_scope(company_of(state, headers).await, async {
        state
            .sessions
            .find(session_id)
            .await?
            .ok_or(LivechatError::SessionNotFound)
    })
    .await
}
