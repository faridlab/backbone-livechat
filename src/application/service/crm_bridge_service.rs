//! The CRM bridge service (hand-written; user-owned; see
//! `metaphor.codegen.yaml`): the conversation-becomes-a-lead seam —
//! the mint verb, the lead-linked read, and the lead-granted agent
//! join.
//!
//! DONOR TRANSLATION (the crm-livechat bridge): the donor had (a) two
//! lead factories sharing `origin_channel_id`, (b) an
//! anti-fabrication guard on the LEAD's create/write (the actor must
//! be able to read every channel it links), and (c) a PAIR of
//! read-grant rules making the channel readable to lead owners once a
//! lead exists — the guard and the grants are one load-bearing unit
//! (the guard is only satisfiable because a lead's existence makes
//! the channel readable). Here:
//!
//! - (a) collapses to ONE server-side verb — the chatbot step-type
//!   factory arm is refused by this module's frozen seven-type
//!   closure, so both factories' outcomes funnel through
//!   [`Self::mint_lead_for_session`] (operator- or host-driven);
//! - (b) holds structurally: the link column has exactly ONE writer
//!   (the repository's conditional UPDATE inside this verb), the
//!   session arrives by path under the host's company gate, and the
//!   lead id comes from the port's mint — NO client surface supplies
//!   either id, so the fabrication vector the donor's ORM guard
//!   fenced cannot be expressed;
//! - (c) ports as the two lead-linked verbs: the read
//!   ([`Self::session_for_lead`]) and the join
//!   ([`Self::join_session_for_lead`]) — a lead owner reads the
//!   conversation behind their lead and may join it as an agent
//!   participant, without being a channel operator.
//!
//! A mint-then-link race note: the mint runs through the external
//! port BEFORE the conditional link stamp, so a race lost at the
//! stamp leaves the freshly minted lead standing in CRM unlinked —
//! visible, mergeable, never silently dropped (the loser gets the
//! typed 409 and an audit row; the same check-then-act window the
//! rating once-wall accepts).

use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;

use super::crm_port::{LeadFromSession, LivechatCrmLeadPort};
use super::livechat_error::LivechatError;
use crate::infrastructure::persistence::{
    CrmBridgeRepository, SessionCommandRepository, SessionRow,
};

/// The mint verb's caller-supplied facts. Everything else is stamped
/// server-side from the session row, the chatbot's sanitized answers,
/// or the acting operator — the client never names the lead id, the
/// session link, or the tenant.
#[derive(Debug, Clone, Default)]
pub struct LeadMintInput {
    /// The lead's display name; `None` = the session title, else a
    /// stable session-derived label.
    pub lead_name: Option<String>,
    /// A bounded free-text note for the lead.
    pub note: Option<String>,
    /// An explicit email; `None` = harvest the chatbot's earliest
    /// answered email step.
    pub email: Option<String>,
    /// An explicit phone; `None` = harvest the chatbot's earliest
    /// answered phone step.
    pub phone: Option<String>,
}

/// The lead module's name column cap — the verb truncates to it so a
/// long session title can never fail the mint late.
const LEAD_NAME_MAX_CHARS: usize = 140;

pub struct CrmBridgeService {
    sessions: SessionCommandRepository,
    bridge: CrmBridgeRepository,
    crm: Arc<dyn LivechatCrmLeadPort>,
}

impl CrmBridgeService {
    /// Compose with the host-installed CRM port (the refusing default
    /// parks the mint verb loudly; reads and joins need no port).
    pub fn new(pool: PgPool, crm: Arc<dyn LivechatCrmLeadPort>) -> Self {
        Self {
            sessions: SessionCommandRepository::new(pool.clone()),
            bridge: CrmBridgeRepository::new(pool),
            crm,
        }
    }

    /// Mint a lead from a session and stamp the link (the operator's
    /// conversation-becomes-a-lead verb). Refusals: the uniform 404
    /// family for a missing or out-of-scope session (the composing
    /// service's tenancy decorator owns scoping; ADR-0029); the typed
    /// 409 when the session already carries its one lead; the typed
    /// 503 when the CRM port is uncomposed (nothing is written on any
    /// refusal).
    pub async fn mint_lead_for_session(
        &self,
        session_id: Uuid,
        input: &LeadMintInput,
        actor: Option<Uuid>,
    ) -> Result<(SessionRow, Uuid), LivechatError> {
        let session = self
            .sessions
            .find(session_id)
            .await?
            .ok_or(LivechatError::SessionNotFound)?;
        if session.crm_lead_id.is_some() {
            return Err(LivechatError::SessionAlreadyHasLead);
        }
        let harvested = if input.email.is_none() || input.phone.is_none() {
            self.bridge.harvest_contact(session_id).await?
        } else {
            Default::default()
        };
        let lead_name = input
            .lead_name
            .clone()
            .or_else(|| session.title.clone())
            .unwrap_or_else(|| format!("Livechat session {}", session.id.simple()));
        let request = LeadFromSession {
            company_id: legacy_twin(),
            session_id,
            lead_name: lead_name.chars().take(LEAD_NAME_MAX_CHARS).collect(),
            contact_email: input.email.clone().or(harvested.email),
            contact_phone: input.phone.clone().or(harvested.phone),
            note: input.note.clone(),
            operator_user_id: actor,
            website_visitor_id: session.website_visitor_id,
            visitor_country_code: session.visitor_country_code.clone(),
            visitor_timezone: session.visitor_timezone.clone(),
        };
        // The port is BLOCKING: an uncomposed bridge answers the typed
        // 503 here and NOTHING is written (no link, no audit of a
        // link).
        let minted = self.crm.mint_lead(&request).await?;
        let row = self
            .bridge
            .link_lead(session_id, minted.lead_id, actor)
            .await?;
        Ok((row, minted.lead_id))
    }

    /// The lead-linked read (the first read-grant rule): the session a
    /// lead was minted from, readable to gated actors within the
    /// caller's scope (the composing service's tenancy decorator owns
    /// scoping; ADR-0029) — the partial index serves exactly this
    /// domain. `None` = no session carries this lead (the uniform
    /// missing family).
    pub async fn session_for_lead(
        &self,
        lead_id: Uuid,
    ) -> Result<Option<SessionRow>, LivechatError> {
        self.bridge.find_by_lead_id(lead_id).await
    }

    /// The lead-granted join (the second read-grant rule): the actor
    /// becomes an agent participant of the conversation behind the
    /// lead — no channel membership, no ownership change, the ladder
    /// untouched. Refuses closed conversations typed (the read side
    /// serves their history).
    pub async fn join_session_for_lead(
        &self,
        lead_id: Uuid,
        user_id: Uuid,
        actor: Option<Uuid>,
    ) -> Result<SessionRow, LivechatError> {
        self.bridge
            .join_agent_for_lead(lead_id, user_id, actor)
            .await
    }
}

/// The legacy tenancy twin the CRM port's request payload still
/// carries (ADR-0029): under the composing service's org request scope
/// it is the scope's legacy echo; nil when undecorated. The port field
/// stays for still-fenced consumers; nothing in this module keys a
/// statement on it.
fn legacy_twin() -> Uuid {
    backbone_orm::org_scope::current_org_scope()
        .and_then(|s| s.legacy_company_id())
        .unwrap_or(Uuid::nil())
}
