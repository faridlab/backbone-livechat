//! The per-record outcome probe: the compute-in-loop defect does not
//! port — every session's outcome is derived from ITS OWN row inside
//! ONE set-based statement (single-row and batch shapes), never a
//! whole-recordset assignment in an iteration. The report reads are
//! bounded and windowed.

use chrono::{Duration, Utc};
use uuid::Uuid;

use backbone_livechat::application::service::livechat_error::LivechatError;
use backbone_livechat::application::service::report_service::ReportService;
use backbone_livechat::infrastructure::persistence::selection_repository::{
    recompute_outcome_tx, recompute_outcomes_batch_tx,
};

use super::common::{open_session, seed_channel_with_operators, TestDb};

#[tokio::test]
async fn outcomes_derive_per_record_never_per_recordset() {
    let db = TestDb::new("outcome").await;
    let pool = db.pool.clone();
    let company = Uuid::new_v4();
    let website = Uuid::new_v4();
    let op_a = Uuid::new_v4();
    let op_b = Uuid::new_v4();
    let channel = seed_channel_with_operators(&pool, company, website, &[op_a, op_b]).await;

    // Three sessions with three DIFFERENT shapes: the per-record
    // derive must land a different outcome on each row from one
    // statement — a loop that assigned through a set handle would
    // smear one value across the batch.
    let s_answer = open_session(&pool, company, channel, "outcome:answer").await;
    let s_escalated = open_session(&pool, company, channel, "outcome:escalated").await;
    let s_agent = open_session(&pool, company, channel, "outcome:agent").await;

    // s_answer: never answered, never assigned (failure no_answer).
    // s_escalated: TWO agent ledger rows (escalated).
    // s_agent: one agent ledger row, answered (no_failure).
    sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, operator_user_id, expertise_names, company_id)
           VALUES ($1, 'agent', $2, '{}', $4), ($1, 'agent', $3, '{}', $4)"#,
    )
    .bind(s_escalated.id)
    .bind(op_a)
    .bind(op_b)
    .bind(company)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("escalated ledger seed failed: {e}"));
    sqlx::query(
        r#"INSERT INTO livechat.member_histories
               (session_id, persona, operator_user_id, expertise_names, company_id)
           VALUES ($1, 'agent', $2, '{}', $3)"#,
    )
    .bind(s_agent.id)
    .bind(op_a)
    .bind(company)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("agent ledger seed failed: {e}"));
    sqlx::query(
        r#"UPDATE livechat.sessions SET failure = 'no_failure', status = 'in_progress',
                 first_response_at = now(), operator_user_id = $2 WHERE id = $1"#,
    )
    .bind(s_agent.id)
    .bind(op_a)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("agent shape failed: {e}"));

    // The batch recompute: ONE statement over the three rows.
    let mut tx = pool
        .begin()
        .await
        .unwrap_or_else(|e| panic!("tx failed: {e}"));
    recompute_outcomes_batch_tx(&mut tx, &[s_answer.id, s_escalated.id, s_agent.id])
        .await
        .unwrap_or_else(|e| panic!("batch recompute failed: {e:?}"));
    tx.commit()
        .await
        .unwrap_or_else(|e| panic!("commit failed: {e}"));

    let outcomes: Vec<(Uuid, Option<String>)> = sqlx::query_as(
        r#"SELECT id, outcome::text FROM livechat.sessions
            WHERE id = ANY($1) ORDER BY id"#,
    )
    .bind(vec![s_answer.id, s_escalated.id, s_agent.id])
    .fetch_all(&pool)
    .await
    .unwrap_or_else(|e| panic!("outcome read failed: {e}"));
    let by_id = |id: Uuid| {
        outcomes
            .iter()
            .find(|(row_id, _)| *row_id == id)
            .map(|(_, o)| o.as_deref())
            .unwrap_or_else(|| panic!("row {id} missing"))
    };
    assert_eq!(by_id(s_answer.id), Some("no_answer"), "the unanswered row");
    assert_eq!(
        by_id(s_escalated.id),
        Some("escalated"),
        "the two-agent row"
    );
    assert_eq!(by_id(s_agent.id), Some("no_failure"), "the answered row");
    assert_eq!(
        outcomes.len(),
        3,
        "one statement, three rows, three answers — the recordset was never smeared"
    );

    // The single-row entry point agrees (per-record, not per-scan).
    let mut tx = pool
        .begin()
        .await
        .unwrap_or_else(|e| panic!("tx failed: {e}"));
    recompute_outcome_tx(&mut tx, s_answer.id)
        .await
        .unwrap_or_else(|e| panic!("single recompute failed: {e:?}"));
    tx.commit()
        .await
        .unwrap_or_else(|e| panic!("commit failed: {e}"));
    let (again,): (Option<String>,) =
        sqlx::query_as("SELECT outcome::text FROM livechat.sessions WHERE id = $1")
            .bind(s_answer.id)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| panic!("single re-read failed: {e}"));
    assert_eq!(
        again.as_deref(),
        Some("no_answer"),
        "the single-row derive is stable"
    );

    db.dispose().await;
}

#[tokio::test]
async fn report_reads_require_bounded_windows() {
    let db = TestDb::new("report").await;
    let pool = db.pool.clone();
    let company = Uuid::new_v4();
    let website = Uuid::new_v4();
    let channel = seed_channel_with_operators(&pool, company, website, &[Uuid::new_v4()]).await;
    let _ = open_session(&pool, company, channel, "report:seed").await;

    let reports = ReportService::new(pool.clone());

    // A valid bounded window answers.
    let from = Utc::now() - Duration::days(7);
    let to = Utc::now();
    let summary = reports
        .session_summary(from, to, Some("day"), Some(1))
        .await
        .unwrap_or_else(|e| panic!("bounded report failed: {e:?}"));
    assert_eq!(
        summary.summary.sessions_total, 1,
        "the seeded session is counted"
    );
    assert_eq!(
        summary.week_start, 1,
        "the explicit week anchor is carried, not locale-derived"
    );
    assert!(
        !summary.series.is_empty(),
        "the daily series has its bucket"
    );

    // Backwards bounds refuse typed.
    match reports.session_summary(to, from, None, None).await {
        Err(LivechatError::Validation(msg)) => {
            assert!(
                msg.contains("from must precede to"),
                "the typed refusal carries its reason"
            );
        }
        other => panic!("backwards bounds must refuse typed, got {other:?}"),
    }

    // The window cap refuses the unbounded scan.
    match reports
        .session_summary(Utc::now() - Duration::days(400), Utc::now(), None, None)
        .await
    {
        Err(LivechatError::Validation(msg)) => {
            assert!(msg.contains("capped"), "the cap refusal names the cap");
        }
        other => panic!("a 400-day window must refuse typed, got {other:?}"),
    }

    // An out-of-range week anchor refuses typed.
    match reports.session_summary(from, to, None, Some(9)).await {
        Err(LivechatError::Validation(_)) => {}
        other => panic!("week anchor 9 must refuse typed, got {other:?}"),
    }

    // The windowed happiness KPI: ratings count ONLY inside the
    // window (never a lifetime average).
    let rated = open_session(&pool, company, channel, "report:rated").await;
    sqlx::query(
        r#"INSERT INTO livechat.ratings
               (session_id, value, rated_persona, operator_user_id, company_id)
           VALUES ($1, 10, 'agent', $2, $3)"#,
    )
    .bind(rated.id)
    .bind(Uuid::new_v4())
    .bind(company)
    .execute(&pool)
    .await
    .unwrap_or_else(|e| panic!("rating seed failed: {e}"));
    // A fresh upper bound: the window is half-open, so the rating
    // stamped AFTER the original `to` must land inside a window whose
    // upper edge is captured after the insert.
    let summary = reports
        .session_summary(from, Utc::now(), None, None)
        .await
        .unwrap_or_else(|e| panic!("second report failed: {e:?}"));
    assert_eq!(
        summary.summary.rated_count, 1,
        "the in-window rating counts"
    );
    assert_eq!(
        summary.summary.happy_count, 1,
        "a 10 is happy in the windowed mix"
    );

    // The view itself carries no company leak: reading through the
    // scoped helper under ANOTHER company sees zero rows. This MUST
    // run on the fenced app role — the owner role bypasses row-level
    // security even with the company GUC set, so a leak check on the
    // owner pool proves nothing.
    let other_company = Uuid::new_v4();
    let fenced = super::common::fenced_role_pool(&pool, &db.name).await;
    let (leak,): (i64,) =
        backbone_orm::company_scope::with_company_scope(Some(other_company), async {
            sqlx::query_as::<_, (i64,)>("SELECT count(*) FROM livechat.session_report")
                .fetch_one(&fenced)
                .await
        })
        .await
        .unwrap_or_else(|e| panic!("fenced view read failed: {e}"));
    assert_eq!(
        leak, 0,
        "the report view flows the fence (no cross-company rows)"
    );

    // The report and the ladder share the ONE window constant.
    assert_eq!(
        backbone_livechat::infrastructure::persistence::ONGOING_WINDOW_SECS,
        1800,
        "the report and the ladder share the ONE window constant"
    );
    db.dispose().await;
}
