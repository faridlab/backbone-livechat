//! The website bridge port (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! Visitor identity is the WEBSITE visitor, not a second livechat
//! digest family: the host composes this trait over backbone-website's
//! `PgWebsiteSurface` / `VisitorEngine` (the adapter installs in the
//! host's seams and nowhere else — the crate graph stays uncoupled;
//! no sibling Cargo edge). The refusing default parks loudly so an
//! uncomposed host is a typed 503, never a silent skip.
//!
//! Row scoping is owned by the composing service's tenancy decorator
//! — the fence is the fence on every surface, public included
//! (ADR-0029). The binding's company field mirrors the website
//! module's global ownership column (which is not stripped); nothing
//! in this module scopes a statement on it.

use async_trait::async_trait;
use uuid::Uuid;

use super::livechat_error::LivechatError;

/// A resolved website: its id, plus the website's owning company —
/// the documented legacy twin mirroring the website module's global
/// ownership column (that column is NOT stripped; ADR-0029).
/// Informational for consumers; the composing service's tenancy
/// decorator owns row scoping.
#[derive(Debug, Clone, Copy)]
pub struct WebsiteBinding {
    pub website_id: Uuid,
    pub company_id: Uuid,
}

/// The request-side facts a visit is tracked under.
#[derive(Debug, Clone)]
pub struct VisitFacts {
    pub website_id: Uuid,
    pub ip: String,
    pub user_agent: Option<String>,
    pub url: Option<String>,
}

/// The website visitor identity livechat binds: the website visitor
/// row id (stored as `sessions.website_visitor_id`), the visitor
/// digest (stored as the ledger's `visitor_key` — livechat never
/// mints its own visitor identity), and the visitor's geo snapshot
/// (frozen onto the session's bridge columns).
#[derive(Debug, Clone)]
pub struct VisitorIdentity {
    pub visitor_id: Uuid,
    pub visitor_key: String,
    pub country_code: Option<String>,
    pub timezone: Option<String>,
}

/// The website surface livechat needs (four methods + the visitor
/// mint). The host adapter implements it over backbone-website.
#[async_trait]
pub trait LivechatWebsiteBridge: Send + Sync {
    /// Resolve a request Host header to the bound website. A miss is
    /// the typed `livechat_channel_not_found` 404 — no fallback site.
    async fn resolve_website_by_host(&self, host: &str) -> Result<WebsiteBinding, LivechatError>;

    /// Mint-or-return the website visitor for these facts (the
    /// bridge mints; livechat only binds the returned identity).
    async fn ensure_visitor(&self, facts: &VisitFacts) -> Result<VisitorIdentity, LivechatError>;

    /// The declared visit seam — every visitor/bot message piggybacks
    /// a track (chat activity IS the visitor heartbeat).
    async fn track_visit(&self, facts: &VisitFacts, visitor_id: Uuid) -> Result<(), LivechatError>;

    /// The website visitor digest for these facts, if the bridge
    /// knows one (None = first visit).
    async fn visitor_key(&self, facts: &VisitFacts) -> Result<Option<String>, LivechatError>;

    /// Read a known website visitor's identity by row id (the
    /// operator-initiated invite freezes the VISITOR's own geo onto
    /// the pending session — per-visitor binding, not the acting
    /// operator's locale; also the test verb's harvest fallback).
    /// A miss is `None` (the invite opens without geo rather than
    /// refusing).
    async fn visitor_by_id(
        &self,
        website_id: Uuid,
        visitor_id: Uuid,
    ) -> Result<Option<VisitorIdentity>, LivechatError>;
}

/// The refusing default: the bridge is not composed. Blocking for
/// the open and availability verbs (typed 503, rows are never
/// minted); probes stub the trait instead.
pub struct RefusingLivechatWebsiteBridge;

#[async_trait]
impl LivechatWebsiteBridge for RefusingLivechatWebsiteBridge {
    async fn resolve_website_by_host(&self, _host: &str) -> Result<WebsiteBinding, LivechatError> {
        Err(LivechatError::WebsiteBridgeNotComposed)
    }

    async fn ensure_visitor(&self, _facts: &VisitFacts) -> Result<VisitorIdentity, LivechatError> {
        Err(LivechatError::WebsiteBridgeNotComposed)
    }

    async fn track_visit(
        &self,
        _facts: &VisitFacts,
        _visitor_id: Uuid,
    ) -> Result<(), LivechatError> {
        Err(LivechatError::WebsiteBridgeNotComposed)
    }

    async fn visitor_key(&self, _facts: &VisitFacts) -> Result<Option<String>, LivechatError> {
        Err(LivechatError::WebsiteBridgeNotComposed)
    }

    async fn visitor_by_id(
        &self,
        _website_id: Uuid,
        _visitor_id: Uuid,
    ) -> Result<Option<VisitorIdentity>, LivechatError> {
        Err(LivechatError::WebsiteBridgeNotComposed)
    }
}
