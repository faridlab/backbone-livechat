-- Hand-authored (user-owned). Not regenerated.
--
-- Best-effort restore sketch for the tenancy strip (ADR-0029). This is a breaking module
-- release against dev-stage databases: the down re-adds the company_id column as nullable
-- with its company-leading uniques and the company isolation policy shape, but restores NO
-- data — rows written after the strip (or after the decorator re-keyed them) carry
-- org_unit_id only. The composing service's tenancy decorator remains the live fence;
-- treat this down as a schema-shape sketch for archaeology, not a usable rollback.
--
-- The org-scoped re-declarations (operator profile per unit, the three case-insensitive
-- namespaces) are NOT restored here either: they were never this module's post-strip
-- shape. The session_report view is rebuilt WITH its company_id projection column again,
-- reading NULL for post-strip rows.

ALTER TABLE livechat.channels             ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.channel_members      ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.channel_rules        ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.chatbot_answers      ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.chatbot_messages     ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.chatbot_scripts      ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.chatbot_steps        ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.chatbot_step_triggers ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.conversation_tags    ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.expertise_tags       ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.livechat_audit_log   ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.member_histories     ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.operator_expertise   ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.operator_profiles    ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.ratings              ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.sessions             ADD COLUMN IF NOT EXISTS company_id uuid;
ALTER TABLE livechat.session_tags         ADD COLUMN IF NOT EXISTS company_id uuid;

CREATE UNIQUE INDEX IF NOT EXISTS idx_operator_profiles_company_id_user_id
    ON livechat.operator_profiles (company_id, user_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_conversation_tags_company_lower_name
    ON livechat.conversation_tags (company_id, lower(name));
CREATE UNIQUE INDEX IF NOT EXISTS uq_expertise_tags_company_lower_name
    ON livechat.expertise_tags (company_id, lower(name));
CREATE UNIQUE INDEX IF NOT EXISTS uq_chatbot_scripts_company_lower_title
    ON livechat.chatbot_scripts (company_id, lower(title));

CREATE POLICY channels_company_isolation ON livechat.channels
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY channel_members_company_isolation ON livechat.channel_members
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY channel_rules_company_isolation ON livechat.channel_rules
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY chatbot_answers_company_isolation ON livechat.chatbot_answers
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY chatbot_messages_company_isolation ON livechat.chatbot_messages
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY chatbot_scripts_company_isolation ON livechat.chatbot_scripts
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY chatbot_steps_company_isolation ON livechat.chatbot_steps
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY chatbot_step_triggers_company_isolation ON livechat.chatbot_step_triggers
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY conversation_tags_company_isolation ON livechat.conversation_tags
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY expertise_tags_company_isolation ON livechat.expertise_tags
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY livechat_audit_log_company_isolation ON livechat.livechat_audit_log
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY member_histories_company_isolation ON livechat.member_histories
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY operator_expertise_company_isolation ON livechat.operator_expertise
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY operator_profiles_company_isolation ON livechat.operator_profiles
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY ratings_company_isolation ON livechat.ratings
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY sessions_company_isolation ON livechat.sessions
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);
CREATE POLICY session_tags_company_isolation ON livechat.session_tags
    FOR ALL USING (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid);

DROP VIEW IF EXISTS livechat.session_report;
CREATE VIEW livechat.session_report
WITH (security_invoker = true) AS
SELECT
    s.id                                                   AS session_id,
    s.company_id                                           AS company_id,
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
