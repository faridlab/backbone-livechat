//! The member-history ledger probe: the three per-persona partial
//! uniques and the STRICT persona trichotomy are DB walls — raw
//! owner-pool INSERTs violate them by NAME (23505 / 23514), and the
//! ledger's upsert paths re-point instead of duplicating.

use uuid::Uuid;

use super::common::{open_session, seed_channel_with_operators, TestDb};
use backbone_livechat::infrastructure::persistence::upsert_agent_ledger_tx;

fn constraint_of(err: &sqlx::Error) -> String {
    match err {
        sqlx::Error::Database(db) => db
            .constraint()
            .map(str::to_string)
            .or_else(|| Some(db.message().to_string()))
            .unwrap_or_default(),
        other => format!("{other}"),
    }
}

#[tokio::test]
async fn partial_uniques_and_trichotomy_are_db_walls() {
    let db = TestDb::new("ledger").await;
    let pool = db.pool.clone();
    let website = Uuid::new_v4();
    let op_a = Uuid::new_v4();
    let op_b = Uuid::new_v4();
    let channel = seed_channel_with_operators(&pool, website, &[op_a, op_b]).await;
    let session = open_session(&pool, channel, "ledger:visitor").await;

    // The FIRST agent row is legal; a SECOND for the SAME operator on
    // the SAME session violates the agent partial unique BY NAME.
    sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, operator_user_id, expertise_names)
           VALUES ($1, 'agent', $2, '{}')"#,
    )
    .bind(session.id)
    .bind(op_a)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("the first agent row is legal: {e}"));
    let err = sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, operator_user_id, joined_at, left_at, expertise_names)
           VALUES ($1, 'agent', $2, now(), now(), '{}')"#,
    )
    .bind(session.id)
    .bind(op_a)
    .execute(&pool)
    .await
    .err()
        .unwrap_or_else(|| panic!("a duplicate agent row must be refused by the DB"));
    assert!(
        constraint_of(&err).contains("uq_member_histories_agent"),
        "the agent partial unique must fire by name, got {:?}",
        constraint_of(&err)
    );

    // A DIFFERENT operator on the same session is legal (the
    // escalation shape — one row per operator).
    sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, operator_user_id, expertise_names)
           VALUES ($1, 'agent', $2, '{}')"#,
    )
    .bind(session.id)
    .bind(op_b)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("a second distinct agent row is legal: {e}"));

    // A duplicate VISITOR row for the same digest violates the
    // visitor partial unique (the open already bound the visitor).
    let err = sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, visitor_key, expertise_names)
           VALUES ($1, 'visitor', $2, '{}')"#,
    )
    .bind(session.id)
    .bind("ledger:visitor")
    .execute(&pool)
    .await
    .err()
    .unwrap_or_else(|| panic!("a duplicate visitor row must be refused by the DB"));
    assert!(
        constraint_of(&err).contains("uq_member_histories_visitor"),
        "the visitor partial unique must fire by name, got {:?}",
        constraint_of(&err)
    );

    // A duplicate BOT row for the same script violates the bot
    // partial unique.
    let script = Uuid::new_v4();
    sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, chatbot_script_id, expertise_names)
           VALUES ($1, 'bot', $2, '{}')"#,
    )
    .bind(session.id)
    .bind(script)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("the first bot row is legal: {e}"));
    let err = sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, chatbot_script_id, expertise_names)
           VALUES ($1, 'bot', $2, '{}')"#,
    )
    .bind(session.id)
    .bind(script)
    .execute(&pool)
    .await
    .err()
    .unwrap_or_else(|| panic!("a duplicate bot row must be refused by the DB"));
    assert!(
        constraint_of(&err).contains("uq_member_histories_bot"),
        "the bot partial unique must fire by name, got {:?}",
        constraint_of(&err)
    );

    // ── The STRICT trichotomy: every malformed persona binding ────
    let trichotomy = "ck_member_histories_persona_trichotomy";
    // An agent row with NO operator id.
    let err = sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, operator_user_id, expertise_names)
           VALUES ($1, 'agent', NULL, '{}')"#,
    )
    .bind(session.id)
    .execute(&pool)
    .await
    .err()
    .unwrap_or_else(|| panic!("an identity-less agent row must be refused"));
    assert!(
        constraint_of(&err).contains(trichotomy),
        "agent/NULL must hit the trichotomy, got {:?}",
        constraint_of(&err)
    );
    // A visitor row carrying an operator id.
    let err = sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, visitor_key, operator_user_id, expertise_names)
           VALUES ($1, 'visitor', $2, $3, '{}')"#,
    )
    .bind(session.id)
    .bind("ledger:other-visitor")
    .bind(op_a)
    .execute(&pool)
    .await
    .err()
    .unwrap_or_else(|| panic!("a visitor row with an operator id must be refused"));
    assert!(
        constraint_of(&err).contains(trichotomy),
        "visitor+operator must hit the trichotomy, got {:?}",
        constraint_of(&err)
    );
    // A bot row carrying a visitor key.
    let err = sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, chatbot_script_id, visitor_key, expertise_names)
           VALUES ($1, 'bot', $2, $3, '{}')"#,
    )
    .bind(session.id)
    .bind(Uuid::new_v4())
    .bind("ledger:stray-key")
    .execute(&pool)
    .await
    .err()
    .unwrap_or_else(|| panic!("a bot row with a visitor key must be refused"));
    assert!(
        constraint_of(&err).contains(trichotomy),
        "bot+visitor must hit the trichotomy, got {:?}",
        constraint_of(&err)
    );
    // An agent row carrying BOTH operator and visitor key.
    let err = sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, operator_user_id, visitor_key, expertise_names)
           VALUES ($1, 'agent', $2, $3, '{}')"#,
    )
    .bind(session.id)
    .bind(Uuid::new_v4())
    .bind("ledger:cross-key")
    .execute(&pool)
    .await
    .err()
    .unwrap_or_else(|| panic!("a both-columns agent row must be refused"));
    assert!(
        constraint_of(&err).contains(trichotomy),
        "agent+visitor must hit the trichotomy, got {:?}",
        constraint_of(&err)
    );

    // ── The rejoin law: the upsert RE-POINTS, never duplicates ────
    let rejoined = open_session(&pool, channel, "ledger:rejoin").await;
    let mut tx = pool.begin().await.unwrap_or_else(|e| panic!("tx: {e}"));
    upsert_agent_ledger_tx(&mut tx, rejoined.id, op_a)
        .await
        .unwrap_or_else(|e| panic!("first ledger upsert: {e:?}"));
    upsert_agent_ledger_tx(&mut tx, rejoined.id, op_a)
        .await
        .unwrap_or_else(|e| panic!("the rejoin upsert must re-point, not duplicate: {e:?}"));
    tx.commit().await.unwrap_or_else(|e| panic!("commit: {e}"));
    let (agent_rows,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM livechat.member_histories
            WHERE session_id = $1 AND persona = 'agent' AND operator_user_id = $2"#,
    )
    .bind(rejoined.id)
    .bind(op_a)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("rejoin count: {e}"));
    assert_eq!(agent_rows, 1, "a rejoin re-points the one ledger row");

    db.dispose().await;
}
