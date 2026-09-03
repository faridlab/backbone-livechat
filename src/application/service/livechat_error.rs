//! The module's typed error surface (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! ONE enum for every hand verb, mapped to HTTP once, here. Every
//! refusal a client can distinguish is a NAMED variant with a STABLE
//! `code` string — the wire contract probes and the webapp assert
//! against. Refusals a client must NOT be able to distinguish (the
//! capability-gate family: missing session, malformed token, wrong
//! version, wrong purpose, bad signature, expired, cross-session
//! token) all surface as the ONE uniform `livechat_session_not_found`
//! 404 — no enumeration oracle.

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// The module's typed error enum.
#[derive(Debug, thiserror::Error)]
pub enum LivechatError {
    /// THE UNIFORM CAPABILITY/SESSION 404: missing session, closed
    /// and swept id, malformed token, wrong version, wrong purpose,
    /// bad signature, expired, or cross-session token — ONE answer,
    /// no oracle.
    #[error("livechat session not found")]
    SessionNotFound,

    /// A channel or website miss on a gated verb (the availability
    /// and open families; the from-website wizard).
    #[error("livechat channel not found")]
    ChannelNotFound,

    /// An operator profile miss on the profile verbs (self or
    /// officer read/update of a user id with no profile row).
    #[error("operator profile not found")]
    OperatorProfileNotFound,

    /// A presented capability at the open verb failed verification —
    /// `401`, and ZERO rows are minted (the mint-on-wrong-token
    /// probing cover is refused).
    #[error("livechat guest token invalid")]
    GuestTokenInvalid,

    /// Fixed-window throttle refusal (per-IP AND per-identity arms).
    #[error("livechat throttled")]
    Throttled { retry_after_secs: u32 },

    /// `LIVECHAT_CAPABILITY_SECRET` is unset: every minting verb
    /// refuses typed rather than mint under an empty key
    /// (fail-closed).
    #[error("livechat capability secret not configured")]
    CapabilitySecretNotConfigured,

    /// The website bridge port is not composed (its refusing
    /// default): the open and availability verbs park loudly.
    #[error("livechat website bridge not composed")]
    WebsiteBridgeNotComposed,

    /// The mail carrier port is not composed (its refusing default):
    /// the message verbs park loudly; chatbot steps park on
    /// `sessions.error_detail` + audit instead.
    #[error("livechat carrier not composed")]
    CarrierNotComposed,

    /// The transcript mailer port is not composed (its refusing
    /// default): the transcript verb parks loudly.
    #[error("livechat transcript mailer not composed")]
    TranscriptNotComposed,

    /// The digest queue port is not composed (its refusing default):
    /// the digest verb parks loudly. KPI reads never need it.
    #[error("livechat digest queue not composed")]
    DigestNotComposed,

    /// The serialized first-wins assignment/take loser: another
    /// operator's conditional UPDATE won the row (or the session is
    /// closed/no longer assignable).
    #[error("livechat operator busy")]
    OperatorBusy,

    /// The host's actor bridge could not resolve `tenant.user_id`
    /// into an operator id — the admin verbs refuse rather than run
    /// as an unidentified principal. (Raised by the host-side bridge
    /// composition; carried here so the wire contract lives in ONE
    /// enum.)
    #[error("livechat actor unresolved")]
    ActorUnresolved,

    /// A second rating for a session — the DB UNIQUE made typed (the
    /// silent-overwrite shape is refused).
    #[error("livechat rating already submitted")]
    RatingAlreadySubmitted,

    /// A canonical-name collision: conversation tags, expertise
    /// tags, and script titles are UNIQUE per company on
    /// `lower(name)` — 'Bug' and 'bug' cannot coexist.
    #[error("livechat tag name conflict")]
    TagNameConflict { name: String },

    /// Save-time channel-rule regex validation: the pattern is
    /// malformed, or EMPTY (match-all must be an explicit `.*`).
    #[error("livechat rule regex invalid")]
    RuleRegexInvalid,

    /// A chatbot selection answer outside the step's declared
    /// options.
    #[error("livechat answer invalid")]
    AnswerInvalid,

    /// The ONE input contract: an email/phone/free-input answer that
    /// fails normalization.
    #[error("livechat input invalid")]
    InputInvalid,

    /// The chatbot pointer race: the answered step is no longer the
    /// session's current step.
    #[error("livechat step not current")]
    StepNotCurrent,

    /// The general request-shape/domain family (missing report
    /// bounds, windows over the 366-day cap, backwards chatbot
    /// triggers, question steps without answers, deleting a step a
    /// pointer references).
    #[error("validation: {0}")]
    Validation(String),

    /// Infrastructure failures (never mapped from a domain refusal);
    /// internal shapes never leak text.
    #[error("database: {0}")]
    Database(String),

    #[error("internal: {0}")]
    Internal(String),
}

impl LivechatError {
    /// The stable wire code (the string probes and clients match on).
    pub fn code(&self) -> &'static str {
        match self {
            Self::SessionNotFound => "livechat_session_not_found",
            Self::ChannelNotFound => "livechat_channel_not_found",
            Self::OperatorProfileNotFound => "livechat_operator_profile_not_found",
            Self::GuestTokenInvalid => "livechat_guest_token_invalid",
            Self::Throttled { .. } => "livechat_throttled",
            Self::CapabilitySecretNotConfigured => "livechat_capability_secret_not_configured",
            Self::WebsiteBridgeNotComposed => "livechat_website_bridge_not_composed",
            Self::CarrierNotComposed => "livechat_carrier_not_composed",
            Self::TranscriptNotComposed => "livechat_transcript_not_composed",
            Self::DigestNotComposed => "livechat_digest_not_composed",
            Self::OperatorBusy => "livechat_operator_busy",
            Self::ActorUnresolved => "livechat_actor_unresolved",
            Self::RatingAlreadySubmitted => "livechat_rating_already_submitted",
            Self::TagNameConflict { .. } => "livechat_tag_name_conflict",
            Self::RuleRegexInvalid => "livechat_rule_regex_invalid",
            Self::AnswerInvalid => "livechat_answer_invalid",
            Self::InputInvalid => "livechat_input_invalid",
            Self::StepNotCurrent => "livechat_step_not_current",
            Self::Validation(_) => "livechat_validation",
            Self::Database(_) => "livechat_database",
            Self::Internal(_) => "livechat_internal",
        }
    }

    fn status(&self) -> StatusCode {
        match self {
            Self::SessionNotFound | Self::ChannelNotFound | Self::OperatorProfileNotFound => {
                StatusCode::NOT_FOUND
            }
            Self::GuestTokenInvalid => StatusCode::UNAUTHORIZED,
            Self::ActorUnresolved => StatusCode::FORBIDDEN,
            Self::OperatorBusy | Self::RatingAlreadySubmitted | Self::TagNameConflict { .. } => {
                StatusCode::CONFLICT
            }
            Self::RuleRegexInvalid
            | Self::AnswerInvalid
            | Self::InputInvalid
            | Self::StepNotCurrent
            | Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::Throttled { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::CapabilitySecretNotConfigured
            | Self::WebsiteBridgeNotComposed
            | Self::CarrierNotComposed
            | Self::TranscriptNotComposed
            | Self::DigestNotComposed => StatusCode::SERVICE_UNAVAILABLE,
            Self::Database(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for LivechatError {
    fn into_response(self) -> Response {
        let status = self.status();
        let mut headers = HeaderMap::new();
        if let Self::Throttled { retry_after_secs } = &self {
            if let Ok(v) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                headers.insert(header::RETRY_AFTER, v);
            }
        }
        // The refusal detail the caller may see. The capability-gate
        // family deliberately carries NO distinguishing detail: the
        // body is the same for every member (uniform
        // `livechat_session_not_found`). Internal shapes never leak
        // their text.
        let message = match &self {
            Self::Database(_) => "database error".to_string(),
            Self::Internal(_) => "internal error".to_string(),
            _ => self.to_string(),
        };
        let body = Json(json!({
            "error": { "code": self.code(), "message": message }
        }));
        (status, headers, body).into_response()
    }
}

impl From<sqlx::Error> for LivechatError {
    fn from(e: sqlx::Error) -> Self {
        Self::Database(e.to_string())
    }
}

impl From<anyhow::Error> for LivechatError {
    fn from(e: anyhow::Error) -> Self {
        Self::Internal(e.to_string())
    }
}

/// Result alias over [`LivechatError`].
pub type LivechatResult<T> = Result<T, LivechatError>;
