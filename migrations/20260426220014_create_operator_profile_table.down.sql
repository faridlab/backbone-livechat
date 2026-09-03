-- Down: drop livechat.operator_profiles table
DROP TABLE IF EXISTS livechat.operator_profiles CASCADE;
DROP FUNCTION IF EXISTS livechat.operator_profiles_audit_timestamp() CASCADE;
