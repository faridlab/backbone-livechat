//! The extension of `services.rs` (hand-written; ADR-0031).

#[allow(unused_imports)]
use super::*;

// The module's public cross-module surface: the verb services and
// ports a host or sibling module composes. Writes go through these
// typed verbs — never through the generic CRUD services.
pub use crate::application::service::{
    AvailabilityService, ChatbotService, CrmBridgeService, LeadMintInput, LivechatError,
    LivechatResult, RatingSubmitService, ReportService, SelectionService, SessionCommandService,
    SweepService, WebsiteRequestService, REPORT_WINDOW_MAX_DAYS,
};
pub use crate::application::service::{
    CarrierMessage, DigestEntry, LeadFromSession, LeadMinted, LivechatCrmLeadPort,
    LivechatDigestQueue, LivechatMailCarrier, LivechatNotice, LivechatNotifier, LivechatRtcCarrier,
    LivechatTranscriptMailer, LivechatWebsiteBridge, MessageAuthor, RefusingCrmLeadPort,
    RefusingDigestQueue, RefusingLivechatWebsiteBridge, RefusingMailCarrier, RefusingRtcCarrier,
    RefusingTranscriptMailer, TranscriptRequest, UnwiredNotifier, VisitFacts, VisitorIdentity,
    WebsiteBinding,
};
pub use crate::infrastructure::persistence::{
    AssignOutcome, OpenSessionInput, SessionListFilter, SessionRow,
};
pub use crate::presentation::http::{
    livechat_admin_routes, livechat_public_routes, LivechatActor, LivechatAdminState,
    LivechatPublicState,
};
