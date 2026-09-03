//! THE BOUNDARY PROBE — the declared public surface, exercised
//! through the REAL router (tower oneshot) on a pool connected AS
//! THE FENCED NOSUPERUSER ROLE (the production posture: the RLS
//! fence is what turns an unbound Host into the uniform 404).
//!
//! The allowlist is exhaustive; a wrong guest token fails TYPED
//! (401) without minting anything; the capability-gate family is ONE
//! uniform 404 with no oracle; no response ever mirrors
//! `Access-Control-Allow-Origin`; the fixed windows shape the open.

use axum::body::Body;
use axum::http::{header, HeaderMap, Method, Request, StatusCode};
use tower::ServiceExt;

use backbone_livechat::application::service::capability::mint_guest_capability;
use backbone_livechat::application::service::chatbot_service::ChatbotService;
use backbone_livechat::application::service::notifier_port::UnwiredNotifier;
use backbone_livechat::application::service::session_service::SessionCommandService;
use backbone_livechat::application::service::transcript_port::RefusingTranscriptMailer;
use backbone_livechat::application::service::website_request_service::WebsiteRequestService;
use backbone_livechat::infrastructure::persistence::{
    upsert_agent_ledger_tx, AdminConfigRepository, AnswerInput, StepInput,
};
use backbone_livechat::presentation::http::public_routes::{
    livechat_public_routes, LivechatPublicState,
};

use super::common::{
    fenced_role_pool, open_session, seed_channel_with_operators, RecordingMailCarrier,
    StubWebsiteBridge, TestDb, PROBE_SECRET,
};

use uuid::Uuid;

/// One request through the real router; returns (status, headers,
/// parsed body).
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
    builder = builder.header(header::HOST, host);
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

fn id_of(row: &serde_json::Value) -> Uuid {
    row.get("id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(|| panic!("row carries no id: {row}"))
}

#[tokio::test]
async fn the_public_surface_is_the_declared_allowlist_behind_the_fence() {
    let db = TestDb::new("boundary").await;
    let owner = db.pool.clone();
    let company = Uuid::new_v4();
    let website = Uuid::new_v4();
    let operator = Uuid::new_v4();
    let channel = seed_channel_with_operators(&owner, company, website, &[operator]).await;

    // The serving pool connects AS THE FENCED ROLE — every unscoped
    // read the router could make sees ZERO rows (the production
    // posture; this is what turns a wrong Host into the 404).
    let fenced = fenced_role_pool(&owner, &db.name).await;

    let bridge = std::sync::Arc::new(StubWebsiteBridge::new("site.example", website, company));
    let bridge_dyn: std::sync::Arc<
        dyn backbone_livechat::application::service::website_bridge::LivechatWebsiteBridge,
    > = bridge.clone();
    let state = LivechatPublicState::compose_with_trusted_proxy(
        fenced.clone(),
        bridge_dyn.clone(),
        std::sync::Arc::new(RecordingMailCarrier::default()),
        std::sync::Arc::new(UnwiredNotifier),
        std::sync::Arc::new(RefusingTranscriptMailer),
        PROBE_SECRET.to_string(),
        false,
    );
    assert!(state.secret_is_configured(), "the probe secret is composed");
    let router = livechat_public_routes(state);

    // ── 1. Availability: the anonymous arm answers, no CORS mirror ─
    let (status, headers, body) =
        call(&router, "GET", "/public/availability", "site.example", None).await;
    assert_eq!(status, StatusCode::OK, "availability answers, body {body}");
    assert!(
        body["available"].as_bool().unwrap_or(false),
        "a staffed channel is available"
    );
    assert!(
        !headers.contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN),
        "the module never mirrors CORS on availability"
    );

    // ── 2. Open: the mint arm (hit 1 of the open-ip budget) ────────
    let (status, headers, body) = call(
        &router,
        "POST",
        "/public/sessions",
        "site.example",
        Some("{}"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "the first open mints, body {body}"
    );
    let capability = body["capability"]
        .as_str()
        .unwrap_or_else(|| panic!("the open mints a capability"))
        .to_string();
    let session_id = body["session"]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("the open returns the session view"))
        .to_string();
    assert!(
        !body["resumed"].as_bool().unwrap_or(true),
        "the first open is fresh"
    );
    assert!(
        !headers.contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN),
        "the module never mirrors CORS on open"
    );

    // ── 3. The session view under the RIGHT fence ──────────────────
    let (status, headers, body) = call(
        &router,
        "GET",
        &format!("/public/sessions/{capability}"),
        "site.example",
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the session view answers, body {body}"
    );
    assert_eq!(body["id"].as_str(), Some(session_id.as_str()));
    assert!(
        !headers.contains_key(header::ACCESS_CONTROL_ALLOW_ORIGIN),
        "the module never mirrors CORS on the session view"
    );

    // ── 4. The SAME valid token under a WRONG host: uniform 404 ────
    // The HMAC verifies; the FENCE then reads the session as missing.
    // One code for every gate-family member — no oracle.
    let (status, _, body) = call(
        &router,
        "GET",
        &format!("/public/sessions/{capability}"),
        "other.example",
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "a cross-site token reads as missing"
    );
    assert_eq!(error_code(&body), "livechat_session_not_found");

    // ── 5. A garbage token: the SAME uniform 404 ───────────────────
    let (status, _, body) = call(
        &router,
        "GET",
        "/public/sessions/not-a-token",
        "site.example",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(
        error_code(&body),
        "livechat_session_not_found",
        "the malformed-token arm is indistinguishable from the missing-session arm"
    );

    // ── 6. The cursor poll and the visitor message ─────────────────
    let (status, _, body) = call(
        &router,
        "GET",
        &format!("/public/sessions/{capability}/messages"),
        "site.example",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the poll answers, body {body}");
    assert!(
        body["messages"].is_array(),
        "the poll returns the messages array"
    );
    let (status, _, body) = call(
        &router,
        "POST",
        &format!("/public/sessions/{capability}/messages"),
        "site.example",
        Some(r#"{"body": "hello from the visitor"}"#),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "the visitor message lands, body {body}"
    );
    assert_eq!(
        body["message"]["body"].as_str(),
        Some("hello from the visitor")
    );

    // ── 7. A presented INVALID token at open: TYPED 401, zero rows ─
    let (before,): (i64,) = sqlx::query_as("SELECT count(*) FROM livechat.sessions")
        .fetch_one(&owner)
        .await
        .unwrap_or_else(|e| panic!("session count: {e}"));
    let (status, _, body) = call(
        &router,
        "POST",
        "/public/sessions",
        "site.example",
        Some(r#"{"capability": "v1.c2lnbmVkLWluLXZhaW4.aW52YWxpZA"}"#),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "a presented capability that fails verify is the typed 401, body {body}"
    );
    assert_eq!(error_code(&body), "livechat_guest_token_invalid");
    let (after,): (i64,) = sqlx::query_as("SELECT count(*) FROM livechat.sessions")
        .fetch_one(&owner)
        .await
        .unwrap_or_else(|e| panic!("session recount: {e}"));
    assert_eq!(before, after, "a failed verify NEVER mints a session");

    // ── 8. The answers arm through the real route ──────────────────
    let admin = AdminConfigRepository::new(owner.clone());
    let chatbot = ChatbotService::new(
        owner.clone(),
        std::sync::Arc::new(RecordingMailCarrier::default()),
    );
    let (script_id, question_step, answer_id) =
        backbone_orm::company_scope::with_company_scope(Some(company), async {
            let script = admin.script_create("boundary script").await.unwrap();
            let script_id = id_of(&script);
            admin
                .step_create(&StepInput {
                    chatbot_script_id: script_id,
                    sequence: 1,
                    step_type: "question_selection".into(),
                    message: Some("Pick".into()),
                    expertise_tag_ids: Vec::new(),
                    answers: vec![AnswerInput {
                        sequence: 1,
                        label: "Go".into(),
                        redirect_url: None,
                    }],
                })
                .await
                .unwrap();
            let answers = admin
                .answer_list(question_step_of(&admin, script_id).await)
                .await
                .unwrap();
            (
                script_id,
                question_step_of(&admin, script_id).await,
                id_of(&answers[0]),
            )
        })
        .await;
    let bot_session = open_session(&owner, company, channel, "boundary:bot").await;
    backbone_orm::company_scope::with_company_scope(Some(company), async {
        chatbot.start_script(bot_session.id, script_id, None).await
    })
    .await
    .unwrap_or_else(|e| panic!("script bind: {e:?}"));
    let bot_capability = mint_guest_capability(
        PROBE_SECRET,
        &bot_session.id,
        "boundary:bot",
        chrono::Utc::now(),
        backbone_livechat::application::service::capability::LIVECHAT_GUEST_TOKEN_TTL_SECS,
    )
    .unwrap();
    let (status, _, body) = call(
        &router,
        "POST",
        &format!("/public/sessions/{bot_capability}/answers"),
        "site.example",
        Some(&format!(
            r#"{{"step_id": "{question_step}", "answer_id": "{answer_id}"}}"#
        )),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the answer route answers, body {body}"
    );
    assert_eq!(
        body["outcome"].as_str(),
        Some("done"),
        "the script exhausted on the answered question"
    );

    // ── 9. The rating arm: 201 then the once-wall 409 ──────────────
    let rate_session = open_session(&owner, company, channel, "boundary:rated").await;
    let mut tx = owner.begin().await.unwrap();
    upsert_agent_ledger_tx(&mut tx, rate_session.id, operator, company)
        .await
        .unwrap();
    sqlx::query("UPDATE livechat.sessions SET operator_user_id = $2 WHERE id = $1")
        .bind(rate_session.id)
        .bind(operator)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let rate_capability = mint_guest_capability(
        PROBE_SECRET,
        &rate_session.id,
        "boundary:rated",
        chrono::Utc::now(),
        backbone_livechat::application::service::capability::LIVECHAT_GUEST_TOKEN_TTL_SECS,
    )
    .unwrap();
    let uri = format!("/public/sessions/{rate_capability}/rating");
    let (status, _, body) = call(
        &router,
        "POST",
        &uri,
        "site.example",
        Some(r#"{"value": 10, "rated_persona": "agent"}"#),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "the rating lands, body {body}");
    assert_eq!(body["value"].as_i64(), Some(10));
    let (status, _, body) = call(
        &router,
        "POST",
        &uri,
        "site.example",
        Some(r#"{"value": 5, "rated_persona": "agent"}"#),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "the second rating hits the once-wall"
    );
    assert_eq!(error_code(&body), "livechat_rating_already_submitted");

    // ── 10. The close arm ──────────────────────────────────────────
    let (status, _, body) = call(
        &router,
        "POST",
        &format!("/public/sessions/{capability}/close"),
        "site.example",
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the visitor leave closes, body {body}"
    );
    assert!(
        body["closed"].as_bool().unwrap_or(false),
        "the view reports closed"
    );

    // ── 11. The invite handoff arm ─────────────────────────────────
    let invitee = Uuid::new_v4();
    bridge.register_visitor(invitee, "boundary:invitee", Some("ID"));
    let requests = WebsiteRequestService::new(
        owner.clone(),
        bridge_dyn.clone(),
        std::sync::Arc::new(UnwiredNotifier),
    );
    let operator_sessions = SessionCommandService::new(
        owner.clone(),
        std::sync::Arc::new(RecordingMailCarrier::default()),
        std::sync::Arc::new(UnwiredNotifier),
        std::sync::Arc::new(RefusingTranscriptMailer),
    );
    let invite = backbone_orm::company_scope::with_company_scope(Some(company), async {
        requests
            .create_request(website, invitee, Some(operator))
            .await
    })
    .await
    .unwrap_or_else(|e| panic!("invite create: {e:?}"));
    backbone_orm::company_scope::with_company_scope(Some(company), async {
        operator_sessions
            .post_operator_message(invite.id, operator, "we saw you browsing")
            .await
    })
    .await
    .unwrap_or_else(|e| panic!("invite message: {e:?}"));
    backbone_orm::company_scope::with_company_scope(Some(company), async {
        requests
            .audit_delivered_if_pending(invite.id, operator)
            .await
    })
    .await
    .unwrap_or_else(|e| panic!("invite delivery: {e:?}"));
    let (status, _, body) = call(
        &router,
        "GET",
        "/public/availability?visitor_key=boundary:invitee",
        "site.example",
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "availability carries the invite, body {body}"
    );
    let accept_capability = body["pending_invite"]["accept_capability"]
        .as_str()
        .unwrap_or_else(|| panic!("the delivered invite surfaces its accept capability"))
        .to_string();
    let (status, _, body) = call(
        &router,
        "POST",
        &format!("/public/invites/{accept_capability}/accept"),
        "site.example",
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the invite handoff answers, body {body}"
    );
    assert!(
        body["capability"].as_str().is_some_and(|c| !c.is_empty()),
        "accept mints the guest session capability"
    );
    // A garbage invite token is the same uniform 404.
    let (status, _, body) = call(
        &router,
        "POST",
        "/public/invites/v1.not.a.token/accept",
        "site.example",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(error_code(&body), "livechat_session_not_found");

    // ── 12. The allowlist is exhaustive: an unknown path 404s ──────
    let (status, _, _) = call(&router, "GET", "/public/nonsense", "site.example", None).await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "the surface allows nothing beyond the declared paths"
    );

    // ── 13. An unset secret fails closed at the minting verbs ─────
    let unconfigured = LivechatPublicState::compose_with_trusted_proxy(
        fenced.clone(),
        std::sync::Arc::new(StubWebsiteBridge::new("site.example", website, company)),
        std::sync::Arc::new(RecordingMailCarrier::default()),
        std::sync::Arc::new(UnwiredNotifier),
        std::sync::Arc::new(RefusingTranscriptMailer),
        String::new(),
        false,
    );
    assert!(!unconfigured.secret_is_configured());
    let bare = livechat_public_routes(unconfigured);
    let (status, _, body) = call(
        &bare,
        "POST",
        "/public/sessions",
        "site.example",
        Some("{}"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::SERVICE_UNAVAILABLE,
        "an unset secret never mints, body {body}"
    );
    assert_eq!(
        error_code(&body),
        "livechat_capability_secret_not_configured"
    );

    // ── 14. The open throttle: 6/hour per ip, then the typed 429 ──
    // Hits so far on the open-ip bucket: the open in step 2, the
    // failed-verify open in step 7 — four more land inside budget.
    for hit in 3..=6u32 {
        let (status, _, body) = call(
            &router,
            "POST",
            "/public/sessions",
            "site.example",
            Some("{}"),
        )
        .await;
        assert_ne!(
            status,
            StatusCode::TOO_MANY_REQUESTS,
            "open hit {hit} is inside the 6/hour budget, body {body}"
        );
    }
    let (status, headers, body) = call(
        &router,
        "POST",
        "/public/sessions",
        "site.example",
        Some("{}"),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::TOO_MANY_REQUESTS,
        "the 7th open in the window refuses, body {body}"
    );
    assert_eq!(error_code(&body), "livechat_throttled");
    assert_eq!(
        headers
            .get(header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok()),
        Some("3600"),
        "the throttle names its retry horizon"
    );

    db.dispose().await;
}

/// The (single) step of the boundary script (probe-local helper).
async fn question_step_of(admin: &AdminConfigRepository, script_id: Uuid) -> Uuid {
    let steps = admin
        .step_list(script_id)
        .await
        .unwrap_or_else(|e| panic!("step list: {e:?}"));
    id_of(&steps[0])
}
