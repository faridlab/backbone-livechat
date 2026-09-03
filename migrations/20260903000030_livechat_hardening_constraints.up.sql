-- Hand-written: constraint hardening (user-owned; see metaphor.codegen.yaml).
--
-- The schema generator emits plain columns and primary keys but drops most
-- model-declared UNIQUE indexes and multi-column CHECKs. This migration
-- lands them idempotently, after the generated table migrations. Nothing
-- here changes column shapes — regen reproduces identical tables.
--
-- Idempotency contract: every statement is guarded (IF NOT EXISTS for
-- indexes, DROP IF EXISTS + ADD for CHECKs), so re-running is a no-op.

-- =============================================================================
-- 1. The member-history ledger (the dual-duty table: reporting snapshot AND
--    the ladder's ongoing-count source).
--
--    Three partial uniques — one per persona — plus the STRICT trichotomy
--    CHECK. The trichotomy bans the both-NULL row too: a persona must bind
--    exactly its identity column and no other, so a ledger row can never
--    exist that no partial unique covers (the fence against duplicate or
--    identity-less membership rows).
-- =============================================================================

-- One agent row per operator per session (rejoins re-point via ON CONFLICT
-- DO UPDATE, never duplicate).
CREATE UNIQUE INDEX IF NOT EXISTS uq_member_histories_agent
    ON livechat.member_histories (session_id, operator_user_id)
    WHERE persona = 'agent' AND operator_user_id IS NOT NULL;

-- One visitor row per visitor digest per session.
CREATE UNIQUE INDEX IF NOT EXISTS uq_member_histories_visitor
    ON livechat.member_histories (session_id, visitor_key)
    WHERE persona = 'visitor' AND visitor_key IS NOT NULL;

-- One bot row per script per session.
CREATE UNIQUE INDEX IF NOT EXISTS uq_member_histories_bot
    ON livechat.member_histories (session_id, chatbot_script_id)
    WHERE persona = 'bot' AND chatbot_script_id IS NOT NULL;

-- The strict persona trichotomy.
ALTER TABLE livechat.member_histories
    DROP CONSTRAINT IF EXISTS ck_member_histories_persona_trichotomy;
ALTER TABLE livechat.member_histories
    ADD CONSTRAINT ck_member_histories_persona_trichotomy CHECK (
        (persona = 'agent'
            AND operator_user_id IS NOT NULL
            AND visitor_key IS NULL
            AND chatbot_script_id IS NULL)
     OR (persona = 'visitor'
            AND visitor_key IS NOT NULL
            AND operator_user_id IS NULL
            AND chatbot_script_id IS NULL)
     OR (persona = 'bot'
            AND chatbot_script_id IS NOT NULL
            AND operator_user_id IS NULL
            AND visitor_key IS NULL)
    );

-- =============================================================================
-- 2. Session lifecycle shape.
-- =============================================================================

-- An ended session (closed_at set) carries NO live status — the upstream
-- "ended" pseudo-status is the closed_at timestamp, not a status value.
ALTER TABLE livechat.sessions
    DROP CONSTRAINT IF EXISTS ck_sessions_closed_no_status;
ALTER TABLE livechat.sessions
    ADD CONSTRAINT ck_sessions_closed_no_status CHECK (
        closed_at IS NULL OR status IS NULL
    );

-- Ladder and sweep support indexes (the ongoing-window predicate, the
-- open-session assignment scan, the idle-close sweep scan).
CREATE INDEX IF NOT EXISTS idx_sessions_open_operator
    ON livechat.sessions (operator_user_id)
    WHERE closed_at IS NULL AND operator_user_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_sessions_channel_last_interest
    ON livechat.sessions (channel_id, last_interest_at);
CREATE INDEX IF NOT EXISTS idx_sessions_open_by_interest
    ON livechat.sessions (last_interest_at)
    WHERE closed_at IS NULL;

-- =============================================================================
-- 3. Carrier and chatbot machine shape.
-- =============================================================================

-- One ledger-visible chatbot message per carrier id (the mail carrier's
-- message ids are unique; NULL rows are exempt).
CREATE UNIQUE INDEX IF NOT EXISTS uq_chatbot_messages_carrier_id
    ON livechat.chatbot_messages (carrier_message_id)
    WHERE carrier_message_id IS NOT NULL;

-- One step per sequence per script (forward-only routing's total order).
CREATE UNIQUE INDEX IF NOT EXISTS uq_chatbot_steps_script_sequence
    ON livechat.chatbot_steps (chatbot_script_id, sequence);

-- One trigger per (target step, answer) pair.
CREATE UNIQUE INDEX IF NOT EXISTS uq_chatbot_step_triggers_target_answer
    ON livechat.chatbot_step_triggers (target_step_id, answer_id);

-- =============================================================================
-- 4. Rating shape.
-- =============================================================================

-- The community scale: 1 (unhappy), 5 (neutral), 10 (happy). No 0, no
-- stored "not rated" row — an absent rating is the absent row.
ALTER TABLE livechat.ratings
    DROP CONSTRAINT IF EXISTS ck_ratings_value_scale;
ALTER TABLE livechat.ratings
    ADD CONSTRAINT ck_ratings_value_scale CHECK (value IN (1, 5, 10));

-- Attribution follows the rated persona exactly: an agent rating names the
-- operator; a bot rating names the script. Never both, never neither.
ALTER TABLE livechat.ratings
    DROP CONSTRAINT IF EXISTS ck_ratings_attribution;
ALTER TABLE livechat.ratings
    ADD CONSTRAINT ck_ratings_attribution CHECK (
        (rated_persona = 'agent'
            AND operator_user_id IS NOT NULL
            AND chatbot_script_id IS NULL)
     OR (rated_persona = 'bot'
            AND chatbot_script_id IS NOT NULL
            AND operator_user_id IS NULL)
    );

-- =============================================================================
-- 5. Canonical-name uniques (case-insensitive within a company).
-- =============================================================================

CREATE UNIQUE INDEX IF NOT EXISTS uq_conversation_tags_company_lower_name
    ON livechat.conversation_tags (company_id, lower(name));
CREATE UNIQUE INDEX IF NOT EXISTS uq_expertise_tags_company_lower_name
    ON livechat.expertise_tags (company_id, lower(name));
CREATE UNIQUE INDEX IF NOT EXISTS uq_chatbot_scripts_company_lower_title
    ON livechat.chatbot_scripts (company_id, lower(title));

-- =============================================================================
-- 6. Join-table uniques.
-- =============================================================================

CREATE UNIQUE INDEX IF NOT EXISTS uq_operator_expertise_profile_tag
    ON livechat.operator_expertise (operator_profile_id, expertise_tag_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_session_tags_session_tag
    ON livechat.session_tags (session_id, tag_id);

-- =============================================================================
-- 7. Channel capacity floor.
-- =============================================================================

-- A channel's concurrent-session ceiling is at least 1 (an operator who
-- wants "no concurrent cap" writes max_sessions_mode = 'unlimited', not 0).
ALTER TABLE livechat.channels
    DROP CONSTRAINT IF EXISTS ck_channels_max_sessions_positive;
ALTER TABLE livechat.channels
    ADD CONSTRAINT ck_channels_max_sessions_positive CHECK (max_sessions > 0);
