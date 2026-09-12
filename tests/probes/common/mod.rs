//! Shared harness: one DISPOSABLE scratch database per probe,
//! FAIL-HARD (the website/events-module suite's contract, verbatim in
//! shape).
//!
//! The suite never runs against a shared database (and NEVER against
//! the live dev database on 5432): each probe mints
//! `livechat_probe_<marker>_<hex>` on the local scratch Postgres
//! (127.0.0.1:5433 — the pinned scratch container), applies this
//! module's migrations with a raw SQL file runner, runs, and drops
//! the database.
//!
//! FAIL-HARD CONTRACT: a probe that cannot reach its scratch
//! database PANICS — [`TestDb::new`] refuses to return `None`, and
//! [`skipped`] panics on principle. A green suite means the
//! behaviors were exercised, not that they were unreachable.

use std::time::Duration;

use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use uuid::Uuid;

/// The scratch Postgres every probe database is born on and dropped
/// from. 127.0.0.1:5433 — the pinned scratch container, NEVER a live
/// service database.
pub const SCRATCH_ADMIN_URL: &str = "postgres://postgres:postgres@127.0.0.1:5433/postgres";

/// The probe capability secret (explicit, never from the environment
/// — probes must not depend on host configuration).
pub const PROBE_SECRET: &str = "livechat-probe-capability-secret";

fn admin_url() -> String {
    std::env::var("LIVECHAT_TEST_ADMIN_URL").unwrap_or_else(|_| SCRATCH_ADMIN_URL.into())
}

/// The fail-hard skip: reaching this is a FAILURE, never a green
/// tick.
pub fn skipped(reason: &str) -> ! {
    panic!("VACUOUS SKIP IS A FAILURE: {reason}");
}

/// One disposable scratch database, migrations applied. Panics
/// (never returns `None`) when the scratch Postgres is unreachable.
pub struct TestDb {
    pub pool: PgPool,
    pub name: String,
    admin: PgPool,
}

impl TestDb {
    pub async fn new(marker: &str) -> Self {
        let url = admin_url();
        let admin = match PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&url)
            .await
        {
            Ok(a) => a,
            Err(e) => {
                eprintln!("PROBE-FAIL: {marker}: admin connect to {url} failed: {e}");
                skipped(&format!("scratch Postgres unreachable: {e}"));
            }
        };
        // Stale probe databases from THIS MARKER's crashed runs go
        // first. The pattern is scoped to the marker on purpose: probe
        // tests run in parallel, each with its own disposable database,
        // and a sweep over every `livechat_probe_%` name would drop a
        // SIBLING test's live database mid-run.
        let marker_pattern = format!(r"livechat\_probe\_{marker}\_%");
        if let Err(e) = sqlx::query(
            r#"SELECT pg_terminate_backend(pid) FROM pg_stat_activity
                WHERE datname LIKE $1 AND pid <> pg_backend_pid()"#,
        )
        .bind(&marker_pattern)
        .execute(&admin)
        .await
        {
            eprintln!("PROBE-WARN: {marker}: stale-session sweep failed: {e}");
        }
        let stale: Vec<String> = match sqlx::query_scalar(
            r#"SELECT quote_ident(datname) FROM pg_database
                WHERE datname LIKE $1"#,
        )
        .bind(&marker_pattern)
        .fetch_all(&admin)
        .await
        {
            Ok(names) => names,
            Err(e) => {
                eprintln!("PROBE-WARN: {marker}: stale-db listing failed: {e}");
                Vec::new()
            }
        };
        for ident in &stale {
            let _ = sqlx::query(&format!(r#"DROP DATABASE IF EXISTS {ident} WITH (FORCE)"#))
                .execute(&admin)
                .await;
        }
        let suffix: String = Uuid::new_v4()
            .simple()
            .to_string()
            .chars()
            .take(8)
            .collect();
        let name = format!("livechat_probe_{marker}_{suffix}");
        if let Err(e) = sqlx::query(&format!(r#"CREATE DATABASE "{name}""#))
            .execute(&admin)
            .await
        {
            eprintln!("PROBE-FAIL: {marker}: create database {name} failed: {e}");
            skipped(&format!("scratch create failed: {e}"));
        }
        let db_url = match url.rfind('/') {
            Some(i) => format!("{}{}", &url[..=i], name),
            None => url.clone(),
        };
        let pool = match PgPoolOptions::new()
            .max_connections(12)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&db_url)
            .await
        {
            Ok(p) => p,
            Err(e) => {
                eprintln!("PROBE-FAIL: {marker}: connect to {db_url} failed: {e}");
                skipped(&format!("scratch connect failed: {e}"));
            }
        };
        if let Err(what) = apply_module_migrations(&pool, marker).await {
            skipped(&what);
        }
        Self { pool, name, admin }
    }

    /// Explicit teardown: drop the scratch database entirely.
    pub async fn dispose(self) {
        self.drop_db().await;
    }

    async fn drop_db(&self) {
        // FORCE: the connected probe pool may still hold an idle
        // session.
        let _ = sqlx::query(&format!(
            r#"DROP DATABASE IF EXISTS "{}" WITH (FORCE)"#,
            self.name
        ))
        .execute(&self.admin)
        .await;
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let name = self.name.clone();
        let url = admin_url();
        // Leak-guard teardown for panicking probes; dispose() is the
        // happy path.
        std::thread::spawn(move || {
            if let Ok(rt) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                rt.block_on(async move {
                    if let Ok(admin) = sqlx::PgPool::connect(&url).await {
                        let _ = sqlx::query(&format!(
                            r#"DROP DATABASE IF EXISTS "{name}" WITH (FORCE)"#
                        ))
                        .execute(&admin)
                        .await;
                    }
                });
            }
        });
    }
}

/// Apply this module's migrations with a raw SQL file runner (sorted
/// `.up.sql` order — the module's files are self-contained).
async fn apply_module_migrations(pool: &PgPool, marker: &str) -> Result<(), String> {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let dir = format!("{manifest}/migrations");
    let mut files: Vec<std::path::PathBuf> = match std::fs::read_dir(&dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.ends_with(".up.sql"))
                    .unwrap_or(false)
            })
            .collect(),
        Err(e) => return Err(format!("PROBE-FAIL: {marker}: cannot read {dir}: {e}")),
    };
    files.sort();
    let mut conn = pool
        .acquire()
        .await
        .map_err(|e| format!("PROBE-FAIL: {marker}: cannot acquire pool conn: {e}"))?;
    for file in files {
        let sql = std::fs::read_to_string(&file)
            .map_err(|e| format!("PROBE-FAIL: {marker}: cannot read {}: {e}", file.display()))?;
        if let Err(e) = sqlx::raw_sql(&sql).execute(&mut *conn).await {
            return Err(format!(
                "PROBE-FAIL: {marker}: migration {} failed: {e}",
                file.display()
            ));
        }
    }
    Ok(())
}

// ── shared fixtures ─────────────────────────────────────────────────────────

use async_trait::async_trait;
use std::sync::Mutex;

use backbone_livechat::application::service::crm_port::{
    LeadFromSession, LeadMinted, LivechatCrmLeadPort,
};
use backbone_livechat::application::service::livechat_error::LivechatError;
use backbone_livechat::application::service::mail_port::{
    LivechatMailCarrier, MessageAuthor, RefusingMailCarrier,
};
use backbone_livechat::application::service::notifier_port::UnwiredNotifier;
use backbone_livechat::application::service::transcript_port::RefusingTranscriptMailer;
use backbone_livechat::application::service::website_bridge::{
    LivechatWebsiteBridge, RefusingLivechatWebsiteBridge, VisitFacts, VisitorIdentity,
    WebsiteBinding,
};
use backbone_livechat::infrastructure::persistence::SessionCommandRepository;

/// The recording CRM port: mints a FRESH lead id per call and records
/// every request the bridge probes assert against (the seam's success
/// arm — the lead module itself is host-composed, never a sibling
/// crate here).
#[derive(Default)]
pub struct RecordingCrmLeadPort {
    pub minted: Mutex<Vec<(LeadFromSession, Uuid)>>,
}

impl RecordingCrmLeadPort {
    /// The lead ids this port minted, in mint order.
    pub fn lead_ids(&self) -> Vec<Uuid> {
        self.minted
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(|(_, id)| *id)
            .collect()
    }
}

#[async_trait]
impl LivechatCrmLeadPort for RecordingCrmLeadPort {
    async fn mint_lead(&self, req: &LeadFromSession) -> Result<LeadMinted, LivechatError> {
        let lead_id = Uuid::new_v4();
        self.minted
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((req.clone(), lead_id));
        Ok(LeadMinted { lead_id })
    }
}

/// The recording carrier: an in-memory transcript the probes assert
/// against (the seam's success arm, no external dependency).
#[derive(Default)]
pub struct RecordingMailCarrier {
    pub posted: Mutex<Vec<(Uuid, String, String)>>,
}

#[async_trait]
impl LivechatMailCarrier for RecordingMailCarrier {
    async fn post(
        &self,
        session_id: Uuid,
        author: &MessageAuthor,
        body: &str,
    ) -> Result<String, backbone_livechat::application::service::livechat_error::LivechatError>
    {
        let id = format!("carrier-{}", Uuid::new_v4().simple());
        let who = match author {
            MessageAuthor::Visitor => "visitor",
            MessageAuthor::Operator(_) => "operator",
            MessageAuthor::Bot => "bot",
        };
        self.posted.lock().unwrap_or_else(|p| p.into_inner()).push((
            session_id,
            who.to_string(),
            body.to_string(),
        ));
        Ok(id)
    }

    async fn fetch(
        &self,
        _session_id: Uuid,
        _after: Option<&str>,
        _limit: i64,
    ) -> Result<
        Vec<backbone_livechat::application::service::mail_port::CarrierMessage>,
        backbone_livechat::application::service::livechat_error::LivechatError,
    > {
        // The recording carrier is write-observable; read-back arms
        // are probed through the refusing default's typed refusal.
        Ok(Vec::new())
    }

    async fn remove(
        &self,
        _session_id: Uuid,
    ) -> Result<u64, backbone_livechat::application::service::livechat_error::LivechatError> {
        Ok(0)
    }
}

/// The stub website bridge: a fixed host binding the website probes
/// resolve, a per-visitor identity registry the invite probes read,
/// visit-fact recording, and a per-IP key map for returning visitors.
/// The binding's `company_id` is the website module's legacy ownership
/// echo — the port field survives the tenancy strip (ADR-0029) because
/// the website module keeps its global ownership column.
pub struct StubWebsiteBridge {
    pub host: String,
    pub website_id: Uuid,
    pub company_id: Uuid,
    visitors: Mutex<std::collections::HashMap<Uuid, VisitorIdentity>>,
    by_ip: Mutex<std::collections::HashMap<String, String>>,
    visits: Mutex<Vec<(VisitFacts, Uuid)>>,
}

impl StubWebsiteBridge {
    pub fn new(host: &str, website_id: Uuid, company_id: Uuid) -> Self {
        Self {
            host: host.to_string(),
            website_id,
            company_id,
            visitors: Mutex::new(std::collections::HashMap::new()),
            by_ip: Mutex::new(std::collections::HashMap::new()),
            visits: Mutex::new(Vec::new()),
        }
    }

    /// Register a KNOWN visitor the bridge will answer for by row id
    /// (the invite probes' target).
    pub fn register_visitor(&self, visitor_id: Uuid, key: &str, country: Option<&str>) {
        self.visitors
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(
                visitor_id,
                VisitorIdentity {
                    visitor_id,
                    visitor_key: key.to_string(),
                    country_code: country.map(str::to_string),
                    timezone: Some("UTC".into()),
                },
            );
    }

    /// The recorded visit facts (probe assertions on the heartbeat
    /// piggyback).
    pub fn recorded_visits(&self) -> Vec<(VisitFacts, Uuid)> {
        self.visits
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }
}

#[async_trait]
impl LivechatWebsiteBridge for StubWebsiteBridge {
    async fn resolve_website_by_host(
        &self,
        host: &str,
    ) -> Result<
        WebsiteBinding,
        backbone_livechat::application::service::livechat_error::LivechatError,
    > {
        if host.eq_ignore_ascii_case(&self.host) {
            Ok(WebsiteBinding {
                website_id: self.website_id,
                company_id: self.company_id,
            })
        } else {
            Err(backbone_livechat::application::service::livechat_error::LivechatError::ChannelNotFound)
        }
    }

    async fn ensure_visitor(
        &self,
        facts: &VisitFacts,
    ) -> Result<
        VisitorIdentity,
        backbone_livechat::application::service::livechat_error::LivechatError,
    > {
        // A returning IP keeps its digest; a fresh IP mints one.
        if let Some(key) = self
            .by_ip
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&facts.ip)
        {
            if let Some((_, identity)) = self
                .visitors
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .iter()
                .find(|(_, v)| &v.visitor_key == key)
            {
                return Ok(identity.clone());
            }
        }
        let visitor_id = Uuid::new_v4();
        let identity = VisitorIdentity {
            visitor_id,
            visitor_key: format!("digest:{}", visitor_id.simple()),
            country_code: None,
            timezone: None,
        };
        self.visitors
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(visitor_id, identity.clone());
        self.by_ip
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .insert(facts.ip.clone(), identity.visitor_key.clone());
        Ok(identity)
    }

    async fn track_visit(
        &self,
        facts: &VisitFacts,
        visitor_id: Uuid,
    ) -> Result<(), backbone_livechat::application::service::livechat_error::LivechatError> {
        self.visits
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push((facts.clone(), visitor_id));
        Ok(())
    }

    async fn visitor_key(
        &self,
        facts: &VisitFacts,
    ) -> Result<
        Option<String>,
        backbone_livechat::application::service::livechat_error::LivechatError,
    > {
        Ok(self
            .by_ip
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&facts.ip)
            .cloned())
    }

    async fn visitor_by_id(
        &self,
        _website_id: Uuid,
        visitor_id: Uuid,
    ) -> Result<
        Option<VisitorIdentity>,
        backbone_livechat::application::service::livechat_error::LivechatError,
    > {
        Ok(self
            .visitors
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&visitor_id)
            .cloned())
    }
}

/// A visitor key helper (the digest shape the ledger binds).
pub fn visitor_key(seed: &str) -> String {
    format!("probe-digest:{seed}")
}

/// Seed a livechat surface: a channel (bound to the stub website),
/// N operator profiles with live heartbeats, and channel memberships.
/// The module is tenant-agnostic (ADR-0029): row scoping is installed
/// by the composing service's tenancy decorator, so the seeds carry no
/// tenancy column. Returns (channel_id, operator ids).
pub async fn seed_channel_with_operators(
    pool: &sqlx::PgPool,
    website_id: Uuid,
    operators: &[Uuid],
) -> Uuid {
    let channel: (Uuid,) = sqlx::query_as(
        r#"INSERT INTO livechat.channels
               (name, website_id, max_sessions_mode, max_sessions)
           VALUES ('probe channel', $1, 'unlimited', 1)
           RETURNING id"#,
    )
    .bind(website_id)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|e| panic!("seed channel failed: {e}"));
    let channel_id = channel.0;
    for op in operators {
        sqlx::query(
            r#"INSERT INTO livechat.operator_profiles
                   (user_id, display_name, languages, last_heartbeat_at)
               VALUES ($1, $2, ARRAY['en'], now())"#,
        )
        .bind(op)
        .bind(format!("operator-{}", op.simple()))
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("seed operator profile failed: {e}"));
        sqlx::query(
            r#"INSERT INTO livechat.channel_members (channel_id, user_id)
               VALUES ($1, $2)"#,
        )
        .bind(channel_id)
        .bind(op)
        .execute(pool)
        .await
        .unwrap_or_else(|e| panic!("seed channel member failed: {e}"));
    }
    channel_id
}

/// Open a session through the REAL repository on the plain probe pool
/// (the probe path for every session-minting probe). The module sets
/// no tenancy scope of its own — row isolation belongs to the
/// composing service's tenancy decorator (ADR-0029).
pub async fn open_session(
    pool: &sqlx::PgPool,
    channel_id: Uuid,
    key: &str,
) -> backbone_livechat::infrastructure::persistence::SessionRow {
    let repo = SessionCommandRepository::new(pool.clone());
    let input = backbone_livechat::infrastructure::persistence::OpenSessionInput {
        channel_id,
        title: Some("probe".into()),
        visitor_key: key.to_string(),
        website_visitor_id: None,
        visitor_country_code: None,
        visitor_timezone: None,
        visitor_language: None,
        chatbot_script_id: None,
        is_pending_request: false,
        is_test: false,
    };
    repo.open_session(&input, None)
        .await
        .unwrap_or_else(|e| panic!("probe open_session failed: {e:?}"))
}

/// The refusing port bundle (composition arms that must park loudly).
pub fn refusing_ports() -> (
    std::sync::Arc<RefusingLivechatWebsiteBridge>,
    std::sync::Arc<RefusingMailCarrier>,
    std::sync::Arc<UnwiredNotifier>,
    std::sync::Arc<RefusingTranscriptMailer>,
) {
    (
        std::sync::Arc::new(RefusingLivechatWebsiteBridge),
        std::sync::Arc::new(RefusingMailCarrier),
        std::sync::Arc::new(UnwiredNotifier),
        std::sync::Arc::new(RefusingTranscriptMailer),
    )
}

/// The fenced-role probe credentials (NOSUPERUSER NOBYPASSRLS — the
/// RLS fence is only PROVEN held by a role that cannot bypass it).
/// The role is named PER DATABASE: probe tests run in parallel, and a
/// shared role name would see one test's DROP/CREATE cycle kill
/// another's live connections.
pub const FENCED_PASSWORD: &str = "livechat_probe_pw";

fn fenced_role_for(db_name: &str) -> String {
    let suffix: String = db_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("livechat_app_{}", &suffix[..suffix.len().min(40)])
}

/// Mint (or recreate) the fenced probe role and connect a pool to
/// `db_name` AS THAT ROLE. The module ships the half-fence (ADR-0029):
/// RLS ENABLE + FORCE with zero policies, so a role that cannot bypass
/// RLS is default-denied — every read sees ZERO rows and every write
/// is refused — until the composing service's decorator installs the
/// org-scoped policies.
pub async fn fenced_role_pool(admin: &PgPool, db_name: &str) -> PgPool {
    let role = fenced_role_for(db_name);
    // Order note: this drops the ROLE before any stale probe DBs are
    // cleared (blog's probe common does the reverse — DBs first, then
    // the role — the self-healing order for a 2BP01 role-still-in-use
    // refusal). Safe here because the role name embeds the per-test DB
    // name, so a role can never be pinned by another probe's DB; if
    // that ever changes, swap to blog's order.
    if let Err(e) = sqlx::query(&format!(r#"DROP ROLE IF EXISTS {role}"#))
        .execute(admin)
        .await
    {
        skipped(&format!("cannot clear the fenced probe role: {e}"));
    }
    if let Err(e) = sqlx::query(&format!(
        r#"CREATE ROLE {role} LOGIN NOSUPERUSER NOBYPASSRLS PASSWORD '{FENCED_PASSWORD}'"#
    ))
    .execute(admin)
    .await
    {
        skipped(&format!("cannot create the fenced probe role: {e}"));
    }
    let grants = format!(
        r#"GRANT USAGE ON SCHEMA livechat TO {role};
           GRANT SELECT, INSERT, UPDATE, DELETE ON ALL TABLES IN SCHEMA livechat TO {role};
           GRANT USAGE, SELECT ON ALL SEQUENCES IN SCHEMA livechat TO {role};"#
    );
    if let Err(e) = sqlx::raw_sql(&grants).execute(admin).await {
        skipped(&format!("cannot grant the fenced probe role: {e}"));
    }
    let url = format!("postgres://{role}:{FENCED_PASSWORD}@127.0.0.1:5433/{db_name}");
    match PgPoolOptions::new()
        .max_connections(8)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&url)
        .await
    {
        Ok(p) => p,
        Err(e) => skipped(&format!("fenced role connect failed: {e}")),
    }
}
