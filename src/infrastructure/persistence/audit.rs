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

/// Stamp one audited fact from a verb that holds a pool rather than a
/// transaction.
///
/// The shared trail is org-fenced and its guard runs as a BEFORE INSERT
/// trigger, so it fires first: a row written on a connection carrying no scope
/// has no unit, is refused, and the refusal rolls back the business write that
/// triggered the audit. A bare pool acquire is always such a connection,
/// because the request's scope lives on a different one.
///
/// So this opens a short transaction and relays the caller's ambient scope onto
/// it. The transaction is required rather than incidental: the scope binder
/// sets its variables LOCAL, and outside a transaction they are gone before the
/// next statement. Outside any request scope nothing is bound and the write
/// behaves exactly as it did before.
pub async fn record_audit_on_pool(
    pool: &sqlx::PgPool,
    action: &str,
    actor: Option<Uuid>,
    subject_type: &str,
    subject_id: Option<Uuid>,
    detail: serde_json::Value,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    if let Some(scope) = backbone_orm::org_scope::current_org_scope() {
        backbone_orm::org_scope::bind_org_scope_on(&mut tx, &scope).await?;
    }
    record_audit(&mut *tx, action, actor, subject_type, subject_id, detail).await?;
    tx.commit().await?;
    Ok(())
}
