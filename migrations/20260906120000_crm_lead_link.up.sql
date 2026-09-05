-- The CRM lead link: one lead per conversation, one conversation per
-- lead (the donor's has_crm_lead partial-index translation, tightened
-- to a partial UNIQUE both directions), plus the four audit events the
-- bridge's verbs emit.
--
-- Enum types are referenced UNQUALIFIED, matching the generated
-- create-enums migration (which also creates them unqualified, i.e. on
-- the connection's default schema). PG 12+ allows ADD VALUE inside the
-- migration's transaction as long as the new values are not used
-- within it; PG cannot drop enum values, so the down migration leaves
-- them (audit history may reference them).

ALTER TABLE livechat.sessions ADD COLUMN IF NOT EXISTS crm_lead_id uuid;

CREATE UNIQUE INDEX IF NOT EXISTS session_crm_lead_uq
    ON livechat.sessions (crm_lead_id)
    WHERE crm_lead_id IS NOT NULL;

ALTER TYPE livechat_audit_event ADD VALUE IF NOT EXISTS 'lead_linked';
ALTER TYPE livechat_audit_event ADD VALUE IF NOT EXISTS 'lead_link_refused';
ALTER TYPE livechat_audit_event ADD VALUE IF NOT EXISTS 'lead_session_joined';
ALTER TYPE livechat_audit_event ADD VALUE IF NOT EXISTS 'lead_session_join_refused';
