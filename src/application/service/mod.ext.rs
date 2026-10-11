// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// Hand-written verb services + ports (user-owned; see
// `metaphor.codegen.yaml`). Each generated file listed above that is
// ALSO user-owned carries the generated CRUD alias inside it, so the
// generated wiring keeps compiling.
pub mod availability_service;
pub mod capability;
pub mod chatbot_service;
pub mod crm_bridge_service;
pub mod crm_port;
pub mod digest_port;
pub mod livechat_error;
pub mod mail_port;
pub mod notifier_port;
pub mod report_service;
pub mod rtc_port;
pub mod selection_service;
pub mod sweep_service;
pub mod throttle;
pub mod transcript_port;
pub mod website_bridge;
pub mod website_request_service;
pub use availability_service::AvailabilityService;
pub use capability::{
    capability_secret_from_env, mint_guest_capability, mint_invite_capability, CapabilityClaims,
    LIVECHAT_CAPABILITY_SECRET_ENV, LIVECHAT_GUEST_TOKEN_TTL_SECS, LIVECHAT_INVITE_TTL_SECS,
    PURPOSE_GUEST_SESSION, PURPOSE_INVITE_ACCEPT,
};
pub use chatbot_service::{ChatbotService, StepPreview};
pub use crm_bridge_service::{CrmBridgeService, LeadMintInput};
pub use crm_port::{LeadFromSession, LeadMinted, LivechatCrmLeadPort, RefusingCrmLeadPort};
pub use digest_port::{DigestEntry, LivechatDigestQueue, RefusingDigestQueue};
pub use livechat_error::{LivechatError, LivechatResult};
pub use mail_port::{CarrierMessage, LivechatMailCarrier, MessageAuthor, RefusingMailCarrier};
pub use notifier_port::{LivechatNotice, LivechatNotifier, UnwiredNotifier};
pub use rating_service::RatingSubmitService;
pub use report_service::{ReportService, SessionSummaryReport, REPORT_WINDOW_MAX_DAYS};
pub use rtc_port::{LivechatRtcCarrier, RefusingRtcCarrier};
pub use selection_service::SelectionService;
pub use session_service::SessionCommandService;
pub use sweep_service::SweepService;
pub use throttle::{caller_ip, trusted_proxy_from_env, FixedWindows, LivechatRatePolicy};
pub use transcript_port::{LivechatTranscriptMailer, RefusingTranscriptMailer, TranscriptRequest};
pub use website_bridge::{
    LivechatWebsiteBridge, RefusingLivechatWebsiteBridge, VisitFacts, VisitorIdentity,
    WebsiteBinding,
};
pub use website_request_service::WebsiteRequestService;
