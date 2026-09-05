-- Down: drop the CRM lead link surface.
--
-- The three audit enum values are NOT dropped: PostgreSQL cannot remove
-- enum values, and audit history rows may reference them. The column and
-- the partial index go; sessions themselves are never deleted.

DROP INDEX IF EXISTS livechat.session_crm_lead_uq;
ALTER TABLE livechat.sessions DROP COLUMN IF EXISTS crm_lead_id;
