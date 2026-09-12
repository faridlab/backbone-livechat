//! The audit trail probe: every decision leaves its row (open,
//! assign, help flags, close — each with actor and subject), the
//! take is first-wins typed, and the event vocabulary is a CLOSED
//! enum (an unknown event cannot be written).

use uuid::Uuid;

use backbone_livechat::application::service::livechat_error::LivechatError;
use backbone_livechat::application::service::notifier_port::UnwiredNotifier;
use backbone_livechat::application::service::session_service::SessionCommandService;
use backbone_livechat::application::service::transcript_port::RefusingTranscriptMailer;

use super::common::{open_session, seed_channel_with_operators, TestDb};

#[tokio::test]
async fn every_decision_leaves_its_row_and_the_vocabulary_is_closed() {
    let db = TestDb::new("audit").await;
    let pool = db.pool.clone();
    let website = Uuid::new_v4();
    let op_a = Uuid::new_v4();
    let op_b = Uuid::new_v4();
    let channel = seed_channel_with_operators(&pool, website, &[op_a, op_b]).await;

    let sessions = SessionCommandService::new(
        pool.clone(),
        std::sync::Arc::new(super::common::RecordingMailCarrier::default()),
        std::sync::Arc::new(UnwiredNotifier),
        std::sync::Arc::new(RefusingTranscriptMailer),
    );

    // The full human lifecycle, each verb with its actor.
    let session = open_session(&pool, channel, "audit:visitor").await;
    let taken = sessions
        .take(session.id, op_a, Some(op_a))
        .await
        .unwrap_or_else(|e| panic!("take: {e:?}"));
    assert_eq!(
        taken.operator_user_id,
        Some(op_a),
        "the first take wins the row"
    );
    sessions
        .set_need_help(session.id, true, Some(op_a))
        .await
        .unwrap_or_else(|e| panic!("help on: {e:?}"));
    sessions
        .set_need_help(session.id, false, Some(op_a))
        .await
        .unwrap_or_else(|e| panic!("help off: {e:?}"));

    // The serialized loser: a second operator's take is the typed 409.
    let refused = sessions.take(session.id, op_b, Some(op_b)).await;
    assert!(
        matches!(refused, Err(LivechatError::OperatorBusy)),
        "the second take loses first-wins typed, got {refused:?}"
    );

    sessions
        .close(session.id, "operator_closed", Some(op_a))
        .await
        .unwrap_or_else(|e| panic!("close: {e:?}"));

    // ── Every decision left its audit row ─────────────────────────
    let trail: Vec<(String, Option<Uuid>)> = sqlx::query_as(
        r#"SELECT event::text, actor FROM livechat.livechat_audit_log
            WHERE subject_id = $1 ORDER BY created_at, id"#,
    )
    .bind(session.id)
    .fetch_all(&pool)
    .await
    .unwrap_or_else(|e| panic!("trail read: {e}"));
    let events: Vec<&str> = trail.iter().map(|(e, _)| e.as_str()).collect();
    for expected in [
        "session_opened",
        "operator_assigned",
        "help_requested",
        "help_resolved",
        "operator_busy",
        "session_closed",
    ] {
        assert!(
            events.contains(&expected),
            "the trail must carry {expected}, got {events:?}"
        );
    }
    assert!(
        trail
            .iter()
            .filter(|(e, _)| e == "operator_assigned")
            .all(|(_, actor)| *actor == Some(op_a)),
        "the assignment audit names the acting operator"
    );
    // The assignment row carries the replay facts (the ladder probe
    // re-checks the full set; here the presence of the rung).
    let (with_rung,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM livechat.livechat_audit_log
            WHERE subject_id = $1 AND event = 'operator_assigned'
              AND detail ? 'rung' AND detail ? 'candidates_considered'"#,
    )
    .bind(session.id)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("rung read: {e}"));
    assert_eq!(
        with_rung, 1,
        "the assignment audit carries its replay facts"
    );

    // ── The event vocabulary is CLOSED: an unknown event refuses ──
    let err = sqlx::query(
        r#"INSERT INTO livechat.livechat_audit_log (event, subject_type, subject_id, detail)
           VALUES ('made_up_event', 'session', $1, '{}'::jsonb)"#,
    )
    .bind(session.id)
    .execute(&pool)
    .await
    .err()
    .unwrap_or_else(|| panic!("an event outside the enum must be refused by the DB"));
    assert!(
        err.to_string().contains("livechat_audit_event"),
        "the enum wall fires naming the type, got {err}"
    );

    // The closed reason vocabulary too: a made-up reason refuses at
    // the DB cast (the session row already exists; only the cast is
    // under test).
    let err = sqlx::query(
        r#"UPDATE livechat.sessions SET close_reason = 'made_up_reason' WHERE id = $1"#,
    )
    .bind(session.id)
    .execute(&pool)
    .await
    .err()
    .unwrap_or_else(|| panic!("a close reason outside the enum must be refused"));
    assert!(
        err.to_string().contains("livechat_close_reason"),
        "the close-reason wall fires naming the type, got {err}"
    );

    db.dispose().await;
}
