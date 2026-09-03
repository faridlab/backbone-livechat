-- Down: drop livechat.session_tags table
DROP TABLE IF EXISTS livechat.session_tags CASCADE;
DROP FUNCTION IF EXISTS livechat.session_tags_audit_timestamp() CASCADE;
