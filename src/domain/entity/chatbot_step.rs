use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::LivechatStepType;
use super::AuditMetadata;

/// Strongly-typed ID for ChatbotStep
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChatbotStepId(pub Uuid);

impl ChatbotStepId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for ChatbotStepId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ChatbotStepId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for ChatbotStepId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<ChatbotStepId> for Uuid {
    fn from(id: ChatbotStepId) -> Self { id.0 }
}

impl AsRef<Uuid> for ChatbotStepId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for ChatbotStepId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChatbotStep {
    pub id: Uuid,
    pub chatbot_script_id: Uuid,
    pub sequence: i32,
    pub step_type: LivechatStepType,
    pub message: Option<String>,
    pub expertise_tag_ids: Vec<Uuid>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl ChatbotStep {
    /// Create a builder for ChatbotStep
    pub fn builder() -> ChatbotStepBuilder {
        <ChatbotStepBuilder as Default>::default()
    }

    /// Create a new ChatbotStep with required fields
    pub fn new(chatbot_script_id: Uuid, sequence: i32, step_type: LivechatStepType, expertise_tag_ids: Vec<Uuid>) -> Self {
        Self {
            id: Uuid::new_v4(),
            chatbot_script_id,
            sequence,
            step_type,
            message: None,
            expertise_tag_ids,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> ChatbotStepId {
        ChatbotStepId(self.id)
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

    /// Set the message field (chainable)
    pub fn with_message(mut self, value: String) -> Self {
        self.message = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "chatbot_script_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.chatbot_script_id = v; }
                }
                "sequence" => {
                    if let Ok(v) = serde_json::from_value(value) { self.sequence = v; }
                }
                "step_type" => {
                    if let Ok(v) = serde_json::from_value(value) { self.step_type = v; }
                }
                "message" => {
                    if let Ok(v) = serde_json::from_value(value) { self.message = v; }
                }
                "expertise_tag_ids" => {
                    if let Ok(v) = serde_json::from_value(value) { self.expertise_tag_ids = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for ChatbotStep {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "ChatbotStep"
    }
}

impl backbone_core::PersistentEntity for ChatbotStep {
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

impl backbone_orm::EntityRepoMeta for ChatbotStep {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("chatbot_script_id".to_string(), "uuid".to_string());
        m.insert("step_type".to_string(), "livechat_step_type".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
    fn relations() -> &'static [(&'static str, &'static str, &'static str)] {
        &[("chatbotScript", "chatbot_scripts", "chatbotScriptId")]
    }
}

/// Builder for ChatbotStep entity
///
/// Provides a fluent API for constructing ChatbotStep instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct ChatbotStepBuilder {
    chatbot_script_id: Option<Uuid>,
    sequence: Option<i32>,
    step_type: Option<LivechatStepType>,
    message: Option<String>,
    expertise_tag_ids: Option<Vec<Uuid>>,
}

impl ChatbotStepBuilder {
    /// Set the chatbot_script_id field (required)
    pub fn chatbot_script_id(mut self, value: Uuid) -> Self {
        self.chatbot_script_id = Some(value);
        self
    }

    /// Set the sequence field (required)
    pub fn sequence(mut self, value: i32) -> Self {
        self.sequence = Some(value);
        self
    }

    /// Set the step_type field (default: `LivechatStepType::default()`)
    pub fn step_type(mut self, value: LivechatStepType) -> Self {
        self.step_type = Some(value);
        self
    }

    /// Set the message field (optional)
    pub fn message(mut self, value: String) -> Self {
        self.message = Some(value);
        self
    }

    /// Set the expertise_tag_ids field (required)
    pub fn expertise_tag_ids(mut self, value: Vec<Uuid>) -> Self {
        self.expertise_tag_ids = Some(value);
        self
    }

    /// Build the ChatbotStep entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<ChatbotStep, String> {
        let chatbot_script_id = self.chatbot_script_id.ok_or_else(|| "chatbot_script_id is required".to_string())?;
        let sequence = self.sequence.ok_or_else(|| "sequence is required".to_string())?;
        let expertise_tag_ids = self.expertise_tag_ids.ok_or_else(|| "expertise_tag_ids is required".to_string())?;

        Ok(ChatbotStep {
            id: Uuid::new_v4(),
            chatbot_script_id,
            sequence,
            step_type: self.step_type.unwrap_or_default(),
            message: self.message,
            expertise_tag_ids,
            metadata: AuditMetadata::default(),
        })
    }
}
