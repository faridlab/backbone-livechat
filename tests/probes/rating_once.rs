//! The rating probe: ONE rating per session (the DB UNIQUE is the
//! wall; a repeat is the typed 409 with an audited `rating_refused`),
//! the 1/5/10 scale validated at the verb AND by a CHECK, and the
//! persona dichotomy (agent|bot only).

use uuid::Uuid;

use backbone_livechat::application::service::livechat_error::LivechatError;
use backbone_livechat::application::service::notifier_port::UnwiredNotifier;
use backbone_livechat::application::service::rating_service::RatingSubmitService;
use backbone_livechat::infrastructure::persistence::upsert_agent_ledger_tx;

use super::common::{open_session, seed_channel_with_operators, TestDb};

/// A session with an operator bound (the agent-rating attribution
/// shape: the rating names the session's operator).
async fn agent_session(
    pool: &sqlx::PgPool,
    channel: Uuid,
    operator: Uuid,
    key: &str,
) -> backbone_livechat::infrastructure::persistence::SessionRow {
    let row = open_session(pool, channel, key).await;
    let mut tx = pool.begin().await.unwrap_or_else(|e| panic!("tx: {e}"));
    upsert_agent_ledger_tx(&mut tx, row.id, operator)
        .await
        .unwrap_or_else(|e| panic!("agent ledger: {e:?}"));
    sqlx::query("UPDATE livechat.sessions SET operator_user_id = $2 WHERE id = $1")
        .bind(row.id)
        .bind(operator)
        .execute(&mut *tx)
        .await
        .unwrap_or_else(|e| panic!("operator bind: {e}"));
    tx.commit().await.unwrap_or_else(|e| panic!("commit: {e}"));
    row
}

#[tokio::test]
async fn one_rating_per_session_and_the_scale_is_closed() {
    let db = TestDb::new("rating").await;
    let pool = db.pool.clone();
    let website = Uuid::new_v4();
    let operator = Uuid::new_v4();
    let channel = seed_channel_with_operators(&pool, website, &[operator]).await;
    let ratings = RatingSubmitService::new(pool.clone(), std::sync::Arc::new(UnwiredNotifier));

    // ── The happy path: an agent rating attributes to the operator ─
    let session = agent_session(&pool, channel, operator, "rating:visitor").await;
    let row = ratings
        .submit(session.id, 5, "agent", Some("fine"), None)
        .await
        .unwrap_or_else(|e| panic!("the first rating must land: {e:?}"));
    assert_eq!(row.value, 5);
    assert_eq!(
        row.operator_user_id,
        Some(operator),
        "an agent rating names the operator"
    );
    assert_eq!(row.chatbot_script_id, None);
    let found = ratings
        .find_for_session(session.id)
        .await
        .unwrap_or_else(|e| panic!("rating read: {e:?}"));
    assert!(found.is_some(), "the rating reads back once");

    // ── The once wall: a second submit is the TYPED 409 ────────────
    let refused = ratings.submit(session.id, 10, "agent", None, None).await;
    assert!(
        matches!(refused, Err(LivechatError::RatingAlreadySubmitted)),
        "a second rating is the typed once-wall refusal, got {refused:?}"
    );
    let (refused_audits, stored): (i64, i64) = sqlx::query_as(
        r#"SELECT (SELECT count(*) FROM livechat.livechat_audit_log
                     WHERE subject_id = $1 AND event = 'rating_refused'
                       AND detail->>'reason' = 'already_submitted'),
                  (SELECT count(*) FROM livechat.ratings WHERE session_id = $1)"#,
    )
    .bind(session.id)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("rating audit read: {e}"));
    assert_eq!(refused_audits, 1, "the refused repeat is audited");
    assert_eq!(stored, 1, "exactly ONE rating row exists (no overwrite)");

    // ── The scale: 3 is outside 1/5/10 ─────────────────────────────
    let scale_session = agent_session(&pool, channel, operator, "rating:scale").await;
    let refused = ratings
        .submit(scale_session.id, 3, "agent", None, None)
        .await;
    assert!(
        matches!(&refused, Err(LivechatError::Validation(msg)) if msg.contains("1, 5, 10")),
        "a value outside the scale refuses typed naming it, got {refused:?}"
    );
    let (scale_audits,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM livechat.livechat_audit_log
            WHERE subject_id = $1 AND event = 'rating_refused'
              AND detail->>'reason' LIKE '%scale%'"#,
    )
    .bind(scale_session.id)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("scale audit read: {e}"));
    assert_eq!(scale_audits, 1, "the scale refusal is audited");
    // The DB CHECK is the wall behind the verb: a raw out-of-scale
    // insert cannot land even bypassing the service.
    let err = sqlx::query(
        r#"INSERT INTO livechat.ratings (session_id, value, rated_persona, operator_user_id)
           VALUES ($1, 7, 'agent', $2)"#,
    )
    .bind(scale_session.id)
    .bind(operator)
    .execute(&pool)
    .await
    .err()
        .unwrap_or_else(|| panic!("the DB scale CHECK must refuse a raw 7"));
    assert!(
        err.to_string().contains("ck_ratings_value_scale"),
        "the scale CHECK fires by name, got {err}"
    );

    // ── The persona dichotomy: visitor is not a rated persona ──────
    let refused = ratings
        .submit(scale_session.id, 5, "visitor", None, None)
        .await;
    assert!(
        matches!(&refused, Err(LivechatError::Validation(msg)) if msg.contains("'agent' or 'bot'")),
        "rated_persona accepts agent|bot only, got {refused:?}"
    );

    // ── The bot arm: attribution names the script, never an operator ─
    let script = Uuid::new_v4();
    let bot_session = open_session(&pool, channel, "rating:bot").await;
    sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, chatbot_script_id, expertise_names)
           VALUES ($1, 'bot', $2, '{}')"#,
    )
    .bind(bot_session.id)
    .bind(script)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("bot ledger seed: {e}"));
    let row = ratings
        .submit(bot_session.id, 10, "bot", None, None)
        .await
        .unwrap_or_else(|e| panic!("the bot rating must land: {e:?}"));
    assert_eq!(
        row.chatbot_script_id,
        Some(script),
        "a bot rating names the script"
    );
    assert_eq!(row.operator_user_id, None);

    // ── The missing session is the uniform 404 family ─────────────
    let refused = ratings.submit(Uuid::new_v4(), 5, "agent", None, None).await;
    assert!(
        matches!(refused, Err(LivechatError::SessionNotFound)),
        "a rating on a missing session is the uniform 404 family, got {refused:?}"
    );

    db.dispose().await;
}
