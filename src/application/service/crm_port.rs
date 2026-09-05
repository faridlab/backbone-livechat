//! The CRM lead port (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! The donor's crm-livechat bridge lets a conversation become a CRM
//! lead: the operator mints a lead from the session, and the lead's
//! existence then makes the conversation readable to the lead's owner.
//! Backbone keeps that seam without a sibling Cargo edge: this module
//! owns the SESSION side of the bridge (the link column, the mint
//! verb, and the lead-linked read verbs), and the LEAD side is a
//! host-composed adapter over this trait (the same law as the website
//! bridge and the mail carrier — the adapter installs in the host's
//! seams and nowhere else).
//!
//! The refusing default parks loudly: an uncomposed host gets the
//! typed 503 at the mint verb and NOTHING is written — no session
//! link, no lead. Probes stub the trait instead.

use async_trait::async_trait;
use uuid::Uuid;

use super::livechat_error::LivechatError;

/// The mint request: what the session knows about the person behind
/// the conversation. Every field is stamped SERVER-SIDE by the mint
/// verb (the session row, the chatbot's sanitized answers, the acting
/// operator) — none of it arrives from the client.
#[derive(Debug, Clone)]
pub struct LeadFromSession {
    pub company_id: Uuid,
    pub session_id: Uuid,
    /// The lead's display name (the verb's own default when the
    /// caller gives none: the session title, else a stable
    /// session-derived label).
    pub lead_name: String,
    /// The email the chatbot collected (the earliest answered
    /// `question_email` step), when it exists.
    pub contact_email: Option<String>,
    /// The phone the chatbot collected (the earliest answered
    /// `question_phone` step), when it exists.
    pub contact_phone: Option<String>,
    /// Free-text context for the lead (the caller's note; the donor
    /// put the whole channel history on the lead — the transcript
    /// here stays behind the read verbs, so only a bounded note
    /// crosses).
    pub note: Option<String>,
    /// The operator who minted the lead (the donor's `referred`).
    pub operator_user_id: Option<Uuid>,
    /// The website visitor behind the session, its country and
    /// timezone — the bridge context the lead side may bind at birth
    /// (visitor-spine attribution) instead of reconstructing later.
    pub website_visitor_id: Option<Uuid>,
    pub visitor_country_code: Option<String>,
    pub visitor_timezone: Option<String>,
}

/// The minted lead's id (stamped onto the session link).
#[derive(Debug, Clone, Copy)]
pub struct LeadMinted {
    pub lead_id: Uuid,
}

/// The CRM seam — a composing service implements it over the lead
/// module's capture verb.
#[async_trait]
pub trait LivechatCrmLeadPort: Send + Sync {
    /// Mint a lead from the session's facts. A refusal is the typed
    /// error carried back to the caller (the link is NOT stamped);
    /// the adapter owns mapping the lead module's refusals onto
    /// [`LivechatError`].
    async fn mint_lead(&self, req: &LeadFromSession) -> Result<LeadMinted, LivechatError>;
}

/// The refusing default: the CRM bridge is not composed. Blocking at
/// the mint verb (typed 503, nothing written); probes stub the trait
/// instead.
pub struct RefusingCrmLeadPort;

#[async_trait]
impl LivechatCrmLeadPort for RefusingCrmLeadPort {
    async fn mint_lead(&self, _req: &LeadFromSession) -> Result<LeadMinted, LivechatError> {
        Err(LivechatError::CrmBridgeNotComposed)
    }
}
