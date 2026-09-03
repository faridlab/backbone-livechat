-- Down: drop livechat.channels table
DROP TABLE IF EXISTS livechat.channels CASCADE;
DROP FUNCTION IF EXISTS livechat.channels_audit_timestamp() CASCADE;
