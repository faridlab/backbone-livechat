//! THE TENANCY POSTURE PROBE (ADR-0029) — the module ships NO tenancy
//! of its own: no tenant column, no tenant predicate, and no RLS
//! policy. What it ships instead is the HALF-FENCE the composing
//! service's tenancy decorator completes: every livechat base table
//! carries ENABLE + FORCE ROW LEVEL SECURITY with zero policies.
//! This probe pins that posture from below, the family pattern
//! (proven on backbone-accounting, backbone-billing, backbone-selling,
//! then backbone-pos):
//!
//! - the flags are armed on every base table and the policy set is
//!   empty (schema pin); the session_report view stays
//!   security_invoker and carries no company projection;
//! - a plain NOSUPERUSER NOBYPASSRLS role is default-DENIED — zero
//!   rows, writes refused — no matter what legacy variable is set
//!   (no policy reads `app.company_id` anymore; the decorator's
//!   org-scoped policies will, once composed);
//! - the scratch owner is a superuser and BYPASSES row-level security,
//!   so it still sees its own seeded rows plainly: the denial is the
//!   missing policy, not an empty database.
//!
//! Every assertion here runs on a pool connected as a NOSUPERUSER
//! NOBYPASSRLS role (the production posture); a green suite run as
//! the scratch owner proves nothing about the fence, so the fence
//! claims never use it.

use sqlx::Row;

use super::common::{fenced_role_pool, TestDb};

fn pg_code(err: &sqlx::Error) -> String {
    err.as_database_error()
        .and_then(|db| db.code())
        .map(|c| c.to_string())
        .unwrap_or_default()
}

/// The livechat base tables the strip migration freed of their
/// company axis — every one must stay behind the armed half-fence.
const BASE_TABLES: [&str; 17] = [
    "channels",
    "channel_members",
    "channel_rules",
    "chatbot_answers",
    "chatbot_messages",
    "chatbot_scripts",
    "chatbot_steps",
    "chatbot_step_triggers",
    "conversation_tags",
    "expertise_tags",
    "livechat_audit_log",
    "member_histories",
    "operator_expertise",
    "operator_profiles",
    "ratings",
    "sessions",
    "session_tags",
];

// ── The schema pin: armed flags, empty policy set ─────────────────────────────

/// Every livechat base table carries ENABLE + FORCE ROW LEVEL SECURITY
/// and the module ships ZERO policies — the decorator's half-fence. If
/// a strip or regen ever drops the flags, an undecorated deployment
/// would silently become readable by any role the host grants; if a
/// policy ever reappears module-side, the decorator's org-scoped
/// policies would fight it. The session_report view keeps
/// security_invoker (the decorator's fence must flow through it) and
/// loses its company projection column with the strip.
#[tokio::test]
async fn tables_carry_rls_flags_and_the_module_ships_no_policy() {
    let db = TestDb::new("fencedruntime").await;
    let owner = db.pool.clone();

    let armed: Vec<String> = sqlx::query(
        "SELECT c.relname FROM pg_class c \
         JOIN pg_namespace n ON n.oid = c.relnamespace \
         WHERE n.nspname = 'livechat' AND c.relkind = 'r' \
           AND c.relrowsecurity AND c.relforcerowsecurity \
         ORDER BY c.relname",
    )
    .fetch_all(&owner)
    .await
    .unwrap_or_else(|e| panic!("rls flags read: {e}"))
    .iter()
    .map(|r| r.get::<String, _>("relname"))
    .collect();
    for table in BASE_TABLES {
        assert!(
            armed.iter().any(|t| t == table),
            "{table} must carry ENABLE + FORCE ROW LEVEL SECURITY"
        );
    }

    let policies: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pg_policy WHERE polrelid::regnamespace::text = 'livechat'",
    )
    .fetch_one(&owner)
    .await
    .unwrap_or_else(|e| panic!("policy count read: {e}"));
    assert_eq!(
        policies, 0,
        "the module ships no RLS policy — isolation belongs to the composing service's decorator"
    );

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
        "session_report must be security_invoker (the decorator's fence flows through it)"
    );
    let view_company_columns: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM information_schema.columns \
         WHERE table_schema = 'livechat' AND table_name = 'session_report' \
           AND column_name = 'company_id'",
    )
    .fetch_one(&owner)
    .await
    .unwrap_or_else(|e| panic!("view column read: {e}"));
    assert_eq!(
        view_company_columns, 0,
        "session_report must not project a company_id column after the strip"
    );

    db.dispose().await;
}

// ── Default-deny until composed: the plain probe role ─────────────────────────

/// A plain non-superuser, NOBYPASSRLS role with bare grants sees NOTHING and cannot
/// write — with or without the legacy company variable set. No policy admits it (there
/// are none), and none reads `app.company_id` anymore. The owner pool still sees its
/// seeded row: the denial is the missing policy, not an empty database.
#[tokio::test]
async fn plain_role_is_default_denied_until_the_decorator_composes() {
    let db = TestDb::new("fenceddeny").await;
    let owner = db.pool.clone();

    // The owner seeds a row as the scratch superuser (whom RLS can never bind). No
    // tenant column exists to set — a script is just a row (ADR-0029).
    sqlx::query("INSERT INTO livechat.chatbot_scripts (title) VALUES ('probe')")
        .execute(&owner)
        .await
        .unwrap_or_else(|e| panic!("owner seed: {e}"));

    let fenced = fenced_role_pool(&owner, &db.name).await;

    // Bare read: zero rows — default-deny with no policy admitting the role.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM livechat.chatbot_scripts")
        .fetch_one(&fenced)
        .await
        .unwrap_or_else(|e| panic!("restricted read: {e}"));
    assert_eq!(n, 0, "a role no policy admits sees zero rows");

    // The legacy company variable resurrects nothing: no policy reads it anymore
    // (the decorator's org-scoped policies will, once composed).
    let mut conn = fenced
        .acquire()
        .await
        .unwrap_or_else(|e| panic!("restricted acquire: {e}"));
    sqlx::query("SELECT set_config('app.company_id', $1, false)")
        .bind(uuid::Uuid::new_v4().to_string())
        .execute(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("legacy variable bind: {e}"));
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM livechat.chatbot_scripts")
        .fetch_one(&mut *conn)
        .await
        .unwrap_or_else(|e| panic!("restricted read under the legacy variable: {e}"));
    assert_eq!(
        n, 0,
        "the legacy variable must not bypass the absent policy set"
    );
    drop(conn);

    // A write is refused outright (no WITH CHECK policy admits the new row).
    let err = sqlx::query("INSERT INTO livechat.chatbot_scripts (title) VALUES ('probe write')")
        .execute(&fenced)
        .await
        .err()
        .unwrap_or_else(|| panic!("a write with no admitting policy must be refused"));
    assert_eq!(
        pg_code(&err),
        "42501",
        "the default-denied write hits row-level security, got {err}"
    );
    assert!(
        err.to_string().to_lowercase().contains("row-level security"),
        "the refusal names row-level security, got {err}"
    );

    // The default-deny flows THROUGH the security_invoker view too.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM livechat.session_report")
        .fetch_one(&fenced)
        .await
        .unwrap_or_else(|e| panic!("restricted view read: {e}"));
    assert_eq!(n, 0, "the view is default-denied like its base tables");

    // The owner pool still sees its row.
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM livechat.chatbot_scripts")
        .fetch_one(&owner)
        .await
        .unwrap_or_else(|e| panic!("owner read: {e}"));
    assert_eq!(n, 1, "the owner pool must still see the seeded row");

    db.dispose().await;
}
