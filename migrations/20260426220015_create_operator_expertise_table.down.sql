-- Down: drop livechat.operator_expertise table
DROP TABLE IF EXISTS livechat.operator_expertise CASCADE;
DROP FUNCTION IF EXISTS livechat.operator_expertise_audit_timestamp() CASCADE;
