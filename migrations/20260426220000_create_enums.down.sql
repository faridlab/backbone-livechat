-- Down: drop enum types for livechat module
DROP TYPE IF EXISTS livechat_close_reason CASCADE;
DROP TYPE IF EXISTS livechat_session_outcome CASCADE;
DROP TYPE IF EXISTS livechat_failure CASCADE;
DROP TYPE IF EXISTS livechat_session_status CASCADE;
DROP TYPE IF EXISTS livechat_rated_persona CASCADE;
DROP TYPE IF EXISTS livechat_persona CASCADE;
DROP TYPE IF EXISTS livechat_audit_event CASCADE;
DROP TYPE IF EXISTS livechat_step_type CASCADE;
DROP TYPE IF EXISTS livechat_chatbot_condition CASCADE;
DROP TYPE IF EXISTS livechat_rule_action CASCADE;
DROP TYPE IF EXISTS livechat_max_sessions_mode CASCADE;
