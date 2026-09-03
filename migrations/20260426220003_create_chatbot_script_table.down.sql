-- Down: drop livechat.chatbot_scripts table
DROP TABLE IF EXISTS livechat.chatbot_scripts CASCADE;
DROP FUNCTION IF EXISTS livechat.chatbot_scripts_audit_timestamp() CASCADE;
