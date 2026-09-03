//! The selection service (hand-written; user-owned; see
//! `metaphor.codegen.yaml`): the deterministic operator-selection
//! ladder's application surface.
//!
//! THE LADDER LAWS live in the repository
//! ([`crate::infrastructure::persistence::selection_repository`]) —
//! one statement, one window, the buffer inside the pool, the total
//! order, no die roll. This service derives the stickiness input
//! (the visitor's previous operator on the channel) and exposes the
//! two call sites: the assignment write and the availability count.
//! Both build on the SAME pool — the ONE-window law holds by
//! construction, not by discipline.

use uuid::Uuid;

use super::livechat_error::LivechatError;
use crate::infrastructure::persistence::{
    AssignInput, AssignOutcome, MemberHistoryLedgerRepository, SelectionRepository,
};

pub struct SelectionService {
    selection: SelectionRepository,
    members: MemberHistoryLedgerRepository,
}

impl SelectionService {
    pub fn new(selection: SelectionRepository, members: MemberHistoryLedgerRepository) -> Self {
        Self { selection, members }
    }

    /// The assignment write with the stickiness arm derived: the
    /// visitor's previous operator on this channel becomes rung 0 —
    /// INSIDE the pool, behind the buffer and the capacity gate.
    #[allow(clippy::too_many_arguments)]
    pub async fn assign_session(
        &self,
        session_id: Uuid,
        channel_id: Uuid,
        visitor_key: Option<&str>,
        visitor_language: Option<&str>,
        expertise: &[String],
        visitor_country: Option<&str>,
        actor: Option<Uuid>,
    ) -> Result<AssignOutcome, LivechatError> {
        let previous = match visitor_key {
            Some(key) => {
                self.members
                    .previous_operator_for_visitor(channel_id, key)
                    .await?
            }
            None => None,
        };
        self.selection
            .assign(&AssignInput {
                session_id,
                channel_id,
                previous_operator: previous,
                visitor_language: visitor_language.map(str::to_string),
                expertise: expertise.to_vec(),
                visitor_country: visitor_country.map(str::to_string),
                actor,
            })
            .await
    }

    /// The raw assignment write (the caller owns every input).
    pub async fn assign(&self, input: &AssignInput) -> Result<AssignOutcome, LivechatError> {
        self.selection.assign(input).await
    }

    /// The availability arm's operator count — the SAME pool as the
    /// assignment (the ONE window, the buffer, the capacity gate),
    /// read-only.
    pub async fn eligible_operator_count(&self, channel_id: Uuid) -> Result<i64, LivechatError> {
        self.selection.eligible_operator_count(channel_id).await
    }
}
