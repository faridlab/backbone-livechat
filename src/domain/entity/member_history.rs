use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::LivechatPersona;
use super::AuditMetadata;

/// Strongly-typed ID for MemberHistory
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MemberHistoryId(pub Uuid);

impl MemberHistoryId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for MemberHistoryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for MemberHistoryId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for MemberHistoryId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<MemberHistoryId> for Uuid {
    fn from(id: MemberHistoryId) -> Self { id.0 }
}

impl AsRef<Uuid> for MemberHistoryId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for MemberHistoryId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MemberHistory {
    pub id: Uuid,
    pub session_id: Uuid,
    pub persona: LivechatPersona,
    pub operator_user_id: Option<Uuid>,
    pub visitor_key: Option<String>,
    pub chatbot_script_id: Option<Uuid>,
    pub joined_at: DateTime<Utc>,
    pub left_at: Option<DateTime<Utc>>,
    pub message_count: i32,
    pub response_time_secs: Option<i32>,
    pub expertise_names: Vec<String>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl MemberHistory {
    /// Create a builder for MemberHistory
    pub fn builder() -> MemberHistoryBuilder {
        <MemberHistoryBuilder as Default>::default()
    }

    /// Create a new MemberHistory with required fields
    pub fn new(session_id: Uuid, persona: LivechatPersona, joined_at: DateTime<Utc>, message_count: i32, expertise_names: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            persona,
            operator_user_id: None,
            visitor_key: None,
            chatbot_script_id: None,
            joined_at,
            left_at: None,
            message_count,
            response_time_secs: None,
            expertise_names,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> MemberHistoryId {
        MemberHistoryId(self.id)
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

    /// Set the operator_user_id field (chainable)
    pub fn with_operator_user_id(mut self, value: Uuid) -> Self {
        self.operator_user_id = Some(value);
        self
    }

    /// Set the visitor_key field (chainable)
    pub fn with_visitor_key(mut self, value: String) -> Self {
        self.visitor_key = Some(value);
        self
    }

    /// Set the chatbot_script_id field (chainable)
    pub fn with_chatbot_script_id(mut self, value: Uuid) -> Self {
        self.chatbot_script_id = Some(value);
        self
    }

    /// Set the left_at field (chainable)
    pub fn with_left_at(mut self, value: DateTime<Utc>) -> Self {
        self.left_at = Some(value);
        self
    }

    /// Set the response_time_secs field (chainable)
    pub fn with_response_time_secs(mut self, value: i32) -> Self {
        self.response_time_secs = Some(value);
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
                    if let Ok(v) = serde_json::from_value(value) { self.session_id = v; }
                }
                "persona" => {
                    if let Ok(v) = serde_json::from_value(value) { self.persona = v; }
                }
                "operator_user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.operator_user_id = v; }
                }
                "visitor_key" => {
                    if let Ok(v) = serde_json::from_value(value) { self.visitor_key = v; }
                }
                "chatbot_script_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.chatbot_script_id = v; }
                }
                "joined_at" => {
                    if let Ok(v) = serde_json::from_value(value) { self.joined_at = v; }
                }
                "left_at" => {
                    if let Ok(v) = serde_json::from_value(value) { self.left_at = v; }
                }
                "message_count" => {
                    if let Ok(v) = serde_json::from_value(value) { self.message_count = v; }
                }
                "response_time_secs" => {
                    if let Ok(v) = serde_json::from_value(value) { self.response_time_secs = v; }
                }
                "expertise_names" => {
                    if let Ok(v) = serde_json::from_value(value) { self.expertise_names = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for MemberHistory {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "MemberHistory"
    }
}

impl backbone_core::PersistentEntity for MemberHistory {
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

impl backbone_orm::EntityRepoMeta for MemberHistory {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("session_id".to_string(), "uuid".to_string());
        m.insert("operator_user_id".to_string(), "uuid".to_string());
        m.insert("chatbot_script_id".to_string(), "uuid".to_string());
        m.insert("persona".to_string(), "livechat_persona".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
    fn relations() -> &'static [(&'static str, &'static str, &'static str)] {
        &[("session", "sessions", "sessionId")]
    }
}

/// Builder for MemberHistory entity
///
/// Provides a fluent API for constructing MemberHistory instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct MemberHistoryBuilder {
    session_id: Option<Uuid>,
    persona: Option<LivechatPersona>,
    operator_user_id: Option<Uuid>,
    visitor_key: Option<String>,
    chatbot_script_id: Option<Uuid>,
    joined_at: Option<DateTime<Utc>>,
    left_at: Option<DateTime<Utc>>,
    message_count: Option<i32>,
    response_time_secs: Option<i32>,
    expertise_names: Option<Vec<String>>,
}

impl MemberHistoryBuilder {
    /// Set the session_id field (required)
    pub fn session_id(mut self, value: Uuid) -> Self {
        self.session_id = Some(value);
        self
    }

    /// Set the persona field (required)
    pub fn persona(mut self, value: LivechatPersona) -> Self {
        self.persona = Some(value);
        self
    }

    /// Set the operator_user_id field (optional)
    pub fn operator_user_id(mut self, value: Uuid) -> Self {
        self.operator_user_id = Some(value);
        self
    }

    /// Set the visitor_key field (optional)
    pub fn visitor_key(mut self, value: String) -> Self {
        self.visitor_key = Some(value);
        self
    }

    /// Set the chatbot_script_id field (optional)
    pub fn chatbot_script_id(mut self, value: Uuid) -> Self {
        self.chatbot_script_id = Some(value);
        self
    }

    /// Set the joined_at field (default: `Utc::now()`)
    pub fn joined_at(mut self, value: DateTime<Utc>) -> Self {
        self.joined_at = Some(value);
        self
    }

    /// Set the left_at field (optional)
    pub fn left_at(mut self, value: DateTime<Utc>) -> Self {
        self.left_at = Some(value);
        self
    }

    /// Set the message_count field (default: `0`)
    pub fn message_count(mut self, value: i32) -> Self {
        self.message_count = Some(value);
        self
    }

    /// Set the response_time_secs field (optional)
    pub fn response_time_secs(mut self, value: i32) -> Self {
        self.response_time_secs = Some(value);
        self
    }

    /// Set the expertise_names field (required)
    pub fn expertise_names(mut self, value: Vec<String>) -> Self {
        self.expertise_names = Some(value);
        self
    }

    /// Build the MemberHistory entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<MemberHistory, String> {
        let session_id = self.session_id.ok_or_else(|| "session_id is required".to_string())?;
        let persona = self.persona.ok_or_else(|| "persona is required".to_string())?;
        let expertise_names = self.expertise_names.ok_or_else(|| "expertise_names is required".to_string())?;

        Ok(MemberHistory {
            id: Uuid::new_v4(),
            session_id,
            persona,
            operator_user_id: self.operator_user_id,
            visitor_key: self.visitor_key,
            chatbot_script_id: self.chatbot_script_id,
            joined_at: self.joined_at.unwrap_or(Utc::now()),
            left_at: self.left_at,
            message_count: self.message_count.unwrap_or(0),
            response_time_secs: self.response_time_secs,
            expertise_names,
            metadata: AuditMetadata::default(),
        })
    }
}
