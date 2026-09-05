//! THE CRM-BRIDGE PROBE — the conversation-becomes-a-lead seam: the
//! mint verb (the single-writer link stamp + the chatbot contact
//! harvest), the two lead-linked read grants (the lead-owner read and
//! the lead-granted agent join), the refusing default's loud park, and
//! the RLS fence over the new link column.

use std::sync::Arc;

use uuid::Uuid;

use backbone_livechat::application::service::crm_bridge_service::{
    CrmBridgeService, LeadMintInput,
};
use backbone_livechat::application::service::crm_port::RefusingCrmLeadPort;
use backbone_livechat::infrastructure::persistence::CrmBridgeRepository;
use backbone_orm::company_scope;

use super::common::{open_session, seed_channel_with_operators, RecordingCrmLeadPort, TestDb};

/// Count the audit rows of one event kind for one subject.
async fn audit_count(pool: &sqlx::PgPool, event: &str, subject: Uuid) -> i64 {
    sqlx::query_scalar(
        r#"SELECT count(*) FROM livechat.livechat_audit_log
            WHERE event = $1::livechat_audit_event AND subject_id = $2"#,
    )
    .bind(event)
    .bind(subject)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("audit count for {event}: {e}"))
}

/// The session's stamped lead id, straight from the table.
async fn stamped_lead(pool: &sqlx::PgPool, session: Uuid) -> Option<Uuid> {
    sqlx::query_scalar("SELECT crm_lead_id FROM livechat.sessions WHERE id = $1")
        .bind(session)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("stamped lead read: {e}"))
}

// ── 1. The refusing default parks the mint verb loudly ───────────────────────

#[tokio::test]
async fn uncomposed_port_parks_the_mint_typed_and_writes_nothing() {
    let db = TestDb::new("crmbridge1").await;
    let company = Uuid::new_v4();
    let operator = Uuid::new_v4();
    let channel = seed_channel_with_operators(&db.pool, company, Uuid::new_v4(), &[operator]).await;
    let session = open_session(&db.pool, company, channel, "crm:uncomposed").await;

    let service = CrmBridgeService::new(db.pool.clone(), Arc::new(RefusingCrmLeadPort));
    let err = service
        .mint_lead_for_session(session.id, &LeadMintInput::default(), Some(operator))
        .await
        .expect_err("an uncomposed CRM bridge must refuse the mint");
    assert_eq!(
        err.code(),
        "livechat_crm_bridge_not_composed",
        "the refusal is the typed bridge error, got {err:?}"
    );

    // NOTHING is written: no link stamp, no audit row of the new kinds.
    assert_eq!(
        stamped_lead(&db.pool, session.id).await,
        None,
        "a refused mint must not stamp the link"
    );
    assert_eq!(
        audit_count(&db.pool, "lead_linked", session.id).await,
        0,
        "a refused mint must not audit a link"
    );
    assert_eq!(
        audit_count(&db.pool, "lead_link_refused", session.id).await,
        0,
        "a refused mint is a port refusal, not a link refusal"
    );
    db.dispose().await;
}

// ── 2. The mint stamps the first-wins link, serves the read, audits ──────────

#[tokio::test]
async fn mint_stamps_the_link_serves_the_read_and_audits() {
    let db = TestDb::new("crmbridge2").await;
    let company = Uuid::new_v4();
    let operator = Uuid::new_v4();
    let channel = seed_channel_with_operators(&db.pool, company, Uuid::new_v4(), &[operator]).await;
    let session = open_session(&db.pool, company, channel, "crm:mint").await;

    let port = Arc::new(RecordingCrmLeadPort::default());
    let service = CrmBridgeService::new(db.pool.clone(), port.clone());
    let (row, lead_id) = company_scope::with_company_scope(
        Some(company),
        service.mint_lead_for_session(
            session.id,
            &LeadMintInput {
                lead_name: None,
                note: Some("wants the enterprise plan".into()),
                email: None,
                phone: None,
            },
            Some(operator),
        ),
    )
    .await
    .unwrap_or_else(|e| panic!("the mint through a composed port succeeds: {e}"));

    // The link is stamped and returned.
    assert_eq!(row.crm_lead_id, Some(lead_id));
    assert_eq!(row.id, session.id);
    assert_eq!(
        stamped_lead(&db.pool, session.id).await,
        Some(lead_id),
        "the stamp is durable"
    );

    // The port received ONLY server-stamped facts. (Scoped: the guard
    // drops before anything later could re-enter the port's mutex.)
    let requests = port.lead_ids();
    assert_eq!(requests.len(), 1, "exactly one mint crossed the port");
    {
        let minted = &port.minted.lock().unwrap_or_else(|p| p.into_inner())[0];
        assert_eq!(minted.0.company_id, company, "the tenant is the session's");
        assert_eq!(minted.0.session_id, session.id);
        assert_eq!(
            minted.0.lead_name,
            session.title.clone().unwrap_or_default(),
            "the name defaults to the session title"
        );
        assert_eq!(minted.0.operator_user_id, Some(operator));
        assert_eq!(minted.0.note.as_deref(), Some("wants the enterprise plan"));
    }

    // The read grant resolves the session by its lead (same company).
    let read = company_scope::with_company_scope(Some(company), service.session_for_lead(lead_id))
        .await
        .unwrap_or_else(|e| panic!("the lead-linked read resolves: {e}"));
    assert_eq!(read.map(|r| r.id), Some(session.id));

    // The audit trail carries exactly the link.
    assert_eq!(audit_count(&db.pool, "lead_linked", session.id).await, 1);

    // The partial UNIQUE (the has-crm-lead partial-index translation) exists.
    let idx: i64 = sqlx::query_scalar(
        r#"SELECT count(*) FROM pg_indexes
            WHERE schemaname = 'livechat' AND indexname = 'session_crm_lead_uq'"#,
    )
    .fetch_one(&db.pool)
    .await
    .unwrap_or_else(|e| panic!("index lookup: {e}"));
    assert_eq!(idx, 1, "the session_crm_lead_uq partial index must exist");
    db.dispose().await;
}

// ── 3. A replayed mint loses first-wins, typed, DB unmoved ───────────────────

#[tokio::test]
async fn second_mint_on_a_linked_session_refuses_typed_and_leaves_the_row_unmoved() {
    let db = TestDb::new("crmbridge3").await;
    let company = Uuid::new_v4();
    let operator = Uuid::new_v4();
    let channel = seed_channel_with_operators(&db.pool, company, Uuid::new_v4(), &[operator]).await;
    let session = open_session(&db.pool, company, channel, "crm:replay").await;

    let service = CrmBridgeService::new(db.pool.clone(), Arc::new(RecordingCrmLeadPort::default()));
    let (_, first) = company_scope::with_company_scope(
        Some(company),
        service.mint_lead_for_session(session.id, &LeadMintInput::default(), Some(operator)),
    )
    .await
    .unwrap_or_else(|e| panic!("the first mint wins: {e}"));

    let err = company_scope::with_company_scope(
        Some(company),
        service.mint_lead_for_session(session.id, &LeadMintInput::default(), Some(operator)),
    )
    .await
    .expect_err("a linked session refuses the second mint");
    assert_eq!(
        err.code(),
        "livechat_session_already_has_lead",
        "the replay is the typed first-wins refusal, got {err:?}"
    );

    // The row still carries the FIRST winner's link.
    assert_eq!(
        stamped_lead(&db.pool, session.id).await,
        Some(first),
        "a lost race never moves the stamped link"
    );
    // The sequential replay refuses at the service's own pre-check,
    // BEFORE any write: no second link audit, no refusal audit (the
    // audited refusal is the repo-level race arm, probed below).
    assert_eq!(audit_count(&db.pool, "lead_linked", session.id).await, 1);
    assert_eq!(
        audit_count(&db.pool, "lead_link_refused", session.id).await,
        0,
        "the pre-check refusal writes nothing"
    );
    db.dispose().await;
}

// ── 4. The partial UNIQUE is a DB-level wall, not just verb etiquette ────────

#[tokio::test]
async fn a_second_session_cannot_steal_the_same_lead_id() {
    let db = TestDb::new("crmbridge4").await;
    let company = Uuid::new_v4();
    let operator = Uuid::new_v4();
    let channel = seed_channel_with_operators(&db.pool, company, Uuid::new_v4(), &[operator]).await;
    let first = open_session(&db.pool, company, channel, "crm:one").await;
    let second = open_session(&db.pool, company, channel, "crm:two").await;

    let repo = CrmBridgeRepository::new(db.pool.clone());
    let lead_id = Uuid::new_v4();
    company_scope::with_company_scope(
        Some(company),
        repo.link_lead(first.id, lead_id, Some(operator)),
    )
    .await
    .unwrap_or_else(|e| panic!("the first session links the lead: {e}"));

    // The verb layer guards, but the CONSTRAINT is the wall: linking the
    // SAME lead onto another session must fail at the DB (unique 23505
    // surfacing through the module's database arm).
    let err = company_scope::with_company_scope(
        Some(company),
        repo.link_lead(second.id, lead_id, Some(operator)),
    )
    .await
    .expect_err("one lead cannot sit on two sessions");
    assert_eq!(
        err.code(),
        "livechat_database",
        "the refusal is the DB arm, got {err:?}"
    );
    assert!(
        err.to_string().to_lowercase().contains("duplicate key"),
        "the DB names the unique violation, got {err}"
    );

    // The repo-level race arm: a second link attempt on the ALREADY
    // LINKED session (the service pre-check bypassed — two mints that
    // raced past it) loses first-wins, surfaces the typed 409, and
    // leaves an audited refusal naming the existing link.
    let err = company_scope::with_company_scope(
        Some(company),
        repo.link_lead(first.id, Uuid::new_v4(), Some(operator)),
    )
    .await
    .expect_err("a raced second link must refuse");
    assert_eq!(
        err.code(),
        "livechat_session_already_has_lead",
        "the race loser gets the typed first-wins refusal, got {err:?}"
    );
    assert_eq!(
        stamped_lead(&db.pool, first.id).await,
        Some(lead_id),
        "the raced link never moved the row"
    );
    let refused: i64 = sqlx::query_scalar(
        r#"SELECT count(*) FROM livechat.livechat_audit_log
            WHERE event = 'lead_link_refused' AND subject_id = $1
              AND detail->>'reason' = 'already_linked'
              AND (detail->>'existing_lead_id')::uuid = $2"#,
    )
    .bind(first.id)
    .bind(lead_id)
    .fetch_one(&db.pool)
    .await
    .unwrap_or_else(|e| panic!("refusal audit read: {e}"));
    assert_eq!(
        refused, 1,
        "the raced refusal is audited, naming the existing link"
    );
    db.dispose().await;
}

// ── 5. Cross-company: the uniform missing family ─────────────────────────────

#[tokio::test]
async fn cross_company_reads_and_joins_are_uniformly_missing() {
    let db = TestDb::new("crmbridge5").await;
    let company_a = Uuid::new_v4();
    let company_b = Uuid::new_v4();
    let op_a = Uuid::new_v4();
    let op_b = Uuid::new_v4();
    let website = Uuid::new_v4();
    let channel_a = seed_channel_with_operators(&db.pool, company_a, website, &[op_a]).await;
    let channel_b = seed_channel_with_operators(&db.pool, company_b, website, &[op_b]).await;
    let session_a = open_session(&db.pool, company_a, channel_a, "crm:a").await;
    let session_b = open_session(&db.pool, company_b, channel_b, "crm:b").await;

    // The fence claims below must run as the NON-superuser role: the
    // scratch pool owns the tables and BYPASSES row-level security, so
    // a cross-company leak probed there would "pass" silently. Fixtures
    // seed through the owner pool; every service verb runs fenced.
    let fenced = super::common::fenced_role_pool(&db.pool, &db.name).await;
    let service = CrmBridgeService::new(fenced, Arc::new(RecordingCrmLeadPort::default()));
    let (_, lead) = company_scope::with_company_scope(
        Some(company_a),
        service.mint_lead_for_session(session_a.id, &LeadMintInput::default(), Some(op_a)),
    )
    .await
    .unwrap_or_else(|e| panic!("company A mints: {e}"));

    // Company B reads the lead id: missing, never leaked.
    let read = company_scope::with_company_scope(Some(company_b), service.session_for_lead(lead))
        .await
        .unwrap_or_else(|e| panic!("a cross-company read answers Ok(None): {e}"));
    assert!(read.is_none(), "the fence reads as missing, not as a leak");

    // Company B joins by that lead: the uniform not-found.
    let err = company_scope::with_company_scope(
        Some(company_b),
        service.join_session_for_lead(lead, op_b, Some(op_b)),
    )
    .await
    .expect_err("a cross-company join must refuse");
    assert_eq!(
        err.code(),
        "livechat_session_not_found",
        "the uniform missing family, got {err:?}"
    );

    // Company B mints at A's session id: also uniformly missing.
    let err = company_scope::with_company_scope(
        Some(company_b),
        service.mint_lead_for_session(session_a.id, &LeadMintInput::default(), Some(op_b)),
    )
    .await
    .expect_err("a cross-company mint must refuse");
    assert_eq!(err.code(), "livechat_session_not_found");

    // The positive arm: the same verbs serve company B's OWN session —
    // the fence isolates, it does not break.
    let (row_b, lead_b) = company_scope::with_company_scope(
        Some(company_b),
        service.mint_lead_for_session(session_b.id, &LeadMintInput::default(), Some(op_b)),
    )
    .await
    .unwrap_or_else(|e| panic!("company B mints its own session: {e}"));
    assert_eq!(row_b.id, session_b.id);
    assert_ne!(lead_b, lead, "each company mints its own lead");
    db.dispose().await;
}

// ── 6. The join writes the ledger, audits, and escalates honestly ────────────

#[tokio::test]
async fn the_join_writes_the_agent_ledger_audits_and_escalates() {
    let db = TestDb::new("crmbridge6").await;
    let company = Uuid::new_v4();
    let operator = Uuid::new_v4();
    let joiner1 = Uuid::new_v4();
    let joiner2 = Uuid::new_v4();
    let channel = seed_channel_with_operators(&db.pool, company, Uuid::new_v4(), &[operator]).await;
    let session = open_session(&db.pool, company, channel, "crm:join").await;

    let service = CrmBridgeService::new(db.pool.clone(), Arc::new(RecordingCrmLeadPort::default()));
    let (_, lead) = company_scope::with_company_scope(
        Some(company),
        service.mint_lead_for_session(session.id, &LeadMintInput::default(), Some(operator)),
    )
    .await
    .unwrap_or_else(|e| panic!("mint for the join probes: {e}"));

    // First join: one agent row, not yet escalated (the derive is a pure
    // function of THIS row's inputs — one agent is not a handoff).
    let row = company_scope::with_company_scope(
        Some(company),
        service.join_session_for_lead(lead, joiner1, Some(operator)),
    )
    .await
    .unwrap_or_else(|e| panic!("the first join succeeds: {e}"));
    assert_eq!(row.id, session.id);
    let (agents, outcome): (i64, String) = sqlx::query_as(
        r#"SELECT count(*),
                  (SELECT outcome::text FROM livechat.sessions WHERE id = $1)
           FROM livechat.member_histories
            WHERE session_id = $1 AND persona = 'agent' AND operator_user_id = $2"#,
    )
    .bind(session.id)
    .bind(joiner1)
    .fetch_one(&db.pool)
    .await
    .unwrap_or_else(|e| panic!("ledger read: {e}"));
    assert_eq!(agents, 1, "the join writes exactly one agent ledger row");
    assert_ne!(outcome, "escalated", "one agent does not escalate");

    // Second join (a different agent): the derive escalates honestly.
    company_scope::with_company_scope(
        Some(company),
        service.join_session_for_lead(lead, joiner2, Some(operator)),
    )
    .await
    .unwrap_or_else(|e| panic!("the second join succeeds: {e}"));
    let (outcome,): (String,) =
        sqlx::query_as("SELECT outcome::text FROM livechat.sessions WHERE id = $1")
            .bind(session.id)
            .fetch_one(&db.pool)
            .await
            .unwrap_or_else(|e| panic!("outcome read: {e}"));
    assert_eq!(outcome, "escalated", "a second agent escalates the outcome");

    // Re-join the first agent: the upsert re-points, never duplicates.
    company_scope::with_company_scope(
        Some(company),
        service.join_session_for_lead(lead, joiner1, Some(operator)),
    )
    .await
    .unwrap_or_else(|e| panic!("the re-join succeeds: {e}"));
    let (rows,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM livechat.member_histories
            WHERE session_id = $1 AND persona = 'agent'"#,
    )
    .bind(session.id)
    .fetch_one(&db.pool)
    .await
    .unwrap_or_else(|e| panic!("ledger re-read: {e}"));
    assert_eq!(rows, 2, "a re-join never duplicates a ledger row");

    // The join trail is audited.
    assert_eq!(
        audit_count(&db.pool, "lead_session_joined", session.id).await,
        3
    );
    db.dispose().await;
}

// ── 7. The join refuses closed conversations typed, the read serves history ──

#[tokio::test]
async fn the_join_refuses_closed_conversations_typed_but_the_read_serves_history() {
    let db = TestDb::new("crmbridge7").await;
    let company = Uuid::new_v4();
    let operator = Uuid::new_v4();
    let joiner = Uuid::new_v4();
    let channel = seed_channel_with_operators(&db.pool, company, Uuid::new_v4(), &[operator]).await;
    let session = open_session(&db.pool, company, channel, "crm:closed").await;

    let service = CrmBridgeService::new(db.pool.clone(), Arc::new(RecordingCrmLeadPort::default()));
    let (_, lead) = company_scope::with_company_scope(
        Some(company),
        service.mint_lead_for_session(session.id, &LeadMintInput::default(), Some(operator)),
    )
    .await
    .unwrap_or_else(|e| panic!("mint for the closed probe: {e}"));

    // Close the conversation (the sweep's own shape: closed_at set,
    // close_reason recorded, status cleared — the hardening migration's
    // check constraint ck_sessions_closed_no_status forbids both set).
    sqlx::query(
        r#"UPDATE livechat.sessions
              SET closed_at = now(), close_reason = 'expired', status = NULL
            WHERE id = $1"#,
    )
    .bind(session.id)
    .execute(&db.pool)
    .await
    .unwrap_or_else(|e| panic!("close: {e}"));

    // The join is refused typed and audited.
    let err = company_scope::with_company_scope(
        Some(company),
        service.join_session_for_lead(lead, joiner, Some(operator)),
    )
    .await
    .expect_err("a closed conversation refuses the join");
    assert_eq!(err.code(), "livechat_validation");
    assert!(
        err.to_string().contains("closed"),
        "the refusal names the closed state, got {err}"
    );
    assert_eq!(
        audit_count(&db.pool, "lead_session_join_refused", session.id).await,
        1,
        "the refused join is audited"
    );
    let (agents,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM livechat.member_histories
            WHERE session_id = $1 AND persona = 'agent' AND operator_user_id = $2"#,
    )
    .bind(session.id)
    .bind(joiner)
    .fetch_one(&db.pool)
    .await
    .unwrap_or_else(|e| panic!("ledger read: {e}"));
    assert_eq!(agents, 0, "a refused join writes no ledger row");

    // The read still serves the closed conversation's history.
    let read = company_scope::with_company_scope(Some(company), service.session_for_lead(lead))
        .await
        .unwrap_or_else(|e| panic!("the read verb answers: {e}"))
        .unwrap_or_else(|| panic!("the closed session still resolves by its lead"));
    assert_eq!(read.id, session.id);
    db.dispose().await;
}

// ── 8. The harvest picks the EARLIEST answered contact steps ─────────────────

#[tokio::test]
async fn the_harvest_feeds_the_earliest_answered_contact_to_the_mint() {
    let db = TestDb::new("crmbridge8").await;
    let company = Uuid::new_v4();
    let operator = Uuid::new_v4();
    let channel = seed_channel_with_operators(&db.pool, company, Uuid::new_v4(), &[operator]).await;
    let session = open_session(&db.pool, company, channel, "crm:harvest").await;

    // A script with two email steps and two phone steps; the visitor
    // answered both arms of each, LATER first (created_at is set
    // explicitly so the earliest is unambiguous).
    let (script,): (Uuid,) = sqlx::query_as(
        r#"INSERT INTO livechat.chatbot_scripts (title, company_id)
           VALUES ('probe harvest', $1) RETURNING id"#,
    )
    .bind(company)
    .fetch_one(&db.pool)
    .await
    .unwrap_or_else(|e| panic!("seed script: {e}"));

    async fn seed_step(
        pool: &sqlx::PgPool,
        script: Uuid,
        company: Uuid,
        seq: i32,
        kind: &str,
    ) -> Uuid {
        let (id,): (Uuid,) = sqlx::query_as(
            r#"INSERT INTO livechat.chatbot_steps
                   (chatbot_script_id, sequence, step_type, expertise_tag_ids, company_id)
               VALUES ($1, $2, $3::livechat_step_type, '{}', $4)
               RETURNING id"#,
        )
        .bind(script)
        .bind(seq)
        .bind(kind)
        .bind(company)
        .fetch_one(pool)
        .await
        .unwrap_or_else(|e| panic!("seed step {kind}: {e}"));
        id
    }

    async fn seed_message(
        pool: &sqlx::PgPool,
        session: Uuid,
        company: Uuid,
        step_id: Uuid,
        answer: Option<&str>,
        ago: &str,
    ) {
        sqlx::query(
            r#"INSERT INTO livechat.chatbot_messages
                   (session_id, step_id, visitor_answer, created_at, company_id)
               VALUES ($1, $2, $3, now() - $4::interval, $5)"#,
        )
        .bind(session)
        .bind(step_id)
        .bind(answer)
        .bind(ago)
        .bind(company)
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("seed message: {e}"));
    }

    let email_late = seed_step(&db.pool, script, company, 1, "question_email").await;
    let email_early = seed_step(&db.pool, script, company, 2, "question_email").await;
    let email_unanswered = seed_step(&db.pool, script, company, 3, "question_email").await;
    let phone_late = seed_step(&db.pool, script, company, 4, "question_phone").await;
    let phone_early = seed_step(&db.pool, script, company, 5, "question_phone").await;

    for (step_id, answer, ago) in [
        (email_late, Some("late@example.com"), "1 minute"),
        (email_early, Some("early@example.com"), "10 minutes"),
        (email_unanswered, None, "30 minutes"),
        (phone_late, Some("+62 811-000-1000"), "2 minutes"),
        (phone_early, Some("+62 811-000-2000"), "11 minutes"),
    ] {
        seed_message(&db.pool, session.id, company, step_id, answer, ago).await;
    }

    // The repository harvests the earliest of each arm, skipping NULLs.
    let repo = CrmBridgeRepository::new(db.pool.clone());
    let harvested =
        company_scope::with_company_scope(Some(company), repo.harvest_contact(session.id))
            .await
            .unwrap_or_else(|e| panic!("the harvest answers: {e}"));
    assert_eq!(harvested.email.as_deref(), Some("early@example.com"));
    assert_eq!(harvested.phone.as_deref(), Some("+62 811-000-2000"));

    // The mint carries the harvest into the port request (no explicit
    // contact given).
    let port = Arc::new(RecordingCrmLeadPort::default());
    let service = CrmBridgeService::new(db.pool.clone(), port.clone());
    company_scope::with_company_scope(
        Some(company),
        service.mint_lead_for_session(session.id, &LeadMintInput::default(), Some(operator)),
    )
    .await
    .unwrap_or_else(|e| panic!("the mint succeeds: {e}"));
    let mints = port.minted.lock().unwrap_or_else(|p| p.into_inner());
    assert_eq!(mints.len(), 1);
    assert_eq!(
        mints[0].0.contact_email.as_deref(),
        Some("early@example.com")
    );
    assert_eq!(
        mints[0].0.contact_phone.as_deref(),
        Some("+62 811-000-2000")
    );
    // The guard MUST drop here: the second mint below re-enters the
    // port, which locks the same mutex — a guard held across it
    // self-deadlocks the single-threaded runtime.
    drop(mints);

    // An EXPLICIT contact overrides the harvest: a fresh session whose
    // chatbot collected a DIFFERENT email still mints with the caller's.
    let session2 = open_session(&db.pool, company, channel, "crm:harvest2").await;
    seed_message(
        &db.pool,
        session2.id,
        company,
        email_late,
        Some("harvested@example.com"),
        "5 minutes",
    )
    .await;
    company_scope::with_company_scope(
        Some(company),
        service.mint_lead_for_session(
            session2.id,
            &LeadMintInput {
                email: Some("explicit@example.com".into()),
                phone: None,
                ..Default::default()
            },
            None,
        ),
    )
    .await
    .unwrap_or_else(|e| panic!("the second mint succeeds: {e}"));
    let mints = port.minted.lock().unwrap_or_else(|p| p.into_inner());
    assert_eq!(mints.len(), 2);
    assert_eq!(
        mints[1].0.contact_email.as_deref(),
        Some("explicit@example.com"),
        "the explicit contact overrides the harvest"
    );
    db.dispose().await;
}

// ── 9. The fence over the NEW column, held by the app role ───────────────────

#[tokio::test]
async fn the_fenced_role_cannot_reach_the_link_column_across_companies() {
    let db = TestDb::new("crmbridge9").await;
    let owner = db.pool.clone();
    let company_a = Uuid::new_v4();
    let company_b = Uuid::new_v4();
    let op_a = Uuid::new_v4();
    let op_b = Uuid::new_v4();
    let website = Uuid::new_v4();
    let channel_a = seed_channel_with_operators(&owner, company_a, website, &[op_a]).await;
    let channel_b = seed_channel_with_operators(&owner, company_b, website, &[op_b]).await;
    let session_a = open_session(&owner, company_a, channel_a, "crm:fence-a").await;
    let session_b = open_session(&owner, company_b, channel_b, "crm:fence-b").await;

    let fenced = super::common::fenced_role_pool(&owner, &db.name).await;

    // Unscoped: the new column reads as nothing (the table row itself is
    // invisible).
    let (unscoped,): (i64,) = sqlx::query_as(
        "SELECT count(crm_lead_id) FROM livechat.sessions WHERE crm_lead_id IS NOT NULL",
    )
    .fetch_one(&fenced)
    .await
    .unwrap_or_else(|e| panic!("unscoped new-column read: {e}"));
    assert_eq!(
        unscoped, 0,
        "an unscoped read of the link column sees ZERO rows"
    );

    // Scoped to B: an UPDATE of A's link column matches ZERO rows (the
    // USING arm of the fence — no cross-tenant stamp, no error to leak by).
    let mut conn = fenced
        .acquire()
        .await
        .unwrap_or_else(|e| panic!("fenced acquire: {e}"));
    sqlx::query("SELECT set_config('app.company_id', $1, false)")
        .bind(company_b.to_string())
        .execute(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("scope bind: {e}"));
    let touched = sqlx::query("UPDATE livechat.sessions SET crm_lead_id = $1 WHERE id = $2")
        .bind(Uuid::new_v4())
        .bind(session_a.id)
        .execute(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("cross-tenant stamp attempt: {e}"))
        .rows_affected();
    assert_eq!(touched, 0, "a scoped cross-company stamp touches ZERO rows");

    // Scoped to A: the app's own path works (the verb's stamp under the
    // fence).
    sqlx::query("SELECT set_config('app.company_id', $1, false)")
        .bind(company_a.to_string())
        .execute(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("scope rebind: {e}"));
    let own = Uuid::new_v4();
    let touched = sqlx::query("UPDATE livechat.sessions SET crm_lead_id = $1 WHERE id = $2")
        .bind(own)
        .bind(session_a.id)
        .execute(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("own-company stamp: {e}"))
        .rows_affected();
    assert_eq!(touched, 1, "the app role can stamp its OWN company's rows");
    drop(conn);

    // And B's session was never touched.
    assert_eq!(
        stamped_lead(&owner, session_b.id).await,
        None,
        "company B's link column is untouched"
    );
    db.dispose().await;
}
