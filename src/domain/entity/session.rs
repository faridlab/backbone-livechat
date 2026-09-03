use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::AuditMetadata;
use super::LivechatCloseReason;
use super::LivechatFailure;
use super::LivechatSessionOutcome;
use super::LivechatSessionStatus;

/// Strongly-typed ID for Session
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new(id: Uuid) -> Self {
        Self(id)
    }
    pub fn generate() -> Self {
        Self(Uuid::new_v4())
    }
    pub fn into_inner(self) -> Uuid {
        self.0
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for SessionId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for SessionId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<SessionId> for Uuid {
    fn from(id: SessionId) -> Self {
        id.0
    }
}

impl AsRef<Uuid> for SessionId {
    fn as_ref(&self) -> &Uuid {
        &self.0
    }
}

impl std::ops::Deref for SessionId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Session {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub title: Option<String>,
    pub status: Option<LivechatSessionStatus>,
    pub failure: LivechatFailure,
    pub outcome: Option<LivechatSessionOutcome>,
    pub close_reason: Option<LivechatCloseReason>,
    pub closed_at: Option<DateTime<Utc>>,
    pub operator_user_id: Option<Uuid>,
    pub chatbot_current_step_id: Option<Uuid>,
    pub expertise_names: Vec<String>,
    pub website_visitor_id: Option<Uuid>,
    pub visitor_country_code: Option<String>,
    pub visitor_timezone: Option<String>,
    pub is_pending_request: bool,
    pub visitor_language: Option<String>,
    pub message_count: i32,
    pub first_response_at: Option<DateTime<Utc>>,
    pub last_interest_at: DateTime<Utc>,
    pub last_visitor_message_at: Option<DateTime<Utc>>,
    pub last_operator_message_at: Option<DateTime<Utc>>,
    pub is_test: bool,
    pub error_detail: Option<String>,
    pub company_id: Uuid,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl Session {
    /// Create a builder for Session
    pub fn builder() -> SessionBuilder {
        <SessionBuilder as Default>::default()
    }

    /// Create a new Session with required fields
    pub fn new(
        channel_id: Uuid,
        failure: LivechatFailure,
        expertise_names: Vec<String>,
        is_pending_request: bool,
        message_count: i32,
        last_interest_at: DateTime<Utc>,
        is_test: bool,
        company_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            channel_id,
            title: None,
            status: None,
            failure,
            outcome: None,
            close_reason: None,
            closed_at: None,
            operator_user_id: None,
            chatbot_current_step_id: None,
            expertise_names,
            website_visitor_id: None,
            visitor_country_code: None,
            visitor_timezone: None,
            is_pending_request,
            visitor_language: None,
            message_count,
            first_response_at: None,
            last_interest_at,
            last_visitor_message_at: None,
            last_operator_message_at: None,
            is_test,
            error_detail: None,
            company_id,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> SessionId {
        SessionId(self.id)
    }

    /// Get when this entity was created
    pub fn created_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.created_at.as_ref()
    }

    /// Get when this entity was last updated
    pub fn updated_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.updated_at.as_ref()
    }

    /// Check if this entity is soft deleted
    pub fn is_deleted(&self) -> bool {
        self.metadata.deleted_at.is_some()
    }

    /// Check if this entity is active (not deleted)
    pub fn is_active(&self) -> bool {
        self.metadata.deleted_at.is_none()
    }

    /// Get when this entity was deleted
    pub fn deleted_at(&self) -> Option<&DateTime<Utc>> {
        self.metadata.deleted_at.as_ref()
    }

    /// Get who created this entity
    pub fn created_by(&self) -> Option<&Uuid> {
        self.metadata.created_by.as_ref()
    }

    /// Get who last updated this entity
    pub fn updated_by(&self) -> Option<&Uuid> {
        self.metadata.updated_by.as_ref()
    }

    /// Get who deleted this entity
    pub fn deleted_by(&self) -> Option<&Uuid> {
        self.metadata.deleted_by.as_ref()
    }

    /// Get the current status
    pub fn status(&self) -> Option<&LivechatSessionStatus> {
        self.status.as_ref()
    }

    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the title field (chainable)
    pub fn with_title(mut self, value: String) -> Self {
        self.title = Some(value);
        self
    }

    /// Set the status field (chainable)
    pub fn with_status(mut self, value: LivechatSessionStatus) -> Self {
        self.status = Some(value);
        self
    }

    /// Set the outcome field (chainable)
    pub fn with_outcome(mut self, value: LivechatSessionOutcome) -> Self {
        self.outcome = Some(value);
        self
    }

    /// Set the close_reason field (chainable)
    pub fn with_close_reason(mut self, value: LivechatCloseReason) -> Self {
        self.close_reason = Some(value);
        self
    }

    /// Set the closed_at field (chainable)
    pub fn with_closed_at(mut self, value: DateTime<Utc>) -> Self {
        self.closed_at = Some(value);
        self
    }

    /// Set the operator_user_id field (chainable)
    pub fn with_operator_user_id(mut self, value: Uuid) -> Self {
        self.operator_user_id = Some(value);
        self
    }

    /// Set the chatbot_current_step_id field (chainable)
    pub fn with_chatbot_current_step_id(mut self, value: Uuid) -> Self {
        self.chatbot_current_step_id = Some(value);
        self
    }

    /// Set the website_visitor_id field (chainable)
    pub fn with_website_visitor_id(mut self, value: Uuid) -> Self {
        self.website_visitor_id = Some(value);
        self
    }

    /// Set the visitor_country_code field (chainable)
    pub fn with_visitor_country_code(mut self, value: String) -> Self {
        self.visitor_country_code = Some(value);
        self
    }

    /// Set the visitor_timezone field (chainable)
    pub fn with_visitor_timezone(mut self, value: String) -> Self {
        self.visitor_timezone = Some(value);
        self
    }

    /// Set the visitor_language field (chainable)
    pub fn with_visitor_language(mut self, value: String) -> Self {
        self.visitor_language = Some(value);
        self
    }

    /// Set the first_response_at field (chainable)
    pub fn with_first_response_at(mut self, value: DateTime<Utc>) -> Self {
        self.first_response_at = Some(value);
        self
    }

    /// Set the last_visitor_message_at field (chainable)
    pub fn with_last_visitor_message_at(mut self, value: DateTime<Utc>) -> Self {
        self.last_visitor_message_at = Some(value);
        self
    }

    /// Set the last_operator_message_at field (chainable)
    pub fn with_last_operator_message_at(mut self, value: DateTime<Utc>) -> Self {
        self.last_operator_message_at = Some(value);
        self
    }

    /// Set the error_detail field (chainable)
    pub fn with_error_detail(mut self, value: String) -> Self {
        self.error_detail = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "channel_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.channel_id = v;
                    }
                }
                "title" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.title = v;
                    }
                }
                "status" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.status = v;
                    }
                }
                "failure" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.failure = v;
                    }
                }
                "outcome" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.outcome = v;
                    }
                }
                "close_reason" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.close_reason = v;
                    }
                }
                "closed_at" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.closed_at = v;
                    }
                }
                "operator_user_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.operator_user_id = v;
                    }
                }
                "chatbot_current_step_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.chatbot_current_step_id = v;
                    }
                }
                "expertise_names" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.expertise_names = v;
                    }
                }
                "website_visitor_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.website_visitor_id = v;
                    }
                }
                "visitor_country_code" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.visitor_country_code = v;
                    }
                }
                "visitor_timezone" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.visitor_timezone = v;
                    }
                }
                "is_pending_request" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.is_pending_request = v;
                    }
                }
                "visitor_language" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.visitor_language = v;
                    }
                }
                "message_count" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.message_count = v;
                    }
                }
                "first_response_at" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.first_response_at = v;
                    }
                }
                "last_interest_at" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.last_interest_at = v;
                    }
                }
                "last_visitor_message_at" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.last_visitor_message_at = v;
                    }
                }
                "last_operator_message_at" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.last_operator_message_at = v;
                    }
                }
                "is_test" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.is_test = v;
                    }
                }
                "error_detail" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.error_detail = v;
                    }
                }
                "company_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.company_id = v;
                    }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for Session {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "Session"
    }
}

impl backbone_core::PersistentEntity for Session {
    fn entity_id(&self) -> String {
        self.id.to_string()
    }
    fn set_entity_id(&mut self, id: String) {
        if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
            self.id = uuid;
        }
    }
    fn created_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.created_at
    }
    fn set_created_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.created_at = Some(ts);
    }
    fn updated_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.updated_at
    }
    fn set_updated_at(&mut self, ts: chrono::DateTime<chrono::Utc>) {
        self.metadata.updated_at = Some(ts);
    }
    fn deleted_at(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.metadata.deleted_at
    }
    fn set_deleted_at(&mut self, ts: Option<chrono::DateTime<chrono::Utc>>) {
        self.metadata.deleted_at = ts;
    }
}

impl backbone_orm::EntityRepoMeta for Session {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("channel_id".to_string(), "uuid".to_string());
        m.insert("operator_user_id".to_string(), "uuid".to_string());
        m.insert("chatbot_current_step_id".to_string(), "uuid".to_string());
        m.insert("website_visitor_id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m.insert("status".to_string(), "livechat_session_status".to_string());
        m.insert("failure".to_string(), "livechat_failure".to_string());
        m.insert(
            "outcome".to_string(),
            "livechat_session_outcome".to_string(),
        );
        m.insert(
            "close_reason".to_string(),
            "livechat_close_reason".to_string(),
        );
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
    fn company_field() -> Option<&'static str> {
        Some("company_id")
    }
    fn relations() -> &'static [(&'static str, &'static str, &'static str)] {
        &[("channel", "channels", "channelId")]
    }
}

/// Builder for Session entity
///
/// Provides a fluent API for constructing Session instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct SessionBuilder {
    channel_id: Option<Uuid>,
    title: Option<String>,
    status: Option<LivechatSessionStatus>,
    failure: Option<LivechatFailure>,
    outcome: Option<LivechatSessionOutcome>,
    close_reason: Option<LivechatCloseReason>,
    closed_at: Option<DateTime<Utc>>,
    operator_user_id: Option<Uuid>,
    chatbot_current_step_id: Option<Uuid>,
    expertise_names: Option<Vec<String>>,
    website_visitor_id: Option<Uuid>,
    visitor_country_code: Option<String>,
    visitor_timezone: Option<String>,
    is_pending_request: Option<bool>,
    visitor_language: Option<String>,
    message_count: Option<i32>,
    first_response_at: Option<DateTime<Utc>>,
    last_interest_at: Option<DateTime<Utc>>,
    last_visitor_message_at: Option<DateTime<Utc>>,
    last_operator_message_at: Option<DateTime<Utc>>,
    is_test: Option<bool>,
    error_detail: Option<String>,
    company_id: Option<Uuid>,
}

impl SessionBuilder {
    /// Set the channel_id field (required)
    pub fn channel_id(mut self, value: Uuid) -> Self {
        self.channel_id = Some(value);
        self
    }

    /// Set the title field (optional)
    pub fn title(mut self, value: String) -> Self {
        self.title = Some(value);
        self
    }

    /// Set the status field (default: `Default::default()`)
    pub fn status(mut self, value: LivechatSessionStatus) -> Self {
        self.status = Some(value);
        self
    }

    /// Set the failure field (default: `LivechatFailure::default()`)
    pub fn failure(mut self, value: LivechatFailure) -> Self {
        self.failure = Some(value);
        self
    }

    /// Set the outcome field (optional)
    pub fn outcome(mut self, value: LivechatSessionOutcome) -> Self {
        self.outcome = Some(value);
        self
    }

    /// Set the close_reason field (optional)
    pub fn close_reason(mut self, value: LivechatCloseReason) -> Self {
        self.close_reason = Some(value);
        self
    }

    /// Set the closed_at field (optional)
    pub fn closed_at(mut self, value: DateTime<Utc>) -> Self {
        self.closed_at = Some(value);
        self
    }

    /// Set the operator_user_id field (optional)
    pub fn operator_user_id(mut self, value: Uuid) -> Self {
        self.operator_user_id = Some(value);
        self
    }

    /// Set the chatbot_current_step_id field (optional)
    pub fn chatbot_current_step_id(mut self, value: Uuid) -> Self {
        self.chatbot_current_step_id = Some(value);
        self
    }

    /// Set the expertise_names field (required)
    pub fn expertise_names(mut self, value: Vec<String>) -> Self {
        self.expertise_names = Some(value);
        self
    }

    /// Set the website_visitor_id field (optional)
    pub fn website_visitor_id(mut self, value: Uuid) -> Self {
        self.website_visitor_id = Some(value);
        self
    }

    /// Set the visitor_country_code field (optional)
    pub fn visitor_country_code(mut self, value: String) -> Self {
        self.visitor_country_code = Some(value);
        self
    }

    /// Set the visitor_timezone field (optional)
    pub fn visitor_timezone(mut self, value: String) -> Self {
        self.visitor_timezone = Some(value);
        self
    }

    /// Set the is_pending_request field (default: `false`)
    pub fn is_pending_request(mut self, value: bool) -> Self {
        self.is_pending_request = Some(value);
        self
    }

    /// Set the visitor_language field (optional)
    pub fn visitor_language(mut self, value: String) -> Self {
        self.visitor_language = Some(value);
        self
    }

    /// Set the message_count field (default: `0`)
    pub fn message_count(mut self, value: i32) -> Self {
        self.message_count = Some(value);
        self
    }

    /// Set the first_response_at field (optional)
    pub fn first_response_at(mut self, value: DateTime<Utc>) -> Self {
        self.first_response_at = Some(value);
        self
    }

    /// Set the last_interest_at field (default: `Utc::now()`)
    pub fn last_interest_at(mut self, value: DateTime<Utc>) -> Self {
        self.last_interest_at = Some(value);
        self
    }

    /// Set the last_visitor_message_at field (optional)
    pub fn last_visitor_message_at(mut self, value: DateTime<Utc>) -> Self {
        self.last_visitor_message_at = Some(value);
        self
    }

    /// Set the last_operator_message_at field (optional)
    pub fn last_operator_message_at(mut self, value: DateTime<Utc>) -> Self {
        self.last_operator_message_at = Some(value);
        self
    }

    /// Set the is_test field (default: `false`)
    pub fn is_test(mut self, value: bool) -> Self {
        self.is_test = Some(value);
        self
    }

    /// Set the error_detail field (optional)
    pub fn error_detail(mut self, value: String) -> Self {
        self.error_detail = Some(value);
        self
    }

    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

    /// Build the Session entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<Session, String> {
        let channel_id = self
            .channel_id
            .ok_or_else(|| "channel_id is required".to_string())?;
        let expertise_names = self
            .expertise_names
            .ok_or_else(|| "expertise_names is required".to_string())?;
        let company_id = self
            .company_id
            .ok_or_else(|| "company_id is required".to_string())?;

        Ok(Session {
            id: Uuid::new_v4(),
            channel_id,
            title: self.title,
            status: self.status,
            failure: self.failure.unwrap_or_default(),
            outcome: self.outcome,
            close_reason: self.close_reason,
            closed_at: self.closed_at,
            operator_user_id: self.operator_user_id,
            chatbot_current_step_id: self.chatbot_current_step_id,
            expertise_names,
            website_visitor_id: self.website_visitor_id,
            visitor_country_code: self.visitor_country_code,
            visitor_timezone: self.visitor_timezone,
            is_pending_request: self.is_pending_request.unwrap_or(false),
            visitor_language: self.visitor_language,
            message_count: self.message_count.unwrap_or(0),
            first_response_at: self.first_response_at,
            last_interest_at: self.last_interest_at.unwrap_or(Utc::now()),
            last_visitor_message_at: self.last_visitor_message_at,
            last_operator_message_at: self.last_operator_message_at,
            is_test: self.is_test.unwrap_or(false),
            error_detail: self.error_detail,
            company_id,
            metadata: AuditMetadata::default(),
        })
    }
}
