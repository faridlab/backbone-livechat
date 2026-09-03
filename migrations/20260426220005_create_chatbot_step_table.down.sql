-- Down: drop livechat.chatbot_steps table
DROP TABLE IF EXISTS livechat.chatbot_steps CASCADE;
DROP FUNCTION IF EXISTS livechat.chatbot_steps_audit_timestamp() CASCADE;
