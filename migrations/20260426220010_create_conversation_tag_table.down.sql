-- Down: drop livechat.conversation_tags table
DROP TABLE IF EXISTS livechat.conversation_tags CASCADE;
DROP FUNCTION IF EXISTS livechat.conversation_tags_audit_timestamp() CASCADE;
