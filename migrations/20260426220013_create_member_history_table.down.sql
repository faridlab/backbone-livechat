-- Down: drop livechat.member_histories table
DROP TABLE IF EXISTS livechat.member_histories CASCADE;
DROP FUNCTION IF EXISTS livechat.member_histories_audit_timestamp() CASCADE;
