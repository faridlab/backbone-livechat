//! The sweep repository (hand-written; user-owned; see
//! `metaphor.codegen.yaml`): the two declared scheduled passes —
//! GC lives HERE, never on a read path.
//!
//! Shape: bounded set-based closes —
//! `UPDATE ... WHERE id IN (SELECT ... FOR UPDATE SKIP LOCKED
//! LIMIT $batch)` — idempotent on `closed_at IS NULL`, per-row audits
//! bulk-inserted from the RETURNING ids (one `unnest` INSERT, never
//! a loop), batch const 200. No leases (single-statement closes); no
//! walker holds rows across awaits. NO HARD DELETE exists anywhere
//! in the module — the 1-hour message-less unlink is refused; rows
//! survive with audited endings.
//!
//! The sweep runs on the host jobs loop; row scoping there is owned
//! by the composing service's tenancy decorator, not by this module
//! (ADR-0029).

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::application::service::livechat_error::LivechatError;

use super::selection_repository::recompute_outcomes_batch_tx;
use super::relay_ambient_scope;

/// The bounded-batch size.
pub const SWEEP_BATCH: i64 = 200;

/// One sweep pass's outcome (both lists are the ids the pass acted
/// on — the jobs loop's log line).
#[derive(Debug, Clone, Default)]
pub struct SweepOutcome {
    pub idle_closed: Vec<Uuid>,
    pub invites_expired: Vec<Uuid>,
}

pub struct SweepRepository {
    pool: PgPool,
}

impl SweepRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// The idle-close pass: OPEN sessions whose `last_interest_at`
    /// is older than the cutoff (the caller derives it from
    /// `LIVECHAT_IDLE_CLOSE_HOURS`, default 24) close with reason
    /// `expired`; outcomes recompute per record; every close is
    /// audited `session_closed`.
    pub async fn idle_close(&self, idle_cutoff: DateTime<Utc>) -> Result<Vec<Uuid>, LivechatError> {
        let mut tx = self.pool.begin().await?;
        relay_ambient_scope(&mut tx).await?;
        let closed: Vec<(Uuid,)> = sqlx::query_as(
            r#"UPDATE livechat.sessions s
                  SET closed_at = now(), close_reason = 'expired', status = NULL
                 WHERE s.id IN (SELECT id FROM livechat.sessions
                                WHERE closed_at IS NULL
                                  AND last_interest_at < $1
                                ORDER BY last_interest_at
                                FOR UPDATE SKIP LOCKED LIMIT $2)
             RETURNING s.id"#,
        )
        .bind(idle_cutoff)
        .bind(SWEEP_BATCH)
        .fetch_all(&mut *tx)
        .await?;
        let ids: Vec<Uuid> = closed.into_iter().map(|(id,)| id).collect();
        if !ids.is_empty() {
            recompute_outcomes_batch_tx(&mut tx, &ids).await?;
            bulk_audit(&mut tx, "session_closed", "session", &ids, {
                serde_json::json!({ "close_reason": "expired", "via": "idle_sweep" })
            })
            .await?;
        }
        tx.commit().await?;
        Ok(ids)
    }

    /// The invite-expiry pass: pending invites older than the
    /// cutoff clear `is_pending_request` and close (reason
    /// `expired`), audited `invite_expired` + `session_closed`; the
    /// rows survive (no untraced destroy).
    pub async fn expire_invites(
        &self,
        invite_cutoff: DateTime<Utc>,
    ) -> Result<Vec<Uuid>, LivechatError> {
        let mut tx = self.pool.begin().await?;
        relay_ambient_scope(&mut tx).await?;
        let expired: Vec<(Uuid,)> = sqlx::query_as(
            r#"UPDATE livechat.sessions s
                  SET is_pending_request = FALSE, closed_at = now(),
                      close_reason = 'expired', status = NULL
                 WHERE s.id IN (SELECT id FROM livechat.sessions
                                WHERE is_pending_request
                                  AND closed_at IS NULL
                                  AND (metadata ->> 'created_at')::timestamptz < $1
                                ORDER BY metadata ->> 'created_at'
                                FOR UPDATE SKIP LOCKED LIMIT $2)
             RETURNING s.id"#,
        )
        .bind(invite_cutoff)
        .bind(SWEEP_BATCH)
        .fetch_all(&mut *tx)
        .await?;
        let ids: Vec<Uuid> = expired.into_iter().map(|(id,)| id).collect();
        if !ids.is_empty() {
            recompute_outcomes_batch_tx(&mut tx, &ids).await?;
            bulk_audit(&mut tx, "invite_expired", "session", &ids, {
                serde_json::json!({ "close_reason": "expired", "via": "invite_expiry_sweep" })
            })
            .await?;
            bulk_audit(&mut tx, "session_closed", "session", &ids, {
                serde_json::json!({ "close_reason": "expired", "via": "invite_expiry_sweep" })
            })
            .await?;
        }
        tx.commit().await?;
        Ok(ids)
    }

    /// Both passes in order (the host jobs loop's single entry point).
    pub async fn sweep(
        &self,
        idle_cutoff: DateTime<Utc>,
        invite_cutoff: DateTime<Utc>,
    ) -> Result<SweepOutcome, LivechatError> {
        let idle_closed = self.idle_close(idle_cutoff).await?;
        let invites_expired = self.expire_invites(invite_cutoff).await?;
        Ok(SweepOutcome {
            idle_closed,
            invites_expired,
        })
    }
}

/// Per-row audits bulk-inserted from the close's RETURNING ids (one
/// `unnest` statement — never a loop of inserts).
async fn bulk_audit(
    tx: &mut sqlx::PgConnection,
    kind: &str,
    subject_type: &str,
    ids: &[Uuid],
    detail: serde_json::Value,
) -> Result<(), LivechatError> {
    sqlx::query(
        r#"INSERT INTO livechat.livechat_audit_log
               (event, actor, subject_type, subject_id, detail)
           SELECT $1::livechat_audit_event, NULL, $2, i, $3
             FROM unnest($4::uuid[]) AS i"#,
    )
    .bind(kind)
    .bind(subject_type)
    .bind(detail)
    .bind(ids)
    .execute(&mut *tx)
    .await?;
    Ok(())
}
