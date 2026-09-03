-- Down: drop livechat.ratings table
DROP TABLE IF EXISTS livechat.ratings CASCADE;
DROP FUNCTION IF EXISTS livechat.ratings_audit_timestamp() CASCADE;
