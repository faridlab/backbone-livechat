use super::AuditMetadata;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

/// Strongly-typed ID for OperatorExpertise
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OperatorExpertiseId(pub Uuid);

impl OperatorExpertiseId {
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

impl std::fmt::Display for OperatorExpertiseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for OperatorExpertiseId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for OperatorExpertiseId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<OperatorExpertiseId> for Uuid {
    fn from(id: OperatorExpertiseId) -> Self {
        id.0
    }
}

impl AsRef<Uuid> for OperatorExpertiseId {
    fn as_ref(&self) -> &Uuid {
        &self.0
    }
}

impl std::ops::Deref for OperatorExpertiseId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OperatorExpertise {
    pub id: Uuid,
    pub operator_profile_id: Uuid,
    pub expertise_tag_id: Uuid,
    pub company_id: Uuid,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl OperatorExpertise {
    /// Create a builder for OperatorExpertise
    pub fn builder() -> OperatorExpertiseBuilder {
        <OperatorExpertiseBuilder as Default>::default()
    }

    /// Create a new OperatorExpertise with required fields
    pub fn new(operator_profile_id: Uuid, expertise_tag_id: Uuid, company_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            operator_profile_id,
            expertise_tag_id,
            company_id,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> OperatorExpertiseId {
        OperatorExpertiseId(self.id)
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
    // Partial Update
    // ==========================================================

    /// Apply partial updates from a map of field name to JSON value
    pub fn apply_patch(&mut self, fields: std::collections::HashMap<String, serde_json::Value>) {
        for (key, value) in fields {
            match key.as_str() {
                "operator_profile_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.operator_profile_id = v;
                    }
                }
                "expertise_tag_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.expertise_tag_id = v;
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

impl super::Entity for OperatorExpertise {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "OperatorExpertise"
    }
}

impl backbone_core::PersistentEntity for OperatorExpertise {
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

impl backbone_orm::EntityRepoMeta for OperatorExpertise {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("operator_profile_id".to_string(), "uuid".to_string());
        m.insert("expertise_tag_id".to_string(), "uuid".to_string());
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
            ("operatorProfile", "operator_profiles", "operatorProfileId"),
            ("expertiseTag", "expertise_tags", "expertiseTagId"),
        ]
    }
}

/// Builder for OperatorExpertise entity
///
/// Provides a fluent API for constructing OperatorExpertise instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct OperatorExpertiseBuilder {
    operator_profile_id: Option<Uuid>,
    expertise_tag_id: Option<Uuid>,
    company_id: Option<Uuid>,
}

impl OperatorExpertiseBuilder {
    /// Set the operator_profile_id field (required)
    pub fn operator_profile_id(mut self, value: Uuid) -> Self {
        self.operator_profile_id = Some(value);
        self
    }

    /// Set the expertise_tag_id field (required)
    pub fn expertise_tag_id(mut self, value: Uuid) -> Self {
        self.expertise_tag_id = Some(value);
        self
    }

    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

    /// Build the OperatorExpertise entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<OperatorExpertise, String> {
        let operator_profile_id = self
            .operator_profile_id
            .ok_or_else(|| "operator_profile_id is required".to_string())?;
        let expertise_tag_id = self
            .expertise_tag_id
            .ok_or_else(|| "expertise_tag_id is required".to_string())?;
        let company_id = self
            .company_id
            .ok_or_else(|| "company_id is required".to_string())?;

        Ok(OperatorExpertise {
            id: Uuid::new_v4(),
            operator_profile_id,
            expertise_tag_id,
            company_id,
            metadata: AuditMetadata::default(),
        })
    }
}
