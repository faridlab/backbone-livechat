//! One place this module records an audited fact.
//!
//! Consolidated from `livechat.livechat_audit_log` onto `auditlog.audit_trails`
//! — the shared trail the record-history and activity-feed surfaces read.
//!
//! The actor is passed explicitly rather than left to the trail's session-GUC
//! default: a sweep runs outside any request and would otherwise be attributed
//! to `system` even when the verb knew who asked for it. The event vocabulary
//! carries over verbatim into `action`; it used to be constrained by the
//! `livechat_audit_event` enum, and the shared column is text, so the values
//! survive but the constraint does not.

use uuid::Uuid;

/// Record one audited fact in the caller's transaction.
pub async fn record_audit(
    exec: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    action: &str,
    actor: Option<Uuid>,
    subject_type: &str,
    subject_id: Option<Uuid>,
    detail: serde_json::Value,
) -> Result<(), sqlx::Error> {
    backbone_auditlog::application::service::append(
        exec,
        backbone_auditlog::application::service::AuditEvent {
            event_type: backbone_auditlog::domain::entity::AuditEventType::DataChange,
            action: action.to_string(),
            // Schema-qualified, matching what the capture trigger writes, so a
            // verb row and a trigger row key the same way.
            subject_type: Some(if subject_type.contains('.') {
                subject_type.to_string()
            } else {
                format!("livechat.{subject_type}")
            }),
            subject_id: subject_id.map(|id| id.to_string()),
            changed: Some(detail),
            reason: None,
            status: backbone_auditlog::domain::entity::AuditStatus::Success,
            actor: actor.map(|id| id.to_string()),
        },
    )
    .await
    .map(|_| ())
}
