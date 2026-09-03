-- Down: drop livechat.chatbot_step_triggers table
DROP TABLE IF EXISTS livechat.chatbot_step_triggers CASCADE;
DROP FUNCTION IF EXISTS livechat.chatbot_step_triggers_audit_timestamp() CASCADE;
