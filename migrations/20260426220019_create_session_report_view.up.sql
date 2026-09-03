-- Hand-written: the session report view (user-owned; see
-- metaphor.codegen.yaml).
--
-- One row per session, read ONLY through the bounded-window report verb
-- (the verb requires explicit from/to bounds, window capped at 366 days —
-- the view itself carries NO date predicate, so the unbounded
-- every-session-ever scan of the upstream report simply has no caller).
--
-- Determinism laws (no locale, no drift):
--  - duration is bounded: closed sessions only (closed_at minus the
--    metadata created_at stamp); an open session reports is_open with a
--    NULL duration — never COALESCE(end, NOW()) drift;
--  - time_to_answer comes from the once-only first_response_at;
--  - session_outcome is the STORED per-record derive; `escalated` stays a
--    live derive over the agent ledger rows (never stored);
--  - handled_by_agent / handled_by_bot are EXISTS reads over the ledger;
--  - the rating is the one row per session (1/5/10 -> unhappy/neutral/
--    happy; an absent rating is NULL, never a stored 0);
--  - the chatbot answer path is a deterministic label join off
--    chatbot_messages.selected_answer_id (latest by created_at, then id).
--
-- security_invoker = true: the view executes with the querying role's
-- privileges, so the company RLS fence flows through the view — the
-- fenced-runtime probe asserts this under the NOSUPERUSER NOBYPASSRLS
-- probe role.

CREATE OR REPLACE VIEW livechat.session_report
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
