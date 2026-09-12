use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::LivechatMaxSessionsMode;
use super::AuditMetadata;

/// Strongly-typed ID for Channel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChannelId(pub Uuid);

impl ChannelId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for ChannelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ChannelId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for ChannelId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<ChannelId> for Uuid {
    fn from(id: ChannelId) -> Self { id.0 }
}

impl AsRef<Uuid> for ChannelId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for ChannelId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Channel {
    pub id: Uuid,
    pub name: String,
    pub website_id: Option<Uuid>,
    pub button_text: Option<String>,
    pub welcome_message: Option<String>,
    pub max_sessions_mode: LivechatMaxSessionsMode,
    pub max_sessions: i32,
    pub block_assignment_during_call: bool,
    pub review_link: Option<String>,
    pub is_active: bool,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl Channel {
    /// Create a builder for Channel
    pub fn builder() -> ChannelBuilder {
        <ChannelBuilder as Default>::default()
    }

    /// Create a new Channel with required fields
    pub fn new(name: String, max_sessions_mode: LivechatMaxSessionsMode, max_sessions: i32, block_assignment_during_call: bool, is_active: bool) -> Self {
        Self {
            id: Uuid::new_v4(),
            name,
            website_id: None,
            button_text: None,
            welcome_message: None,
            max_sessions_mode,
            max_sessions,
            block_assignment_during_call,
            review_link: None,
            is_active,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> ChannelId {
        ChannelId(self.id)
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

    /// Set the website_id field (chainable)
    pub fn with_website_id(mut self, value: Uuid) -> Self {
        self.website_id = Some(value);
        self
    }

    /// Set the button_text field (chainable)
    pub fn with_button_text(mut self, value: String) -> Self {
        self.button_text = Some(value);
        self
    }

    /// Set the welcome_message field (chainable)
    pub fn with_welcome_message(mut self, value: String) -> Self {
        self.welcome_message = Some(value);
        self
    }

    /// Set the review_link field (chainable)
    pub fn with_review_link(mut self, value: String) -> Self {
        self.review_link = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "name" => {
                    if let Ok(v) = serde_json::from_value(value) { self.name = v; }
                }
                "website_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.website_id = v; }
                }
                "button_text" => {
                    if let Ok(v) = serde_json::from_value(value) { self.button_text = v; }
                }
                "welcome_message" => {
                    if let Ok(v) = serde_json::from_value(value) { self.welcome_message = v; }
                }
                "max_sessions_mode" => {
                    if let Ok(v) = serde_json::from_value(value) { self.max_sessions_mode = v; }
                }
                "max_sessions" => {
                    if let Ok(v) = serde_json::from_value(value) { self.max_sessions = v; }
                }
                "block_assignment_during_call" => {
                    if let Ok(v) = serde_json::from_value(value) { self.block_assignment_during_call = v; }
                }
                "review_link" => {
                    if let Ok(v) = serde_json::from_value(value) { self.review_link = v; }
                }
                "is_active" => {
                    if let Ok(v) = serde_json::from_value(value) { self.is_active = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for Channel {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "Channel"
    }
}

impl backbone_core::PersistentEntity for Channel {
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

impl backbone_orm::EntityRepoMeta for Channel {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("website_id".to_string(), "uuid".to_string());
        m.insert("max_sessions_mode".to_string(), "livechat_max_sessions_mode".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["name"]
    }
}

/// Builder for Channel entity
///
/// Provides a fluent API for constructing Channel instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct ChannelBuilder {
    name: Option<String>,
    website_id: Option<Uuid>,
    button_text: Option<String>,
    welcome_message: Option<String>,
    max_sessions_mode: Option<LivechatMaxSessionsMode>,
    max_sessions: Option<i32>,
    block_assignment_during_call: Option<bool>,
    review_link: Option<String>,
    is_active: Option<bool>,
}

impl ChannelBuilder {
    /// Set the name field (required)
    pub fn name(mut self, value: String) -> Self {
        self.name = Some(value);
        self
    }

    /// Set the website_id field (optional)
    pub fn website_id(mut self, value: Uuid) -> Self {
        self.website_id = Some(value);
        self
    }

    /// Set the button_text field (optional)
    pub fn button_text(mut self, value: String) -> Self {
        self.button_text = Some(value);
        self
    }

    /// Set the welcome_message field (optional)
    pub fn welcome_message(mut self, value: String) -> Self {
        self.welcome_message = Some(value);
        self
    }

    /// Set the max_sessions_mode field (default: `LivechatMaxSessionsMode::default()`)
    pub fn max_sessions_mode(mut self, value: LivechatMaxSessionsMode) -> Self {
        self.max_sessions_mode = Some(value);
        self
    }

    /// Set the max_sessions field (default: `1`)
    pub fn max_sessions(mut self, value: i32) -> Self {
        self.max_sessions = Some(value);
        self
    }

    /// Set the block_assignment_during_call field (default: `false`)
    pub fn block_assignment_during_call(mut self, value: bool) -> Self {
        self.block_assignment_during_call = Some(value);
        self
    }

    /// Set the review_link field (optional)
    pub fn review_link(mut self, value: String) -> Self {
        self.review_link = Some(value);
        self
    }

    /// Set the is_active field (default: `true`)
    pub fn is_active(mut self, value: bool) -> Self {
        self.is_active = Some(value);
        self
    }

    /// Build the Channel entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<Channel, String> {
        let name = self.name.ok_or_else(|| "name is required".to_string())?;

        Ok(Channel {
            id: Uuid::new_v4(),
            name,
            website_id: self.website_id,
            button_text: self.button_text,
            welcome_message: self.welcome_message,
            max_sessions_mode: self.max_sessions_mode.unwrap_or_default(),
            max_sessions: self.max_sessions.unwrap_or(1),
            block_assignment_during_call: self.block_assignment_during_call.unwrap_or(false),
            review_link: self.review_link,
            is_active: self.is_active.unwrap_or(true),
            metadata: AuditMetadata::default(),
        })
    }
}
