//! THE FENCED-RUNTIME PROBE — the RLS fence proven AS THE APP RUNS:
//! every assertion here runs on a pool connected as a NOSUPERUSER
//! NOBYPASSRLS role (the production posture). The scratch owner is a
//! superuser and BYPASSES the fence — a green suite run as that role
//! proves nothing, so this probe never uses it for fence claims.

use uuid::Uuid;

use backbone_livechat::infrastructure::persistence::SessionRow;

use super::common::{fenced_role_pool, open_session, seed_channel_with_operators, TestDb};

fn pg_code(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|db| db.code())
        .map(|c| c.to_string())
        .unwrap_or_default()
}

#[tokio::test]
async fn the_fence_holds_for_the_app_role_on_every_table() {
    let db = TestDb::new("fencedruntime").await;
    let owner = db.pool.clone();
    let company_a = Uuid::new_v4();
    let company_b = Uuid::new_v4();
    let website = Uuid::new_v4();
    let op_a = Uuid::new_v4();
    let op_b = Uuid::new_v4();
    let channel_a = seed_channel_with_operators(&owner, company_a, website, &[op_a]).await;
    let channel_b = seed_channel_with_operators(&owner, company_b, website, &[op_b]).await;
    let session_a: SessionRow = open_session(&owner, company_a, channel_a, "fence:a").await;
    let session_b: SessionRow = open_session(&owner, company_b, channel_b, "fence:b").await;
    let (owner_rows,): (i64,) = sqlx::query_as("SELECT count(*) FROM livechat.sessions")
        .fetch_one(&owner)
        .await
        .unwrap_or_else(|e| panic!("owner count: {e}"));
    assert_eq!(
        owner_rows, 2,
        "the seed pair landed (the owner bypasses the fence)"
    );

    let fenced = fenced_role_pool(&owner, &db.name).await;

    // ── 1. Unscoped read: the app role with no company bound sees 0 ─
    let (unscoped,): (i64,) = sqlx::query_as("SELECT count(*) FROM livechat.sessions")
        .fetch_one(&fenced)
        .await
        .unwrap_or_else(|e| panic!("unscoped count: {e}"));
    assert_eq!(
        unscoped, 0,
        "an unscoped read under the app role sees ZERO rows"
    );

    // ── 2. A bound company reads ONLY its own rows ──────────────────
    let mut conn = fenced
        .acquire()
        .await
        .unwrap_or_else(|e| panic!("fenced acquire: {e}"));
    sqlx::query("SELECT set_config('app.company_id', $1, false)")
        .bind(company_a.to_string())
        .execute(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("scope bind: {e}"));
    let scoped_rows: Vec<(Uuid,)> = sqlx::query_as("SELECT company_id FROM livechat.sessions")
        .fetch_all(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("scoped read: {e}"));
    assert_eq!(
        scoped_rows.len(),
        1,
        "a scoped read sees exactly company A's row"
    );
    assert_eq!(scoped_rows[0].0, company_a);
    sqlx::query("SELECT set_config('app.company_id', '', false)")
        .execute(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("scope reset: {e}"));
    drop(conn);

    // ── 3. An unscoped write is refused by row-level security ───────
    let err = sqlx::query(
        r#"INSERT INTO livechat.sessions (channel_id, expertise_names, company_id)
           VALUES ($1, '{}', $2)"#,
    )
    .bind(channel_a)
    .bind(company_a)
    .execute(&fenced)
    .await
    .err()
    .unwrap_or_else(|| panic!("an unscoped INSERT must be refused for the app role"));
    assert_eq!(
        pg_code(&err),
        "42501",
        "the unscoped write hits the RLS wall, got {err}"
    );
    assert!(
        err.to_string()
            .to_lowercase()
            .contains("row-level security"),
        "the refusal names row-level security, got {err}"
    );

    // ── 4. A scoped write may only write its OWN company's rows ────
    let mut conn = fenced
        .acquire()
        .await
        .unwrap_or_else(|e| panic!("fenced acquire (write): {e}"));
    sqlx::query("SELECT set_config('app.company_id', $1, false)")
        .bind(company_a.to_string())
        .execute(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("scope bind (write): {e}"));
    let err = sqlx::query(
        r#"INSERT INTO livechat.sessions (channel_id, expertise_names, company_id)
           VALUES ($1, '{}', $2)"#,
    )
    .bind(channel_a)
    .bind(company_b)
    .execute(&mut *conn)
    .await
    .err()
    .unwrap_or_else(|| panic!("a scoped INSERT into ANOTHER company must hit the WITH CHECK wall"));
    assert_eq!(
        pg_code(&err),
        "42501",
        "the cross-company write hits the policy WITH CHECK, got {err}"
    );
    // The SAME shape for its OWN company succeeds (the fence permits
    // the app's own writes — it is a fence, not a lockout).
    sqlx::query(
        r#"INSERT INTO livechat.sessions (channel_id, expertise_names, company_id)
           VALUES ($1, '{}', $2)"#,
    )
    .bind(channel_a)
    .bind(company_a)
    .execute(&mut *conn)
    .await
    .unwrap_or_else(|e| panic!("the in-company write must land for the app role: {e}"));

    // ── 5. A scoped UPDATE cannot touch another company's row ───────
    let result = sqlx::query("UPDATE livechat.sessions SET title = 'probe' WHERE id = $1")
        .bind(session_b.id)
        .execute(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("cross-company update must answer (0 rows), got {e}"));
    assert_eq!(
        result.rows_affected(),
        0,
        "company A's scope cannot UPDATE company B's row (filtered, not leaked)"
    );
    let leaked: Option<Uuid> = sqlx::query_scalar("SELECT id FROM livechat.sessions WHERE id = $1")
        .bind(session_b.id)
        .fetch_optional(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("cross-company point read: {e}"));
    assert!(
        leaked.is_none(),
        "company B's row is INVISIBLE to company A's scope, even by id"
    );
    sqlx::query("SELECT set_config('app.company_id', '', false)")
        .execute(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("scope reset (write): {e}"));
    drop(conn);

    // ── 6. The report view flows the fence (security_invoker) ──────
    let (view_unscoped,): (i64,) = sqlx::query_as("SELECT count(*) FROM livechat.session_report")
        .fetch_one(&fenced)
        .await
        .unwrap_or_else(|e| panic!("view unscoped count: {e}"));
    assert_eq!(view_unscoped, 0, "the unscoped view reads ZERO rows");
    let mut conn = fenced
        .acquire()
        .await
        .unwrap_or_else(|e| panic!("fenced acquire (view): {e}"));
    sqlx::query("SELECT set_config('app.company_id', $1, false)")
        .bind(company_b.to_string())
        .execute(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("scope bind (view): {e}"));
    let (view_scoped,): (i64,) = sqlx::query_as("SELECT count(*) FROM livechat.session_report")
        .fetch_one(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("view scoped count: {e}"));
    assert_eq!(
        view_scoped, 1,
        "the scoped view reads exactly company B's row (the fence flows THROUGH the view)"
    );
    sqlx::query("SELECT set_config('app.company_id', '', false)")
        .execute(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("scope reset (view): {e}"));
    drop(conn);

    // ── 7. The audit trail is fenced like every other table ────────
    let err = sqlx::query(
        r#"INSERT INTO livechat.livechat_audit_log (event, subject_type, subject_id, detail, company_id)
           VALUES ('session_opened', 'session', $1, '{}'::jsonb, $2)"#,
    )
    .bind(session_a.id)
    .bind(company_a)
    .execute(&fenced)
    .await
    .err()
        .unwrap_or_else(|| panic!("an unscoped audit write must be refused for the app role"));
    assert_eq!(
        pg_code(&err),
        "42501",
        "the audit trail sits behind the same fence, got {err}"
    );

    // ── 8. THE APP PATH: the scoped helpers on the app role's pool ─
    let rows: Vec<(Uuid,)> = backbone_orm::company_scope::with_company_scope(
        Some(company_a),
        backbone_orm::company_scope::fetch_all_scoped(
            &fenced,
            sqlx::query_as("SELECT company_id FROM livechat.sessions"),
        ),
    )
    .await
    .unwrap_or_else(|e| panic!("scoped helper read: {e}"));
    assert!(
        rows.iter().all(|(c,)| *c == company_a),
        "with_company_scope + fetch_all_scoped on the app role's pool returns ONLY company A rows"
    );
    assert_eq!(
        rows.len(),
        2,
        "company A owns its two sessions (seed + in-company write)"
    );
    let unscoped_rows: Vec<(Uuid,)> = backbone_orm::company_scope::with_company_scope(
        None,
        backbone_orm::company_scope::fetch_all_scoped(
            &fenced,
            sqlx::query_as("SELECT company_id FROM livechat.sessions"),
        ),
    )
    .await
    .unwrap_or_else(|e| panic!("unscoped helper read: {e}"));
    assert!(
        unscoped_rows.is_empty(),
        "with_company_scope(None) on the app role's pool reads ZERO rows (fail closed)"
    );

    // ── 9. THE LAW ITSELF: every livechat table is FORCEd and ──────
    //    carries the one policy, and the view is security_invoker.
    let laws: Vec<(String, bool, bool, bool, Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT c.relname,
                  c.relforcerowsecurity,
                  EXISTS (SELECT 1 FROM pg_policies p
                           WHERE p.schemaname = 'livechat'
                             AND p.tablename = c.relname
                             AND p.policyname = c.relname || '_company_isolation'
                             AND p.cmd = 'ALL'
                             AND 'public' = ANY (p.roles)),
                  (SELECT p.qual LIKE '%app.company_id%'
                     AND p.with_check LIKE '%app.company_id%'
                     FROM pg_policies p
                    WHERE p.schemaname = 'livechat'
                      AND p.tablename = c.relname
                      AND p.policyname = c.relname || '_company_isolation'),
                  (SELECT p.qual FROM pg_policies p
                    WHERE p.schemaname = 'livechat'
                      AND p.tablename = c.relname
                      AND p.policyname = c.relname || '_company_isolation'),
                  (SELECT p.with_check FROM pg_policies p
                    WHERE p.schemaname = 'livechat'
                      AND p.tablename = c.relname
                      AND p.policyname = c.relname || '_company_isolation')
             FROM pg_class c
             JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'livechat' AND c.relkind = 'r'
            ORDER BY c.relname"#,
    )
    .fetch_all(&owner)
    .await
    .unwrap_or_else(|e| panic!("catalog law read: {e}"));
    assert!(
        laws.len() >= 17,
        "the module's own tables are all subject to the law, got {} tables",
        laws.len()
    );
    for (table, forced, has_policy, fence_pred, qual, with_check) in &laws {
        assert!(
            forced,
            "{table} must FORCE row-level security (the owner role must not silently bypass)"
        );
        assert!(
            has_policy,
            "{table} must carry the {table}_company_isolation FOR ALL policy"
        );
        assert!(
            fence_pred,
            "{table}'s policy must key on app.company_id in BOTH directions, got qual {qual:?} / check {with_check:?}"
        );
    }
    let named: Vec<&str> = laws.iter().map(|(t, ..)| t.as_str()).collect();
    for core in [
        "channels",
        "sessions",
        "member_histories",
        "ratings",
        "livechat_audit_log",
        "chatbot_steps",
        "chatbot_messages",
    ] {
        assert!(
            named.contains(&core),
            "the core table {core} must exist under the law"
        );
    }
    let view_invoker: Option<bool> = sqlx::query_scalar(
        r#"SELECT reloptions @> ARRAY ['security_invoker=true']
             FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
            WHERE n.nspname = 'livechat' AND c.relname = 'session_report'"#,
    )
    .fetch_one(&owner)
    .await
    .unwrap_or_else(|e| panic!("view options read: {e}"));
    assert_eq!(
        view_invoker,
        Some(true),
        "session_report must be security_invoker (the fence flows through it)"
    );

    db.dispose().await;
}
