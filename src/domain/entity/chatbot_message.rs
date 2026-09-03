use super::AuditMetadata;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Strongly-typed ID for ChatbotMessage
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChatbotMessageId(pub Uuid);

impl ChatbotMessageId {
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

impl std::fmt::Display for ChatbotMessageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ChatbotMessageId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for ChatbotMessageId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<ChatbotMessageId> for Uuid {
    fn from(id: ChatbotMessageId) -> Self {
        id.0
    }
}

impl AsRef<Uuid> for ChatbotMessageId {
    fn as_ref(&self) -> &Uuid {
        &self.0
    }
}

impl std::ops::Deref for ChatbotMessageId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChatbotMessage {
    pub id: Uuid,
    pub session_id: Uuid,
    pub step_id: Option<Uuid>,
    pub carrier_message_id: Option<String>,
    pub selected_answer_id: Option<Uuid>,
    pub visitor_answer: Option<String>,
    pub created_at: DateTime<Utc>,
    pub company_id: Uuid,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl ChatbotMessage {
    /// Create a builder for ChatbotMessage
    pub fn builder() -> ChatbotMessageBuilder {
        <ChatbotMessageBuilder as Default>::default()
    }

    /// Create a new ChatbotMessage with required fields
    pub fn new(session_id: Uuid, company_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            step_id: None,
            carrier_message_id: None,
            selected_answer_id: None,
            visitor_answer: None,
            created_at: Utc::now(),
            company_id,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> ChatbotMessageId {
        ChatbotMessageId(self.id)
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

    // ==========================================================
    // Fluent Setters (with_* for optional fields)
    // ==========================================================

    /// Set the step_id field (chainable)
    pub fn with_step_id(mut self, value: Uuid) -> Self {
        self.step_id = Some(value);
        self
    }

    /// Set the carrier_message_id field (chainable)
    pub fn with_carrier_message_id(mut self, value: String) -> Self {
        self.carrier_message_id = Some(value);
        self
    }

    /// Set the selected_answer_id field (chainable)
    pub fn with_selected_answer_id(mut self, value: Uuid) -> Self {
        self.selected_answer_id = Some(value);
        self
    }

    /// Set the visitor_answer field (chainable)
    pub fn with_visitor_answer(mut self, value: String) -> Self {
        self.visitor_answer = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "session_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.session_id = v;
                    }
                }
                "step_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.step_id = v;
                    }
                }
                "carrier_message_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.carrier_message_id = v;
                    }
                }
                "selected_answer_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.selected_answer_id = v;
                    }
                }
                "visitor_answer" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.visitor_answer = v;
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

impl super::Entity for ChatbotMessage {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "ChatbotMessage"
    }
}

impl backbone_core::PersistentEntity for ChatbotMessage {
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

impl backbone_orm::EntityRepoMeta for ChatbotMessage {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("session_id".to_string(), "uuid".to_string());
        m.insert("step_id".to_string(), "uuid".to_string());
        m.insert("selected_answer_id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
    fn company_field() -> Option<&'static str> {
        Some("company_id")
    }
    fn relations() -> &'static [(&'static str, &'static str, &'static str)] {
        &[
            ("session", "sessions", "sessionId"),
            ("step", "chatbot_steps", "stepId"),
        ]
    }
}

/// Builder for ChatbotMessage entity
///
/// Provides a fluent API for constructing ChatbotMessage instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct ChatbotMessageBuilder {
    session_id: Option<Uuid>,
    step_id: Option<Uuid>,
    carrier_message_id: Option<String>,
    selected_answer_id: Option<Uuid>,
    visitor_answer: Option<String>,
    company_id: Option<Uuid>,
}

impl ChatbotMessageBuilder {
    /// Set the session_id field (required)
    pub fn session_id(mut self, value: Uuid) -> Self {
        self.session_id = Some(value);
        self
    }

    /// Set the step_id field (optional)
    pub fn step_id(mut self, value: Uuid) -> Self {
        self.step_id = Some(value);
        self
    }

    /// Set the carrier_message_id field (optional)
    pub fn carrier_message_id(mut self, value: String) -> Self {
        self.carrier_message_id = Some(value);
        self
    }

    /// Set the selected_answer_id field (optional)
    pub fn selected_answer_id(mut self, value: Uuid) -> Self {
        self.selected_answer_id = Some(value);
        self
    }

    /// Set the visitor_answer field (optional)
    pub fn visitor_answer(mut self, value: String) -> Self {
        self.visitor_answer = Some(value);
        self
    }

    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

    /// Build the ChatbotMessage entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<ChatbotMessage, String> {
        let session_id = self
            .session_id
            .ok_or_else(|| "session_id is required".to_string())?;
        let company_id = self
            .company_id
            .ok_or_else(|| "company_id is required".to_string())?;

        Ok(ChatbotMessage {
            id: Uuid::new_v4(),
            session_id,
            step_id: self.step_id,
            carrier_message_id: self.carrier_message_id,
            selected_answer_id: self.selected_answer_id,
            visitor_answer: self.visitor_answer,
            created_at: Utc::now(),
            company_id,
            metadata: AuditMetadata::default(),
        })
    }
}
