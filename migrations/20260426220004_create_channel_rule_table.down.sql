-- Down: drop livechat.channel_rules table
DROP TABLE IF EXISTS livechat.channel_rules CASCADE;
DROP FUNCTION IF EXISTS livechat.channel_rules_audit_timestamp() CASCADE;
