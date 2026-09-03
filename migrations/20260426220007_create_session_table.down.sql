-- Down: drop livechat.sessions table
DROP TABLE IF EXISTS livechat.sessions CASCADE;
DROP FUNCTION IF EXISTS livechat.sessions_audit_timestamp() CASCADE;
