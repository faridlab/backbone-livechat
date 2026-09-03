use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::AuditMetadata;
use super::LivechatAuditEvent;

/// Strongly-typed ID for LivechatAuditLog
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LivechatAuditLogId(pub Uuid);

impl LivechatAuditLogId {
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

impl std::fmt::Display for LivechatAuditLogId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for LivechatAuditLogId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for LivechatAuditLogId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<LivechatAuditLogId> for Uuid {
    fn from(id: LivechatAuditLogId) -> Self {
        id.0
    }
}

impl AsRef<Uuid> for LivechatAuditLogId {
    fn as_ref(&self) -> &Uuid {
        &self.0
    }
}

impl std::ops::Deref for LivechatAuditLogId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct LivechatAuditLog {
    pub id: Uuid,
    pub event: LivechatAuditEvent,
    pub actor: Option<Uuid>,
    pub subject_type: Option<String>,
    pub subject_id: Option<Uuid>,
    pub detail: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub company_id: Uuid,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl LivechatAuditLog {
    /// Create a builder for LivechatAuditLog
    pub fn builder() -> LivechatAuditLogBuilder {
        <LivechatAuditLogBuilder as Default>::default()
    }

    /// Create a new LivechatAuditLog with required fields
    pub fn new(event: LivechatAuditEvent, company_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            event,
            actor: None,
            subject_type: None,
            subject_id: None,
            detail: None,
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
    pub fn typed_id(&self) -> LivechatAuditLogId {
        LivechatAuditLogId(self.id)
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

    /// Set the actor field (chainable)
    pub fn with_actor(mut self, value: Uuid) -> Self {
        self.actor = Some(value);
        self
    }

    /// Set the subject_type field (chainable)
    pub fn with_subject_type(mut self, value: String) -> Self {
        self.subject_type = Some(value);
        self
    }

    /// Set the subject_id field (chainable)
    pub fn with_subject_id(mut self, value: Uuid) -> Self {
        self.subject_id = Some(value);
        self
    }

    /// Set the detail field (chainable)
    pub fn with_detail(mut self, value: serde_json::Value) -> Self {
        self.detail = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "event" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.event = v;
                    }
                }
                "actor" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.actor = v;
                    }
                }
                "subject_type" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.subject_type = v;
                    }
                }
                "subject_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.subject_id = v;
                    }
                }
                "detail" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.detail = v;
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

impl super::Entity for LivechatAuditLog {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "LivechatAuditLog"
    }
}

impl backbone_core::PersistentEntity for LivechatAuditLog {
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

impl backbone_orm::EntityRepoMeta for LivechatAuditLog {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("subject_id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m.insert("event".to_string(), "livechat_audit_event".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
    fn company_field() -> Option<&'static str> {
        Some("company_id")
    }
}

/// Builder for LivechatAuditLog entity
///
/// Provides a fluent API for constructing LivechatAuditLog instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct LivechatAuditLogBuilder {
    event: Option<LivechatAuditEvent>,
    actor: Option<Uuid>,
    subject_type: Option<String>,
    subject_id: Option<Uuid>,
    detail: Option<serde_json::Value>,
    company_id: Option<Uuid>,
}

impl LivechatAuditLogBuilder {
    /// Set the event field (required)
    pub fn event(mut self, value: LivechatAuditEvent) -> Self {
        self.event = Some(value);
        self
    }

    /// Set the actor field (optional)
    pub fn actor(mut self, value: Uuid) -> Self {
        self.actor = Some(value);
        self
    }

    /// Set the subject_type field (optional)
    pub fn subject_type(mut self, value: String) -> Self {
        self.subject_type = Some(value);
        self
    }

    /// Set the subject_id field (optional)
    pub fn subject_id(mut self, value: Uuid) -> Self {
        self.subject_id = Some(value);
        self
    }

    /// Set the detail field (optional)
    pub fn detail(mut self, value: serde_json::Value) -> Self {
        self.detail = Some(value);
        self
    }

    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

    /// Build the LivechatAuditLog entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<LivechatAuditLog, String> {
        let event = self.event.ok_or_else(|| "event is required".to_string())?;
        let company_id = self
            .company_id
            .ok_or_else(|| "company_id is required".to_string())?;

        Ok(LivechatAuditLog {
            id: Uuid::new_v4(),
            event,
            actor: self.actor,
            subject_type: self.subject_type,
            subject_id: self.subject_id,
            detail: self.detail,
            created_at: Utc::now(),
            company_id,
            metadata: AuditMetadata::default(),
        })
    }
}
