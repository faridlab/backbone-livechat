//! The chatbot pointer state machine probe: the WHOLE runtime state
//! is `sessions.chatbot_current_step_id` + the `chatbot_messages`
//! execution log. The seven-type closure is a save-time wall, routing
//! is forward-only, the welcome is LAZY (the open mints ZERO message
//! rows), the pointer race is guarded typed, and answers store
//! sanitized plain text only.

use uuid::Uuid;

use backbone_livechat::application::service::chatbot_service::{
    AnswerPayload, ChatbotService, EngineOutcome,
};
use backbone_livechat::application::service::livechat_error::LivechatError;
use backbone_livechat::infrastructure::persistence::{
    AdminConfigRepository, AnswerInput, StepInput,
};

use super::common::{open_session, seed_channel_with_operators, TestDb};

fn id_of(row: &serde_json::Value) -> Uuid {
    row.get("id")
        .and_then(|v| v.as_str())
        .and_then(|s| Uuid::parse_str(s).ok())
        .unwrap_or_else(|| panic!("row carries no id: {row}"))
}

fn answer_input(sequence: i32, label: &str) -> AnswerInput {
    AnswerInput {
        sequence,
        label: label.to_string(),
        redirect_url: None,
    }
}

#[tokio::test]
async fn the_pointer_state_machine_is_forward_only_and_sanitized() {
    let db = TestDb::new("chatbot").await;
    let pool = db.pool.clone();
    let website = Uuid::new_v4();
    let operator = Uuid::new_v4();
    let channel = seed_channel_with_operators(&pool, website, &[operator]).await;

    let admin = AdminConfigRepository::new(pool.clone());
    let carrier = std::sync::Arc::new(super::common::RecordingMailCarrier::default());
    let chatbot = ChatbotService::new(pool.clone(), carrier.clone());

    // The script: text -> selection -> email -> free input -> forward.
    let script = {
        let script = admin
            .script_create("pointer script")
            .await
            .unwrap_or_else(|e| panic!("script create: {e:?}"));
        let script_id = id_of(&script);
        let text = admin
            .step_create(&StepInput {
                chatbot_script_id: script_id,
                sequence: 1,
                step_type: "text".into(),
                message: Some("Welcome to the probe".into()),
                expertise_tag_ids: Vec::new(),
                answers: Vec::new(),
            })
            .await
            .unwrap_or_else(|e| panic!("text step: {e:?}"));
        let question = admin
            .step_create(&StepInput {
                chatbot_script_id: script_id,
                sequence: 2,
                step_type: "question_selection".into(),
                message: Some("Pick one".into()),
                expertise_tag_ids: Vec::new(),
                answers: vec![answer_input(1, "Yes"), answer_input(2, "No")],
            })
            .await
            .unwrap_or_else(|e| panic!("selection step: {e:?}"));
        let email = admin
            .step_create(&StepInput {
                chatbot_script_id: script_id,
                sequence: 3,
                step_type: "question_email".into(),
                message: Some("Your email".into()),
                expertise_tag_ids: Vec::new(),
                answers: Vec::new(),
            })
            .await
            .unwrap_or_else(|e| panic!("email step: {e:?}"));
        let free = admin
            .step_create(&StepInput {
                chatbot_script_id: script_id,
                sequence: 4,
                step_type: "free_input_single".into(),
                message: Some("Anything to add".into()),
                expertise_tag_ids: Vec::new(),
                answers: Vec::new(),
            })
            .await
            .unwrap_or_else(|e| panic!("free step: {e:?}"));
        let forward = admin
            .step_create(&StepInput {
                chatbot_script_id: script_id,
                sequence: 5,
                step_type: "forward_operator".into(),
                message: Some("Connecting you".into()),
                expertise_tag_ids: Vec::new(),
                answers: Vec::new(),
            })
            .await
            .unwrap_or_else(|e| panic!("forward step: {e:?}"));
        (
            script_id,
            id_of(&text),
            id_of(&question),
            id_of(&email),
            id_of(&free),
            id_of(&forward),
        )
    };
    let (script_id, text_step, question_step, email_step, free_step, _forward_step) = script;

    // The declared answers off the question step.
    let (answer_yes,) = {
        let answers = admin
            .answer_list(question_step)
            .await
            .unwrap_or_else(|e| panic!("answer list: {e:?}"));
        (id_of(&answers[0]),)
    };

    // ── LAZY WELCOME: the bind mints ZERO message rows ────────────
    let session = open_session(&pool, channel, "chatbot:visitor").await;
    let first = chatbot
        .start_script(session.id, script_id, None)
        .await
        .unwrap_or_else(|e| panic!("start script: {e:?}"))
        .unwrap_or_else(|| panic!("start script returned no step"));
    assert_eq!(first.id, text_step, "the pointer starts at the FIRST step");
    let (minted,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM livechat.chatbot_messages WHERE session_id = $1")
            .bind(session.id)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| panic!("message count: {e}"));
    assert_eq!(minted, 0, "the open mints ZERO message rows (lazy welcome)");
    // The bot ledger row exists (persona trichotomy's third arm).
    let (bot_rows,): (i64,) = sqlx::query_as(
        r#"SELECT count(*) FROM livechat.member_histories
            WHERE session_id = $1 AND persona = 'bot' AND chatbot_script_id = $2"#,
    )
    .bind(session.id)
    .bind(script_id)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("bot ledger count: {e}"));
    assert_eq!(bot_rows, 1, "the bind joins the bot ledger row");

    // ── The first interaction materializes the leading text run ──
    let outcome = chatbot
        .on_visitor_interaction(session.id, None)
        .await
        .unwrap_or_else(|e| panic!("first interaction: {e:?}"));
    match &outcome {
        EngineOutcome::Waiting { step } => {
            assert_eq!(
                step.id, question_step,
                "the text step posted, the pointer advanced"
            );
        }
        other => panic!("the run must wait at the question, got {other:?}"),
    }
    let (posted,): (i64,) =
        sqlx::query_as("SELECT count(*) FROM livechat.chatbot_messages WHERE session_id = $1")
            .bind(session.id)
            .fetch_one(&pool)
            .await
            .unwrap_or_else(|e| panic!("posted count: {e}"));
    assert_eq!(
        posted, 1,
        "the leading text step materialized on first contact"
    );

    // ── The pointer race guard: a stale step id refuses typed ─────
    let refused = chatbot
        .answer(
            session.id,
            Some(text_step),
            AnswerPayload::Text { input: "x".into() },
            None,
        )
        .await;
    assert!(
        matches!(refused, Err(LivechatError::StepNotCurrent)),
        "the stale-step race is the typed StepNotCurrent, got {refused:?}"
    );

    // ── An undeclared selection answer refuses typed ──────────────
    let refused = chatbot
        .answer(
            session.id,
            Some(question_step),
            AnswerPayload::Selection {
                answer_id: Uuid::new_v4(),
            },
            None,
        )
        .await;
    assert!(
        matches!(refused, Err(LivechatError::AnswerInvalid)),
        "an off-menu selection is AnswerInvalid, got {refused:?}"
    );

    // ── The declared answer advances the pointer FORWARD ──────────
    let outcome = chatbot
        .answer(
            session.id,
            Some(question_step),
            AnswerPayload::Selection {
                answer_id: answer_yes,
            },
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("declared answer: {e:?}"));
    match &outcome {
        EngineOutcome::Waiting { step } => {
            assert_eq!(
                step.id, email_step,
                "the selection advanced to the email step"
            );
        }
        other => panic!("the answer must wait at the email step, got {other:?}"),
    }

    // ── Email normalization + sanitized storage ───────────────────
    let refused = chatbot
        .answer(
            session.id,
            Some(email_step),
            AnswerPayload::Text {
                input: "not-an-email".into(),
            },
            None,
        )
        .await;
    assert!(
        matches!(refused, Err(LivechatError::InputInvalid)),
        "a malformed email is InputInvalid, got {refused:?}"
    );
    let outcome = chatbot
        .answer(
            session.id,
            Some(email_step),
            AnswerPayload::Text {
                input: "  User@Example.COM  ".into(),
            },
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("good email: {e:?}"));
    match &outcome {
        EngineOutcome::Waiting { step } => assert_eq!(step.id, free_step),
        other => panic!("the email step must advance to free input, got {other:?}"),
    }
    let (stored_email,): (Option<String>,) = sqlx::query_as(
        r#"SELECT visitor_answer FROM livechat.chatbot_messages
            WHERE session_id = $1 AND visitor_answer IS NOT NULL
            ORDER BY created_at DESC LIMIT 1"#,
    )
    .bind(session.id)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("stored answer read: {e}"));
    // The LAST stored visitor answer is the normalized email...
    assert_eq!(
        stored_email.as_deref(),
        Some("user@example.com"),
        "emails store normalized (trimmed, lowercased), got {stored_email:?}"
    );

    // ── Free input sanitization: raw HTML never lands ─────────────
    let outcome = chatbot
        .answer(
            session.id,
            Some(free_step),
            AnswerPayload::Text {
                input: "<b>hello</b> <script>alert(1)</script>".into(),
            },
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("free input: {e:?}"));
    match &outcome {
        // The forward step auto-fires inside the engine; the operator
        // joins, so the run ENDS with the human owning the session.
        EngineOutcome::Done => {}
        other => panic!("the forward handoff ends the bot run, got {other:?}"),
    }
    let (sanitized, forward_fired, session_closed, assigned): (
        Option<String>,
        i64,
        bool,
        Option<Uuid>,
    ) = sqlx::query_as(
        r#"SELECT (SELECT visitor_answer FROM livechat.chatbot_messages
                          WHERE session_id = $1 AND visitor_answer IS NOT NULL
                          ORDER BY created_at DESC LIMIT 1),
                      (SELECT count(*) FROM livechat.livechat_audit_log
                        WHERE subject_id = $1 AND event = 'chatbot_forwarded'),
                      (closed_at IS NOT NULL),
                      operator_user_id
                 FROM livechat.sessions WHERE id = $1"#,
    )
    .bind(session.id)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("final state read: {e}"));
    assert_eq!(
        sanitized.as_deref(),
        Some("hello alert(1)"),
        "free input stores sanitized plain text, got {sanitized:?}"
    );
    assert_eq!(forward_fired, 1, "the forward step's handoff is audited");
    assert!(
        !session_closed,
        "an agent-owned session does not bot-complete"
    );
    assert_eq!(
        assigned,
        Some(operator),
        "the forward handoff assigned the operator"
    );

    // ── The pointer-delete fence: a pointed-at step refuses delete ─
    // (a fresh session pointed at the question step.)
    let fence_session = open_session(&pool, channel, "chatbot:fence").await;
    chatbot
        .start_script(fence_session.id, script_id, None)
        .await
        .unwrap_or_else(|e| panic!("fence bind: {e:?}"));
    let refused = admin.step_delete(text_step).await;
    assert!(
        matches!(&refused, Err(LivechatError::Validation(refusal)) if refusal.contains("pointer")),
        "deleting a step an open pointer references refuses typed, got {refused:?}"
    );

    // ── The no-agent forward: bot-only completion closes the run ──
    let empty_channel = seed_channel_with_operators(&pool, website, &[]).await;
    let solo = open_session(&pool, empty_channel, "chatbot:solo").await;
    chatbot
        .start_script(solo.id, script_id, None)
        .await
        .unwrap_or_else(|e| panic!("solo bind: {e:?}"));
    // Walk to the forward step; the empty pool writes no_agent and the
    // script EXHAUSTS into the bot-only completion close.
    let outcome = chatbot
        .on_visitor_interaction(solo.id, None)
        .await
        .unwrap_or_else(|e| panic!("solo interaction: {e:?}"));
    assert!(
        matches!(outcome, EngineOutcome::Waiting { .. }),
        "the solo run reaches the question"
    );
    let outcome = chatbot
        .answer(
            solo.id,
            Some(question_step),
            AnswerPayload::Selection {
                answer_id: answer_yes,
            },
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("solo selection: {e:?}"));
    assert!(matches!(outcome, EngineOutcome::Waiting { .. }));
    let outcome = chatbot
        .answer(
            solo.id,
            Some(email_step),
            AnswerPayload::Text {
                input: "solo@x.y".into(),
            },
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("solo email: {e:?}"));
    assert!(matches!(outcome, EngineOutcome::Waiting { .. }));
    let outcome = chatbot
        .answer(
            solo.id,
            Some(free_step),
            AnswerPayload::Text {
                input: "done".into(),
            },
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("solo free input: {e:?}"));
    assert!(
        matches!(outcome, EngineOutcome::Done),
        "the solo run ends at the script's end"
    );
    let (closed, reason, failure): (bool, Option<String>, String) = sqlx::query_as(
        r#"SELECT (closed_at IS NOT NULL), close_reason::text, failure::text
             FROM livechat.sessions WHERE id = $1"#,
    )
    .bind(solo.id)
    .fetch_one(&pool)
    .await
    .unwrap_or_else(|e| panic!("solo close read: {e}"));
    assert!(closed, "a bot-only session closes when the script exhausts");
    assert_eq!(
        reason.as_deref(),
        Some("bot_completed"),
        "the bot-only close reason"
    );
    assert_eq!(failure, "no_agent", "the empty forward pool wrote no_agent");

    // The carrier seam carried the engine's posts throughout (the
    // recording carrier's log holds the welcome text).
    let posted = carrier
        .posted
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    assert!(
        posted
            .iter()
            .any(|(id, who, body)| *id == session.id && who == "bot" && body.contains("Welcome")),
        "the engine's text steps posted through the carrier seam, got {posted:?}"
    );
    db.dispose().await;
}

#[tokio::test]
async fn the_seven_type_closure_and_forward_only_routing_are_save_time_walls() {
    let db = TestDb::new("chatbotlaws").await;
    let pool = db.pool.clone();
    let admin = AdminConfigRepository::new(pool.clone());

    let (script_id, first_step, question_step, answer_id) = {
        let script = admin
            .script_create("law script")
            .await
            .unwrap_or_else(|e| panic!("script create: {e:?}"));
        let script_id = id_of(&script);
        let text = admin
            .step_create(&StepInput {
                chatbot_script_id: script_id,
                sequence: 1,
                step_type: "text".into(),
                message: Some("first".into()),
                expertise_tag_ids: Vec::new(),
                answers: Vec::new(),
            })
            .await
            .unwrap();
        let question = admin
            .step_create(&StepInput {
                chatbot_script_id: script_id,
                sequence: 2,
                step_type: "question_selection".into(),
                message: Some("pick".into()),
                expertise_tag_ids: Vec::new(),
                answers: vec![answer_input(1, "Go")],
            })
            .await
            .unwrap();
        let answers = admin.answer_list(id_of(&question)).await.unwrap();
        (
            script_id,
            id_of(&text),
            id_of(&question),
            id_of(&answers[0]),
        )
    };

    // EVERY community type is accepted...
    let mut sequence = 10;
    for step_type in [
        "text",
        "question_email",
        "question_phone",
        "forward_operator",
        "free_input_single",
        "free_input_multi",
    ] {
        sequence += 1;
        admin
            .step_create(&StepInput {
                chatbot_script_id: script_id,
                sequence,
                step_type: step_type.into(),
                message: Some(format!("{step_type} step")),
                expertise_tag_ids: Vec::new(),
                answers: Vec::new(),
            })
            .await
            .unwrap_or_else(|e| panic!("the community type {step_type} must be accepted: {e:?}"));
    }
    // ...and question_selection (already created) completes the seven.

    // The upstream create arms are REFUSED by omission.
    for banned in ["create_lead", "create_ticket"] {
        let refused = admin
            .step_create(&StepInput {
                chatbot_script_id: script_id,
                sequence: 50,
                step_type: banned.into(),
                message: None,
                expertise_tag_ids: Vec::new(),
                answers: Vec::new(),
            })
            .await;
        assert!(
            matches!(&refused, Err(LivechatError::Validation(msg)) if msg.contains("seven")),
            "the banned arm {banned} refuses typed against the closure, got {refused:?}"
        );
    }
    // So does an unknown type outright.
    let refused = admin
        .step_create(&StepInput {
            chatbot_script_id: script_id,
            sequence: 51,
            step_type: "gibberish".into(),
            message: None,
            expertise_tag_ids: Vec::new(),
            answers: Vec::new(),
        })
        .await;
    assert!(
        matches!(&refused, Err(LivechatError::Validation(msg)) if msg.contains("seven")),
        "an unknown type refuses typed against the closure, got {refused:?}"
    );

    // A question step WITHOUT answers refuses at save time.
    let refused = admin
        .step_create(&StepInput {
            chatbot_script_id: script_id,
            sequence: 60,
            step_type: "question_selection".into(),
            message: Some("empty".into()),
            expertise_tag_ids: Vec::new(),
            answers: Vec::new(),
        })
        .await;
    assert!(
        matches!(&refused, Err(LivechatError::Validation(msg)) if msg.contains("answer")),
        "a question step born without answers refuses, got {refused:?}"
    );

    // A NON-question step refuses inline answers.
    let refused = admin
        .step_create(&StepInput {
            chatbot_script_id: script_id,
            sequence: 61,
            step_type: "text".into(),
            message: Some("with answers".into()),
            expertise_tag_ids: Vec::new(),
            answers: vec![answer_input(1, "stray")],
        })
        .await;
    assert!(
        matches!(&refused, Err(LivechatError::Validation(msg)) if msg.contains("answers")),
        "a non-question step refuses answers, got {refused:?}"
    );

    // The detached answer verb refuses non-question steps too.
    let refused = admin
        .answer_create(first_step, &answer_input(1, "stray"))
        .await;
    assert!(
        matches!(&refused, Err(LivechatError::Validation(msg)) if msg.contains("question")),
        "answers attach to question steps only, got {refused:?}"
    );

    // ── FORWARD-ONLY routing: a backwards trigger refuses at save ──
    let refused = admin.trigger_create(answer_id, first_step).await;
    assert!(
        matches!(&refused, Err(LivechatError::Validation(msg)) if msg.contains("forward-only")),
        "a backwards trigger refuses typed, got {refused:?}"
    );
    // A cross-script target refuses too (no such step here: a random
    // id is neither later nor same-script).
    let refused = admin.trigger_create(answer_id, Uuid::new_v4()).await;
    assert!(
        matches!(&refused, Err(LivechatError::Validation(_))),
        "a cross-script trigger refuses typed, got {refused:?}"
    );
    // A FORWARD trigger lands.
    let forward_target = {
        let step = admin
            .step_create(&StepInput {
                chatbot_script_id: script_id,
                sequence: 70,
                step_type: "text".into(),
                message: Some("later".into()),
                expertise_tag_ids: Vec::new(),
                answers: Vec::new(),
            })
            .await
            .unwrap();
        let target = id_of(&step);
        admin
            .trigger_create(answer_id, target)
            .await
            .map(|_| target)
    }
    .unwrap_or_else(|e| panic!("a forward trigger is legal: {e:?}"));
    assert_ne!(forward_target, question_step);
    db.dispose().await;
}
