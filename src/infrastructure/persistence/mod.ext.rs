// This directory's extension (hand-written; ADR-0031). The generated mod.rs (lib.rs for
// src/) pulls it in with include!, so its lines resolve against this directory.

// The shared-audit-trail funnel. Inside the markers so a regeneration
// cannot drop the declaration and leave the file orphaned.
pub mod audit;
// Hand-written repositories carrying the verbs' transactional SQL
// (user-owned; see metaphor.codegen.yaml). The generated CRUD
// repositories keep their own files — these are the verb layer's.
pub mod admin_config_repository;
pub mod chatbot_command_repository;
pub mod crm_bridge_repository;
pub mod report_repository;
pub mod selection_repository;
pub mod session_command_repository;
pub mod sweep_repository;
pub mod website_request_repository;
// Re-bind the caller's ambient org scope onto a transaction opened on the plain pool — the
// scope is task-local and a fresh pool transaction carries none of it. With no ambient scope
// (standalone deployment, jobs) the transaction stays plain: the module is tenant-agnostic and
// the composing service's tenancy decorator owns isolation (ADR-0029). Same convention as the
// backbone-pos write service's helper.
pub(crate) async fn relay_ambient_scope(conn: &mut sqlx::PgConnection) -> Result<(), sqlx::Error> {
    if let Some(scope) = backbone_orm::org_scope::current_org_scope() {
        backbone_orm::org_scope::bind_org_scope_on(conn, &scope).await?;
    }
    Ok(())
}
pub use admin_config_repository::{
    AdminConfigRepository, AnswerInput, ChannelPatch, RuleInput, StepInput, STEP_TYPES,
};
pub use chatbot_command_repository::{
    AnswerRow, ChatbotCommandRepository, ChatbotMessageRow, StepRow,
};
pub use crm_bridge_repository::{CrmBridgeRepository, HarvestedContact};
pub use member_history_repository::{MemberHistoryLedgerRepository, MemberHistoryRow};
pub use rating_repository::{RatingCommandRepository, RatingRow};
pub use report_repository::{OutcomeMixRow, ReportRepository, SeriesBucketRow, SessionSummaryRow};
pub use selection_repository::{
    ongoing_window_expr, record_audit, AssignInput, AssignOutcome, ForwardAssignInput, LadderPick,
    SelectionRepository, ASSIGNMENT_BUFFER_SECS, ONGOING_WINDOW_SECS, PRESENCE_WINDOW_SECS,
};
pub use session_command_repository::{
    apply_message_tx, upsert_agent_ledger_tx, ChatbotMessageAppend, CloseOutcome, MessageDeltas,
    OpenSessionInput, PostMessageCommand, SessionCommandRepository, SessionListFilter, SessionRow,
};
pub use sweep_repository::{SweepOutcome, SweepRepository, SWEEP_BATCH};
pub use website_request_repository::{ChannelSummary, RuleMatch, WebsiteRequestRepository};
