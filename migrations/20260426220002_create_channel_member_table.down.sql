-- Down: drop livechat.channel_members table
DROP TABLE IF EXISTS livechat.channel_members CASCADE;
DROP FUNCTION IF EXISTS livechat.channel_members_audit_timestamp() CASCADE;
