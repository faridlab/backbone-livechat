use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;
use super::AuditMetadata;

/// Strongly-typed ID for OperatorProfile
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperatorProfileId(pub Uuid);

impl OperatorProfileId {
    pub fn new(id: Uuid) -> Self { Self(id) }
    pub fn generate() -> Self { Self(Uuid::new_v4()) }
    pub fn into_inner(self) -> Uuid { self.0 }
}

impl std::fmt::Display for OperatorProfileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for OperatorProfileId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for OperatorProfileId {
    fn from(id: Uuid) -> Self { Self(id) }
}

impl From<OperatorProfileId> for Uuid {
    fn from(id: OperatorProfileId) -> Self { id.0 }
}

impl AsRef<Uuid> for OperatorProfileId {
    fn as_ref(&self) -> &Uuid { &self.0 }
}

impl std::ops::Deref for OperatorProfileId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target { &self.0 }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OperatorProfile {
    pub id: Uuid,
    pub user_id: Uuid,
    pub display_name: Option<String>,
    pub languages: Vec<String>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub last_assigned_at: Option<DateTime<Utc>>,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl OperatorProfile {
    /// Create a builder for OperatorProfile
    pub fn builder() -> OperatorProfileBuilder {
        <OperatorProfileBuilder as Default>::default()
    }

    /// Create a new OperatorProfile with required fields
    pub fn new(user_id: Uuid, languages: Vec<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            user_id,
            display_name: None,
            languages,
            last_heartbeat_at: None,
            last_assigned_at: None,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> OperatorProfileId {
        OperatorProfileId(self.id)
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

    /// Set the display_name field (chainable)
    pub fn with_display_name(mut self, value: String) -> Self {
        self.display_name = Some(value);
        self
    }

    /// Set the last_heartbeat_at field (chainable)
    pub fn with_last_heartbeat_at(mut self, value: DateTime<Utc>) -> Self {
        self.last_heartbeat_at = Some(value);
        self
    }

    /// Set the last_assigned_at field (chainable)
    pub fn with_last_assigned_at(mut self, value: DateTime<Utc>) -> Self {
        self.last_assigned_at = Some(value);
        self
    }

    // ==========================================================
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "user_id" => {
                    if let Ok(v) = serde_json::from_value(value) { self.user_id = v; }
                }
                "display_name" => {
                    if let Ok(v) = serde_json::from_value(value) { self.display_name = v; }
                }
                "languages" => {
                    if let Ok(v) = serde_json::from_value(value) { self.languages = v; }
                }
                "last_heartbeat_at" => {
                    if let Ok(v) = serde_json::from_value(value) { self.last_heartbeat_at = v; }
                }
                "last_assigned_at" => {
                    if let Ok(v) = serde_json::from_value(value) { self.last_assigned_at = v; }
                }
                _ => {} // ignore unknown fields
            }
        }
    }

    // <<< CUSTOM METHODS START >>>
    // <<< CUSTOM METHODS END >>>
}

impl super::Entity for OperatorProfile {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "OperatorProfile"
    }
}

impl backbone_core::PersistentEntity for OperatorProfile {
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

impl backbone_orm::EntityRepoMeta for OperatorProfile {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("user_id".to_string(), "uuid".to_string());
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &[]
    }
}

/// Builder for OperatorProfile entity
///
/// Provides a fluent API for constructing OperatorProfile instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct OperatorProfileBuilder {
    user_id: Option<Uuid>,
    display_name: Option<String>,
    languages: Option<Vec<String>>,
    last_heartbeat_at: Option<DateTime<Utc>>,
    last_assigned_at: Option<DateTime<Utc>>,
}

impl OperatorProfileBuilder {
    /// Set the user_id field (required)
    pub fn user_id(mut self, value: Uuid) -> Self {
        self.user_id = Some(value);
        self
    }

    /// Set the display_name field (optional)
    pub fn display_name(mut self, value: String) -> Self {
        self.display_name = Some(value);
        self
    }

    /// Set the languages field (required)
    pub fn languages(mut self, value: Vec<String>) -> Self {
        self.languages = Some(value);
        self
    }

    /// Set the last_heartbeat_at field (optional)
    pub fn last_heartbeat_at(mut self, value: DateTime<Utc>) -> Self {
        self.last_heartbeat_at = Some(value);
        self
    }

    /// Set the last_assigned_at field (optional)
    pub fn last_assigned_at(mut self, value: DateTime<Utc>) -> Self {
        self.last_assigned_at = Some(value);
        self
    }

    /// Build the OperatorProfile entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<OperatorProfile, String> {
        let user_id = self.user_id.ok_or_else(|| "user_id is required".to_string())?;
        let languages = self.languages.ok_or_else(|| "languages is required".to_string())?;

        Ok(OperatorProfile {
            id: Uuid::new_v4(),
            user_id,
            display_name: self.display_name,
            languages,
            last_heartbeat_at: self.last_heartbeat_at,
            last_assigned_at: self.last_assigned_at,
            metadata: AuditMetadata::default(),
        })
    }
}
