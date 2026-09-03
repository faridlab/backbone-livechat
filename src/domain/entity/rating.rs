use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::AuditMetadata;
use super::LivechatRatedPersona;

/// Strongly-typed ID for Rating
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RatingId(pub Uuid);

impl RatingId {
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

impl std::fmt::Display for RatingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for RatingId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for RatingId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<RatingId> for Uuid {
    fn from(id: RatingId) -> Self {
        id.0
    }
}

impl AsRef<Uuid> for RatingId {
    fn as_ref(&self) -> &Uuid {
        &self.0
    }
}

impl std::ops::Deref for RatingId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Rating {
    pub id: Uuid,
    pub session_id: Uuid,
    pub value: i32,
    pub rated_persona: LivechatRatedPersona,
    pub operator_user_id: Option<Uuid>,
    pub chatbot_script_id: Option<Uuid>,
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
    pub company_id: Uuid,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl Rating {
    /// Create a builder for Rating
    pub fn builder() -> RatingBuilder {
        <RatingBuilder as Default>::default()
    }

    /// Create a new Rating with required fields
    pub fn new(
        session_id: Uuid,
        value: i32,
        rated_persona: LivechatRatedPersona,
        company_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            session_id,
            value,
            rated_persona,
            operator_user_id: None,
            chatbot_script_id: None,
            comment: None,
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
    pub fn typed_id(&self) -> RatingId {
        RatingId(self.id)
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

    /// Set the chatbot_script_id field (chainable)
    pub fn with_chatbot_script_id(mut self, value: Uuid) -> Self {
        self.chatbot_script_id = Some(value);
        self
    }

    /// Set the comment field (chainable)
    pub fn with_comment(mut self, value: String) -> Self {
        self.comment = Some(value);
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
                "value" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.value = v;
                    }
                }
                "rated_persona" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.rated_persona = v;
                    }
                }
                "operator_user_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.operator_user_id = v;
                    }
                }
                "chatbot_script_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.chatbot_script_id = v;
                    }
                }
                "comment" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.comment = v;
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

impl super::Entity for Rating {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "Rating"
    }
}

impl backbone_core::PersistentEntity for Rating {
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

impl backbone_orm::EntityRepoMeta for Rating {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("session_id".to_string(), "uuid".to_string());
        m.insert("operator_user_id".to_string(), "uuid".to_string());
        m.insert("chatbot_script_id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m.insert(
            "rated_persona".to_string(),
            "livechat_rated_persona".to_string(),
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
        &[("session", "sessions", "sessionId")]
    }
}

/// Builder for Rating entity
///
/// Provides a fluent API for constructing Rating instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct RatingBuilder {
    session_id: Option<Uuid>,
    value: Option<i32>,
    rated_persona: Option<LivechatRatedPersona>,
    operator_user_id: Option<Uuid>,
    chatbot_script_id: Option<Uuid>,
    comment: Option<String>,
    company_id: Option<Uuid>,
}

impl RatingBuilder {
    /// Set the session_id field (required)
    pub fn session_id(mut self, value: Uuid) -> Self {
        self.session_id = Some(value);
        self
    }

    /// Set the value field (required)
    pub fn value(mut self, value: i32) -> Self {
        self.value = Some(value);
        self
    }

    /// Set the rated_persona field (default: `LivechatRatedPersona::default()`)
    pub fn rated_persona(mut self, value: LivechatRatedPersona) -> Self {
        self.rated_persona = Some(value);
        self
    }

    /// Set the operator_user_id field (optional)
    pub fn operator_user_id(mut self, value: Uuid) -> Self {
        self.operator_user_id = Some(value);
        self
    }

    /// Set the chatbot_script_id field (optional)
    pub fn chatbot_script_id(mut self, value: Uuid) -> Self {
        self.chatbot_script_id = Some(value);
        self
    }

    /// Set the comment field (optional)
    pub fn comment(mut self, value: String) -> Self {
        self.comment = Some(value);
        self
    }

    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

    /// Build the Rating entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<Rating, String> {
        let session_id = self
            .session_id
            .ok_or_else(|| "session_id is required".to_string())?;
        let value = self.value.ok_or_else(|| "value is required".to_string())?;
        let company_id = self
            .company_id
            .ok_or_else(|| "company_id is required".to_string())?;

        Ok(Rating {
            id: Uuid::new_v4(),
            session_id,
            value,
            rated_persona: self.rated_persona.unwrap_or_default(),
            operator_user_id: self.operator_user_id,
            chatbot_script_id: self.chatbot_script_id,
            comment: self.comment,
            created_at: Utc::now(),
            company_id,
            metadata: AuditMetadata::default(),
        })
    }
}
