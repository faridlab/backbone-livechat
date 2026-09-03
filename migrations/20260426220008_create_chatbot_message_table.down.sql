-- Down: drop livechat.chatbot_messages table
DROP TABLE IF EXISTS livechat.chatbot_messages CASCADE;
DROP FUNCTION IF EXISTS livechat.chatbot_messages_audit_timestamp() CASCADE;
