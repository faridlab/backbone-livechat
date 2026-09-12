-- Hand-authored (user-owned). Not regenerated.
--
-- Strip every company-fence artifact from the livechat tables (ADR-0029): the module is
-- tenant-agnostic; org scoping is installed by the COMPOSING service's tenancy decorator,
-- never by the module. Dropped here, per table: the company-leading uniques, the
-- <table>_company_isolation RLS policy, and the company_id column itself.
--
-- The org-scoped re-declarations move to the composing service's tenancy decorator:
--   - one operator profile per unit (org_unit_id, user_id);
--   - the three case-insensitive canonical namespaces — conversation_tags / expertise_tags
--     on (org_unit_id, lower(name)), chatbot_scripts on (org_unit_id, lower(title)).
-- The member-history, channel-membership, rating, agent-ledger and website-request uniques
-- carry no company axis and stay module-owned.
--
-- The session_report view is rebuilt WITHOUT its company_id projection column: PostgreSQL
-- cannot drop a view column via CREATE OR REPLACE, so the strip drops and recreates the
-- view in one file. security_invoker = true is preserved — the decorator's fence flows
-- through the view exactly as the module fence did.
--
-- Ordering guard (the decorator must run FIRST on any database with data): the module
-- never moves tenancy data. A table is safe to strip when EITHER
--   a) it carries org_unit_id with no NULLs — the decorator backfilled it from company_id —
--      or b) it is empty (a fresh database: the earlier chain files created it empty).
-- Otherwise the strip RAISEs, naming the decorator step, rather than dropping a column
-- that still holds the only tenancy key. The file is re-runnable (every drop is IF EXISTS
-- and the tracker has no checksums), so a failed run retries cleanly after the decorator
-- lands.
--
-- RLS enable/force flags are deliberately NOT touched: the decorator owns those now.

DO $$
DECLARE
    t text;
    has_org boolean;
    org_nulls bigint;
    total bigint;
    offenders text := '';
BEGIN
    FOREACH t IN ARRAY ARRAY[
        'channels', 'channel_members', 'channel_rules', 'chatbot_answers',
        'chatbot_messages', 'chatbot_scripts', 'chatbot_steps',
        'chatbot_step_triggers', 'conversation_tags', 'expertise_tags',
        'livechat_audit_log', 'member_histories', 'operator_expertise',
        'operator_profiles', 'ratings', 'sessions', 'session_tags'
    ]
    LOOP
        IF to_regclass(format('livechat.%I', t)) IS NULL THEN
            CONTINUE; -- chain not fully applied on this database; nothing to strip
        END IF;

        SELECT EXISTS (
                   SELECT 1 FROM information_schema.columns
                   WHERE table_schema = 'livechat' AND table_name = t AND column_name = 'org_unit_id'
               )
        INTO has_org;

        EXECUTE format('SELECT count(*) FROM livechat.%I', t) INTO total;

        IF has_org THEN
            EXECUTE format(
                'SELECT count(*) FROM livechat.%I WHERE org_unit_id IS NULL', t)
            INTO org_nulls;
        ELSE
            org_nulls := total; -- no org column: every row's only tenancy key is company_id
        END IF;

        IF has_org AND org_nulls = 0 THEN
            CONTINUE; -- decorator backfilled: safe
        END IF;
        IF total = 0 THEN
            CONTINUE; -- empty table (fresh database): safe
        END IF;
        offenders := offenders || format(' livechat.%s (%s rows, %s rows not covered by org_unit_id);', t, total, org_nulls);
    END LOOP;

    IF offenders <> '' THEN
        RAISE EXCEPTION 'refusing to strip company_id — these tables are not yet covered by the tenancy decorator:%. Apply the composing service''s tenancy decorator (it backfills org_unit_id from company_id) and re-run; it is the only step that moves tenancy data.', offenders;
    END IF;
END $$;

-- ── session_report view (rebuilt without the company_id projection; MUST precede the
-- table drops — the view's company_id column depends on sessions.company_id) ─────
DROP VIEW IF EXISTS livechat.session_report;
CREATE VIEW livechat.session_report
WITH (security_invoker = true) AS
SELECT
    s.id                                                   AS session_id,
    s.channel_id                                           AS channel_id,
    s.is_test                                              AS is_test,
    (s.closed_at IS NULL)                                  AS is_open,
    (s.metadata ->> 'created_at')::timestamptz             AS opened_at,
    CASE WHEN s.closed_at IS NOT NULL THEN
        EXTRACT(EPOCH FROM (s.closed_at
                            - (s.metadata ->> 'created_at')::timestamptz))::bigint
    END                                                    AS duration_secs,
    CASE WHEN s.first_response_at IS NOT NULL THEN
        EXTRACT(EPOCH FROM (s.first_response_at
                            - (s.metadata ->> 'created_at')::timestamptz))::bigint
    END                                                    AS time_to_answer_secs,
    s.outcome::text                                        AS session_outcome,
    ((SELECT count(*)
        FROM livechat.member_histories h
       WHERE h.session_id = s.id AND h.persona = 'agent') > 1)
                                                           AS escalated,
    EXISTS (SELECT 1 FROM livechat.member_histories h
             WHERE h.session_id = s.id AND h.persona = 'agent')
                                                           AS handled_by_agent,
    EXISTS (SELECT 1 FROM livechat.member_histories h
             WHERE h.session_id = s.id AND h.persona = 'bot')
                                                           AS handled_by_bot,
    r.value                                                AS rating_value,
    CASE r.value
        WHEN 1  THEN 'unhappy'
        WHEN 5  THEN 'neutral'
        WHEN 10 THEN 'happy'
    END                                                    AS rating_text,
    last_answer.label                                      AS chatbot_last_answer_label
FROM livechat.sessions s
LEFT JOIN livechat.ratings r
       ON r.session_id = s.id
LEFT JOIN LATERAL (
    SELECT ca.label
      FROM livechat.chatbot_messages cm
      JOIN livechat.chatbot_answers ca ON ca.id = cm.selected_answer_id
     WHERE cm.session_id = s.id
     ORDER BY cm.created_at DESC, cm.id DESC
     LIMIT 1
) last_answer ON true
WHERE s.metadata ->> 'deleted_at' IS NULL;

-- ── channels ───────────────────────────────────────────────────────────────────
DROP POLICY IF EXISTS channels_company_isolation ON livechat.channels;
ALTER TABLE livechat.channels DROP COLUMN IF EXISTS company_id;

-- ── channel_members ────────────────────────────────────────────────────────────
DROP POLICY IF EXISTS channel_members_company_isolation ON livechat.channel_members;
ALTER TABLE livechat.channel_members DROP COLUMN IF EXISTS company_id;

-- ── channel_rules ──────────────────────────────────────────────────────────────
DROP POLICY IF EXISTS channel_rules_company_isolation ON livechat.channel_rules;
ALTER TABLE livechat.channel_rules DROP COLUMN IF EXISTS company_id;

-- ── chatbot_answers ────────────────────────────────────────────────────────────
DROP POLICY IF EXISTS chatbot_answers_company_isolation ON livechat.chatbot_answers;
ALTER TABLE livechat.chatbot_answers DROP COLUMN IF EXISTS company_id;

-- ── chatbot_messages ───────────────────────────────────────────────────────────
DROP POLICY IF EXISTS chatbot_messages_company_isolation ON livechat.chatbot_messages;
ALTER TABLE livechat.chatbot_messages DROP COLUMN IF EXISTS company_id;

-- ── chatbot_scripts ────────────────────────────────────────────────────────────
DROP INDEX IF EXISTS livechat.uq_chatbot_scripts_company_lower_title;
DROP POLICY IF EXISTS chatbot_scripts_company_isolation ON livechat.chatbot_scripts;
ALTER TABLE livechat.chatbot_scripts DROP COLUMN IF EXISTS company_id;

-- ── chatbot_steps ──────────────────────────────────────────────────────────────
DROP POLICY IF EXISTS chatbot_steps_company_isolation ON livechat.chatbot_steps;
ALTER TABLE livechat.chatbot_steps DROP COLUMN IF EXISTS company_id;

-- ── chatbot_step_triggers ──────────────────────────────────────────────────────
DROP POLICY IF EXISTS chatbot_step_triggers_company_isolation ON livechat.chatbot_step_triggers;
ALTER TABLE livechat.chatbot_step_triggers DROP COLUMN IF EXISTS company_id;

-- ── conversation_tags ──────────────────────────────────────────────────────────
DROP INDEX IF EXISTS livechat.uq_conversation_tags_company_lower_name;
DROP POLICY IF EXISTS conversation_tags_company_isolation ON livechat.conversation_tags;
ALTER TABLE livechat.conversation_tags DROP COLUMN IF EXISTS company_id;

-- ── expertise_tags ─────────────────────────────────────────────────────────────
DROP INDEX IF EXISTS livechat.uq_expertise_tags_company_lower_name;
DROP POLICY IF EXISTS expertise_tags_company_isolation ON livechat.expertise_tags;
ALTER TABLE livechat.expertise_tags DROP COLUMN IF EXISTS company_id;

-- ── livechat_audit_log ─────────────────────────────────────────────────────────
DROP POLICY IF EXISTS livechat_audit_log_company_isolation ON livechat.livechat_audit_log;
ALTER TABLE livechat.livechat_audit_log DROP COLUMN IF EXISTS company_id;

-- ── member_histories ───────────────────────────────────────────────────────────
DROP POLICY IF EXISTS member_histories_company_isolation ON livechat.member_histories;
ALTER TABLE livechat.member_histories DROP COLUMN IF EXISTS company_id;

-- ── operator_expertise ─────────────────────────────────────────────────────────
DROP POLICY IF EXISTS operator_expertise_company_isolation ON livechat.operator_expertise;
ALTER TABLE livechat.operator_expertise DROP COLUMN IF EXISTS company_id;

-- ── operator_profiles ──────────────────────────────────────────────────────────
DROP INDEX IF EXISTS livechat.idx_operator_profiles_company_id_user_id;
DROP POLICY IF EXISTS operator_profiles_company_isolation ON livechat.operator_profiles;
ALTER TABLE livechat.operator_profiles DROP COLUMN IF EXISTS company_id;

-- ── ratings ────────────────────────────────────────────────────────────────────
DROP POLICY IF EXISTS ratings_company_isolation ON livechat.ratings;
ALTER TABLE livechat.ratings DROP COLUMN IF EXISTS company_id;

-- ── sessions ───────────────────────────────────────────────────────────────────
DROP POLICY IF EXISTS sessions_company_isolation ON livechat.sessions;
ALTER TABLE livechat.sessions DROP COLUMN IF EXISTS company_id;

-- ── session_tags ───────────────────────────────────────────────────────────────
DROP POLICY IF EXISTS session_tags_company_isolation ON livechat.session_tags;
ALTER TABLE livechat.session_tags DROP COLUMN IF EXISTS company_id;
