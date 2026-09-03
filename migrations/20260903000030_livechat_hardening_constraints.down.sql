-- Hand-written: revert the constraint hardening (user-owned; see
-- metaphor.codegen.yaml). Drops exactly what the up migration added.

DROP INDEX IF EXISTS livechat.idx_sessions_open_operator;
DROP INDEX IF EXISTS livechat.idx_sessions_channel_last_interest;
DROP INDEX IF EXISTS livechat.idx_sessions_open_by_interest;

DROP INDEX IF EXISTS livechat.uq_member_histories_agent;
DROP INDEX IF EXISTS livechat.uq_member_histories_visitor;
DROP INDEX IF EXISTS livechat.uq_member_histories_bot;
ALTER TABLE livechat.member_histories
    DROP CONSTRAINT IF EXISTS ck_member_histories_persona_trichotomy;

ALTER TABLE livechat.sessions
    DROP CONSTRAINT IF EXISTS ck_sessions_closed_no_status;

DROP INDEX IF EXISTS livechat.uq_chatbot_messages_carrier_id;
DROP INDEX IF EXISTS livechat.uq_chatbot_steps_script_sequence;
DROP INDEX IF EXISTS livechat.uq_chatbot_step_triggers_target_answer;

ALTER TABLE livechat.ratings
    DROP CONSTRAINT IF EXISTS ck_ratings_value_scale;
ALTER TABLE livechat.ratings
    DROP CONSTRAINT IF EXISTS ck_ratings_attribution;

DROP INDEX IF EXISTS livechat.uq_conversation_tags_company_lower_name;
DROP INDEX IF EXISTS livechat.uq_expertise_tags_company_lower_name;
DROP INDEX IF EXISTS livechat.uq_chatbot_scripts_company_lower_title;

DROP INDEX IF EXISTS livechat.uq_operator_expertise_profile_tag;
DROP INDEX IF EXISTS livechat.uq_session_tags_session_tag;

ALTER TABLE livechat.channels
    DROP CONSTRAINT IF EXISTS ck_channels_max_sessions_positive;
