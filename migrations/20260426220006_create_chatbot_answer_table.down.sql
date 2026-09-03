-- Down: drop livechat.chatbot_answers table
DROP TABLE IF EXISTS livechat.chatbot_answers CASCADE;
DROP FUNCTION IF EXISTS livechat.chatbot_answers_audit_timestamp() CASCADE;
