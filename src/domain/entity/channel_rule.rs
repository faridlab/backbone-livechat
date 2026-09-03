use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

use super::AuditMetadata;
use super::LivechatChatbotCondition;
use super::LivechatRuleAction;

/// Strongly-typed ID for ChannelRule
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChannelRuleId(pub Uuid);

impl ChannelRuleId {
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

impl std::fmt::Display for ChannelRuleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::str::FromStr for ChannelRuleId {
    type Err = uuid::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

impl From<Uuid> for ChannelRuleId {
    fn from(id: Uuid) -> Self {
        Self(id)
    }
}

impl From<ChannelRuleId> for Uuid {
    fn from(id: ChannelRuleId) -> Self {
        id.0
    }
}

impl AsRef<Uuid> for ChannelRuleId {
    fn as_ref(&self) -> &Uuid {
        &self.0
    }
}

impl std::ops::Deref for ChannelRuleId {
    type Target = Uuid;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChannelRule {
    pub id: Uuid,
    pub channel_id: Uuid,
    pub regex_url: String,
    pub action: LivechatRuleAction,
    pub auto_popup_timer: i32,
    pub chatbot_script_id: Option<Uuid>,
    pub chatbot_enabled_condition: LivechatChatbotCondition,
    pub country_codes: Vec<String>,
    pub sequence: i32,
    pub company_id: Uuid,
    #[serde(default)]
    #[sqlx(json)]
    pub metadata: AuditMetadata,
}

impl ChannelRule {
    /// Create a builder for ChannelRule
    pub fn builder() -> ChannelRuleBuilder {
        <ChannelRuleBuilder as Default>::default()
    }

    /// Create a new ChannelRule with required fields
    pub fn new(
        channel_id: Uuid,
        regex_url: String,
        action: LivechatRuleAction,
        auto_popup_timer: i32,
        chatbot_enabled_condition: LivechatChatbotCondition,
        country_codes: Vec<String>,
        sequence: i32,
        company_id: Uuid,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            channel_id,
            regex_url,
            action,
            auto_popup_timer,
            chatbot_script_id: None,
            chatbot_enabled_condition,
            country_codes,
            sequence,
            company_id,
            metadata: AuditMetadata::default(),
        }
    }

    /// Get the entity's unique identifier
    pub fn id(&self) -> &Uuid {
        &self.id
    }

    /// Get a strongly-typed ID for this entity
    pub fn typed_id(&self) -> ChannelRuleId {
        ChannelRuleId(self.id)
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

    /// Set the chatbot_script_id field (chainable)
    pub fn with_chatbot_script_id(mut self, value: Uuid) -> Self {
        self.chatbot_script_id = Some(value);
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
                "regex_url" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.regex_url = v;
                    }
                }
                "action" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.action = v;
                    }
                }
                "auto_popup_timer" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.auto_popup_timer = v;
                    }
                }
                "chatbot_script_id" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.chatbot_script_id = v;
                    }
                }
                "chatbot_enabled_condition" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.chatbot_enabled_condition = v;
                    }
                }
                "country_codes" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.country_codes = v;
                    }
                }
                "sequence" => {
                    if let Ok(v) = serde_json::from_value(value) {
                        self.sequence = v;
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

impl super::Entity for ChannelRule {
    type Id = Uuid;

    fn entity_id(&self) -> &Self::Id {
        &self.id
    }

    fn entity_type() -> &'static str {
        "ChannelRule"
    }
}

impl backbone_core::PersistentEntity for ChannelRule {
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

impl backbone_orm::EntityRepoMeta for ChannelRule {
    fn column_types() -> std::collections::HashMap<String, String> {
        let mut m = std::collections::HashMap::new();
        m.insert("id".to_string(), "uuid".to_string());
        m.insert("channel_id".to_string(), "uuid".to_string());
        m.insert("chatbot_script_id".to_string(), "uuid".to_string());
        m.insert("company_id".to_string(), "uuid".to_string());
        m.insert("action".to_string(), "livechat_rule_action".to_string());
        m.insert(
            "chatbot_enabled_condition".to_string(),
            "livechat_chatbot_condition".to_string(),
        );
        m
    }
    fn search_fields() -> &'static [&'static str] {
        &["regex_url"]
    }
    fn company_field() -> Option<&'static str> {
        Some("company_id")
    }
    fn relations() -> &'static [(&'static str, &'static str, &'static str)] {
        &[
            ("channel", "channels", "channelId"),
            ("chatbotScript", "chatbot_scripts", "chatbotScriptId"),
        ]
    }
}

/// Builder for ChannelRule entity
///
/// Provides a fluent API for constructing ChannelRule instances.
/// System fields (id, metadata, timestamps) are auto-initialized.
#[derive(Debug, Clone, Default)]
pub struct ChannelRuleBuilder {
    channel_id: Option<Uuid>,
    regex_url: Option<String>,
    action: Option<LivechatRuleAction>,
    auto_popup_timer: Option<i32>,
    chatbot_script_id: Option<Uuid>,
    chatbot_enabled_condition: Option<LivechatChatbotCondition>,
    country_codes: Option<Vec<String>>,
    sequence: Option<i32>,
    company_id: Option<Uuid>,
}

impl ChannelRuleBuilder {
    /// Set the channel_id field (required)
    pub fn channel_id(mut self, value: Uuid) -> Self {
        self.channel_id = Some(value);
        self
    }

    /// Set the regex_url field (required)
    pub fn regex_url(mut self, value: String) -> Self {
        self.regex_url = Some(value);
        self
    }

    /// Set the action field (default: `LivechatRuleAction::default()`)
    pub fn action(mut self, value: LivechatRuleAction) -> Self {
        self.action = Some(value);
        self
    }

    /// Set the auto_popup_timer field (default: `0`)
    pub fn auto_popup_timer(mut self, value: i32) -> Self {
        self.auto_popup_timer = Some(value);
        self
    }

    /// Set the chatbot_script_id field (optional)
    pub fn chatbot_script_id(mut self, value: Uuid) -> Self {
        self.chatbot_script_id = Some(value);
        self
    }

    /// Set the chatbot_enabled_condition field (default: `LivechatChatbotCondition::default()`)
    pub fn chatbot_enabled_condition(mut self, value: LivechatChatbotCondition) -> Self {
        self.chatbot_enabled_condition = Some(value);
        self
    }

    /// Set the country_codes field (required)
    pub fn country_codes(mut self, value: Vec<String>) -> Self {
        self.country_codes = Some(value);
        self
    }

    /// Set the sequence field (default: `1`)
    pub fn sequence(mut self, value: i32) -> Self {
        self.sequence = Some(value);
        self
    }

    /// Set the company_id field (required)
    pub fn company_id(mut self, value: Uuid) -> Self {
        self.company_id = Some(value);
        self
    }

    /// Build the ChannelRule entity
    ///
    /// Returns Err if any required field without a default is missing.
    pub fn build(self) -> Result<ChannelRule, String> {
        let channel_id = self
            .channel_id
            .ok_or_else(|| "channel_id is required".to_string())?;
        let regex_url = self
            .regex_url
            .ok_or_else(|| "regex_url is required".to_string())?;
        let country_codes = self
            .country_codes
            .ok_or_else(|| "country_codes is required".to_string())?;
        let company_id = self
            .company_id
            .ok_or_else(|| "company_id is required".to_string())?;

        Ok(ChannelRule {
            id: Uuid::new_v4(),
            channel_id,
            regex_url,
            action: self.action.unwrap_or_default(),
            auto_popup_timer: self.auto_popup_timer.unwrap_or(0),
            chatbot_script_id: self.chatbot_script_id,
            chatbot_enabled_condition: self.chatbot_enabled_condition.unwrap_or_default(),
            country_codes,
            sequence: self.sequence.unwrap_or(1),
            company_id,
            metadata: AuditMetadata::default(),
        })
    }
}
