//! The chatbot pointer state machine (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! THE WHOLE RUNTIME STATE is `sessions.chatbot_current_step_id`
//! (one logical pointer) plus the `chatbot_messages` execution log —
//! nothing else carries bot state (the seven-type closure at
//! `livechat_step_type` makes an eighth arm unrepresentable).
//!
//! Execution invariants (each structural):
//! - LAZY WELCOME: the open mints ZERO message rows; the leading
//!   text run materializes on the visitor's FIRST interaction
//!   (message or answer) through [`Self::advance`]. The open answer
//!   carries only a derived preview.
//! - POINTER BEFORE CARRIER: each iteration advances the pointer
//!   BEFORE the carrier write, so every observer sees consistent
//!   pointer state. A carrier refusal mid-run parks loudly
//!   (`sessions.error_detail` + the `carrier_parked` audit) and
//!   stops the run — retried by the next interaction.
//! - ONE CHOKEPOINT: every posted step lands through the session
//!   message chokepoint (its four duties ride along).
//! - FORWARD-ONLY: routing is ascending-sequence only; backwards
//!   edges are refused at save time, so the router never sees one.
//! - THE FORWARD HANDOFF is the one ladder call the bot makes
//!   (stickiness off, self-pick = nobody); on nobody it writes
//!   `no_agent` and CONTINUES the script.
//! - BOT-ONLY COMPLETION: the router exhausting the script on a
//!   session no agent ever joined closes it (`bot_completed`).
//! - SANITIZED STORAGE: free text, emails, and phones are stored as
//!   sanitized plain text — raw HTML never lands anywhere.

use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use super::livechat_error::LivechatError;
use super::mail_port::{LivechatMailCarrier, MessageAuthor};
use crate::infrastructure::persistence::{
    ChatbotCommandRepository, MemberHistoryLedgerRepository, SelectionRepository,
    SessionCommandRepository, SessionRow, StepRow,
};

/// The answer verb's payload (selection vs free input — the ONE
/// input contract).
#[derive(Debug, Clone)]
pub enum AnswerPayload {
    Selection { answer_id: Uuid },
    Text { input: String },
}

/// The engine's outcome after an interaction.
#[derive(Debug, Clone)]
pub enum EngineOutcome {
    /// The script waits on the visitor: this step (a question or
    /// free-input step).
    Waiting { step: StepRow },
    /// The script ran to its end (a bot-only session is closed
    /// `bot_completed`; an agent-owned one simply stops).
    Done,
    /// A carrier refusal parked the run on `sessions.error_detail`
    /// (audited); the next interaction retries.
    Parked,
}

/// The derived preview the widget renders (no rows minted).
#[derive(Debug, Clone, serde::Serialize)]
pub struct StepPreview {
    pub step_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

pub struct ChatbotService {
    sessions: SessionCommandRepository,
    chatbot: ChatbotCommandRepository,
    members: MemberHistoryLedgerRepository,
    selection: SelectionRepository,
    carrier: Arc<dyn LivechatMailCarrier>,
}

impl ChatbotService {
    pub fn new(pool: PgPool, carrier: Arc<dyn LivechatMailCarrier>) -> Self {
        Self {
            sessions: SessionCommandRepository::new(pool.clone()),
            chatbot: ChatbotCommandRepository::new(pool.clone()),
            members: MemberHistoryLedgerRepository::new(pool.clone()),
            selection: SelectionRepository::new(pool),
            carrier,
        }
    }

    /// Bind a session to a script (the bot-first routing at open; the
    /// test verb): the script must be ROUTABLE (active, non-deleted,
    /// at least one step) or the binding refuses typed and the human
    /// path stands. Sets the pointer to the first step, joins the bot
    /// ledger row, audits the first advance. Mints ZERO message rows
    /// (the lazy-welcome law).
    pub async fn start_script(
        &self,
        session_id: Uuid,
        script_id: Uuid,
        actor: Option<Uuid>,
    ) -> Result<Option<StepRow>, LivechatError> {
        if !self.chatbot.script_is_routable(script_id).await? {
            return Err(LivechatError::Validation(
                "chatbot script is not routable (inactive, deleted, or empty)".into(),
            ));
        }
        let session = self
            .sessions
            .find(session_id)
            .await?
            .ok_or(LivechatError::SessionNotFound)?;
        let first = self
            .chatbot
            .first_step(script_id)
            .await?
            .ok_or_else(|| LivechatError::Validation("chatbot script has no steps".into()))?;
        self.members
            .upsert_bot_row(session_id, script_id, session.company_id)
            .await?;
        self.chatbot.set_pointer(session_id, Some(first.id)).await?;
        self.chatbot
            .record_step_reached(session_id, first.id, actor)
            .await?;
        Ok(Some(first))
    }

    /// The derived preview of the pending step (the open answer and
    /// the session view render it; nothing is minted).
    pub async fn pending_preview(&self, session: &SessionRow) -> Option<StepPreview> {
        let step_id = session.chatbot_current_step_id?;
        let step = self.chatbot.step(step_id).await.ok().flatten()?;
        Some(StepPreview {
            step_type: step.step_type.clone(),
            message: step.message.clone(),
        })
    }

    /// The visitor's answer to the pending step: the ONE input
    /// contract (selection answers validated against the step's
    /// declared options; email/phone inputs normalized; free inputs
    /// sanitized), the answer row (sanitized storage), the
    /// forward-only advance, then the engine run.
    pub async fn answer(
        &self,
        session_id: Uuid,
        answered_step_id: Option<Uuid>,
        payload: AnswerPayload,
        actor: Option<Uuid>,
    ) -> Result<EngineOutcome, LivechatError> {
        let session = self
            .sessions
            .find(session_id)
            .await?
            .ok_or(LivechatError::SessionNotFound)?;
        let pointer = session
            .chatbot_current_step_id
            .ok_or_else(|| LivechatError::Validation("no pending chatbot step".into()))?;
        // The race guard: the answered step must BE the pointer.
        if let Some(id) = answered_step_id {
            if id != pointer {
                return Err(LivechatError::StepNotCurrent);
            }
        }
        let step = self
            .chatbot
            .step(pointer)
            .await?
            .ok_or(LivechatError::StepNotCurrent)?;

        let (selected_answer_id, sanitized): (Option<Uuid>, Option<String>) = match payload {
            AnswerPayload::Selection { answer_id } => {
                if step.step_type != "question_selection" {
                    return Err(LivechatError::Validation(
                        "this step does not take a selection answer".into(),
                    ));
                }
                let declared = self.chatbot.answers_for_step(step.id).await?;
                if !declared.iter().any(|a| a.id == answer_id) {
                    return Err(LivechatError::AnswerInvalid);
                }
                (Some(answer_id), None)
            }
            AnswerPayload::Text { input } => match step.step_type.as_str() {
                "question_email" => (
                    None,
                    Some(normalize_email(&input).ok_or(LivechatError::InputInvalid)?),
                ),
                "question_phone" => (
                    None,
                    Some(normalize_phone(&input).ok_or(LivechatError::InputInvalid)?),
                ),
                "free_input_single" => {
                    let s = sanitize_text(&input, false);
                    if s.is_empty() {
                        return Err(LivechatError::InputInvalid);
                    }
                    (None, Some(s))
                }
                "free_input_multi" => {
                    let s = sanitize_text(&input, true);
                    if s.trim().is_empty() {
                        return Err(LivechatError::InputInvalid);
                    }
                    (None, Some(s))
                }
                "text" | "forward_operator" => {
                    return Err(LivechatError::Validation(
                        "this step is not answerable".into(),
                    ));
                }
                other => {
                    return Err(LivechatError::Validation(format!(
                        "unknown step type: {other}"
                    )));
                }
            },
        };

        // The forward step cannot re-fire once a human owns the
        // session (the answer verb refuses it typed).
        if step.step_type == "forward_operator" && session.operator_user_id.is_some() {
            return Err(LivechatError::Validation(
                "a forward step cannot re-fire once a human owns the session".into(),
            ));
        }

        // Record the answer through the ONE chokepoint (visitor
        // author: the visitor's answer flips waiting -> in_progress
        // and stamps interest; the factory append rides along).
        self.sessions
            .apply_message(&crate::infrastructure::persistence::PostMessageCommand {
                session_id,
                author: MessageAuthor::Visitor,
                body: sanitized.clone().unwrap_or_default(),
                carrier_message_id: None,
                chatbot: Some(crate::infrastructure::persistence::ChatbotMessageAppend {
                    step_id: Some(step.id),
                    selected_answer_id,
                    visitor_answer: sanitized,
                }),
            })
            .await?;

        // The forward-only advance, then the engine.
        let selected: Vec<Uuid> = selected_answer_id.into_iter().collect();
        let next = self.chatbot.fetch_next_step(step.id, &selected).await?;
        self.chatbot
            .set_pointer(session_id, next.map(|s| s.id))
            .await?;
        self.chatbot
            .record_step_reached(session_id, step.id, actor)
            .await?;
        self.advance(session_id, actor).await
    }

    /// THE ENGINE: run the pointer forward — posting every text step
    /// (pointer BEFORE the carrier write), auto-firing forward steps,
    /// stopping at the next input step, the script's end, an agent
    /// join, or a carrier refusal (parked loudly). Called on the
    /// visitor's first interaction and after every answer; the open
    /// verb never calls it (the lazy-welcome law).
    pub async fn advance(
        &self,
        session_id: Uuid,
        actor: Option<Uuid>,
    ) -> Result<EngineOutcome, LivechatError> {
        loop {
            let session = self
                .sessions
                .find(session_id)
                .await?
                .ok_or(LivechatError::SessionNotFound)?;
            // The factory runs while the pointer is set AND no agent
            // has joined; an agent join halts the engine.
            if agent_joined(&self.members, session_id).await? {
                return Ok(EngineOutcome::Done);
            }
            let Some(pointer) = session.chatbot_current_step_id else {
                // Script exhausted and no agent joined: the bot-only
                // completion closes the session (audited at close).
                self.sessions
                    .close(session_id, "bot_completed", actor)
                    .await?;
                return Ok(EngineOutcome::Done);
            };
            let step = self
                .chatbot
                .step(pointer)
                .await?
                .ok_or(LivechatError::StepNotCurrent)?;

            match step.step_type.as_str() {
                // A display step: advance the pointer FIRST, then the
                // carrier write (consistent pointer state for every
                // observer), then the chokepoint append.
                "text" => {
                    let next = self.chatbot.fetch_next_step(step.id, &[]).await?;
                    self.chatbot
                        .set_pointer(session_id, next.map(|s| s.id))
                        .await?;
                    self.chatbot
                        .record_step_reached(session_id, step.id, actor)
                        .await?;
                    let body = step.message.clone().unwrap_or_default();
                    match self
                        .carrier
                        .post(session_id, &MessageAuthor::Bot, &body)
                        .await
                    {
                        Ok(carrier_id) => {
                            self.sessions
                                .apply_message(
                                    &crate::infrastructure::persistence::PostMessageCommand {
                                        session_id,
                                        author: MessageAuthor::Bot,
                                        body,
                                        carrier_message_id: Some(carrier_id),
                                        chatbot: Some(
                                            crate::infrastructure::persistence::ChatbotMessageAppend {
                                                step_id: Some(step.id),
                                                selected_answer_id: None,
                                                visitor_answer: None,
                                            },
                                        ),
                                    },
                                )
                                .await?;
                        }
                        Err(e) => {
                            // Park loudly: the run stops here; the next
                            // interaction retries.
                            self.sessions
                                .park_carrier(
                                    session_id,
                                    &format!("chatbot step carrier refused: {}", e.code()),
                                    actor,
                                )
                                .await?;
                            return Ok(EngineOutcome::Parked);
                        }
                    }
                }
                // The handoff: the ladder with stickiness off and the
                // current operator counting as nobody; the step's tag
                // labels freeze onto the session; the script
                // continues after the forward step either way.
                "forward_operator" => {
                    let labels = self
                        .chatbot
                        .expertise_labels(&step.expertise_tag_ids)
                        .await?;
                    // The title law's visitor label: the ledger's
                    // visitor key, truncated — display text only.
                    let visitor_label = self
                        .members
                        .list_for_session(session_id)
                        .await?
                        .iter()
                        .find(|r| r.persona == "visitor")
                        .and_then(|r| r.visitor_key.clone())
                        .map(|k| k.chars().take(12).collect::<String>());
                    let outcome = self
                        .selection
                        .assign_forward(&crate::infrastructure::persistence::ForwardAssignInput {
                            session_id,
                            channel_id: session.channel_id,
                            current_operator: session.operator_user_id,
                            visitor_label: visitor_label,
                            visitor_language: session.visitor_language.clone(),
                            expertise: labels.clone(),
                            visitor_country: session.visitor_country_code.clone(),
                            stamp_expertise: labels,
                            actor,
                        })
                        .await?;
                    if matches!(
                        outcome,
                        crate::infrastructure::persistence::AssignOutcome::Empty
                    ) {
                        self.sessions.mark_no_agent(session_id).await?;
                    }
                    let next = self.chatbot.fetch_next_step(step.id, &[]).await?;
                    self.chatbot
                        .set_pointer(session_id, next.map(|s| s.id))
                        .await?;
                    self.chatbot
                        .record_forwarded(session_id, step.id, actor)
                        .await?;
                }
                // An input step: wait for the visitor.
                _ => return Ok(EngineOutcome::Waiting { step }),
            }
        }
    }

    /// The visitor-interaction hook (a visitor MESSAGE also
    /// materializes the pending run — the first interaction is a
    /// message or an answer). Errors park/propagate per the engine;
    /// the message itself has already landed through the chokepoint,
    /// so a park here never fails the visitor's post.
    pub async fn on_visitor_interaction(
        &self,
        session_id: Uuid,
        actor: Option<Uuid>,
    ) -> Result<EngineOutcome, LivechatError> {
        let session = self
            .sessions
            .find(session_id)
            .await?
            .ok_or(LivechatError::SessionNotFound)?;
        if session.chatbot_current_step_id.is_none() {
            return Ok(EngineOutcome::Done);
        }
        self.advance(session_id, actor).await
    }

    /// The session's execution log (the public projection + the
    /// transcript source).
    pub async fn messages_for_session(
        &self,
        session_id: Uuid,
    ) -> Result<Vec<crate::infrastructure::persistence::ChatbotMessageRow>, LivechatError> {
        self.chatbot.messages_for_session(session_id).await
    }
}

/// Whether an agent row is bound to the session (the factory's and
/// the engine's join check).
async fn agent_joined(
    members: &MemberHistoryLedgerRepository,
    session_id: Uuid,
) -> Result<bool, LivechatError> {
    let rows = members.list_for_session(session_id).await?;
    Ok(rows.iter().any(|r| r.persona == "agent"))
}

/// Normalize an email input: trim, lowercase, exactly one `@`,
/// non-empty local and domain halves, no angle brackets. `None` =
/// the typed 422 input refusal.
pub fn normalize_email(input: &str) -> Option<String> {
    let t = input.trim().to_ascii_lowercase();
    if t.contains('<') || t.contains('>') || t.len() > 254 {
        return None;
    }
    let (local, domain) = t.split_once('@')?;
    if local.is_empty() || domain.is_empty() || domain.contains('@') || !domain.contains('.') {
        return None;
    }
    Some(t)
}

/// Normalize a phone input: keep digits, a leading `+`, spaces, and
/// dashes; 7..=20 significant characters with at least 7 digits.
pub fn normalize_phone(input: &str) -> Option<String> {
    let t = input.trim();
    if t.is_empty() || t.len() > 30 {
        return None;
    }
    let mut out = String::with_capacity(t.len());
    for (i, ch) in t.chars().enumerate() {
        match ch {
            '0'..='9' | ' ' | '-' => out.push(ch),
            '+' if i == 0 => out.push(ch),
            _ => return None,
        }
    }
    let digits = out.chars().filter(|c| c.is_ascii_digit()).count();
    if (7..=20).contains(&out.chars().count()) && digits >= 7 {
        Some(out)
    } else {
        None
    }
}

/// Sanitize free text: strip HTML tag spans and stray angle brackets
/// (raw HTML is never stored), strip control characters (newline
/// kept only for multi-line inputs), trim.
pub fn sanitize_text(input: &str, multi_line: bool) -> String {
    let mut stripped = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                stripped.push(' ');
            }
            _ if in_tag => {}
            '\n' if multi_line => stripped.push('\n'),
            c if (c as u32) < 0x20 => {}
            c => stripped.push(c),
        }
    }
    // A stripped tag leaves its surrounding whitespace behind; collapse
    // the runs that creates so stored text reads as the visitor typed
    // it, never as the skeleton of the markup that was refused.
    let mut collapsed = String::with_capacity(stripped.len());
    let mut pending_space = false;
    for ch in stripped.chars() {
        match ch {
            ' ' | '\t' => pending_space = true,
            '\n' => {
                pending_space = false;
                collapsed.push('\n');
            }
            c => {
                if pending_space && !collapsed.is_empty() {
                    collapsed.push(' ');
                }
                pending_space = false;
                collapsed.push(c);
            }
        }
    }
    collapsed.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_normalization_accepts_and_refuses() {
        assert_eq!(
            normalize_email("  User@Example.COM "),
            Some("user@example.com".to_string())
        );
        assert_eq!(normalize_email("no-at-sign"), None);
        assert_eq!(normalize_email("a@b"), None, "domain needs a dot");
        assert_eq!(normalize_email("a@@b.c"), None);
        assert_eq!(normalize_email("<script>@x.y"), None);
    }

    #[test]
    fn phone_normalization_accepts_and_refuses() {
        assert_eq!(
            normalize_phone(
                "+1 (555) 010-2030"
                    .replace('(', "")
                    .replace(')', "")
                    .as_str()
            ),
            Some("+1 555 010-2030".to_string())
        );
        assert_eq!(
            normalize_phone("+15550102030"),
            Some("+15550102030".to_string())
        );
        assert_eq!(normalize_phone("12345"), None, "too few digits");
        assert_eq!(normalize_phone("abc-defg-hijk"), None);
    }

    #[test]
    fn sanitizer_strips_html_never_stores_it() {
        assert_eq!(
            sanitize_text("<b>hi</b> there <script>alert(1)</script>", false),
            "hi there alert(1)"
        );
        assert_eq!(sanitize_text("line1\nline2", false), "line1line2");
        assert_eq!(sanitize_text("line1\nline2", true), "line1\nline2");
        assert_eq!(sanitize_text("  spaced  ", false), "spaced");
    }
}
