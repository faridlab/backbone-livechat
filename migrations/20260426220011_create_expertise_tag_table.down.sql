-- Down: drop livechat.expertise_tags table
DROP TABLE IF EXISTS livechat.expertise_tags CASCADE;
DROP FUNCTION IF EXISTS livechat.expertise_tags_audit_timestamp() CASCADE;
