//! The admin config verbs' law-carrying SQL (hand-written; user-owned;
//! see `metaphor.codegen.yaml`).
//!
//! Every config verb of the declared admin surface lives here so its
//! LAW lands in ONE place: the canonical-name uniques (typed 409),
//! the save-time rule-regex validation (typed 422, empty refused),
//! the seven-step-type closure with question-steps-need-answers, the
//! forward-only trigger check, the typed channel patch whitelist,
//! and the pointer-reference delete fence. The DB constraints
//! (the hardening migration) are the wall; these verbs translate
//! their violations into the typed wire errors.
//!
//! RLS LAW: every statement rides the tenant-agnostic org_scope /
//! company_scope `*_scoped` helpers — the request-dedicated connection
//! when the composing service bound one, the plain pool otherwise
//! (ADR-0029: the fence itself is the composer's decorator, not the
//! module's).

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::application::service::livechat_error::LivechatError;

// The typed multi-row read twins live only in the legacy `company_scope` module. Their
// connection discipline is what this repository needs — request-dedicated connection when
// the composing service bound one, plain pool otherwise. The helper's legacy task-local
// branch is never taken: this module sets no legacy scope of its own (ADR-0029).
use backbone_orm::company_scope::{fetch_all_scoped, fetch_optional_scoped};
use backbone_orm::org_scope::execute_scoped;
use super::relay_ambient_scope;

/// The seven community step types (the CLOSED set — the create_lead /
/// create_ticket arms of upstream are refused by omission).
pub const STEP_TYPES: [&str; 7] = [
    "text",
    "question_selection",
    "question_email",
    "question_phone",
    "forward_operator",
    "free_input_single",
    "free_input_multi",
];

pub struct AdminConfigRepository {
    pool: PgPool,
}

/// One config row as JSON (the admin projection — `to_jsonb` of the
/// row; enum columns arrive as their text labels).
type JsonRow = Value;

/// The typed channel patch whitelist: exactly these fields may
/// change after create; `website_id` deliberately ABSENT (rebinding
/// a channel to another website is not a patch — it is a re-create).
#[derive(Debug, Default)]
pub struct ChannelPatch {
    pub name: Option<String>,
    pub button_text: Option<Option<String>>,
    pub welcome_message: Option<Option<String>>,
    pub max_sessions_mode: Option<String>,
    pub max_sessions: Option<i32>,
    pub block_assignment_during_call: Option<bool>,
    pub review_link: Option<Option<String>>,
    pub is_active: Option<bool>,
}

/// The chatbot-step create/replace input (answers ride the step: a
/// question step must be born with its options).
#[derive(Debug, Default)]
pub struct StepInput {
    pub chatbot_script_id: Uuid,
    pub sequence: i32,
    pub step_type: String,
    pub message: Option<String>,
    pub expertise_tag_ids: Vec<Uuid>,
    pub answers: Vec<AnswerInput>,
}

#[derive(Debug, Clone)]
pub struct AnswerInput {
    pub sequence: i32,
    pub label: String,
    pub redirect_url: Option<String>,
}

/// A rule create/replace input.
#[derive(Debug, Default)]
pub struct RuleInput {
    pub channel_id: Uuid,
    pub regex_url: String,
    pub action: String,
    pub auto_popup_timer: i32,
    pub chatbot_script_id: Option<Uuid>,
    pub chatbot_enabled_condition: String,
    pub country_codes: Vec<String>,
    pub sequence: i32,
}

impl AdminConfigRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // ── Channels ───────────────────────────────────────────────────

    /// List the company's channels (non-deleted, by name).
    pub async fn channel_list(&self) -> Result<Vec<JsonRow>, LivechatError> {
        let rows = fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"SELECT to_jsonb(e) FROM livechat.channels e
                    WHERE e.metadata->>'deleted_at' IS NULL ORDER BY e.name"#,
            ),
        )
        .await?;
        Ok(rows.into_iter().map(|(j,)| j.0).collect())
    }

    /// One channel (non-deleted).
    pub async fn channel_get(&self, id: Uuid) -> Result<Option<JsonRow>, LivechatError> {
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"SELECT to_jsonb(e) FROM livechat.channels e
                    WHERE e.id = $1 AND e.metadata->>'deleted_at' IS NULL"#,
            )
            .bind(id),
        )
        .await?;
        Ok(row.map(|(j,)| j.0))
    }

    /// Create a channel. The from-website wizard is THIS verb with
    /// `website_id` bound — every channel the wizard creates is bound
    /// to the website at birth, and no bot rule is created silently.
    #[allow(clippy::too_many_arguments)]
    pub async fn channel_create(
        &self,
        name: &str,
        website_id: Option<Uuid>,
        button_text: Option<&str>,
        welcome_message: Option<&str>,
        max_sessions_mode: &str,
        max_sessions: i32,
        block_assignment_during_call: bool,
        review_link: Option<&str>,
        is_active: bool,
    ) -> Result<JsonRow, LivechatError> {
        if name.trim().is_empty() {
            return Err(LivechatError::Validation("channel name is required".into()));
        }
        if !matches!(max_sessions_mode, "unlimited" | "limited") {
            return Err(LivechatError::Validation(
                "max_sessions_mode must be 'unlimited' or 'limited'".into(),
            ));
        }
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"INSERT INTO livechat.channels
                       (name, website_id, button_text, welcome_message,
                        max_sessions_mode, max_sessions, block_assignment_during_call,
                        review_link, is_active)
                   VALUES ($1, $2, $3, $4, $5::livechat_max_sessions_mode, $6, $7, $8, $9)
                   RETURNING to_jsonb(channels)"#,
            )
            .bind(name.trim())
            .bind(website_id)
            .bind(button_text)
            .bind(welcome_message)
            .bind(max_sessions_mode)
            .bind(max_sessions)
            .bind(block_assignment_during_call)
            .bind(review_link)
            .bind(is_active),
        )
        .await?
        .map(|(j,)| j.0);
        row.ok_or_else(|| LivechatError::Database("channel create returned no row".into()))
    }

    /// The typed-whitelist patch (only the declared fields; a field
    /// absent from the patch keeps its value).
    pub async fn channel_patch(
        &self,
        id: Uuid,
        patch: &ChannelPatch,
    ) -> Result<Option<JsonRow>, LivechatError> {
        let mut sets: Vec<String> = Vec::new();
        let mut name: Option<String> = None;
        let mut button: Option<Option<String>> = None;
        let mut welcome: Option<Option<String>> = None;
        let mut mode: Option<String> = None;
        let mut max: Option<i32> = None;
        let mut block: Option<bool> = None;
        let mut review: Option<Option<String>> = None;
        let mut active: Option<bool> = None;
        if let Some(v) = &patch.name {
            if v.trim().is_empty() {
                return Err(LivechatError::Validation("channel name is required".into()));
            }
            sets.push(format!("name = ${}", sets.len() + 1));
            name = Some(v.trim().to_string());
        }
        if let Some(v) = &patch.button_text {
            sets.push(format!("button_text = ${}", sets.len() + 1));
            button = Some(v.clone());
        }
        if let Some(v) = &patch.welcome_message {
            sets.push(format!("welcome_message = ${}", sets.len() + 1));
            welcome = Some(v.clone());
        }
        if let Some(v) = &patch.max_sessions_mode {
            if !matches!(v.as_str(), "unlimited" | "limited") {
                return Err(LivechatError::Validation(
                    "max_sessions_mode must be 'unlimited' or 'limited'".into(),
                ));
            }
            sets.push(format!(
                "max_sessions_mode = ${}::livechat_max_sessions_mode",
                sets.len() + 1
            ));
            mode = Some(v.clone());
        }
        if let Some(v) = patch.max_sessions {
            sets.push(format!("max_sessions = ${}", sets.len() + 1));
            max = Some(v);
        }
        if let Some(v) = patch.block_assignment_during_call {
            sets.push(format!(
                "block_assignment_during_call = ${}",
                sets.len() + 1
            ));
            block = Some(v);
        }
        if let Some(v) = &patch.review_link {
            sets.push(format!("review_link = ${}", sets.len() + 1));
            review = Some(v.clone());
        }
        if let Some(v) = patch.is_active {
            sets.push(format!("is_active = ${}", sets.len() + 1));
            active = Some(v);
        }
        if sets.is_empty() {
            return self.channel_get(id).await;
        }
        let sql = format!(
            r#"UPDATE livechat.channels SET {} WHERE id = ${}
                  AND metadata->>'deleted_at' IS NULL
               RETURNING to_jsonb(channels)"#,
            sets.join(", "),
            sets.len() + 1,
        );
        let mut q = sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(&sql);
        if let Some(v) = name {
            q = q.bind(v);
        }
        if let Some(v) = button {
            q = q.bind(v);
        }
        if let Some(v) = welcome {
            q = q.bind(v);
        }
        if let Some(v) = mode {
            q = q.bind(v);
        }
        if let Some(v) = max {
            q = q.bind(v);
        }
        if let Some(v) = block {
            q = q.bind(v);
        }
        if let Some(v) = review {
            q = q.bind(v);
        }
        if let Some(v) = active {
            q = q.bind(v);
        }
        let q = q.bind(id);
        let row = fetch_optional_scoped(&self.pool, q).await?;
        Ok(row.map(|(j,)| j.0))
    }

    /// Soft-delete a channel (rows survive; the availability and
    /// assignment scans skip deleted channels).
    pub async fn channel_delete(&self, id: Uuid) -> Result<bool, LivechatError> {
        let n = execute_scoped(
            &self.pool,
            sqlx::query(
                r#"UPDATE livechat.channels
                      SET metadata = jsonb_set(metadata, '{{deleted_at}}', to_jsonb(now()))
                    WHERE id = $1 AND metadata->>'deleted_at' IS NULL"#,
            )
            .bind(id),
        )
        .await?;
        Ok(n.rows_affected() > 0)
    }

    // ── Channel membership (add/remove BOTH the same gated shape) ──

    /// Add an operator to a channel. The operator's PROFILE row is
    /// minted on first add (display_name NULL until they set one) —
    /// the capacity gate reads the profile, not the membership.
    pub async fn channel_add_operator(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
    ) -> Result<JsonRow, LivechatError> {
        // The profile mint (idempotent; the membership needs it).
        execute_scoped(
            &self.pool,
            sqlx::query(
                // No conflict target: the per-unit unique (org_unit_id, user_id) is
                // decorator-declared, so the module cannot name it. Untargeted DO NOTHING
                // catches it (and every module-owned unique) exactly as the targeted shape did.
                r#"INSERT INTO livechat.operator_profiles (user_id, languages)
                   VALUES ($1, '{}')
                   ON CONFLICT DO NOTHING"#,
            )
            .bind(user_id),
        )
        .await?;
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"INSERT INTO livechat.channel_members (channel_id, user_id)
                   VALUES ($1, $2)
                   ON CONFLICT (channel_id, user_id) DO UPDATE
                       SET metadata = jsonb_set(channel_members.metadata, '{deleted_at}', 'null'::jsonb)
                   RETURNING to_jsonb(channel_members)"#,
            )
            .bind(channel_id)
            .bind(user_id),
        )
        .await?
        .map(|(j,)| j.0);
        row.ok_or_else(|| LivechatError::Database("member add returned no row".into()))
    }

    /// Remove an operator from a channel (the same gated verb shape
    /// as add — the asymmetry of upstream is closed).
    pub async fn channel_remove_operator(
        &self,
        channel_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, LivechatError> {
        let n = execute_scoped(
            &self.pool,
            sqlx::query(
                "DELETE FROM livechat.channel_members WHERE channel_id = $1 AND user_id = $2",
            )
            .bind(channel_id)
            .bind(user_id),
        )
        .await?;
        Ok(n.rows_affected() > 0)
    }

    /// The leave-all cascade (one declared verb: the operator leaves
    /// every channel of the company).
    pub async fn channel_leave_all(&self, user_id: Uuid) -> Result<u64, LivechatError> {
        let n = execute_scoped(
            &self.pool,
            sqlx::query("DELETE FROM livechat.channel_members WHERE user_id = $1").bind(user_id),
        )
        .await?;
        Ok(n.rows_affected())
    }

    // ── Operator profiles ──────────────────────────────────────────

    /// The profile read (self or officer).
    pub async fn profile_get(&self, user_id: Uuid) -> Result<Option<JsonRow>, LivechatError> {
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"SELECT to_jsonb(e) FROM livechat.operator_profiles e
                    WHERE e.user_id = $1 AND e.metadata->>'deleted_at' IS NULL"#,
            )
            .bind(user_id),
        )
        .await?;
        Ok(row.map(|(j,)| j.0))
    }

    /// The profile write (display_name + languages; upsert).
    pub async fn profile_put(
        &self,
        user_id: Uuid,
        display_name: Option<&str>,
        languages: &[String],
    ) -> Result<JsonRow, LivechatError> {
        // Upsert without a conflict target: the per-unit unique (org_unit_id, user_id) is
        // decorator-declared (ADR-0029), so the module cannot name it in ON CONFLICT. The
        // untargeted DO NOTHING insert returns no row when it fired; the follow-up UPDATE
        // by user_id — fenced to the caller's scope by the composer's decorator — lands
        // the write on the existing profile. Same outcome as the targeted DO UPDATE,
        // including under concurrency (the loser of the insert race takes the UPDATE).
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"INSERT INTO livechat.operator_profiles (user_id, display_name, languages)
                   VALUES ($1, $2, $3)
                   ON CONFLICT DO NOTHING
                   RETURNING to_jsonb(operator_profiles)"#,
            )
            .bind(user_id)
            .bind(display_name)
            .bind(languages),
        )
        .await?;
        let row = match row {
            Some((j,)) => Some(j.0),
            None => fetch_optional_scoped(
                &self.pool,
                sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                    r#"UPDATE livechat.operator_profiles
                          SET display_name = $2, languages = $3
                        WHERE user_id = $1 AND metadata->>'deleted_at' IS NULL
                        RETURNING to_jsonb(operator_profiles)"#,
                )
                .bind(user_id)
                .bind(display_name)
                .bind(languages),
            )
            .await?
            .map(|(j,)| j.0),
        };
        row.ok_or_else(|| LivechatError::Database("profile write returned no row".into()))
    }

    // ── Tags (conversation + expertise: one shape, two tables) ─────

    /// List a tag table (non-deleted, by name).
    pub async fn tag_list(&self, table: &str) -> Result<Vec<JsonRow>, LivechatError> {
        let table = tag_table(table)?;
        let rows = fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(&format!(
                r#"SELECT to_jsonb(e) FROM {table} e
                    WHERE e.metadata->>'deleted_at' IS NULL ORDER BY e.name"#
            )),
        )
        .await?;
        Ok(rows.into_iter().map(|(j,)| j.0).collect())
    }

    /// Create a tag (the canonical lower(name) unique is the typed
    /// 409).
    pub async fn tag_create(&self, table: &str, name: &str) -> Result<JsonRow, LivechatError> {
        let table = tag_table(table)?;
        if name.trim().is_empty() {
            return Err(LivechatError::Validation("tag name is required".into()));
        }
        let row = match fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(&format!(
                r#"INSERT INTO {table} AS t (name)
                   VALUES ($1)
                   RETURNING to_jsonb(t)"#
            ))
            .bind(name.trim()),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(map_unique(
                    e,
                    "lower_name",
                    LivechatError::TagNameConflict {
                        name: name.trim().to_string(),
                    },
                ))
            }
        };
        row.map(|(j,)| j.0)
            .ok_or_else(|| LivechatError::Database("tag create returned no row".into()))
    }

    /// Rename a tag (same canonical-unique law).
    pub async fn tag_rename(
        &self,
        table: &str,
        id: Uuid,
        name: &str,
    ) -> Result<Option<JsonRow>, LivechatError> {
        let table = tag_table(table)?;
        if name.trim().is_empty() {
            return Err(LivechatError::Validation("tag name is required".into()));
        }
        let row = match fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(&format!(
                r#"UPDATE {table} AS t SET name = $2
                    WHERE t.id = $1 AND t.metadata->>'deleted_at' IS NULL
                   RETURNING to_jsonb(t)"#
            ))
            .bind(id)
            .bind(name.trim()),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(map_unique(
                    e,
                    "lower_name",
                    LivechatError::TagNameConflict {
                        name: name.trim().to_string(),
                    },
                ))
            }
        };
        Ok(row.map(|(j,)| j.0))
    }

    /// Soft-delete a tag.
    pub async fn tag_delete(&self, table: &str, id: Uuid) -> Result<bool, LivechatError> {
        let table = tag_table(table)?;
        let n = execute_scoped(
            &self.pool,
            sqlx::query(&format!(
                r#"UPDATE {table}
                      SET metadata = jsonb_set(metadata, '{{deleted_at}}', to_jsonb(now()))
                    WHERE id = $1 AND metadata->>'deleted_at' IS NULL"#
            ))
            .bind(id),
        )
        .await?;
        Ok(n.rows_affected() > 0)
    }

    // ── Channel rules ──────────────────────────────────────────────

    /// List rules (optionally one channel's), sequence-ordered.
    pub async fn rule_list(&self, channel_id: Option<Uuid>) -> Result<Vec<JsonRow>, LivechatError> {
        let rows = fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"SELECT to_jsonb(e) FROM livechat.channel_rules e
                    WHERE e.metadata->>'deleted_at' IS NULL
                      AND ($1::uuid IS NULL OR e.channel_id = $1)
                    ORDER BY (e.regex_url = '.*'), e.sequence"#,
            )
            .bind(channel_id),
        )
        .await?;
        Ok(rows.into_iter().map(|(j,)| j.0).collect())
    }

    /// One rule.
    pub async fn rule_get(&self, id: Uuid) -> Result<Option<JsonRow>, LivechatError> {
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"SELECT to_jsonb(e) FROM livechat.channel_rules e
                    WHERE e.id = $1 AND e.metadata->>'deleted_at' IS NULL"#,
            )
            .bind(id),
        )
        .await?;
        Ok(row.map(|(j,)| j.0))
    }

    /// Create a rule: the regex is VALIDATED at save time (a
    /// malformed OR EMPTY pattern is the typed 422 — match-all must
    /// be an explicit `.*`).
    pub async fn rule_create(&self, input: &RuleInput) -> Result<JsonRow, LivechatError> {
        validate_rule(input)?;
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"INSERT INTO livechat.channel_rules
                       (channel_id, regex_url, action, auto_popup_timer,
                        chatbot_script_id, chatbot_enabled_condition,
                        country_codes, sequence)
                   VALUES ($1, $2, $3::livechat_rule_action, $4, $5,
                           $6::livechat_chatbot_condition, $7, $8)
                   RETURNING to_jsonb(channel_rules)"#,
            )
            .bind(input.channel_id)
            .bind(&input.regex_url)
            .bind(&input.action)
            .bind(input.auto_popup_timer)
            .bind(input.chatbot_script_id)
            .bind(&input.chatbot_enabled_condition)
            .bind(&input.country_codes)
            .bind(input.sequence),
        )
        .await?
        .map(|(j,)| j.0);
        row.ok_or_else(|| LivechatError::Database("rule create returned no row".into()))
    }

    /// Replace a rule (same save-time validation).
    pub async fn rule_replace(
        &self,
        id: Uuid,
        input: &RuleInput,
    ) -> Result<Option<JsonRow>, LivechatError> {
        validate_rule(input)?;
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"UPDATE livechat.channel_rules SET
                       channel_id = $2, regex_url = $3, action = $4::livechat_rule_action,
                       auto_popup_timer = $5, chatbot_script_id = $6,
                       chatbot_enabled_condition = $7::livechat_chatbot_condition,
                       country_codes = $8, sequence = $9
                 WHERE id = $1 AND metadata->>'deleted_at' IS NULL
                 RETURNING to_jsonb(channel_rules)"#,
            )
            .bind(id)
            .bind(input.channel_id)
            .bind(&input.regex_url)
            .bind(&input.action)
            .bind(input.auto_popup_timer)
            .bind(input.chatbot_script_id)
            .bind(&input.chatbot_enabled_condition)
            .bind(&input.country_codes)
            .bind(input.sequence),
        )
        .await?;
        Ok(row.map(|(j,)| j.0))
    }

    /// Soft-delete a rule.
    pub async fn rule_delete(&self, id: Uuid) -> Result<bool, LivechatError> {
        let n = execute_scoped(
            &self.pool,
            sqlx::query(
                r#"UPDATE livechat.channel_rules
                      SET metadata = jsonb_set(metadata, '{{deleted_at}}', to_jsonb(now()))
                    WHERE id = $1 AND metadata->>'deleted_at' IS NULL"#,
            )
            .bind(id),
        )
        .await?;
        Ok(n.rows_affected() > 0)
    }

    // ── Chatbot scripts ────────────────────────────────────────────

    pub async fn script_list(&self) -> Result<Vec<JsonRow>, LivechatError> {
        let rows = fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"SELECT to_jsonb(e) FROM livechat.chatbot_scripts e
                    WHERE e.metadata->>'deleted_at' IS NULL ORDER BY e.title"#,
            ),
        )
        .await?;
        Ok(rows.into_iter().map(|(j,)| j.0).collect())
    }

    pub async fn script_get(&self, id: Uuid) -> Result<Option<JsonRow>, LivechatError> {
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"SELECT to_jsonb(e) FROM livechat.chatbot_scripts e
                    WHERE e.id = $1 AND e.metadata->>'deleted_at' IS NULL"#,
            )
            .bind(id),
        )
        .await?;
        Ok(row.map(|(j,)| j.0))
    }

    /// Create a script (the canonical lower(title) unique is the
    /// typed 409).
    pub async fn script_create(&self, title: &str) -> Result<JsonRow, LivechatError> {
        if title.trim().is_empty() {
            return Err(LivechatError::Validation("script title is required".into()));
        }
        let row = match fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"INSERT INTO livechat.chatbot_scripts (title)
                   VALUES ($1)
                   RETURNING to_jsonb(chatbot_scripts)"#,
            )
            .bind(title.trim()),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(map_unique(
                    e,
                    "uq_chatbot_scripts_org_unit_id_lower_title",
                    LivechatError::TagNameConflict {
                        name: title.trim().to_string(),
                    },
                ))
            }
        };
        row.map(|(j,)| j.0)
            .ok_or_else(|| LivechatError::Database("script create returned no row".into()))
    }

    /// Patch a script (title / is_active; the canonical unique law
    /// rides the rename).
    pub async fn script_patch(
        &self,
        id: Uuid,
        title: Option<&str>,
        is_active: Option<bool>,
    ) -> Result<Option<JsonRow>, LivechatError> {
        if let Some(t) = title {
            if t.trim().is_empty() {
                return Err(LivechatError::Validation("script title is required".into()));
            }
        }
        let row = match fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"UPDATE livechat.chatbot_scripts SET
                       title = COALESCE($2, title), is_active = COALESCE($3, is_active)
                 WHERE id = $1 AND metadata->>'deleted_at' IS NULL
                 RETURNING to_jsonb(chatbot_scripts)"#,
            )
            .bind(id)
            .bind(title.map(str::trim))
            .bind(is_active),
        )
        .await
        {
            Ok(r) => r,
            Err(e) => {
                return Err(map_unique(
                    e,
                    "uq_chatbot_scripts_org_unit_id_lower_title",
                    LivechatError::TagNameConflict {
                        name: title.unwrap_or_default().to_string(),
                    },
                ))
            }
        };
        Ok(row.map(|(j,)| j.0))
    }

    /// Soft-delete a script — REFUSED while a live channel rule
    /// routes to it (the rule's script reference must stay good).
    pub async fn script_delete(&self, id: Uuid) -> Result<bool, LivechatError> {
        let referencing = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (i64,)>(
                r#"SELECT 1::int8 FROM livechat.channel_rules r
                    WHERE r.chatbot_script_id = $1
                      AND r.metadata->>'deleted_at' IS NULL LIMIT 1"#,
            )
            .bind(id),
        )
        .await?;
        if referencing.is_some() {
            return Err(LivechatError::Validation(
                "a channel rule routes to this script; delete or re-point the rule first".into(),
            ));
        }
        let n = execute_scoped(
            &self.pool,
            sqlx::query(
                r#"UPDATE livechat.chatbot_scripts
                      SET metadata = jsonb_set(metadata, '{{deleted_at}}', to_jsonb(now()))
                    WHERE id = $1 AND metadata->>'deleted_at' IS NULL"#,
            )
            .bind(id),
        )
        .await?;
        Ok(n.rows_affected() > 0)
    }

    // ── Chatbot steps (the seven-type closure) ─────────────────────

    /// List a script's steps (with their answers), sequence-ordered.
    pub async fn step_list(&self, script_id: Uuid) -> Result<Vec<JsonRow>, LivechatError> {
        let rows = fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"SELECT to_jsonb(e) FROM livechat.chatbot_steps e
                    WHERE e.chatbot_script_id = $1 AND e.metadata->>'deleted_at' IS NULL
                    ORDER BY e.sequence"#,
            )
            .bind(script_id),
        )
        .await?;
        Ok(rows.into_iter().map(|(j,)| j.0).collect())
    }

    /// Create a step with its answers: the step type must be one of
    /// the SEVEN; a question step is born WITH its answers (at least
    /// one); the script-sequence unique is the typed 422.
    pub async fn step_create(&self, input: &StepInput) -> Result<JsonRow, LivechatError> {
        validate_step(input)?;
        let mut tx = self.pool.begin().await?;
        relay_ambient_scope(&mut tx).await?;
        let step_id = Uuid::new_v4();
        let row: Value = sqlx::query_scalar::<_, Value>(
            r#"INSERT INTO livechat.chatbot_steps
                   (id, chatbot_script_id, sequence, step_type, message,
                    expertise_tag_ids)
               VALUES ($1, $2, $3, $4::livechat_step_type, $5, $6)
               RETURNING to_jsonb(chatbot_steps)"#,
        )
        .bind(step_id)
        .bind(input.chatbot_script_id)
        .bind(input.sequence)
        .bind(&input.step_type)
        .bind(&input.message)
        .bind(&input.expertise_tag_ids)
        .fetch_one(&mut *tx)
        .await
        .map_err(unique_step)?;
        for a in &input.answers {
            sqlx::query(
                r#"INSERT INTO livechat.chatbot_answers
                       (question_step_id, sequence, label, redirect_url)
                   VALUES ($1, $2, $3, $4)"#,
            )
            .bind(step_id)
            .bind(a.sequence)
            .bind(&a.label)
            .bind(&a.redirect_url)
            .execute(&mut *tx)
            .await
            .map_err(unique_step)?;
        }
        tx.commit().await?;
        Ok(row)
    }

    /// Delete a step — REFUSED while any session's pointer references
    /// it (the pointer law); its answers and triggers go with it.
    pub async fn step_delete(&self, id: Uuid) -> Result<bool, LivechatError> {
        let pointed = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (i64,)>(
                r#"SELECT 1::int8 FROM livechat.sessions s
                    WHERE s.chatbot_current_step_id = $1 AND s.closed_at IS NULL LIMIT 1"#,
            )
            .bind(id),
        )
        .await?;
        if pointed.is_some() {
            return Err(LivechatError::Validation(
                "an open session's chatbot pointer references this step".into(),
            ));
        }
        let mut tx = self.pool.begin().await?;
        relay_ambient_scope(&mut tx).await?;
        sqlx::query(
            r#"UPDATE livechat.chatbot_steps
                  SET metadata = jsonb_set(metadata, '{deleted_at}', to_jsonb(now()))
                WHERE id = $1"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r#"UPDATE livechat.chatbot_answers a
                  SET metadata = jsonb_set(a.metadata, '{deleted_at}', to_jsonb(now()))
                WHERE a.question_step_id = $1"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            r#"UPDATE livechat.chatbot_step_triggers t
                  SET metadata = jsonb_set(t.metadata, '{deleted_at}', to_jsonb(now()))
                WHERE t.answer_id IN
                      (SELECT id FROM livechat.chatbot_answers WHERE question_step_id = $1)"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }

    // ── Chatbot answers ────────────────────────────────────────────

    pub async fn answer_list(&self, question_step_id: Uuid) -> Result<Vec<JsonRow>, LivechatError> {
        let rows = fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"SELECT to_jsonb(e) FROM livechat.chatbot_answers e
                    WHERE e.question_step_id = $1 AND e.metadata->>'deleted_at' IS NULL
                    ORDER BY e.sequence"#,
            )
            .bind(question_step_id),
        )
        .await?;
        Ok(rows.into_iter().map(|(j,)| j.0).collect())
    }

    /// Create an answer on a QUESTION step only (the closure law at
    /// the join).
    pub async fn answer_create(
        &self,
        question_step_id: Uuid,
        input: &AnswerInput,
    ) -> Result<JsonRow, LivechatError> {
        if input.label.trim().is_empty() {
            return Err(LivechatError::Validation("answer label is required".into()));
        }
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"INSERT INTO livechat.chatbot_answers
                       (question_step_id, sequence, label, redirect_url)
                   SELECT $1, $2, $3, $4
                    WHERE EXISTS (SELECT 1 FROM livechat.chatbot_steps s
                                   WHERE s.id = $1 AND s.step_type = 'question_selection')
                   RETURNING to_jsonb(chatbot_answers)"#,
            )
            .bind(question_step_id)
            .bind(input.sequence)
            .bind(input.label.trim())
            .bind(&input.redirect_url),
        )
        .await?
        .map(|(j,)| j.0);
        row.ok_or_else(|| {
            LivechatError::Validation(
                "answers attach to question steps (this step is not a question)".into(),
            )
        })
    }

    pub async fn answer_delete(&self, id: Uuid) -> Result<bool, LivechatError> {
        let n = execute_scoped(
            &self.pool,
            sqlx::query(
                r#"UPDATE livechat.chatbot_answers
                      SET metadata = jsonb_set(metadata, '{{deleted_at}}', to_jsonb(now()))
                    WHERE id = $1 AND metadata->>'deleted_at' IS NULL"#,
            )
            .bind(id),
        )
        .await?;
        Ok(n.rows_affected() > 0)
    }

    // ── Chatbot step triggers (forward-only routing) ───────────────

    pub async fn trigger_list(
        &self,
        answer_id: Option<Uuid>,
    ) -> Result<Vec<JsonRow>, LivechatError> {
        let rows = fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"SELECT to_jsonb(e) FROM livechat.chatbot_step_triggers e
                    WHERE e.metadata->>'deleted_at' IS NULL
                      AND ($1::uuid IS NULL OR e.answer_id = $1)"#,
            )
            .bind(answer_id),
        )
        .await?;
        Ok(rows.into_iter().map(|(j,)| j.0).collect())
    }

    /// Create a trigger — FORWARD-ONLY: the target step's sequence
    /// must be GREATER than the answer's step sequence, and both
    /// steps must belong to the SAME script.
    pub async fn trigger_create(
        &self,
        answer_id: Uuid,
        target_step_id: Uuid,
    ) -> Result<JsonRow, LivechatError> {
        let ok = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (i64,)>(
                r#"SELECT 1::int8
                     FROM livechat.chatbot_answers a
                     JOIN livechat.chatbot_steps src ON src.id = a.question_step_id
                     JOIN livechat.chatbot_steps dst ON dst.id = $2
                    WHERE a.id = $1
                      AND src.chatbot_script_id = dst.chatbot_script_id
                      AND dst.sequence > src.sequence"#,
            )
            .bind(answer_id)
            .bind(target_step_id),
        )
        .await?;
        if ok.is_none() {
            return Err(LivechatError::Validation(
                "trigger routing is forward-only: the target must be a later step of the same script"
                    .into(),
            ));
        }
        let row = fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, (sqlx::types::Json<Value>,)>(
                r#"INSERT INTO livechat.chatbot_step_triggers
                       (answer_id, target_step_id)
                   VALUES ($1, $2)
                   RETURNING to_jsonb(chatbot_step_triggers)"#,
            )
            .bind(answer_id)
            .bind(target_step_id),
        )
        .await?
        .map(|(j,)| j.0);
        row.ok_or_else(|| LivechatError::Database("trigger create returned no row".into()))
    }

    pub async fn trigger_delete(&self, id: Uuid) -> Result<bool, LivechatError> {
        let n = execute_scoped(
            &self.pool,
            sqlx::query(
                r#"UPDATE livechat.chatbot_step_triggers
                      SET metadata = jsonb_set(metadata, '{{deleted_at}}', to_jsonb(now()))
                    WHERE id = $1 AND metadata->>'deleted_at' IS NULL"#,
            )
            .bind(id),
        )
        .await?;
        Ok(n.rows_affected() > 0)
    }
}

// ── Validation helpers ────────────────────────────────────────────

/// The two tag tables this repository serves (the guard against
/// SQL injection through the `table` parameter).
fn tag_table(table: &str) -> Result<&'static str, LivechatError> {
    match table {
        "conversation" => Ok("livechat.conversation_tags"),
        "expertise" => Ok("livechat.expertise_tags"),
        _ => Err(LivechatError::Validation(
            "tag table must be 'conversation' or 'expertise'".into(),
        )),
    }
}

/// The save-time rule validation: the regex must COMPILE and must
/// not be empty (match-all is an explicit `.*`).
fn validate_rule(input: &RuleInput) -> Result<(), LivechatError> {
    if input.regex_url.trim().is_empty() {
        return Err(LivechatError::RuleRegexInvalid);
    }
    if regex::Regex::new(&input.regex_url).is_err() {
        return Err(LivechatError::RuleRegexInvalid);
    }
    if !matches!(
        input.action.as_str(),
        "display_button" | "display_button_and_text" | "auto_popup" | "hide_button"
    ) {
        return Err(LivechatError::Validation(
            "action must be display_button | display_button_and_text | auto_popup | hide_button"
                .into(),
        ));
    }
    if !matches!(
        input.chatbot_enabled_condition.as_str(),
        "always" | "only_if_no_operator" | "only_if_operator"
    ) {
        return Err(LivechatError::Validation(
            "chatbot_enabled_condition must be always | only_if_no_operator | only_if_operator"
                .into(),
        ));
    }
    Ok(())
}

/// The step validation: the seven-type closure + question steps are
/// born with answers.
fn validate_step(input: &StepInput) -> Result<(), LivechatError> {
    if !STEP_TYPES.contains(&input.step_type.as_str()) {
        return Err(LivechatError::Validation(format!(
            "step_type must be one of the seven community types: {}",
            STEP_TYPES.join(", ")
        )));
    }
    // Only the SELECTION question carries declared answers; the email
    // and phone questions are validated free-text input steps (their
    // input is checked at answer time, not at save time).
    if input.step_type == "question_selection" && input.answers.is_empty() {
        return Err(LivechatError::Validation(
            "a question step is born with at least one answer".into(),
        ));
    }
    if input.step_type != "question_selection" && !input.answers.is_empty() {
        return Err(LivechatError::Validation(
            "answers attach to question steps only".into(),
        ));
    }
    Ok(())
}

/// Map a Postgres unique violation (23505) whose constraint name
/// contains `needle` onto the typed error; anything else is the
/// database error.
fn map_unique(e: sqlx::Error, needle: &str, err: LivechatError) -> LivechatError {
    if let sqlx::Error::Database(db) = &e {
        if db.code().map(|c| c == "23505").unwrap_or(false)
            && db.constraint().map(|c| c.contains(needle)).unwrap_or(false)
        {
            return err;
        }
    }
    e.into()
}

/// The step-sequence unique (one step per sequence per script).
fn unique_step(e: sqlx::Error) -> LivechatError {
    map_unique(
        e,
        "uq_chatbot_steps_script_sequence",
        LivechatError::Validation("a step already occupies this sequence in the script".into()),
    )
}
