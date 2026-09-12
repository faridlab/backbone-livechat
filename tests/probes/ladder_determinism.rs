//! The deterministic-ladder probe: no randomness, the 120s buffer
//! INSIDE the pool on every path, the ONE 30-minute ongoing window,
//! the total-order tie-break, capacity gating, and no read-path GC.

use uuid::Uuid;

use backbone_livechat::application::service::livechat_error::LivechatError;
use backbone_livechat::infrastructure::persistence::{
    AssignInput, AssignOutcome, SelectionRepository, ASSIGNMENT_BUFFER_SECS, ONGOING_WINDOW_SECS,
    PRESENCE_WINDOW_SECS,
};

use super::common::{open_session, seed_channel_with_operators, TestDb};

fn op_uuid(low: u128) -> Uuid {
    // Deterministic operator ids: the total-order tie-break probe
    // depends on a KNOWN user_id ordering.
    Uuid::from_u128(0xA000_0000_0000_0000_0000_0000_0000_0000u128 | low)
}

#[tokio::test]
async fn ladder_is_deterministic_with_buffer_and_one_window() {
    let db = TestDb::new("ladder").await;
    let pool = db.pool.clone();
    let website = Uuid::new_v4();

    // ── 1. The total-order tie-break replaces the die roll ────────
    // Two operators in IDENTICAL ladder state (same rung, both
    // last_assigned_at NULL): the pick is the smaller user_id, and
    // the SAME pick comes out on a repeated query (faithful replay,
    // not a sample).
    let op_small = op_uuid(0x01);
    let op_large = op_uuid(0x02);
    assert!(op_small < op_large, "fixture must order the ids");
    let channel = seed_channel_with_operators(&pool, website, &[op_small, op_large]).await;
    let selection = SelectionRepository::new(pool.clone());

    let pick_a = selection
        .pick_operator(channel, None, None, &[], None)
        .await
        .unwrap_or_else(|e| panic!("pick failed: {e:?}"))
        .unwrap_or_else(|| panic!("pick returned no candidate"));
    let pick_b = selection
        .pick_operator(channel, None, None, &[], None)
        .await
        .unwrap_or_else(|e| panic!("second pick failed: {e:?}"))
        .unwrap_or_else(|| panic!("second pick returned no candidate"));
    assert_eq!(
        pick_a.operator_user_id, pick_b.operator_user_id,
        "the ladder must be a faithful replay, not a random sample"
    );
    assert_eq!(
        pick_a.operator_user_id, op_small,
        "the tie-break is user_id ASC (the total order), not a die roll"
    );
    assert_eq!(
        pick_a.rung, 9,
        "unmatched candidates land on the fallback rung"
    );

    // ── 2. The buffer is INSIDE the pool (every path) ─────────────
    // First assign lands on op_small (the tie-break), stamping its
    // last_assigned_at. The NEXT assign — even with the stickiness
    // arm pointing at op_small — cannot read around the 120s buffer.
    assert_eq!(ASSIGNMENT_BUFFER_SECS, 120, "the anti-burst buffer is 120s");
    let s0 = open_session(&pool, channel, "ladder:buffer:0").await;
    let outcome = selection
        .assign(&AssignInput {
            session_id: s0.id,
            channel_id: channel,
            previous_operator: None,
            visitor_language: None,
            expertise: Vec::new(),
            visitor_country: None,
            actor: None,
        })
        .await
        .unwrap_or_else(|e| panic!("first assign failed: {e:?}"));
    match &outcome {
        AssignOutcome::Assigned {
            operator_user_id, ..
        } => {
            assert_eq!(
                *operator_user_id, op_small,
                "the first assign follows the tie-break"
            );
        }
        AssignOutcome::Empty => panic!("first assign hit an empty pool unexpectedly"),
    }

    let s1 = open_session(&pool, channel, "ladder:buffer:1").await;
    let outcome = selection
        .assign(&AssignInput {
            session_id: s1.id,
            channel_id: channel,
            previous_operator: Some(op_small),
            visitor_language: None,
            expertise: Vec::new(),
            visitor_country: None,
            actor: None,
        })
        .await
        .unwrap_or_else(|e| panic!("buffered assign failed: {e:?}"));
    match &outcome {
        AssignOutcome::Assigned {
            operator_user_id,
            rung,
            ..
        } => {
            assert_eq!(
                *operator_user_id, op_large,
                "the freshly-assigned operator is buffered OUT of the pool — \
                 even the stickiness arm cannot bypass the 120s buffer"
            );
            assert_ne!(*rung, 0, "rung 0 cannot fire for a buffered-out operator");
        }
        AssignOutcome::Empty => panic!("buffered assign hit an empty pool unexpectedly"),
    }

    // The assign audit row carries the replay facts (rung,
    // candidates, buffer, one window).
    let (audit_count,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM livechat.livechat_audit_log
            WHERE event = 'operator_assigned' AND subject_id = $1
              AND detail->>'buffer_applied' = 'true'
              AND (detail->>'buffer_secs')::int = $2
              AND (detail->>'window_secs')::int = $3"#,
    )
    .bind(s0.id)
    .bind(ASSIGNMENT_BUFFER_SECS)
    .bind(ONGOING_WINDOW_SECS)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("audit read failed: {e}"));
    assert_eq!(
        audit_count, 1,
        "every assignment decision is audited with its replay facts"
    );

    // ── 3. The stickiness arm (rung 0) fires INSIDE the pool ──────
    // Backdate the first operator's stamp past the buffer: the
    // previous operator now wins through rung 0.
    sqlx::query(
        r#"UPDATE livechat.operator_profiles
              SET last_assigned_at = now() - interval '10 minutes'
            WHERE user_id = $1"#,
    )
    .bind(op_small)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("backdate failed: {e}"));
    let s2 = open_session(&pool, channel, "ladder:buffer:2").await;
    let outcome = selection
        .assign(&AssignInput {
            session_id: s2.id,
            channel_id: channel,
            previous_operator: Some(op_small),
            visitor_language: None,
            expertise: Vec::new(),
            visitor_country: None,
            actor: None,
        })
        .await
        .unwrap_or_else(|e| panic!("stickiness assign failed: {e:?}"));
    match &outcome {
        AssignOutcome::Assigned {
            operator_user_id,
            rung,
            ..
        } => {
            assert_eq!(
                *operator_user_id, op_small,
                "the stickiness arm wins once unbuffered"
            );
            assert_eq!(*rung, 0, "previous-operator match is rung 0");
        }
        AssignOutcome::Empty => panic!("stickiness arm found an empty pool unexpectedly"),
    }

    // ── 4. ONE 30-minute window (the divergence resolver) ─────────
    assert_eq!(
        ONGOING_WINDOW_SECS, 1800,
        "the ONE ongoing window is 30 minutes"
    );
    assert_eq!(
        PRESENCE_WINDOW_SECS, 60,
        "presence is the 60s heartbeat window"
    );
    // A session whose last interest is 40 minutes old does NOT count
    // as ongoing (the 15-months arm is gone); 20 minutes old DOES.
    let (old_counts,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM livechat.sessions
            WHERE closed_at IS NULL
              AND last_interest_at >= now() - make_interval(secs => $1)"#,
    )
    .bind(ONGOING_WINDOW_SECS)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("window read failed: {e}"));
    assert_eq!(
        old_counts, 3,
        "the three live sessions are inside the window"
    );
    sqlx::query("UPDATE livechat.sessions SET last_interest_at = now() - interval '40 minutes' WHERE id = $1")
        .bind(s1.id)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("ageing failed: {e}"));
    let (after_ageing,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM livechat.sessions
            WHERE closed_at IS NULL
              AND last_interest_at >= now() - make_interval(secs => $1)"#,
    )
    .bind(ONGOING_WINDOW_SECS)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("window re-read failed: {e}"));
    assert_eq!(
        after_ageing, 2,
        "a 40-minute-old session is OUTSIDE the one window"
    );

    // ── 5. Capacity gating uses the SAME window ───────────────────
    let cap_a = op_uuid(0x11);
    let cap_b = op_uuid(0x12);
    let capped = seed_channel_with_operators(&pool, website, &[cap_a, cap_b]).await;
    sqlx::query(
        r#"UPDATE livechat.channels
              SET max_sessions_mode = 'limited', max_sessions = 1
            WHERE id = $1"#,
    )
    .bind(capped)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("capacity shape failed: {e}"));
    let busy_session = open_session(&pool, capped, "ladder:cap:busy").await;
    sqlx::query("UPDATE livechat.sessions SET operator_user_id = $2 WHERE id = $1")
        .bind(busy_session.id)
        .bind(cap_a)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("busy assignment failed: {e}"));
    sqlx::query("UPDATE livechat.operator_profiles SET last_assigned_at = NULL WHERE user_id = $1")
        .bind(cap_a)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("stamp clear failed: {e}"));
    let pick = selection
        .pick_operator(capped, Some(cap_a), None, &[], None)
        .await
        .unwrap_or_else(|e| panic!("capacity pick failed: {e:?}"))
    .unwrap_or_else(|| panic!("capacity pick returned no candidate"));
    assert_eq!(
        pick.operator_user_id, cap_b,
        "a capacity-full operator leaves the pool — the stickiness arm included"
    );

    // ── 6. The empty pool is a defined, audited path ─────────────
    let empty_website = Uuid::new_v4();
    let solo = op_uuid(0x21);
    let empty_channel =
        seed_channel_with_operators(&pool, empty_website, &[solo]).await;
    sqlx::query("UPDATE livechat.operator_profiles SET last_heartbeat_at = now() - interval '10 minutes' WHERE user_id = $1")
        .bind(solo)
        .execute(&pool)
        .await
        .unwrap_or_else(|e| panic!("heartbeat ageing failed: {e}"));
    let s3 = open_session(&pool, empty_channel, "ladder:empty").await;
    let outcome = selection
        .assign(&AssignInput {
            session_id: s3.id,
            channel_id: empty_channel,
            previous_operator: Some(solo),
            visitor_language: None,
            expertise: Vec::new(),
            visitor_country: None,
            actor: None,
        })
        .await
        .unwrap_or_else(|e| panic!("empty-pool assign failed: {e:?}"));
    assert!(
        matches!(outcome, AssignOutcome::Empty),
        "a dead heartbeat empties the pool"
    );
    let (empty_audits,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM livechat.livechat_audit_log
            WHERE event = 'assignment_empty' AND subject_id = $1"#,
    )
    .bind(s3.id)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("empty audit read failed: {e}"));
    assert_eq!(empty_audits, 1, "the no-agent decision is audited too");

    // ── 7. NO READ-PATH GC ────────────────────────────────────────
    // A read (the pick) must not destroy anything: counts before and
    // after are identical, and no read path removes rows.
    let (before,): (i64,) = sqlx::query_as(
        r#"SELECT (SELECT count(*) FROM livechat.sessions)
                 + (SELECT count(*) FROM livechat.member_histories)
                 + (SELECT count(*) FROM livechat.livechat_audit_log)"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("count failed: {e}"));
    let _ = selection
        .pick_operator(channel, None, None, &[], None)
        .await
        .unwrap_or_else(|e| panic!("gc probe pick failed: {e:?}"));
    let (after,): (i64,) = sqlx::query_as(
        r#"SELECT (SELECT count(*) FROM livechat.sessions)
                 + (SELECT count(*) FROM livechat.member_histories)
                 + (SELECT count(*) FROM livechat.livechat_audit_log)"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("recount failed: {e}"));
    assert_eq!(
        before, after,
        "the ladder's read path mints NOTHING and GCs NOTHING"
    );

    // The pick on the scratch owner pool must always answer — the
    // default-deny posture lives in the posture probe, the
    // determinism contract here.
    let pick_again = selection
        .pick_operator(channel, None, None, &[], None)
        .await;
    assert!(
        pick_again.is_ok(),
        "the pick on the owner pool must answer, got {pick_again:?}"
    );
    let _ = LivechatError::SessionNotFound; // link the typed family
    db.dispose().await;
}
