//! The website bridge overlays probe: the operator-initiated invite
//! is a pending session with the VISITOR's own geo frozen on; it is
//! INVISIBLE to the visitor until the operator's first message (the
//! has-message gate); the visitor's own open WINS (their pending
//! invite cancels, audited); accept clears the flag; the merge
//! relink rebinds sessions and the ledger to the surviving visitor;
//! N invites freeze N distinct geos (the per-row loop leakage cannot
//! reappear); an uncomposed bridge parks loudly; the wizard binds
//! every channel it creates; a visitor message piggybacks the
//! website visit heartbeat.

use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use tower::ServiceExt;
use uuid::Uuid;

use backbone_livechat::application::service::availability_service::AvailabilityService;
use backbone_livechat::application::service::notifier_port::UnwiredNotifier;
use backbone_livechat::application::service::session_service::SessionCommandService;
use backbone_livechat::application::service::transcript_port::RefusingTranscriptMailer;
use backbone_livechat::application::service::website_request_service::WebsiteRequestService;
use backbone_livechat::presentation::http::admin_routes::{
    livechat_admin_routes, LivechatAdminState,
};
use backbone_livechat::presentation::http::public_routes::{
    livechat_public_routes, LivechatPublicState,
};

use super::common::{
    seed_channel_with_operators, visitor_key, RecordingMailCarrier, StubWebsiteBridge, TestDb,
    PROBE_SECRET,
};

#[tokio::test]
async fn the_invite_lifecycle_is_fenced_visible_and_visitor_wins() {
    let db = TestDb::new("bridge").await;
    let pool = db.pool.clone();
    let website = Uuid::new_v4();
    let operator = Uuid::new_v4();
    let channel = seed_channel_with_operators(&pool, website, &[operator]).await;

    // The website binding's `company_id` is the website module's legacy
    // ownership echo (ADR-0029) — the stub feeds it, the module never
    // reads it.
    let bridge = std::sync::Arc::new(StubWebsiteBridge::new(
        "bridge.example",
        website,
        Uuid::new_v4(),
    ));
    let bridge_dyn: std::sync::Arc<
        dyn backbone_livechat::application::service::website_bridge::LivechatWebsiteBridge,
    > = bridge.clone();
    let requests = WebsiteRequestService::new(
        pool.clone(),
        bridge_dyn.clone(),
        std::sync::Arc::new(UnwiredNotifier),
    );
    let availability = AvailabilityService::new(pool.clone(), bridge_dyn.clone());
    let sessions = SessionCommandService::new(
        pool.clone(),
        std::sync::Arc::new(super::common::RecordingMailCarrier::default()),
        std::sync::Arc::new(UnwiredNotifier),
        std::sync::Arc::new(RefusingTranscriptMailer),
    );

    // A known visitor with geo (the invite freezes THIS, not the
    // operator's locale).
    let visitor_id = Uuid::new_v4();
    let invitee_key = "bridge:invitee";
    bridge.register_visitor(visitor_id, invitee_key, Some("ID"));

    // ── CREATE: the pending invite with the visitor's own geo ─────
    let invite = requests
        .create_request(website, visitor_id, Some(operator))
        .await
        .unwrap_or_else(|e| panic!("invite create: {e:?}"));
    assert!(invite.is_pending_request, "the invite is a pending session");
    assert_eq!(
        invite.visitor_country_code.as_deref(),
        Some("ID"),
        "the invite froze the VISITOR's geo"
    );
    assert_eq!(invite.website_visitor_id, Some(visitor_id));
    // Idempotent per visitor: a repeat create returns the SAME row.
    let again = requests
        .create_request(website, visitor_id, Some(operator))
        .await
        .unwrap_or_else(|e| panic!("repeat invite create: {e:?}"));
    assert_eq!(
        again.id, invite.id,
        "the invite create is idempotent per visitor"
    );
    let (invite_created, agent_rows): (i64, i64) = sqlx::query_as(
        r#"SELECT (SELECT count(*) FROM livechat.livechat_audit_log
                     WHERE event = 'invite_created' AND subject_id = $1),
                  (SELECT count(*) FROM livechat.member_histories
                     WHERE session_id = $1 AND persona = 'agent' AND operator_user_id = $2)"#,
    )
    .bind(invite.id)
    .bind(operator)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("invite audit read: {e}"));
    assert_eq!(
        invite_created, 1,
        "the invite create is audited once (idempotent repeat)"
    );
    assert_eq!(agent_rows, 1, "the acting operator self-added a ledger row");

    // ── N invites, N geos: each row freezes ITS visitor's locale ──
    // (a batch is repeated single-visitor calls; the upstream loop
    // that leaked one visitor's country onto every row cannot
    // reappear when each call binds its own.)
    let mut distinct_geo = Vec::new();
    for (seed, country) in [("geo-my", "MY"), ("geo-jp", "JP")] {
        let visitor = Uuid::new_v4();
        bridge.register_visitor(visitor, &visitor_key(seed), Some(country));
        let row = requests
            .create_request(website, visitor, Some(operator))
            .await
            .unwrap_or_else(|e| panic!("invite create ({seed}): {e:?}"));
        assert_eq!(
            row.visitor_country_code.as_deref(),
            Some(country),
            "the invite froze {seed}'s OWN geo, not a neighbor's"
        );
        assert_eq!(row.website_visitor_id, Some(visitor));
        distinct_geo.push((row.id, country));
    }
    assert_ne!(
        distinct_geo[0].0, distinct_geo[1].0,
        "two visitors are two invite sessions"
    );
    assert_ne!(distinct_geo[0].1, distinct_geo[1].1);

    // ── INVISIBLE until the operator's first message ──────────────
    let hidden = availability
        .answer("bridge.example", None, Some(invitee_key), PROBE_SECRET)
        .await
        .unwrap_or_else(|e| panic!("availability (pre-delivery): {e:?}"));
    assert!(
        hidden.pending_invite.is_none(),
        "a message-less invite is INVISIBLE to the visitor"
    );

    // The operator's first message DELIVERS it: the gate opens once.
    sessions
        .post_operator_message(invite.id, operator, "hello from the operator")
        .await
        .unwrap_or_else(|e| panic!("operator message: {e:?}"));
    let delivered = requests
        .audit_delivered_if_pending(invite.id, operator)
        .await
        .unwrap_or_else(|e| panic!("delivery audit: {e:?}"));
    assert!(delivered, "the first message opened the gate");
    let delivered_again = requests
        .audit_delivered_if_pending(invite.id, operator)
        .await
        .unwrap_or_else(|e| panic!("repeat delivery audit: {e:?}"));
    assert!(!delivered_again, "the gate opens ONCE");
    let (delivered_audits,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM livechat.livechat_audit_log
            WHERE event = 'invite_delivered' AND subject_id = $1"#,
    )
    .bind(invite.id)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("delivered audit read: {e}"));
    assert_eq!(delivered_audits, 1, "the delivery is audited exactly once");

    // Now visible, carrying the short-TTL accept capability.
    let shown = availability
        .answer("bridge.example", None, Some(invitee_key), PROBE_SECRET)
        .await
        .unwrap_or_else(|e| panic!("availability (post-delivery): {e:?}"));
    let pending = shown
        .pending_invite
        .as_ref()
        .unwrap_or_else(|| panic!("the delivered invite surfaces with its accept capability"));
    assert_eq!(pending.session_id, invite.id);
    assert!(!pending.accept_capability.is_empty());

    // ── ACCEPT: the visitor handshake clears the pending flag ─────
    let accepted = requests
        .accept(invite.id, invitee_key, None)
        .await
        .unwrap_or_else(|e| panic!("accept: {e:?}"));
    assert!(
        !accepted.is_pending_request,
        "accept cleared the pending flag"
    );
    let (accepted_audits, visitor_rows): (i64, i64) = sqlx::query_as(
        r#"SELECT (SELECT count(*) FROM livechat.livechat_audit_log
                     WHERE event = 'invite_accepted' AND subject_id = $1),
                  (SELECT count(*) FROM livechat.member_histories
                     WHERE session_id = $1 AND persona = 'visitor' AND visitor_key = $2)"#,
    )
    .bind(invite.id)
    .bind(invitee_key)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("accept audit read: {e}"));
    assert_eq!(accepted_audits, 1, "accept is audited");
    assert_eq!(visitor_rows, 1, "accept bound the visitor ledger row");
    // After acceptance there is no pending invite to surface.
    let cleared = availability
        .answer("bridge.example", None, Some(invitee_key), PROBE_SECRET)
        .await
        .unwrap_or_else(|e| panic!("availability (post-accept): {e:?}"));
    assert!(
        cleared.pending_invite.is_none(),
        "an accepted invite never resurfaces"
    );

    // ── VISITOR WINS: their own open cancels a pending invite ─────
    let visitor_b = Uuid::new_v4();
    let key_b = "bridge:second";
    bridge.register_visitor(visitor_b, key_b, None);
    let pending_b = requests
        .create_request(website, visitor_b, Some(operator))
        .await
        .unwrap_or_else(|e| panic!("second invite: {e:?}"));
    // The visitor opens their own session on the channel (the open
    // verb's hook, driven directly here).
    let cancelled = requests
        .cancel_pending_for_visitor(channel, key_b, None)
        .await
        .unwrap_or_else(|e| panic!("visitor-wins cancel: {e:?}"))
    .unwrap_or_else(|| panic!("the visitor's open cancels their pending invite"));
    assert_eq!(cancelled.id, pending_b.id);
    let (b_closed, b_pending, b_reason, cancelled_audits): (bool, bool, Option<String>, i64) =
        sqlx::query_as(
            r#"SELECT (closed_at IS NOT NULL), is_pending_request, close_reason::text,
                      (SELECT count(*) FROM livechat.livechat_audit_log
                        WHERE event = 'invite_cancelled' AND subject_id = s.id)
                 FROM livechat.sessions s WHERE id = $1"#,
        )
        .bind(pending_b.id)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|e| panic!("visitor-wins read: {e}"));
    assert!(
        b_closed && !b_pending,
        "the visitor-wins cancel closed the invite"
    );
    assert_eq!(b_reason.as_deref(), Some("cancelled"));
    assert_eq!(cancelled_audits, 1, "the cancel is audited");
    // The row SURVIVES (no untraced destroy).
    let (survivors,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM livechat.sessions WHERE id = $1")
            .bind(pending_b.id)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| panic!("survivor read: {e}"));
    assert_eq!(survivors, 1, "a cancelled invite row is never deleted");

    // ── The merge relink: sessions and ledger rebind to the survivor ──
    let from_visitor = Uuid::new_v4();
    let to_visitor = Uuid::new_v4();
    let to_key = "bridge:merged-survivor";
    bridge.register_visitor(from_visitor, "bridge:doomed-key", None);
    bridge.register_visitor(to_visitor, to_key, None);
    let doomed = requests
        .create_request(website, from_visitor, Some(operator))
        .await
        .unwrap_or_else(|e| panic!("merge invite: {e:?}"));
    let moved = requests
        .relink_website_visitor(from_visitor, to_visitor, to_key, None)
        .await
        .unwrap_or_else(|e| panic!("relink: {e:?}"));
    assert!(
        moved >= 1,
        "the relink moved at least the doomed visitor's session"
    );
    let (rebound_session, rebound_ledger, relinked_audits): (Option<Uuid>, i64, i64) =
        sqlx::query_as(
            r#"SELECT (SELECT website_visitor_id FROM livechat.sessions WHERE id = $1),
                      (SELECT count(*) FROM livechat.member_histories
                        WHERE session_id = $1 AND persona = 'visitor' AND visitor_key = $2),
                      (SELECT count(*) FROM livechat.livechat_audit_log
                        WHERE event = 'visitor_relinked'
                          AND subject_type = 'visitor' AND subject_id = $3)"#,
        )
        .bind(doomed.id)
        .bind(to_key)
        .bind(to_visitor)
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|e| panic!("relink read: {e}"));
    assert_eq!(
        rebound_session,
        Some(to_visitor),
        "the session rebinds to the survivor"
    );
    assert_eq!(
        rebound_ledger, 1,
        "the ledger's visitor key rebinds to the survivor"
    );
    assert_eq!(relinked_audits, 1, "the relink is audited");

    db.dispose().await;
}

/// One request through a router; returns (status, headers, parsed body).
async fn call(
    router: &axum::Router,
    method: &str,
    uri: &str,
    host: &str,
    body: Option<&str>,
) -> (StatusCode, HeaderMap, serde_json::Value) {
    let mut builder = Request::builder()
        .method(Method::from_bytes(method.as_bytes()).unwrap())
        .uri(uri);
    if !host.is_empty() {
        builder = builder.header(header::HOST, host);
    }
    if body.is_some() {
        builder = builder.header(header::CONTENT_TYPE, "application/json");
    }
    let request = builder
        .body(Body::from(body.unwrap_or_default().to_string()))
        .unwrap();
    let response = router
        .clone()
        .oneshot(request)
        .await
        .unwrap_or_else(|e| panic!("{method} {uri} failed: {e}"));
    let (parts, body) = response.into_parts();
    let bytes = http_body_util::BodyExt::collect(body)
        .await
        .unwrap_or_else(|e| panic!("{method} {uri} body failed: {e}"))
        .to_bytes();
    let parsed = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (parts.status, parts.headers, parsed)
}

fn error_code(body: &serde_json::Value) -> &str {
    body["error"]["code"].as_str().unwrap_or("<no error code>")
}

/// The uncomposed bridge parks loudly on the public surface; the
/// wizard binds every channel it creates and mints no silent bot
/// rule; a visitor message piggybacks the website visit heartbeat
/// (chat activity IS the visitor heartbeat).
#[tokio::test]
async fn uncomposed_bridge_wizard_binding_and_the_visit_heartbeat() {
    let db = TestDb::new("bridge2").await;
    let pool = db.pool.clone();
    let website = Uuid::new_v4();
    let operator = Uuid::new_v4();
    seed_channel_with_operators(&pool, website, &[operator]).await;

    // ── The refusing bridge: the public verbs PARK, never degrade ──
    let (refusing_bridge, refusing_carrier, notifier, transcript) = super::common::refusing_ports();
    let parked = livechat_public_routes(LivechatPublicState::compose_with_trusted_proxy(
        pool.clone(),
        refusing_bridge,
        refusing_carrier,
        notifier,
        transcript,
        PROBE_SECRET.to_string(),
        false,
    ));
    let (status, _, body) =
        call(&parked, "GET", "/public/availability", "site.example", None).await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "availability without the bridge parks loudly, body {body}"
    );
    assert_eq!(error_code(&body), "livechat_website_bridge_not_composed");
    let (status, _, body) = call(
        &parked,
        "POST",
        "/public/sessions",
        "site.example",
        Some("{}"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "open without the bridge parks loudly, body {body}"
    );
    assert_eq!(error_code(&body), "livechat_website_bridge_not_composed");
    let (sessions_after_refusal,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM livechat.sessions")
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| panic!("post-refusal count: {e}"));
    assert_eq!(
        sessions_after_refusal, 0,
        "a parked open mints NOTHING (loud, not silent)"
    );

    // ── The wizard: every channel it creates is BOUND at birth ────
    // (and it creates no bot rule silently — rules are an explicit
    // operator action).
    let admin = livechat_admin_routes(LivechatAdminState::with_bridge(
        pool.clone(),
        std::sync::Arc::new(StubWebsiteBridge::new("bridge2.example", website, Uuid::new_v4())),
        std::sync::Arc::new(RecordingMailCarrier::default()),
        std::sync::Arc::new(UnwiredNotifier),
        std::sync::Arc::new(RefusingTranscriptMailer),
    ));
    for name in ["wizard channel one", "wizard channel two"] {
        // The admin tree is mounted behind the host's company_auth —
        // row scoping on it is the composing service's tenancy
        // decorator's law (ADR-0029), nothing the probe supplies.
        let (status, _, body) = call(
            &admin,
            "POST",
            "/admin/channels/from-website",
            "",
            Some(&format!(
                r#"{{"website_id": "{website}", "name": "{name}"}}"#
            )),
        )
        .await;
        assert_eq!(
            status,
            StatusCode::CREATED,
            "the wizard creates {name}, body {body}"
        );
        assert_eq!(
            body["website_id"].as_str(),
            Some(website.to_string().as_str()),
            "the wizard binds {name} to the website at birth"
        );
    }
    let (wizard_rules,): (i64,) = sqlx::query_as("SELECT count(*) FROM livechat.channel_rules")
        .fetch_one(&pool)
        .await
        .unwrap_or_else(|e| panic!("wizard rule count: {e}"));
    assert_eq!(wizard_rules, 0, "the wizard creates NO bot rule silently");
    let (bound_channels, unbound_channels): (i64, i64) = sqlx::query_as(
        r#"SELECT count(*) FILTER (WHERE website_id IS NOT NULL),
                  count(*) FILTER (WHERE website_id IS NULL)
             FROM livechat.channels"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("channel binding count: {e}"));
    assert_eq!(
        unbound_channels, 0,
        "every channel in this database is website-bound"
    );
    assert!(bound_channels >= 3, "the seeded channel plus the wizard's");

    // ── The visit heartbeat piggyback (chat activity IS the beat) ──
    let bridge = std::sync::Arc::new(StubWebsiteBridge::new(
        "bridge2.example",
        website,
        Uuid::new_v4(),
    ));
    let bridge_dyn: std::sync::Arc<
        dyn backbone_livechat::application::service::website_bridge::LivechatWebsiteBridge,
    > = bridge.clone();
    let public = livechat_public_routes(LivechatPublicState::compose_with_trusted_proxy(
        pool.clone(),
        bridge_dyn,
        std::sync::Arc::new(RecordingMailCarrier::default()),
        std::sync::Arc::new(UnwiredNotifier),
        std::sync::Arc::new(RefusingTranscriptMailer),
        PROBE_SECRET.to_string(),
        false,
    ));
    // A first-visit open (no token): the bridge mints the website
    // visitor the session binds.
    let (status, _, body) = call(
        &public,
        "POST",
        "/public/sessions",
        "bridge2.example",
        Some("{}"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "the open mints, body {body}");
    let capability = body["capability"]
        .as_str()
        .unwrap_or_else(|| panic!("the open returns a capability"))
        .to_string();
    let session_id: Uuid = body["session"]["id"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(|| panic!("the open returns the session id"));
    assert!(
        bridge.recorded_visits().is_empty(),
        "the OPEN itself carries no visit heartbeat (only chat activity does)"
    );
    // The visitor's first message piggybacks track_visit for the
    // session's website visitor.
    let (status, _, body) = call(
        &public,
        "POST",
        &format!("/public/sessions/{capability}/messages"),
        "bridge2.example",
        Some(r#"{"body": "hello, is anyone there?"}"#),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "the visitor message lands, body {body}"
    );
    let (website_visitor_id,): (Option<Uuid>,) =
        sqlx::query_as("SELECT website_visitor_id FROM livechat.sessions WHERE id = $1")
            .bind(session_id)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| panic!("session visitor read: {e}"));
    let visits = bridge.recorded_visits();
    assert_eq!(
        visits.len(),
        1,
        "the visitor message piggybacked exactly one visit heartbeat"
    );
    assert_eq!(
        visits[0].1,
        website_visitor_id.unwrap_or_else(|| panic!("the session binds a website visitor")),
        "the heartbeat carried the SESSION's website visitor id"
    );
    assert_eq!(
        visits[0].0.website_id, website,
        "the heartbeat named the session's website"
    );

    db.dispose().await;
}
