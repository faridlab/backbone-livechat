use super::AuditMetadata;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Strongly-typed ID for ChatbotAnswer
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChatbotAnswerId(pub Uuid);

impl ChatbotAnswerId {
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

impl std::fmt::Display for ChatbotAnswerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ChatbotAnswerId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for ChatbotAnswerId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<ChatbotAnswerId> for Uuid {
    fn from(id: ChatbotAnswerId) -> Self {
        id.0
    }
}

impl AsRef<Uuid> for ChatbotAnswerId {
    fn as_ref(&self) -> &Uuid {
        &self.0
    }
}

impl std::ops::Deref for ChatbotAnswerId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChatbotAnswer {
    pub id: Uuid,
    pub question_step_id: Uuid,
    pub sequence: i32,
    pub label: String,
    pub redirect_url: Option<String>,
    pub company_id: Uuid,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl ChatbotAnswer {
    /// Create a builder for ChatbotAnswer
    pub fn builder() -> ChatbotAnswerBuilder {
        <ChatbotAnswerBuilder as Default>::default()
    }

    /// Create a new ChatbotAnswer with required fields
    pub fn new(question_step_id: Uuid, sequence: i32, label: String, company_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            question_step_id,
            sequence,
            label,
            redirect_url: None,
            company_id,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> ChatbotAnswerId {
        ChatbotAnswerId(self.id)
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

    /// Set the redirect_url field (chainable)
    pub fn with_redirect_url(mut self, value: String) -> Self {
        self.redirect_url = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "question_step_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.question_step_id = v;
                    }
                }
                "sequence" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.sequence = v;
                    }
                }
                "label" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.label = v;
                    }
                }
                "redirect_url" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.redirect_url = v;
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

impl super::Entity for ChatbotAnswer {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "ChatbotAnswer"
    }
}

impl backbone_core::PersistentEntity for ChatbotAnswer {
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

impl backbone_orm::EntityRepoMeta for ChatbotAnswer {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("question_step_id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["label"]
    }
    fn company_field() -> Option<&'static str> {
        Some("company_id")
    }
    fn relations() -> &'static [(&'static str, &'static str, &'static str)] {
        &[("questionStep", "chatbot_steps", "questionStepId")]
    }
}

/// Builder for ChatbotAnswer entity
///
/// Provides a fluent API for constructing ChatbotAnswer instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct ChatbotAnswerBuilder {
    question_step_id: Option<Uuid>,
    sequence: Option<i32>,
    label: Option<String>,
    redirect_url: Option<String>,
    company_id: Option<Uuid>,
}

impl ChatbotAnswerBuilder {
    /// Set the question_step_id field (required)
    pub fn question_step_id(mut self, value: Uuid) -> Self {
        self.question_step_id = Some(value);
        self
    }

    /// Set the sequence field (required)
    pub fn sequence(mut self, value: i32) -> Self {
        self.sequence = Some(value);
        self
    }

    /// Set the label field (required)
    pub fn label(mut self, value: String) -> Self {
        self.label = Some(value);
        self
    }

    /// Set the redirect_url field (optional)
    pub fn redirect_url(mut self, value: String) -> Self {
        self.redirect_url = Some(value);
        self
    }

    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

    /// Build the ChatbotAnswer entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<ChatbotAnswer, String> {
        let question_step_id = self
            .question_step_id
            .ok_or_else(|| "question_step_id is required".to_string())?;
        let sequence = self
            .sequence
            .ok_or_else(|| "sequence is required".to_string())?;
        let label = self.label.ok_or_else(|| "label is required".to_string())?;
        let company_id = self
            .company_id
            .ok_or_else(|| "company_id is required".to_string())?;

        Ok(ChatbotAnswer {
            id: Uuid::new_v4(),
            question_step_id,
            sequence,
            label,
            redirect_url: self.redirect_url,
            company_id,
            metadata: AuditMetadata::default(),
        })
    }
}
