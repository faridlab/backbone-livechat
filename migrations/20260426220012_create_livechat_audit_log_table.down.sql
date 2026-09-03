-- Down: drop livechat.livechat_audit_log table
DROP TABLE IF EXISTS livechat.livechat_audit_log CASCADE;
DROP FUNCTION IF EXISTS livechat.livechat_audit_log_audit_timestamp() CASCADE;
