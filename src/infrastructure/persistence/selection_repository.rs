//! The selection repository (hand-written; user-owned; see
//! `metaphor.codegen.yaml`): the deterministic operator-selection
//! ladder as ONE set-based statement, plus the serialized,
//! first-wins, audited assignment write.
//!
//! THE THREE LAWS (each structurally enforced here, not by policy):
//!
//! 1. ONE WINDOW. [`ONGOING_WINDOW_SECS`] (1800s = 30 minutes) is
//!    the ONLY definition of an "ongoing" session, and every SQL
//!    that needs the notion embeds [`ongoing_window_expr`] — the one
//!    shared fragment (the assignment pool and the availability
//!    count build on the SAME pool CTE). There is no second window
//!    constant in the crate; the upstream 15-months-ORM vs
//!    30-minutes-SQL divergence cannot reappear by construction.
//!
//! 2. THE BUFFER IS INSIDE THE POOL. [`ASSIGNMENT_BUFFER_SECS`]
//!    (120s) gates the candidate pool itself, so the stickiness arm
//!    (rung 0) and the no-rung fallback (rung 9) pass through it
//!    structurally — there is no arm that reads around the pool, so
//!    the upstream silent bypasses (previous operator, unmatched
//!    visitor) cannot reappear.
//!
//! 3. NO DIE ROLL. Every tie breaks by the total order
//!    `rung ASC, ongoing ASC, last_assigned_at ASC NULLS FIRST,
//!    user_id ASC` — upstream's `random.choice` is replaced by a
//!    faithful replay: the audit row records the rung, the candidate
//!    count, and that the buffer and the one window were applied.
//!
//! No GC of any kind runs in any read here (the upstream in-read RTC
//! GC does not port); staleness belongs to the scheduled sweeps.

use sqlx::PgPool;
use uuid::Uuid;

// The typed multi-row read twins live only in the legacy `company_scope` module. Their
// connection discipline is what this repository needs — request-dedicated connection when
// the composing service bound one, plain pool otherwise. The helper's legacy task-local
// branch is never taken: this module sets no legacy scope of its own (ADR-0029).
use backbone_orm::org_scope;

use crate::application::service::livechat_error::LivechatError;
use super::relay_ambient_scope;

/// THE ONE WINDOW: an ongoing session is `closed_at IS NULL AND
/// last_interest_at >= now() - 1800s`. 30 minutes.
pub const ONGOING_WINDOW_SECS: i64 = 1800;

/// The anti-burst buffer: a candidate assigned within the last 120s
/// is not eligible — inside the pool, on every path.
pub const ASSIGNMENT_BUFFER_SECS: i64 = 120;

/// Presence: an operator is online iff their heartbeat is within the
/// last 60s.
pub const PRESENCE_WINDOW_SECS: i64 = 60;

/// The ONE shared "ongoing" predicate fragment (`{p}` is the bind
/// placeholder the embedding statement assigns the window secs).
/// Every statement that needs the notion embeds THIS fragment — the
/// single definition the probes resolve both call sites through.
pub fn ongoing_window_expr(p: &str) -> String {
    format!(
        "s.closed_at IS NULL AND s.last_interest_at >= now() - make_interval(secs => {p}::bigint)"
    )
}

/// The candidate pool, as one CTE body: channel membership + a live
/// heartbeat + the 120s buffer, capacity-gated against the channel's
/// own mode/max using the ONE window. Both the ladder and the
/// availability count build on THIS text verbatim (parameter
/// numbering: $1 channel, $2 window secs, $3 presence secs, $4
/// buffer secs).
const POOL_CTE: &str = r#"cfg AS (
  SELECT c.max_sessions_mode::text AS mode, c.max_sessions
  FROM livechat.channels c
  WHERE c.id = $1
),
heartbeats AS (
  SELECT m.user_id, p.id AS profile_id,
         p.languages, p.last_assigned_at
  FROM livechat.channel_members m
  JOIN livechat.operator_profiles p
    ON p.user_id = m.user_id
  WHERE m.channel_id = $1
    AND p.last_heartbeat_at >= now() - make_interval(secs => $3::bigint)
    AND (p.last_assigned_at IS NULL
         OR p.last_assigned_at < now() - make_interval(secs => $4::bigint))
),
pool AS (
  SELECT h.user_id, h.profile_id, h.languages, h.last_assigned_at,
         cnt.ongoing,
         ARRAY(SELECT t.name
                 FROM livechat.operator_expertise oe
                 JOIN livechat.expertise_tags t ON t.id = oe.expertise_tag_id
                WHERE oe.operator_profile_id = h.profile_id) AS expertise,
         -- Country rungs (4..6): operator profiles carry NO country
         -- column at this schema revision, so the country arm
         -- computes FALSE for every candidate. The rung ORDER is
         -- kept verbatim so landing operator geo arms rungs 4..6
         -- without reshuffling the ladder.
         FALSE AS country_match
  FROM heartbeats h
  CROSS JOIN LATERAL (
    SELECT count(*)::int AS ongoing
    FROM livechat.sessions s
    WHERE s.operator_user_id = h.user_id
      AND s.closed_at IS NULL
      AND s.last_interest_at >= now() - make_interval(secs => $2::bigint)
  ) cnt
  WHERE (SELECT mode FROM cfg) = 'unlimited' OR cnt.ongoing < (SELECT max_sessions FROM cfg)
)"#;

/// The ladder's top pick (or none).
#[derive(Debug, Clone)]
pub struct LadderPick {
    pub operator_user_id: Uuid,
    pub rung: i32,
    pub candidates_considered: i64,
}

/// The assignment write's outcome.
#[derive(Debug, Clone)]
pub enum AssignOutcome {
    /// The conditional UPDATE won; the row carries the operator.
    Assigned {
        operator_user_id: Uuid,
        rung: i32,
        candidates_considered: i64,
    },
    /// The pool was empty (or every candidate failed the gates): the
    /// defined `no_agent` path — the session stays unassigned and
    /// the decision is still audited.
    Empty,
}

/// The assignment input.
#[derive(Debug, Clone)]
pub struct AssignInput {
    pub session_id: Uuid,
    pub channel_id: Uuid,
    /// The operator of the visitor's previous session on this
    /// channel (the stickiness arm, rung 0 — INSIDE the pool).
    pub previous_operator: Option<Uuid>,
    pub visitor_language: Option<String>,
    /// The session's frozen expertise labels (the expertise rungs).
    pub expertise: Vec<String>,
    /// The visitor's country code (the country rungs — inert at this
    /// schema, see [`POOL_CTE`]).
    pub visitor_country: Option<String>,
    /// The acting officer for the audit row (None = the system
    /// actor, e.g. the chatbot's forward step).
    pub actor: Option<Uuid>,
}

pub struct SelectionRepository {
    pool: PgPool,
}

impl SelectionRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// The ONE-statement ladder: the pool, the rung CASE (0..9), and
    /// the total order — no per-operator probes, no N+1, no
    /// randomness. Returns the top candidate or `None` (the defined
    /// `no_agent` path).
    pub async fn pick_operator(
        &self,
        channel_id: Uuid,
        previous_operator: Option<Uuid>,
        visitor_language: Option<&str>,
        expertise: &[String],
        visitor_country: Option<&str>,
    ) -> Result<Option<LadderPick>, LivechatError> {
        let sql = format!(
            r#"WITH {POOL_CTE},
ranked AS (
  SELECT user_id, ongoing, last_assigned_at,
         CASE
           WHEN $5::uuid IS NOT NULL AND user_id = $5::uuid THEN 0
           WHEN $6::text IS NOT NULL AND $6 = ANY(languages) AND $7::text[] <@ expertise THEN 1
           WHEN $6::text IS NOT NULL AND $6 = ANY(languages) AND $7::text[] && expertise THEN 2
           WHEN $6::text IS NOT NULL AND $6 = ANY(languages)                    THEN 3
           WHEN $8::text IS NOT NULL AND country_match AND $7::text[] <@ expertise THEN 4
           WHEN $8::text IS NOT NULL AND country_match AND $7::text[] && expertise THEN 5
           WHEN $8::text IS NOT NULL AND country_match                                THEN 6
           WHEN cardinality($7::text[]) > 0 AND $7::text[] <@ expertise THEN 7
           WHEN cardinality($7::text[]) > 0 AND $7::text[] && expertise THEN 8
           ELSE 9
         END AS rung
  FROM pool
)
SELECT user_id, rung::int AS rung, (count(*) OVER ())::bigint AS candidates_considered
FROM ranked
ORDER BY rung ASC, ongoing ASC, last_assigned_at ASC NULLS FIRST, user_id ASC
LIMIT 1"#
        );
        let row = backbone_orm::company_scope::fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, LadderRow>(&sql)
                .bind(channel_id)
                .bind(ONGOING_WINDOW_SECS)
                .bind(PRESENCE_WINDOW_SECS)
                .bind(ASSIGNMENT_BUFFER_SECS)
                .bind(previous_operator)
                .bind(visitor_language)
                .bind(expertise)
                .bind(visitor_country),
        )
        .await?;
        Ok(row.map(|r| LadderPick {
            operator_user_id: r.user_id,
            rung: r.rung,
            candidates_considered: r.candidates_considered,
        }))
    }

    /// The availability arm's operator count: the SAME pool (the one
    /// window, the buffer, the capacity gate), read-only. ≥1 means a
    /// human can take the next session.
    pub async fn eligible_operator_count(&self, channel_id: Uuid) -> Result<i64, LivechatError> {
        let sql = format!("WITH {POOL_CTE} SELECT count(*)::bigint FROM pool");
        let n: i64 = backbone_orm::company_scope::fetch_one_scalar_scoped(
            &self.pool,
            sqlx::query_scalar::<_, i64>(&sql)
                .bind(channel_id)
                .bind(ONGOING_WINDOW_SECS)
                .bind(PRESENCE_WINDOW_SECS)
                .bind(ASSIGNMENT_BUFFER_SECS),
        )
        .await?;
        Ok(n)
    }

    /// The serialized, audited assignment write. ONE transaction:
    /// the ladder statement → the first-wins conditional UPDATE
    /// (`operator_user_id IS NULL AND closed_at IS NULL`; zero rows =
    /// someone else won, typed 409) →
    /// the agent ledger upsert (rejoin re-points, never duplicates) →
    /// `last_assigned_at` stamp → the per-record outcome recompute →
    /// the `operator_assigned` audit row with the replay facts.
    pub async fn assign(&self, input: &AssignInput) -> Result<AssignOutcome, LivechatError> {
        let mut tx = self.pool.begin().await?;
        relay_ambient_scope(&mut tx).await?;

        // The ladder (inside the same transaction — the pool the
        // decision saw and the row it wins are one snapshot).
        let pick_sql = format!(
            r#"WITH {POOL_CTE},
ranked AS (
  SELECT user_id, ongoing, last_assigned_at,
         CASE
           WHEN $5::uuid IS NOT NULL AND user_id = $5::uuid THEN 0
           WHEN $6::text IS NOT NULL AND $6 = ANY(languages) AND $7::text[] <@ expertise THEN 1
           WHEN $6::text IS NOT NULL AND $6 = ANY(languages) AND $7::text[] && expertise THEN 2
           WHEN $6::text IS NOT NULL AND $6 = ANY(languages)                    THEN 3
           WHEN $8::text IS NOT NULL AND country_match AND $7::text[] <@ expertise THEN 4
           WHEN $8::text IS NOT NULL AND country_match AND $7::text[] && expertise THEN 5
           WHEN $8::text IS NOT NULL AND country_match                                THEN 6
           WHEN cardinality($7::text[]) > 0 AND $7::text[] <@ expertise THEN 7
           WHEN cardinality($7::text[]) > 0 AND $7::text[] && expertise THEN 8
           ELSE 9
         END AS rung
  FROM pool
)
SELECT user_id, rung::int AS rung, (count(*) OVER ())::bigint AS candidates_considered
FROM ranked
ORDER BY rung ASC, ongoing ASC, last_assigned_at ASC NULLS FIRST, user_id ASC
LIMIT 1"#
        );
        let pick: Option<LadderRow> = sqlx::query_as::<_, LadderRow>(&pick_sql)
            .bind(input.channel_id)
            .bind(ONGOING_WINDOW_SECS)
            .bind(PRESENCE_WINDOW_SECS)
            .bind(ASSIGNMENT_BUFFER_SECS)
            .bind(input.previous_operator)
            .bind(input.visitor_language.as_deref())
            .bind(&input.expertise)
            .bind(input.visitor_country.as_deref())
            .fetch_optional(&mut *tx)
            .await?;

        let Some(pick) = pick else {
            // The defined no_agent path: audited, session untouched.
            audit_tx(
                &mut tx,
                "assignment_empty",
                input.actor,
                "session",
                input.session_id,
                serde_json::json!({
                    "channel_id": input.channel_id,
                    "window_secs": ONGOING_WINDOW_SECS,
                    "buffer_applied": true,
                }),
            )
            .await?;
            tx.commit().await?;
            return Ok(AssignOutcome::Empty);
        };

        // First-wins: the conditional UPDATE is the serialization
        // point (row atomicity; no check-then-act race).
        let won: Option<(Uuid,)> = sqlx::query_as::<_, (Uuid,)>(
            r#"UPDATE livechat.sessions
                  SET operator_user_id = $2, status = 'in_progress'
                WHERE id = $1 AND operator_user_id IS NULL AND closed_at IS NULL
                RETURNING id"#,
        )
        .bind(input.session_id)
        .bind(pick.user_id)
        .fetch_optional(&mut *tx)
        .await?;
        if won.is_none() {
            // The loser of the race: commit the refusal audit (the
            // session row itself is untouched), then answer typed.
            audit_tx(
                &mut tx,
                "operator_busy",
                input.actor,
                "session",
                input.session_id,
                serde_json::json!({ "operator_user_id": pick.user_id, "rung": pick.rung }),
            )
            .await?;
            tx.commit().await?;
            return Err(LivechatError::OperatorBusy);
        }

        // The agent ledger row (rejoin re-points: ON CONFLICT on the
        // agent partial unique clears left_at, never duplicates).
        sqlx::query(
            r#"INSERT INTO livechat.member_histories
                   (session_id, persona, operator_user_id, expertise_names)
               VALUES ($1, 'agent', $2,
                       COALESCE((SELECT ARRAY(SELECT t.name
                                  FROM livechat.operator_expertise oe
                                  JOIN livechat.expertise_tags t ON t.id = oe.expertise_tag_id
                                 WHERE oe.operator_profile_id = p.id)
                                FROM livechat.operator_profiles p
                                WHERE p.user_id = $2), '{}'))
               ON CONFLICT (session_id, operator_user_id)
               WHERE persona = 'agent' AND operator_user_id IS NOT NULL
               DO UPDATE SET left_at = NULL, joined_at = now()"#,
        )
        .bind(input.session_id)
        .bind(pick.user_id)
        .execute(&mut *tx)
        .await?;

        // The buffer's anchor.
        sqlx::query(
            "UPDATE livechat.operator_profiles SET last_assigned_at = now() \
             WHERE user_id = $1",
        )
        .bind(pick.user_id)
        .execute(&mut *tx)
        .await?;

        // The per-record outcome recompute (single-row, its own
        // inputs — never a recordset-wide assignment).
        recompute_outcome_tx(&mut tx, input.session_id).await?;

        // The replay facts: with the randomness gone, this audit row
        // is a faithful replay of the decision.
        audit_tx(
            &mut tx,
            "operator_assigned",
            input.actor,
            "session",
            input.session_id,
            serde_json::json!({
                "operator_user_id": pick.user_id,
                "rung": pick.rung,
                "candidates_considered": pick.candidates_considered,
                "previous_operator_considered": input.previous_operator,
                "buffer_applied": true,
                "buffer_secs": ASSIGNMENT_BUFFER_SECS,
                "window_secs": ONGOING_WINDOW_SECS,
            }),
        )
        .await?;

        tx.commit().await?;
        Ok(AssignOutcome::Assigned {
            operator_user_id: pick.user_id,
            rung: pick.rung,
            candidates_considered: pick.candidates_considered,
        })
    }
}

#[derive(Debug, sqlx::FromRow)]
struct LadderRow {
    user_id: Uuid,
    rung: i32,
    candidates_considered: i64,
}

/// The operator-forced / chatbot-triggered forward handoff's input
/// The ladder runs with stickiness OFF (a fresh handoff),
/// the CURRENT operator counts as nobody (a self-pick writes
/// `no_agent` and continues), and the win rebinds the session even
/// though an operator is already set.
#[derive(Debug, Clone)]
pub struct ForwardAssignInput {
    pub session_id: Uuid,
    pub channel_id: Uuid,
    /// The operator currently owning the session (a self-pick counts
    /// as nobody).
    pub current_operator: Option<Uuid>,
    /// The visitor's label for the title law
    /// (`"<visitor label> / <operator display name>"`).
    pub visitor_label: Option<String>,
    pub visitor_language: Option<String>,
    pub expertise: Vec<String>,
    pub visitor_country: Option<String>,
    /// The forward step's tag labels, frozen onto the session at
    /// forward time (deterministic reporting). Empty = operator-
    /// forced forward (no step, no stamp).
    pub stamp_expertise: Vec<String>,
    pub actor: Option<Uuid>,
}

impl SelectionRepository {
    /// The forward handoff's assignment write. Same ladder, same
    /// pool, same audit posture as [`Self::assign`]; the differences
    /// are the declared forward semantics: the rebind UPDATE allows
    /// a live operator (the previous agent's ledger row stays —
    /// escalation derives off it), a self-pick maps onto the `Empty`
    /// path, the title law applies, and the bot ledger row unfollows.
    pub async fn assign_forward(
        &self,
        input: &ForwardAssignInput,
    ) -> Result<AssignOutcome, LivechatError> {
        let mut tx = self.pool.begin().await?;
        relay_ambient_scope(&mut tx).await?;

        let pick_sql = format!(
            r#"WITH {POOL_CTE},
ranked AS (
  SELECT user_id, ongoing, last_assigned_at,
         CASE
           WHEN $6::text IS NOT NULL AND $6 = ANY(languages) AND $7::text[] <@ expertise THEN 1
           WHEN $6::text IS NOT NULL AND $6 = ANY(languages) AND $7::text[] && expertise THEN 2
           WHEN $6::text IS NOT NULL AND $6 = ANY(languages)                    THEN 3
           WHEN $8::text IS NOT NULL AND country_match AND $7::text[] <@ expertise THEN 4
           WHEN $8::text IS NOT NULL AND country_match AND $7::text[] && expertise THEN 5
           WHEN $8::text IS NOT NULL AND country_match                                THEN 6
           WHEN cardinality($7::text[]) > 0 AND $7::text[] <@ expertise THEN 7
           WHEN cardinality($7::text[]) > 0 AND $7::text[] && expertise THEN 8
           ELSE 9
         END AS rung
  FROM pool
)
SELECT user_id, rung::int AS rung, (count(*) OVER ())::bigint AS candidates_considered
FROM ranked
ORDER BY rung ASC, ongoing ASC, last_assigned_at ASC NULLS FIRST, user_id ASC
LIMIT 1"#
        );
        let pick: Option<LadderRow> = sqlx::query_as::<_, LadderRow>(&pick_sql)
            .bind(input.channel_id)
            .bind(ONGOING_WINDOW_SECS)
            .bind(PRESENCE_WINDOW_SECS)
            .bind(ASSIGNMENT_BUFFER_SECS)
            .bind(Option::<Uuid>::None)
            .bind(input.visitor_language.as_deref())
            .bind(&input.expertise)
            .bind(input.visitor_country.as_deref())
            .fetch_optional(&mut *tx)
            .await?;

        let pick = match pick {
            // No candidate at all: the defined nobody path.
            None => {
                audit_tx(
                    &mut tx,
                    "assignment_empty",
                    input.actor,
                    "session",
                    input.session_id,
                    serde_json::json!({
                        "channel_id": input.channel_id,
                        "via": "forward",
                        "window_secs": ONGOING_WINDOW_SECS,
                        "buffer_applied": true,
                    }),
                )
                .await?;
                tx.commit().await?;
                return Ok(AssignOutcome::Empty);
            }
            // A self-pick counts as nobody (the handoff would be a
            // no-op; the caller writes `no_agent` and continues).
            Some(p) if Some(p.user_id) == input.current_operator => {
                audit_tx(
                    &mut tx,
                    "assignment_empty",
                    input.actor,
                    "session",
                    input.session_id,
                    serde_json::json!({
                        "channel_id": input.channel_id,
                        "via": "forward",
                        "reason": "self_pick",
                        "window_secs": ONGOING_WINDOW_SECS,
                        "buffer_applied": true,
                    }),
                )
                .await?;
                tx.commit().await?;
                return Ok(AssignOutcome::Empty);
            }
            Some(p) => p,
        };

        // The rebind: unlike `assign`, an existing operator does not
        // refuse the write — the previous agent's ledger row stays
        // (escalation derives off it), the new agent's row lands.
        let won = sqlx::query_as::<_, (Uuid,)>(
            r#"UPDATE livechat.sessions
                  SET operator_user_id = $2, status = 'in_progress',
                      failure = 'no_answer',
                      title = CASE WHEN $3::text IS NOT NULL THEN
                          $3 || ' / ' || COALESCE((SELECT p.display_name
                                                     FROM livechat.operator_profiles p
                                                    WHERE p.user_id = $2),
                                                    'operator')
                          ELSE title END,
                      expertise_names = CASE WHEN cardinality($4::text[]) > 0
                                             THEN $4 ELSE expertise_names END
                WHERE id = $1 AND closed_at IS NULL
                RETURNING id"#,
        )
        .bind(input.session_id)
        .bind(pick.user_id)
        .bind(input.visitor_label.as_deref())
        .bind(&input.stamp_expertise)
        .fetch_optional(&mut *tx)
        .await;
        let won = match won {
            Ok(v) => v,
            Err(e) => {
                tx.rollback().await?;
                return Err(LivechatError::Database(e.to_string()));
            }
        };
        if won.is_none() {
            // The session closed under the handoff: commit the
            // refusal audit, answer the busy family.
            audit_tx(
                &mut tx,
                "operator_busy",
                input.actor,
                "session",
                input.session_id,
                serde_json::json!({ "operator_user_id": pick.user_id, "via": "forward" }),
            )
            .await?;
            tx.commit().await?;
            return Err(LivechatError::OperatorBusy);
        }

        // The new agent's ledger row (the previous agent's row stays;
        // >1 agent rows is the escalation derive's input).
        sqlx::query(
            r#"INSERT INTO livechat.member_histories
                   (session_id, persona, operator_user_id, expertise_names)
               VALUES ($1, 'agent', $2,
                       COALESCE((SELECT ARRAY(SELECT t.name
                                  FROM livechat.operator_expertise oe
                                  JOIN livechat.expertise_tags t ON t.id = oe.expertise_tag_id
                                 WHERE oe.operator_profile_id = p.id)
                                FROM livechat.operator_profiles p
                                WHERE p.user_id = $2), '{}'))
               ON CONFLICT (session_id, operator_user_id)
               WHERE persona = 'agent' AND operator_user_id IS NOT NULL
               DO UPDATE SET left_at = NULL, joined_at = now()"#,
        )
        .bind(input.session_id)
        .bind(pick.user_id)
        .execute(&mut *tx)
        .await?;

        // The bot unfollows at the handoff.
        sqlx::query(
            "UPDATE livechat.member_histories SET left_at = now() \
             WHERE session_id = $1 AND persona = 'bot' AND left_at IS NULL",
        )
        .bind(input.session_id)
        .execute(&mut *tx)
        .await?;

        // The buffer's anchor.
        sqlx::query(
            "UPDATE livechat.operator_profiles SET last_assigned_at = now() \
             WHERE user_id = $1",
        )
        .bind(pick.user_id)
        .execute(&mut *tx)
        .await?;

        recompute_outcome_tx(&mut tx, input.session_id).await?;
        audit_tx(
            &mut tx,
            "operator_assigned",
            input.actor,
            "session",
            input.session_id,
            serde_json::json!({
                "operator_user_id": pick.user_id,
                "rung": pick.rung,
                "candidates_considered": pick.candidates_considered,
                "previous_operator_considered": null,
                "buffer_applied": true,
                "buffer_secs": ASSIGNMENT_BUFFER_SECS,
                "window_secs": ONGOING_WINDOW_SECS,
                "via": "forward",
            }),
        )
        .await?;
        tx.commit().await?;
        Ok(AssignOutcome::Assigned {
            operator_user_id: pick.user_id,
            rung: pick.rung,
            candidates_considered: pick.candidates_considered,
        })
    }
}

/// The per-record outcome derive, transaction-scoped:
/// `escalated` when the session's agent ledger rows exceed one, else
/// the row's own failure — a pure function of THIS row's inputs,
/// written as a single-row UPDATE at every write that changes them.
pub async fn recompute_outcome_tx(
    tx: &mut sqlx::PgConnection,
    session_id: Uuid,
) -> Result<(), LivechatError> {
    sqlx::query(
        r#"UPDATE livechat.sessions s
              SET outcome = CASE
                    WHEN (SELECT count(*) FROM livechat.member_histories h
                           WHERE h.session_id = s.id AND h.persona = 'agent') > 1
                        THEN 'escalated'::livechat_session_outcome
                    ELSE s.failure::text::livechat_session_outcome
                  END
            WHERE s.id = $1"#,
    )
    .bind(session_id)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// The set-based outcome recompute for many rows at once (the sweep
/// closes): one statement, per-row CASE — never an iteration that
/// assigns through a set handle.
pub async fn recompute_outcomes_batch_tx(
    tx: &mut sqlx::PgConnection,
    session_ids: &[Uuid],
) -> Result<(), LivechatError> {
    sqlx::query(
        r#"UPDATE livechat.sessions s
              SET outcome = CASE
                    WHEN (SELECT count(*) FROM livechat.member_histories h
                           WHERE h.session_id = s.id AND h.persona = 'agent') > 1
                        THEN 'escalated'::livechat_session_outcome
                    ELSE s.failure::text::livechat_session_outcome
                  END
            WHERE s.id = ANY($1)"#,
    )
    .bind(session_ids)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// Append one audit row inside a transaction (the durable trace).
pub async fn audit_tx(
    tx: &mut sqlx::PgConnection,
    kind: &str,
    actor: Option<Uuid>,
    subject_type: &str,
    subject_id: Uuid,
    detail: serde_json::Value,
) -> Result<(), LivechatError> {
    sqlx::query(
        r#"INSERT INTO livechat.livechat_audit_log
               (event, actor, subject_type, subject_id, detail)
           VALUES ($1::livechat_audit_event, $2, $3, $4, $5)"#,
    )
    .bind(kind)
    .bind(actor)
    .bind(subject_type)
    .bind(subject_id)
    .bind(detail)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// Append one audit row on the pool.
pub async fn record_audit(
    pool: &PgPool,
    kind: &str,
    actor: Option<Uuid>,
    subject_type: &str,
    subject_id: Uuid,
    detail: serde_json::Value,
) {
    let _ = org_scope::execute_scoped(
        pool,
        sqlx::query(
            r#"INSERT INTO livechat.livechat_audit_log
               (event, actor, subject_type, subject_id, detail)
           VALUES ($1::livechat_audit_event, $2, $3, $4, $5)"#,
        )
        .bind(kind)
        .bind(actor)
        .bind(subject_type)
        .bind(subject_id)
        .bind(detail),
    )
    .await;
}
