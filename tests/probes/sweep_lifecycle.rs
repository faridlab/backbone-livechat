//! The sweep probe: the two scheduled passes (idle-close, invite
//! expiry) are bounded, audited, per-record-outcome-carrying, and
//! NEVER delete a row. GC lives in the sweep — never on a read path.

use chrono::{Duration, Utc};
use uuid::Uuid;

use backbone_livechat::application::service::notifier_port::UnwiredNotifier;
use backbone_livechat::application::service::sweep_service::{
    SweepService, DEFAULT_IDLE_CLOSE_HOURS, DEFAULT_INVITE_EXPIRY_HOURS,
};
use backbone_livechat::application::service::website_request_service::WebsiteRequestService;
use backbone_livechat::infrastructure::persistence::SWEEP_BATCH;

use super::common::{open_session, seed_channel_with_operators, StubWebsiteBridge, TestDb};

#[tokio::test]
async fn sweeps_close_and_expire_without_ever_deleting() {
    let db = TestDb::new("sweep").await;
    let pool = db.pool.clone();
    let website = Uuid::new_v4();
    let operator = Uuid::new_v4();
    let channel = seed_channel_with_operators(&pool, website, &[operator]).await;

    // Three open sessions: one fresh, two idled past the horizon.
    let fresh = open_session(&pool, channel, "sweep:fresh").await;
    let idle_a = open_session(&pool, channel, "sweep:idle-a").await;
    let idle_b = open_session(&pool, channel, "sweep:idle-b").await;
    // Give the idle pair a FAILURE shape the outcome derive must
    // carry per record (idle_a escalated via two agent ledger rows;
    // idle_b plain no_answer).
    sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, operator_user_id, expertise_names)
           VALUES ($1, 'agent', $2, '{}'), ($1, 'agent', $3, '{}')"#,
    )
    .bind(idle_a.id)
    .bind(operator)
    .bind(Uuid::new_v4())
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("escalation seed: {e}"));

    // A pending invite backdated past the invite horizon.
    let bridge = std::sync::Arc::new(StubWebsiteBridge::new("sweep.example", website, Uuid::new_v4()));
    let visitor_id = Uuid::new_v4();
    bridge.register_visitor(visitor_id, "sweep:invitee", Some("ID"));
    let bridge_dyn: std::sync::Arc<
        dyn backbone_livechat::application::service::website_bridge::LivechatWebsiteBridge,
    > = bridge.clone();
    let requests = WebsiteRequestService::new(
        pool.clone(),
        bridge_dyn,
        std::sync::Arc::new(UnwiredNotifier),
    );
    let invite = requests
        .create_request(website, visitor_id, Some(operator))
        .await
        .unwrap_or_else(|e| panic!("invite create: {e:?}"));
    assert!(invite.is_pending_request, "the invite is born pending");

    let now = Utc::now();
    sqlx::query("UPDATE livechat.sessions SET last_interest_at = $2 - interval '30 hours' WHERE id = ANY($1)")
        .bind(vec![idle_a.id, idle_b.id])
        .bind(now)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("idle backdate: {e}"));
    // The invite's clock is its metadata created_at stamp.
    sqlx::query(
        r#"UPDATE livechat.sessions
              SET metadata = jsonb_set(metadata, '{created_at}',
                    to_jsonb(($2 - interval '30 hours')::timestamptz))
            WHERE id = $1"#,
    )
    .bind(invite.id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("invite backdate: {e}"));

    // The declared horizons and the batch bound.
    assert_eq!(DEFAULT_IDLE_CLOSE_HOURS, 24, "the declared idle horizon");
    assert_eq!(
        DEFAULT_INVITE_EXPIRY_HOURS, 24,
        "the declared invite horizon"
    );
    assert!(SWEEP_BATCH > 0, "the sweep batch bound is positive");
    let sweeps = SweepService::new(pool.clone(), 24, 24);
    assert_eq!(sweeps.horizons(), (24, 24), "the horizons are carried");

    let (before_sessions, before_audit): (i64, i64) = sqlx::query_as(
        r#"SELECT (SELECT count(*) FROM livechat.sessions),
                  (SELECT count(*) FROM livechat.livechat_audit_log)"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("before counts: {e}"));

    let outcome = sweeps
        .sweep_at(now + Duration::seconds(1))
        .await
        .unwrap_or_else(|e| panic!("sweep: {e:?}"));

    // The idle pair closed; the fresh session untouched.
    let mut closed_ids = outcome.idle_closed.clone();
    closed_ids.sort();
    let mut expected_ids = vec![idle_a.id, idle_b.id];
    expected_ids.sort();
    assert_eq!(closed_ids, expected_ids, "both idled sessions closed");
    assert_eq!(
        outcome.invites_expired,
        vec![invite.id],
        "the stale invite expired"
    );
    let shapes: Vec<(Uuid, bool, Option<String>, Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT id, (closed_at IS NOT NULL), status::text, close_reason::text, outcome::text
             FROM livechat.sessions WHERE id = ANY($1)"#,
    )
    .bind(vec![fresh.id, idle_a.id, idle_b.id, invite.id])
    .fetch_all(&pool)
    .await
    .unwrap_or_else(|e| panic!("shape read: {e}"));
    let by_id = |id: Uuid| shapes.iter().find(|(sid, ..)| *sid == id).unwrap();
    let (_, fresh_closed, fresh_status, _, _) = by_id(fresh.id);
    assert!(!fresh_closed, "the fresh session survives the sweep");
    assert!(
        fresh_status.is_some(),
        "the fresh session keeps its live status"
    );
    let (_, a_closed, a_status, a_reason, a_outcome) = by_id(idle_a.id);
    assert!(a_closed, "idle_a closed");
    assert!(
        a_status.is_none(),
        "a closed session carries NO live status"
    );
    assert_eq!(
        a_reason.as_deref(),
        Some("expired"),
        "the idle close reason"
    );
    assert_eq!(
        a_outcome.as_deref(),
        Some("escalated"),
        "the sweep derives the outcome PER RECORD"
    );
    let (_, b_closed, _, b_reason, b_outcome) = by_id(idle_b.id);
    assert!(b_closed, "idle_b closed");
    assert_eq!(b_reason.as_deref(), Some("expired"));
    assert_eq!(
        b_outcome.as_deref(),
        Some("no_answer"),
        "idle_b carries ITS OWN outcome, not idle_a's"
    );
    let (_, i_closed, i_pending, i_reason, _): (Uuid, bool, bool, Option<String>, Option<String>) =
        sqlx::query_as(
            r#"SELECT id, (closed_at IS NOT NULL), is_pending_request, close_reason::text, outcome::text
                 FROM livechat.sessions WHERE id = $1"#,
        )
        .bind(invite.id)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|e| panic!("invite shape: {e}"));
    assert!(
        i_closed && !i_pending,
        "the expired invite closed and cleared its flag"
    );
    assert_eq!(i_reason.as_deref(), Some("expired"));

    // Every swept id is audited, and NO row was deleted.
    let (audit_closed, audit_invite_expired, after_sessions, after_audit): (i64, i64, i64, i64) =
        sqlx::query_as(
            r#"SELECT (SELECT count(*) FROM livechat.livechat_audit_log
                        WHERE event = 'session_closed' AND subject_id = ANY($1)
                          AND detail->>'via' = 'idle_sweep'),
                      (SELECT count(*) FROM livechat.livechat_audit_log
                        WHERE event = 'invite_expired' AND subject_id = $2),
                      (SELECT count(*) FROM livechat.sessions),
                      (SELECT count(*) FROM livechat.livechat_audit_log)"#,
        )
        .bind(vec![idle_a.id, idle_b.id])
        .bind(invite.id)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|e| panic!("audit counts: {e}"));
    assert_eq!(audit_closed, 2, "each idle close is audited");
    assert_eq!(audit_invite_expired, 1, "the invite expiry is audited");
    assert_eq!(after_sessions, before_sessions, "the sweep deletes NOTHING");
    assert!(
        after_audit > before_audit,
        "the sweep's decisions all left audit rows"
    );

    // A second sweep over the same ground is a no-op (idempotent).
    let outcome = sweeps
        .sweep_at(now + Duration::seconds(2))
        .await
        .unwrap_or_else(|e| panic!("second sweep: {e:?}"));
    assert!(
        outcome.idle_closed.is_empty(),
        "the second idle pass finds nothing"
    );
    assert!(
        outcome.invites_expired.is_empty(),
        "the second invite pass finds nothing"
    );

    db.dispose().await;
}
