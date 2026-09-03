# backbone-livechat — Module Specification

> Crate `backbone-livechat` (version 0.1.0) · schema `livechat` · host mount `/api/v1/livechat`.
> Donors: **backbone-website v0.1.0** (Tier A HMAC family, per-IP fixed-window throttles, typed
> 404 no-enumeration, `*_TRUSTED_PROXY` client-IP posture, hostname binding, visitor identity) and
> **backbone-events v0.2.1** (typed error codes, leased/scheduled passes, audit rows as durable
> trace, seam ports that park loudly, the fenced-runtime probe, per-table RLS fence).
> Source audit: Odoo 19 community `addons/im_livechat` + `addons/website_livechat` (commit
> `b9eb72eb`), transcribed in `docs/odoo/website/live-chat/` of this framework checkout — the
> `LC-1..22` census (minus unpopulated `LC-18`), the `LC-R1..R15` rule layer, and the
> `WLC-2..12` bridge namespace. This spec disposes every ID; the disposition register is
> §18.

## 0. Acceptance bar

Six register rows (workspace `docs/plan/w7-register-deltas.md` §WB-5) are the acceptance bar.
Everything below is organized to satisfy exactly these:

| Row | Demand | Spec section |
|---|---|---|
| **LC-11** | The compute-in-loop defect is NOT ported: outcome derived per record (set-based / single-row writes), never whole-recordset assignment inside an iteration | §3 |
| **LC-LADDER** | Deterministic ladder minus `random.choice`: capacity → stickiness → language/expertise rungs; the 120 s anti-burst buffer on EVERY path; the 15-months-ORM vs 30-minutes-SQL window divergence resolved to ONE window; RTC GC off the read path; every decision audited | §2 |
| **LC-BOUNDARY** | Declared public surface: no `cors='*'` mirror, no mint-on-wrong-token, guest tokens are Tier A HMAC, the open route is per-IP/per-identity rate limited, visitors join-but-never-start calls on every origin | §8, §1.3 |
| **LEDGER-UNIQUES** | The member-history ledger as 3 DB uniques + the persona constraint (the events partial-unique pattern), in a numbered migration of this module's own sequence | §5, §13 |
| **CHATBOT** | The pointer state machine: one column + message rows; step enum CLOSED at the seven community types (no `create_lead`/`create_ticket`); forward-only routing; declared restart semantics; sanitized answer storage | §6 |
| **LC-CENSUS** | Every census ID disposed by ID; the cycle-24 website bridge lands INSIDE this module (overlays over backbone-website surfaces, not a second crate) | §18, §10 |

Module bar (same as blog): RLS LAW on every transaction + fenced-runtime probe, own-sequence
migrations, `cargo clippy --all-targets -- -D clippy::expect_used` EXIT=0 from inside the module
directory, schema regen DRIFT=0, fail-hard probe suite on scratch `127.0.0.1:5433`.

## 1. Architecture decisions

### 1.1 Hosting posture: mail/discuss-hosted via seam ports, not website-hosted

Upstream `im_livechat` is hosted on mail/discuss: the session IS a discuss channel, the visitor a
mail guest, messages mail messages. Backbone has no discuss module composed into the host, so the
port keeps the *posture* and re-homes the mechanics:

- **Messages live behind a `LivechatMailCarrier` port** (post + fetch + remove). The default is
  `RefusingMailCarrier` — a typed `livechat_carrier_not_composed` refusal that parks loudly. The
  host composes the real adapter over backbone-mail (`message_post` + read back) in
  `src/infrastructure/seams/livechat_compose.rs`. The module never depends on backbone-mail as a
  crate.
- **Chatbot rows are the module's own execution log** (`livechat.chatbot_messages`), linked to the
  carrier's message id by a partial unique — upstream's `UNIQUE(mail_message_id)` translated.
- **Realtime delivery, rating prompts, digest, transcript, RTC are carriers, not columns**: each is
  a process-local port with a typed refusing/unwired default (§11). Where a carrier parks, the
  durable trace is the audit row plus `sessions.error_detail` (the events loud-parking idiom).

This module is ONE crate (ADR-0016): the cycle-24 website bridge is behavior + columns inside it,
not a second crate (§10).

### 1.2 Real-time transport: REST verbs + cursor polling — decision and justification

**Decision**: no websocket/bus at this pin. Presence, message delivery, and the help feed are REST
verbs polled by clients with monotonic cursors.

**Justification**: (a) the donor stack offers no push transport to compose — website and events are
request/response with typed codes, and upstream's bus/websocket/presence services are Odoo-platform
machinery with no backbone equivalent at this pin; (b) every realtime need degrades cleanly to
polling: presence = heartbeat within a window (`operator_profiles.last_heartbeat_at`, 60 s window),
delivery = `GET messages?after=<cursor>` with a per-visitor state digest, help feed = the admin
session list filter; (c) each realtime concern is *also* declared as a port (`rtc_port`,
`notifier_port`, `mail_port`) so a future push transport composes host-side without schema churn.
Poll budget is enforced by the throttle table (§8.3), so the polling posture cannot become the
write-amplifier the open route upstream was.

### 1.3 Visitor identity and tokens (LC-BOUNDARY)

- **Visitor identity is the website visitor, not a second digest family.** The
  `LivechatWebsiteBridge` port (host-composed over backbone-website's `PgWebsiteSurface` /
  `VisitorEngine`) answers `resolve_website_by_host`, `track_visit` (the declared visit seam), and
  `visitor_key(session_facts)` — the website visitor digest. Livechat stores that digest as its
  `visitor_key` on ledger rows and the website visitor row id as `sessions.website_visitor_id`. No
  livechat-minted visitor identity exists, ever.
- **Guest tokens are Tier A HMAC capabilities**, file-shape-copied from
  `backbone-events/src/application/service/capability.rs` with a module-local domain-separation
  context `"livechat-capability-v1"` (never events'). Secret: env `LIVECHAT_CAPABILITY_SECRET`;
  minting under an empty secret is the typed 503 `livechat_capability_secret_not_configured`
  (fail-closed; the state exposes only `secret_is_configured()`).
  - Purpose `livechat-guest-session`: `data = [session_id, visitor_key]`, `exp = now + 24 h`
    (`LIVECHAT_GUEST_TOKEN_TTL_SECS`). Rotates by construction — every open mints fresh.
  - Purpose `livechat-invite-accept`: `data = [session_id, visitor_key]`, `exp = now + 15 min`
    (the operator-initiated invite handoff, §10).
  - Verify is constant-time (`subtle::ConstantTimeEq`), fail-closed on every malformed arm, expiry
    checked after the signature; the whole refusal family collapses to ONE uniform answer (§8.1).
- **A wrong token fails typed; it never mints.** `POST /public/sessions` with a presented bearer
  capability that fails verify answers `401 livechat_guest_token_invalid` and creates ZERO rows
  (probe-asserted). Upstream's mint-on-wrong-token probing cover is refused outright.
- **No CORS mirror.** Zero `cors='*'` routes exist in the module; the public router is a same-origin
  mount under `/api/v1/livechat`. Third-party embedding is not offered at this pin; when it is, it
  re-keys on these same Tier A tokens behind a host-mediated surface (declared future, not
  scaffolding — LC-20's reserved-seam posture).
- **Join-not-start calls hold on every origin** (LC-6): the `LivechatRtcCarrier` port carries the
  contract ("visitors may join an existing session call, never start one"), and *no start verb
  exists anywhere in the module* — the guard holds by construction, with no shadow controller to
  bypass it. No RTC route is mounted at this pin.
- **The "search-visible ⇒ member" assumption is refused.** Every session-scoped public route
  resolves the session through the capability token only; membership is the ledger's business, and
  reads are token-scoped under the module's RLS binding (public routes run with
  `app.company_id` bound from the resolved website's company — see §12).

### 1.4 One crate, no sibling Cargo edges

`Cargo.toml` pins framework crates at `v2.7.11` (backbone-core +postgres, backbone-orm,
backbone-auth, backbone-rate-limit; backbone-messaging forced by the pinned generator with the
same zero-events-with-plumbing-dep comment as website/events). **No sibling module edge**:
backbone-website and backbone-mail are consumed through host-composed adapters over this module's
port traits, recorded as comments so no later seat adds them casually. `regex = "1"` is added for
save-time rule validation (§9). Remaining deps mirror events (axum 0.7, sqlx 0.8, thiserror,
hmac/sha2/subtle/rand, async-trait).

## 2. Operator selection — the deterministic ladder (LC-LADDER, LC-1/LC-1b/LC-2)

### 2.1 The three laws

1. **ONE window.** `LIVECHAT_ONGOING_WINDOW_SECS = 1800` (30 minutes) is the only definition of an
   "ongoing" session: `closed_at IS NULL AND last_interest_at >= now() − 1800s`. It is written
   once (a shared SQL fragment / helper used verbatim by) the assignment pool, the availability
   answer, and the admin console's ongoing counts. Upstream's 15-months-vs-30-minutes divergence
   (the `-15M` uppercase-M parse) is resolved by construction: there is no second window constant
   in the crate (probe asserts both call sites resolve through the one helper).
2. **The buffer is enforced on every path.** `LIVECHAT_ASSIGNMENT_BUFFER_SECS = 120`: a candidate
   whose `last_assigned_at` is within 120 s is not eligible — inside the candidate pool itself, so
   the stickiness arm and the no-rung fallback pass through it structurally. Upstream's two silent
   bypasses (previous operator, unmatched visitor) cannot reappear because there is no arm that
   reads around the pool.
3. **No die roll.** Every tie breaks by a total order: `ongoing_count ASC, last_assigned_at ASC
   NULLS FIRST, operator_user_id ASC`. Upstream's `random.choice` is replaced by that order; the
   fallback arm (no language/country/expertise matched) uses the same total order over the same
   pool rather than burst-assigning.

### 2.2 The ladder, as one set-based statement

Selection is ONE SQL statement (no per-operator presence probes, no N+1):

```sql
WITH pool AS (   -- stages 1+4 fused: capacity gate AND the 120s buffer gate
  SELECT m.user_id, p.languages, p.last_assigned_at,
         (SELECT count(*) FROM livechat.sessions s
           WHERE s.company_id = m.company_id
             AND s.operator_user_id = m.user_id
             AND s.closed_at IS NULL
             AND s.last_interest_at >= now() - make_interval(secs => $ongoing_window)) AS ongoing
  FROM livechat.channel_members m
  JOIN livechat.operator_profiles p
    ON p.user_id = m.user_id AND p.company_id = m.company_id
  WHERE m.channel_id = $channel
    AND p.last_heartbeat_at >= now() - make_interval(secs => $presence_window)
    AND ($mode = 'unlimited' OR ongoing < $max_sessions)
    AND (p.last_assigned_at IS NULL
         OR p.last_assigned_at < now() - make_interval(secs => $buffer))
),
ranked AS (
  SELECT user_id,
         CASE
           WHEN user_id = $previous_operator                THEN 0  -- stickiness, INSIDE the pool
           WHEN $lang  = ANY(languages) AND superset(exp)   THEN 1  -- lang + all expertise
           WHEN $lang  = ANY(languages) AND intersects(exp) THEN 2  -- lang + ≥1 expertise
           WHEN $lang  = ANY(languages)                     THEN 3  -- lang
           WHEN $country_match AND superset(exp)            THEN 4
           WHEN $country_match AND intersects(exp)          THEN 5
           WHEN $country_match                              THEN 6
           WHEN superset(exp)                               THEN 7  -- expertise over nothing
           WHEN intersects(exp)                             THEN 8
           ELSE 9                                            -- the defined fallback
         END AS rung
  FROM pool
)
SELECT user_id FROM ranked
ORDER BY rung ASC, (SELECT ongoing ...) ASC, last_assigned_at ASC NULLS FIRST, user_id ASC
LIMIT 1;
```

- **Rung order** is upstream's 8-rung ladder verbatim (language strictly dominates country,
  expertise refines both), with rank 0 the stickiness arm and rank 9 the defined fallback. The
  upstream `prefer NO status row, else best (count, in_call)` tie-break is the deterministic
  descendant of the total order's first keys (no ongoing rows sorts first).
- **The upstream stickiness relief** ("bypass stickiness when the previous operator holds ≥2 active
  chats AND is in a call") is subsumed: an over-capacity previous operator simply fails the pool
  gate and the ladder runs. The register's "≥2-active-and-in-call relief or its declared
  equivalent" is this.
- **`block_assignment_during_call`** is ported as a channel column and documented inert at this
  pin: with no RTC carrier composed, `in_call` is uniformly false (the RTC GC's fix — stale
  in-call state — is that there is no in-call state yet; the column arms when a carrier lands).
- **The RTC GC does not run in the read path** (upstream's stage 0 with its FIXME): no GC of any
  kind executes inside selection or any read; staleness is handled by the scheduled sweeps (§2.4).
- **Bot priority**: a channel whose matched rule carries an active chatbot script routes the
  session to the bot first — the bot takes absolute priority over every human stage; the ladder
  runs only at a `forward_operator` step (or when no script matched).
- **No candidates** → the defined `no_agent` path: the session stays unassigned, failure becomes
  `no_agent` (chatbot continues per §6.4), and the decision is still audited.

### 2.3 The assignment write (serialized, audited)

One transaction: `bind_current_company` → the ladder statement → `UPDATE livechat.sessions SET
operator_user_id = $op, status = 'in_progress' WHERE id = $s AND operator_user_id IS NULL AND
closed_at IS NULL RETURNING` (first-wins by row atomicity; zero rows = someone else won → typed
409) → insert-or-rebind the agent ledger row (§5) → `UPDATE operator_profiles SET
last_assigned_at = now()` → per-record outcome recompute (§3) → **audit
`OperatorAssigned`** with `{rung, candidates_considered, previous_operator_considered,
buffer_applied: true, window_secs: 1800}`. The upstream's own justification for logging — a random
tiebreak is unreproducible in disputes — is made structural: removing the randomness makes the
audit row a faithful replay of the decision.

### 2.4 Presence and the sweeps (GC off the read path)

Presence is a heartbeat: operators are online iff `last_heartbeat_at >= now() − 60 s`
(`LIVECHAT_PRESENCE_WINDOW_SECS`, const, documented). The console refresh and an explicit
heartbeat verb touch it. Upstream's three autovacuum GCs + the in-read-path RTC GC become two
declared scheduled passes (host jobs loop, per-company `with_company_scope`, SKIP LOCKED bounded
batches — §12.3): the **idle-close sweep** (open sessions with `last_interest_at` older than
`LIVECHAT_IDLE_CLOSE_HOURS = 24` → `closed_at`, reason `expired`, audited `SessionClosed`) and the
**invite-expiry sweep** (§10). The 1-hour raw-SQL unlink of message-less sessions is refused: no
untraced hard delete exists anywhere in the module.

## 3. The outcome derive law (LC-11)

Upstream's `_compute_livechat_outcome` assigns `self.livechat_outcome` inside `for channel in
self:` — a stored compute writing the whole recordset per iteration. **The defect is not ported,
in either shape:**

- Outcome is a pure per-record function of the row's own inputs:
  `outcome = 'escalated' WHEN agent ledger rows > 1, ELSE failure`.
- It is written at the writes that change its inputs (agent join / rebind, forward, close,
  restart) as either a single-row `UPDATE ... WHERE id = $1` or a set-based
  `UPDATE ... SET outcome = CASE <per-row expression> END WHERE id = ANY($ids)` — never by
  iterating a recordset and assigning through a set handle.
- No compatibility knob preserves the source behavior; the corrected shape is the only shape.
- The report view (§7.9) reads the stored column; escalation itself stays a derive (LC-17) —
  `escalated` is computed from the ledger count at read, never stored.

**Probe** (`outcome_per_record`): replicates the upstream failure scenario — a multi-session
fixture whose inputs differ, recomputed in one pass — and asserts every row carries its own value
(the upstream shape would write the last value everywhere).

## 4. Session lifecycle (LC-9, LC-9b)

The bare `livechat_end_dt` flag becomes declared state with explicit, audited transitions:

- **Open** = `closed_at IS NULL`. `close_reason` (`visitor_left | operator_closed | bot_completed |
  expired | cancelled | request_declined`) records which ending fired; `CHECK (closed_at IS NULL OR
  status IS NULL)` mirrors upstream's ended⇒no-status constraint.
- **Endings are verbs, all audited `SessionClosed`**: visitor leave (public close verb —
  idempotent; a second close is a no-op returning current state), operator close, bot completion
  (the router hitting last-step, §6.4), expiry (the idle-close sweep), invite cancel/decline
  (§10). The last-operator-unfollow path ports as the operator close verb (the take/close verbs
  are the only session-writers besides the visitor's own); the unpin GC and the message-less
  unlink do not port.
- **Status machine** (`waiting | in_progress | need_help`): born `waiting`; flips to `in_progress`
  on assignment, on the operator's first message, and on the visitor's first post (upstream's
  hook duties, relocated to the message chokepoint); `need_help` is set/cleared only by the
  explicit verbs (§9) with `HelpRequested`/`HelpResolved` audits; taking a `need_help` session is
  the serialized take (the check-then-act first-wins race is closed by the conditional UPDATE).
- **Failure** (`no_answer | no_agent | no_failure`) is a plain column with declared writers only:
  born `no_answer` on the human path / `no_failure` on the bot path; the operator's first message
  clears to `no_failure`; a forward that finds nobody writes `no_agent`; a successful forward
  resets to `no_answer` until the human speaks.
- **Restart is declared, never silent** (LC-9b): the admin chatbot-restart verb takes an explicit
  `reset_failure: bool` — reset or *explicitly* preserve, audited either way (`ChatbotRestarted`
  with the flag), reopens the session (`closed_at = NULL`, status `waiting`, audit
  `SessionReopened`), resets the pointer to the first step, and clears the module's execution-log
  rows (carrier transcript cleanup is requested through the port; if the carrier cannot, that
  parks as `error_detail` + audit, never a silent skip).

## 5. The member-history ledger (LEDGER-UNIQUES, LC-3)

`livechat.member_histories` keeps the dual duty: the per-participant reporting snapshot AND the
table the ladder's ongoing counts run against (the contention is a declared non-blocking scaling
note — reporting derives and selection share one table by design here).

Columns: `id, company_id, session_id (FK sessions, restrict), persona ('agent'|'visitor'|'bot'),
operator_user_id?, visitor_key?, chatbot_script_id?, joined_at, left_at?, message_count,
response_time_secs?, expertise_names text[]?` (the agent-expertise freeze; labels, not ids —
deterministic reporting, no translation mining). `read_only: true` over HTTP — system-only writes.

Constraints, in a numbered migration of this module's own sequence (the events partial-unique
family):

```sql
-- one row per (session, agent)
CREATE UNIQUE INDEX member_history_agent_uq
  ON livechat.member_histories (session_id, operator_user_id)
  WHERE persona = 'agent' AND operator_user_id IS NOT NULL;
-- one row per (session, visitor)
CREATE UNIQUE INDEX member_history_visitor_uq
  ON livechat.member_histories (session_id, visitor_key)
  WHERE persona = 'visitor' AND visitor_key IS NOT NULL;
-- one row per (session, bot)
CREATE UNIQUE INDEX member_history_bot_uq
  ON livechat.member_histories (session_id, chatbot_script_id)
  WHERE persona = 'bot' AND chatbot_script_id IS NOT NULL;
-- the persona trichotomy, STRICT: each persona binds exactly its identity column
ALTER TABLE livechat.member_histories ADD CONSTRAINT member_history_persona_check CHECK (
  (persona = 'agent'  AND operator_user_id IS NOT NULL AND visitor_key IS NULL AND chatbot_script_id IS NULL)
  OR (persona = 'visitor' AND visitor_key IS NOT NULL AND operator_user_id IS NULL AND chatbot_script_id IS NULL)
  OR (persona = 'bot'    AND chatbot_script_id IS NOT NULL AND operator_user_id IS NULL AND visitor_key IS NULL)
);
```

Upstream's XOR banned only both-set; both-NULL rows passed. The port bans both-NULL too — the
trichotomy is total (declared tightening; a ledger row without a persona identity is meaningless
here). **Rejoin re-points, never duplicates**: joins are `INSERT ... ON CONFLICT (<persona unique
index>) DO UPDATE SET left_at = NULL, joined_at = now()` — upstream's member_id rebind translated
to the (session, persona-identity) grain. Persona is frozen at create (never rewritten). The
escalation derive (`>1 agent row`), rating attribution (the agent/bot row, §7.7), once-only
response time, and per-participant message counts all key off these rows.

## 6. The chatbot pointer state machine (CHATBOT, LC-10/LC-10b/LC-R8/R9/R13)

### 6.1 Shape

The whole runtime is `livechat.sessions.chatbot_current_step_id` (a logical uuid pointer to a
step — no SQL FK, the cohort DSL's own `@exclude_from_foreign_key_check` treatment, so step
deletion is refused at the verb rather than cascading SQL) **plus** `livechat.chatbot_messages`
rows. The script/step/answer trio is the static graph; the message table is the per-session
execution log. Nothing else carries bot state.

### 6.2 Step vocabulary — CLOSED at the seven community types

`livechat_step_type`: `text | question_selection | question_email | question_phone |
forward_operator | free_input_single | free_input_multi`. There are no `create_lead` /
`create_ticket` arms and no reserved seam for them (the docstring-promised crm/helpdesk bridges
are absent upstream; the reserved-but-absent posture is refused). Lead capture stays
message-mining: first email + first phone from sanitized answers, surfaced through the report +
the invite/visitor views — no partner materialization in this module (the host's lead modules own
that, via a seam if ever composed).

### 6.3 Routing (forward-only, verbatim semantics)

`fetch_next_step(current, selected_answers)`: same script, `sequence >` current's, and the
**no-triggering step wins first**, else per-step AND / within-step OR over triggering answers
(`livechat.chatbot_step_triggers` rows: `target_step_id` + `answer_id`, `UNIQUE(target_step_id,
answer_id)`). Empty result = script done. The graph is kept forward-only at SAVE time — a trigger
whose target step's sequence is ≤ the answering step's sequence is a typed 422 refusal (upstream
pruned backwards edges with a stored-EDITABLE compute; the port refuses them at the verb). The
visitor answer verb is also race-guarded: answering a step that is no longer the pointer is
`422 livechat_step_not_current`.

### 6.4 Execution invariants

- **Welcome steps post lazily**: the leading run of `text` steps materializes on the visitor's
  FIRST interaction (message or answer). The open verb returns a derived *preview* of the pending
  step (so the widget renders the greeting) without minting any rows — this also keeps the open
  route from being a message-row amplifier (LC-7). Posting steps the pointer per iteration BEFORE
  the carrier write, so every observer sees consistent pointer state (upstream's subtle coupling,
  kept as a declared invariant).
- **One chokepoint, four duties** (LC-R13 relocated from mail's hook to the module's message-post
  service): `message_count++` on the author's ledger row; `response_time_secs` written once (only
  while NULL) for the agent row; an agent post clears `no_answer`; a visitor post flips
  `waiting → in_progress`; and the chatbot-message factory runs while the pointer is set and no
  agent has joined. Every message lands through this one service — visitors, operators, and the
  bot alike.
- **Input validation has ONE contract** (upstream's twin ValidationError-vs-retry-message paths
  are refused): `question_email` / `question_phone` inputs that fail normalization are
  `422 livechat_input_invalid`; selection answers not among the step's options are
  `422 livechat_answer_invalid`.
- **Sanitized storage** (LC-10b fence): free-text/email/phone answers are stored as sanitized
  plain text in `chatbot_messages.visitor_answer` — raw HTML is never stored anywhere.
- **The forward handoff** (LC-R9) is an explicit verb (chatbot-triggered or operator-forced):
  run the ladder (a self-pick counts as nobody); on success — set the session title
  (`"<visitor label> / <operator display name>"`), leave the bot ledger row (`left_at`),
  reset failure to `no_answer`, assign the operator (§2.3), tag the session with the step's
  expertise names, and continue the script after the forward step; on nobody — write `no_agent`
  and CONTINUE the script (the ask-email-then-hand-off shape survives). Once a human owns the
  session, forward steps cannot re-fire (the answer verb refuses them typed). Each pointer
  advance is audited `ChatbotStepReached`.
- **Bot-only completion**: the router hitting last-step on a session with no agent closes the
  session (`bot_completed`, audited).

## 7. Schema inventory (schema `livechat`)

All tables carry `company_id uuid NOT NULL` + the standard `id/company_id/metadata` shape; every
table gets the RLS fence of §12.1. Entities in `schema/models/*.model.yaml` (SSoT); hand
constraints the DSL cannot express land in the numbered hardening migration (the events pattern).

**Config side (the "core nine" upstream models, translated):**

1. `livechat.channels` — the routing aggregate: `name`, `website_id?` (the bridge binding — a
   channel bound to one website; §10), `button_text?`, `welcome_message?`,
   `max_sessions_mode ('unlimited'|'limited')`, `max_sessions` (CHECK > 0; only `limited`
   channels count), `block_assignment_during_call`, `review_link?` (validated http(s)+netloc at
   the verb), `is_active`. The four raw widget color strings are dropped (widget theming is the
   webapp's concern) — declared delta.
2. `livechat.channel_members` — `(channel_id, user_id)` UNIQUE; membership IS operator-ness
   (upstream's group), so the removal cascade is ONE verb ("leave all channels", §9) instead of
   the duplicated res.users/res.groups writes (LC-12). `user_id` is a logical uuid into the
   host's identity store (no FK, no crate edge — the events partner-ref posture).
3. `livechat.operator_profiles` — `(company_id, user_id)` UNIQUE; `display_name?` (the
   visitor-facing alias — the ONLY operator field the public projection ever exposes, which is
   the LC-13b company-suffix fence by construction), `languages text[]` (BCP-47),
   `last_heartbeat_at?`, `last_assigned_at?` (the buffer + tiebreak anchor). Upstream's two-layer
   settings proxy and its sudo privilege bridge are refused (LC-13): preferences are module-owned
   columns behind the self/officer PUT verb.
4. `livechat.expertise_tags` — `name`; `UNIQUE(company_id, lower(name))` (hardening expression
   unique — canonical, non-translated, case-insensitive; LC-14's translated-JSONB unique and the
   case-sensitive tag namespace do not carry).
5. `livechat.operator_expertise` — join `(operator_profile_id, expertise_tag_id)` UNIQUE.
6. `livechat.channel_rules` — `channel_id` (required — orphan rules are not representable here,
   declared delta), `regex_url` (validated at save: malformed → `422 livechat_rule_regex_invalid`;
   EMPTY string refused — upstream's empty-regex-matches-everything is gone; an operator who
   wants match-all writes `.*` explicitly), `action ('display_button'|'display_button_and_text'|
   'auto_popup'|'hide_button')`, `auto_popup_timer`, `chatbot_script_id?` (inactive/stepless
   scripts skipped at match), `chatbot_enabled_condition ('always'|'only_if_no_operator'|
   'only_if_operator')`, `country_codes text[]`, `sequence`. Matching: two-pass (country-specific
   rules first, then countryless), regex match in Rust against the request Referer — the Referer
   is display config ONLY, never an authorization input. When no rule matches, the availability
   answer is `available: false` (upstream silently omitted the key; the port answers explicitly).
7. `livechat.conversation_tags` — `name`; `UNIQUE(company_id, lower(name))` ('Bug'/'bug' cannot
   coexist). Deleting a tag is a plain FK cascade off `session_tags` (LC-19: the sudo pre-delete
   display-name resync hack has no equivalent to keep — there are no denormalized display names
   to resync). The random kanban color default is dropped.
8. `livechat.session_tags` — join `(session_id, tag_id)` UNIQUE.
9. `livechat.ratings` — see §7.7.

**Runtime side:**

- `livechat.sessions` — §4/§6 columns: `channel_id` (FK), `title?`, `status`,
  `failure`, `outcome`, `close_reason?`, `operator_user_id?` (logical uuid),
  `chatbot_current_step_id?` (logical pointer), `expertise_names text[]?`, bridge columns
  `website_visitor_id? / visitor_country_code? / visitor_timezone? / is_pending_request`
  (§10), `visitor_language?`, `message_count`, `first_response_at?`, `last_interest_at`,
  `last_visitor_message_at? / last_operator_message_at?`, `closed_at?`, `is_test`,
  `error_detail? @max(500)` (the loud parking lot), `metadata`. Indexes: partials on open
  sessions, `(channel_id, last_interest_at)`, `(operator_user_id) WHERE closed_at IS NULL`.
- `livechat.member_histories` — §5.
- Chatbot family: `livechat.chatbot_scripts` (`title`, `is_active`; the archived bot partner is
  dropped — the bot is a script, its display name the title); `livechat.chatbot_steps`
  (`chatbot_script_id` FK restrict, `sequence` UNIQUE per script, `step_type`, `message`,
  `expertise_tag_ids uuid[]?` on forward steps); `livechat.chatbot_answers`
  (`question_step_id` FK, `sequence`, `label`, `redirect_url?` — validated; an external redirect
  ends the script at the router); `livechat.chatbot_step_triggers` (forward-only at save);
  `livechat.chatbot_messages` (`session_id` FK, `step_id?` FK, `carrier_message_id?` with a
  partial UNIQUE WHERE NOT NULL, `selected_answer_id?`, `visitor_answer?` sanitized, `created_at`;
  `read_only: true`).
- `livechat.ratings` (LC-8 hardened): `session_id` **UNIQUE** (one rating per session — the DB
  constraint that replaces read-then-create; repeat submits are `409
  livechat_rating_already_submitted`, audited — upstream's silent overwrite is refused),
  `value smallint CHECK (value IN (1,5,10))` (the three-emoji scale, validated at the verb),
  `rated_persona ('agent'|'bot')` + `operator_user_id?` + `chatbot_script_id?` — attribution
  bound to the ledger's agent/bot row of the session's operator at rating time (the ONE answer;
  upstream's `channel_partner_ids[0]` vs `livechat_operator_id` disagreement is resolved to the
  operator path), `comment?`, `created_at`.
- `livechat.livechat_audit_log` — `event, actor, subject_type?, subject_id?, detail jsonb?,
  created_at` (the website/events `record_audit` shape; audit rows are the durable trace).
- `livechat.session_report` — **SQL view, `WITH (security_invoker = true)`** (the fence flows
  through the view; probe-asserted under the fenced role). One row per session: duration bounded
  (`closed_at − opened_at`, closed sessions only — no `COALESCE(end, NOW())` drift; open sessions
  report `is_open` with no duration), `time_to_answer` from the once-only `first_response_at`
  (the 4-branch CASE has no port to carry it), `session_outcome` (stored), `escalated` (derived:
  agent rows > 1), `handled_by_agent / handled_by_bot` (BOOL_OR over the ledger), rating value +
  text (1→unhappy, 5→neutral, 10→happy; absent rating = NULL, never a stored 0), and the chatbot
  answer path as a deterministic label join off `chatbot_messages.selected_answer_id` (no
  `jsonb_each_text ... LIMIT 1`). The view itself carries NO date WHERE; the only access verb
  requires explicit `from`/`to` bounds (422 without; window capped at 366 days) — the unbounded
  every-session-ever scan does not port (LC-15). Week-day grouping uses an explicit
  `week_start` parameter, not the locale.

## 8. The public surface (LC-BOUNDARY)

`pub fn livechat_public_routes(state) -> Router` — exported bare (the host merges it into
`/api/v1/livechat` with NO tenant layers); exhaustive allowlist in the doc comment; the module's
own gates are the fence. State: `LivechatPublicState::compose(pool) / from_env() /
compose_with_trusted_proxy(...)` (probe entry avoids env).

### 8.1 Capability-gated session routes

Every session-scoped route addresses the session by capability token in the path
(`/public/sessions/:capability`). The ENTIRE refusal family — malformed token, wrong version,
wrong purpose, bad signature, expired, session missing, cross-session token — collapses to ONE
uniform `404 livechat_session_not_found` (no oracle; the events gate-404 idiom).

| Route | Verb | Notes |
|---|---|---|
| `/public/availability` | GET | The website button answer (§10, WLC-11). Resolves Host → website via the bridge port (miss = typed 404, no fallback), finds the website's bound active channel, runs the two-pass rule match against the Referer, answers `{available, mode: operators|chatbot|both, button_text, welcome_preview, pending_invite?}`. Availability = a chatbot script matched (available 24/7) OR ≥1 operator passing the capacity gate — the same ONE-window pool as assignment, read-only. |
| `/public/sessions` | POST | Open/resume. Throttled per-IP AND per-identity. A presented capability that verifies ⇒ continuity (resume its session if open; else mint a new session under the same `visitor_key`); a presented capability that fails ⇒ `401 livechat_guest_token_invalid`, ZERO rows minted; no token ⇒ first visit (bridge mints the website visitor; livechat binds `visitor_key`). Creates the session + visitor ledger row; NO message rows (welcome is a preview). Runs the bot-first routing (§2.2). Returns the guest capability + public session view. |
| `/public/sessions/:capability` | GET | Session state (status, failure-opaque state, pointer preview, operator projection = display name only). |
| `/public/sessions/:capability/messages` | GET | Cursor poll (`?after=<carrier_message_id>`); returns messages + a state digest. |
| `/public/sessions/:capability/messages` | POST | Visitor message — through the one chokepoint (§6.4); heartbeat piggybacks `track_visit` (WLC-7: chat activity IS the visitor heartbeat). |
| `/public/sessions/:capability/answers` | POST | Chatbot answer / free input (§6.4 validation contracts). |
| `/public/sessions/:capability/close` | POST | Visitor leave (idempotent; audited). |
| `/public/sessions/:capability/rating` | POST | 1/5/10, validated; one per session by constraint (§7.7). The rating PROMPT at close rides the notifier port (non-blocking seam). |
| `/public/invites/:capability/accept` | POST | Accept an operator-initiated invite (the capability is the `livechat-invite-accept` mint from the availability answer) — binds the visitor ledger row, flips `is_pending_request` off, returns the guest token (§10). |

Nine paths, no more. No CORS-permissive arm, no `csrf` special cases, no file uploads at this pin.

### 8.2 Throttle posture (per-IP fixed windows)

`FixedWindows` copied from events (`Mutex<HashMap<key,(window,count)>>`, `allow(key, max,
window_secs)`; poison-lock fails CLOSED via `into_inner`), wired per-declaration consts
(`LivechatRatePolicy`), per-IP and per-identity arms — identity buckets keyed on the visitor
digest, never the IP. `429 livechat_throttled` + `Retry-After` header + `retry_after_secs` body.
Client-IP: `LIVECHAT_TRUSTED_PROXY` tolerant-truth env, rightmost `X-Forwarded-For` hop when
armed, bare IP (never ip:port) otherwise; IP feeds rate shaping and digests only, never
authorization. Declared windows (consts, documented): open 6/h (ip+identity); message 30/min
identity, 60/min ip; poll 120/min; availability 240/min ip; answers 30/min identity; rating 3/h.

## 9. The admin surface

`pub fn livechat_admin_routes(state) -> Router` + `pub struct LivechatActor(pub Uuid)` extension;
NO self-gating — the host applies company_auth → `ModuleWriteGate("livechat")` → the actor bridge
(innermost-1). Every verb and every refusal audited. Config verbs ride generated CRUD where the
shape is plain (channels/tags/expertise/scripts listing) and hand verbs where a law applies:

- **Channels**: list/create/read/patch (typed whitelist: name, button_text, welcome_message,
  max_sessions pair, block_assignment_during_call, review_link, is_active); operators add/remove
  — BOTH gated identically (LC-21's asymmetry closed); `POST /admin/channels/from-website`
  (the wizard: creates and binds EVERY channel it creates to the website — WLC-10b — and creates
  NO bot rule silently); `POST /admin/channels/leave-all` (the operator-removal cascade as one
  declared verb — LC-12).
- **Operator profile**: `GET/PUT /admin/operator-profiles/:user_id` (self or officer;
  display_name + languages), `POST /admin/operator-profiles/heartbeat` (presence).
- **Expertise tags / conversation tags / channel rules**: CRUD with the canonical-unique and
  regex-validation laws (typed 409 / 422).
- **Chatbot scripts/steps/answers/triggers**: CRUD with the seven-type closure,
  question-steps-need-answers validation, and the forward-only trigger check at save (422).
  `POST /admin/chatbot-scripts/:id/test-sessions` — the test verb (WLC-9): operator-gated, opens
  a REAL session flagged `is_test` driving the real surface (no phantom `persisted=False`
  thread, no unpin trick — LC-22 refused); test sessions are excluded from availability counts
  and the report.
- **Sessions**: `GET /admin/sessions` (filters: open / need_help / mine / channel; closed windows
  REQUIRE explicit date bounds); `GET /admin/sessions/:id`; `POST .../take` (serialized
  first-wins; loser gets `409 livechat_operator_busy`); `POST .../close {reason}`;
  `POST .../need-help` + `POST .../resolve-need-help` (audited HelpRequested/HelpResolved);
  `POST .../forward` (operator-forced handoff, §6.4); `POST .../messages` + `GET .../messages`
  (the operator side of the chokepoint); `POST .../chatbot/restart {reset_failure}` (§4);
  `POST .../tags {tag_ids}`; `POST .../transcript {email?}` (the transcript-mailer seam — parks
  loudly when uncomposed).
- **Website chat requests** (§10): `POST /admin/website-chat-requests {website_id,
  website_visitor_id}` — single-visitor by design (a batch is repeated audited calls; each row
  binds its own country/timezone and its own operator ledger row — WLC-2b's loop leakage cannot
  reappear); `POST /admin/sessions/:id/cancel-request` (both sides notified through the notifier;
  audited).
- **Report**: `GET /admin/report/session-summary?from&to` — required bounds (422 without, window
  cap 366 days); carries the conversation count, average time-to-answer, duration percentiles,
  outcome mix, and the windowed happiness KPI (LC-16's windowed computation ports; the
  install-time digest flip is refused — installs are inert, no seeds ship: no YourWebsite.com
  channel, no Welcome Bot, no /contactus auto-popup rule; operators declare their own).

## 10. The website bridge overlays (WLC-2..12, inside this module)

The cycle-24 bridge lands as behavior + **five columns over this module's own tables** (upstream's
five columns across three host tables, re-homed — zero website-side schema changes, no second
crate): `channels.website_id`, `sessions.website_visitor_id`, `sessions.is_pending_request`,
`sessions.visitor_country_code`, `sessions.visitor_timezone`. "Speaking with" (WLC-12) is a
derived flag over open sessions (report/list), deliberately not stored — installs inert, nothing
back-filled.

- **WLC-2** The operator-initiated invite: availability check-then-act replaced by the declared
  verb (per-visitor, per-website binding; the operator self-add is a ledger row per session, not
  a channel-membership side effect). The pending session is created `is_pending_request` with the
  visitor's own country/timezone.
- **WLC-3/WLC-4** Bounded lifecycle, both sides visible: the pending invite is INVISIBLE to the
  visitor until the operator's first message (`has_message` gate) — it then surfaces in the
  availability answer as `pending_invite` with a short-TTL `livechat-invite-accept` capability.
  A visitor opening their own session CANCELS their pending invite (explicit cancel verb
  semantics, "visitor wins", both sides notified, audited `InviteCancelled`); accept, decline,
  and expiry all clear the flag with audits.
- **WLC-5** No untraced destroy: the unpin hard-delete and the 1-hour message-less unlink are
  refused; invite expiry is the sweep's audited close (`request_declined`/`expired`), rows
  survive.
- **WLC-6** The five columns above.
- **WLC-7** Chat activity is the heartbeat: every visitor/bot message piggybacks
  `track_visit` through the bridge port.
- **WLC-8** Continuity: the host composes website's `VisitorEngine::merge_visitor` with
  livechat's `pub async fn relink_website_visitor(...)` (rebinds `sessions.website_visitor_id`,
  audits `VisitorRelinked`) — sessions survive cookie loss and the visitor→partner merge.
- **WLC-9** The test verb (§9) — operator-gated, real surface, harvest fallback reads the
  visitor record through the bridge.
- **WLC-10** Every created channel binds; auto-config (a Welcome-Bot rule) is explicit operator
  action, never wizard-silent; no install-time popup rule ships.
- **WLC-11** The availability decision is server-side per request under the public surface
  (§8.1); the page-cache interplay does not carry (the host's page cache simply serves pages;
  the widget asks `/public/availability` live).
- **WLC-12** Derived, above.

The bridge port: `trait LivechatWebsiteBridge { resolve_website_by_host; track_visit;
visitor_key(session_facts) -> Option<digest> }` + `RefusingLivechatWebsiteBridge` →
`503 livechat_website_bridge_not_composed` (the open and availability routes require it; probes
stub it). The host adapter wraps backbone-website's `PgWebsiteSurface`/`VisitorEngine` — real
adapters install in the host and nowhere else. (Considered and rejected: a direct Cargo edge on
backbone-website to name its `WebsiteSurface` trait — website exports that trait as the declared
downstream contract, but livechat needs three methods of it, and the port+adapter law keeps the
refusing default typed in `LivechatError` and the graph uncoupled.)

## 11. Ports (carriers that park loudly)

| Port | Default | Posture |
|---|---|---|
| `LivechatWebsiteBridge` | `RefusingLivechatWebsiteBridge` → 503 | blocking (open/availability refuse typed) |
| `LivechatMailCarrier` (post/fetch/remove messages) | `RefusingMailCarrier` → 503 | blocking for message verbs; chatbot steps park on `sessions.error_detail` + audit, retried by sweep/next interaction |
| `LivechatTranscriptMailer` | `RefusingTranscriptMailer` → 503 | blocking at the transcript verb |
| `LivechatNotifier` (cancel/rating-prompt notices) | `UnwiredNotifier` → WARN + `notified=false` | NON-blocking — the write is never refused by the port (the website notifier posture) |
| `LivechatDigestQueue` | `RefusingDigestQueue` → 503 | blocking at the digest verb; KPI computation is a read and never needs it |
| `LivechatRtcCarrier` | `RefusingRtcCarrier` | mounted nowhere at this pin; exists to carry the join-not-start law when a transport lands |

## 12. Repository layer and the RLS LAW

### 12.1 The fence

Every table: `ENABLE ROW LEVEL SECURITY; FORCE ROW LEVEL SECURITY;` + the NULLIF policy
(copied per table from events' `20260426220018_enable_company_rls.up.sql`):

```sql
CREATE POLICY <table>_company_isolation ON livechat.<table>
    FOR ALL
    USING      (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid OR company_id IS NULL)
    WITH CHECK (company_id = NULLIF(current_setting('app.company_id', true), '')::uuid OR company_id IS NULL);
```

### 12.2 The law on every path

- Every repository transaction opens `let mut tx = pool.begin().await?;
  company_scope::bind_current_company(&mut tx).await?;` — no exceptions.
- Direct-pool statements go through the `backbone_orm::company_scope::*_scoped` helpers.
- Public routes bind the company scope of the website resolved by the bridge (request scope) —
  the fence is the fence on every surface, public included.
- Jobs/sweeps use `bind_company_on` / per-company `with_company_scope` off the active-company
  enumeration (`organization.companies WHERE status='active' AND metadata->>'deleted_at' IS
  NULL`), driven by the host jobs loop.
- Hand repositories (services hold no raw sqlx): `selection_repository` (the ladder statement),
  `session_command_repository` (open/close/take/forward/restart/message chokepoint/outcome
  recompute), `member_history_repository` (persona upserts/rebinds), `chatbot_command_repository`
  (script graph CRUD + pointer routing), `rating_repository`, `website_request_repository`
  (invites), `report_repository` (bounded window reads + the view), `sweep_repository` (SKIP
  LOCKED batched closes with per-row audit). Generated `<entity>_repository.rs` files carry plain
  CRUD for config entities.

### 12.3 Sweep shape

Bounded `UPDATE ... WHERE id IN (SELECT ... FOR UPDATE SKIP LOCKED LIMIT $batch)` set-based
closes, idempotent on `closed_at IS NULL`, per-row audits bulk-inserted from `RETURNING`; batch
const 200. No leases needed (single-statement closes); no walker holds rows across awaits.

## 13. Migrations (the module's own sequence)

`<YYYYMMDDHHMMSS>_<snake>.{up,down}.sql`, sqlx 14-digit prefix, starting at the synthetic base —
final numbers come from generation; the sequence below is the dependency-ordered intent. Known
hazard: the pinned generator numbers a batch alphabetically by table name, which can strand FKs
ahead of their referenced table (the events defect) — any strand is handled by the events freeze
pattern (hand-remove + re-add in the hardening tail, both files user_owned-listed with the defect
note and the unlist condition). Cross-model references (`operator_user_id`, `website_*`,
`chatbot_current_step_id`, the ledger's bot arm, ratings' rated-row snapshot) are logical uuids
(`@exclude_from_foreign_key_check`) — the cohort DSL's own treatment — so the only strand-risk
FK is `channel_rules.chatbot_script_id → chatbot_scripts`.

```
20260426220000_create_enums            livechat_persona, _session_status, _failure,
                                       _session_outcome, _close_reason, _rule_action,
                                       _chatbot_condition, _step_type (the seven)
20260426220001_create_channel_table
20260426220002_create_channel_member_table
20260426220003_create_operator_profile_table
20260426220004_create_expertise_tag_table
20260426220005_create_operator_expertise_table
20260426220006_create_channel_rule_table
20260426220007_create_session_table
20260426220008_create_member_history_table
20260426220009_create_chatbot_script_table
20260426220010_create_chatbot_step_table
20260426220011_create_chatbot_answer_table
20260426220012_create_chatbot_step_trigger_table
20260426220013_create_chatbot_message_table
20260426220014_create_rating_table
20260426220015_create_conversation_tag_table
20260426220016_create_session_tag_table
20260426220017_create_livechat_audit_log_table
20260426220018_enable_company_rls         (every table; §12.1)
20260426220019_create_session_report_view (hand; security_invoker; user_owned-listed)
20260426220020_add_audit_triggers         (hand; up-only, the events shape)
20260903000030_livechat_hardening_constraints
                                         (hand; the ledger's 3 partial uniques + strict
                                          trichotomy CHECK — §5; lower(name) expression
                                          uniques; max_sessions > 0; ended⇒no-status CHECK;
                                          chatbot_messages partial carrier unique; any
                                          stranded-FK re-homes; idempotent IF NOT EXISTS /
                                          DO $$ conname guards; NO generator header)
```

Hand files carry no "Generated by metaphor-schema" header; everything hand-written is under
`user_owned` (§15).

## 14. Probe suite (fail-hard, scratch 5433)

`tests/livechat_probes.rs` (doc header names every gate) + `tests/probes/` (one module per
behavior family) + `tests/probes/common/mod.rs` (the events TestDb harness copied: one disposable
`livechat_<marker>_<hex>` DB per probe on `SCRATCH_ADMIN_URL = postgres://postgres:postgres@
127.0.0.1:5433/postgres`, env override `LIVECHAT_TEST_ADMIN_URL`, pre-drop + CREATE + sorted
`.up.sql` runner + `dispose()` + Drop leak-guard WITH (FORCE); "vacuous skip is a failure" —
`skipped()` panics; unreachable scratch panics; NEVER 5432). Common also ships port stubs
(`RefusingMailCarrier`, `RecordingMailCarrier`, `StubWebsiteBridge`, `UnwiredNotifier`,
`RefusingTranscriptMailer`).

| Probe | Gate |
|---|---|
| `fenced_runtime` | The events probe copied in shape: role `livechat_probe_app` (LOGIN, NOSUPERUSER NOBYPASSRLS, USAGE + DML on schema `livechat`), stale `livechat_fenced_*` DBs dropped FIRST, then the seven assertions (unscoped write → typed Database containing "row-level security"; forged cross-tenant write refused; scoped write passes; scoped list own-rows-only; cross-company find → `SessionNotFound`; no scope → zero rows; other company intact) **plus** the view fence: under the fenced role, `session_report` reads only the scoped company (security_invoker holds) |
| `ladder_determinism` | capacity excludes over-cap/offline; buffer excludes the just-assigned on EVERY arm (previous-operator and no-rung cases both asserted — the upstream bypass scenarios); rung order 0..9; same fixture → same pick, twice (no randomness); the ONE window: a session at 31 minutes counts closed in BOTH the availability answer and the assignment pool |
| `outcome_per_record` | §3's multi-record recompute scenario |
| `ledger_uniques` | duplicate agent/visitor/bot rows refused by the DB; both-NULL persona refused (strict trichotomy); rejoin rebinds (one row survives with `left_at` cleared) |
| `chatbot_pointer` | seven-type closure (an eighth arm is unrepresentable); forward-only trigger refusal; no-trigger-default-next; welcome laziness (open mints zero message rows; first interaction materializes); handoff semantics (title/bot-unfollow/failure reset/no_agent-continue); restart reset-vs-preserve explicit both ways; sanitized answer storage |
| `boundary_gate` | the uniform 404 family (bad sig / expired / malformed / cross-session all ONE code); wrong-token-at-open → 401 + ZERO rows created; secret unconfigured → 503, no mint; throttles arm |
| `throttle_windows` | 429 + Retry-After; window roll; identity bucket keyed on digest not IP |
| `rating_once` | value validation 422; UNIQUE refusal typed 409; attribution = the operator path |
| `website_bridge_overlays` | refusing bridge parks loudly (availability/open 503); stubbed bridge drives: invite per-visitor binding (no loop leakage — N invites, N distinct country/tz), invisible-until-operator-message, visitor-open cancels pending + notifies both sides, expiry closes with audit and rows survive (no hard delete), wizard binds every channel, merge relink rebinds `website_visitor_id` |
| `sweep_lifecycle` | idle close audited with reason; invite expiry; SKIP LOCKED batch bounds; no untraced deletes anywhere |
| `audit_trail` | critical events on the trail (`OperatorAssigned` with rung, `SessionClosed`, `SessionRated`, `SessionReopened`, `ChatbotStepReached`, `ChatbotRestarted`, `HelpRequested`, `InviteCreated/Cancelled/Delivered`, `VisitorRelinked`) and every refusal audited |

Inline `#[cfg(test)]` unit tests for pure logic: capability mint/verify (constant-time arms,
TTL), digest/token helpers, rung ranking, regex validation.

## 15. `metaphor.codegen.yaml` — `user_owned` (declared BEFORE anything lands)

- `src/lib.rs` — ⚠ GENERATOR-DEFECT FREEZE (supersedes the marker plan below for this one
  file): the builder constructor's `LivechatModule { ... }` initializer carries a
  hand-added `livechat_pool: db_pool.clone(),` line the generator template does not emit;
  a `--force` regen rebuilds the constructor body and drops it, leaving the crate at E0063
  (observed live; the same constructor-body marker unreliability the blog module froze
  against). Frozen at its current state; unlist only after the generator honors hand edits
  inside emitted constructor bodies deterministically.
- `src/application/service/livechat_error.rs`, `capability.rs`, `throttle.rs`,
  `selection_service.rs`, `session_service.rs`, `chatbot_service.rs`,
  `availability_service.rs`, `rating_service.rs`, `report_service.rs`, `sweep_service.rs`,
  `website_request_service.rs`
- ports: `src/application/service/{website_bridge,mail_port,transcript_port,notifier_port,
  digest_port,rtc_port}.rs`
- hand repos: `src/infrastructure/persistence/{selection,session_command,member_history,
  chatbot_command,rating,website_request,report,sweep}_repository.rs`
- `src/presentation/http/public_routes.rs`, `src/presentation/http/admin_routes.rs`
- `migrations/*hardening*` + explicit entries for `20260426220019_create_session_report_view.*`
  and `20260426220020_add_audit_triggers.up.sql` (+ any stranded-FK freeze files, each with the
  defect note and unlist condition)
- `tests/**`, `README.md`, `SPEC.md`, `docs/**`

Re-exports ride `// <<< CUSTOM ... // END CUSTOM` markers in
`application/service/mod.rs` and `exports/services.rs` (lib.rs is the frozen exception
above). `lib.rs`: `LivechatModule::builder()
.with_database(pool).build()`, `all_crud_routes()`, `readonly_routes()`.

## 16. Typed error vocabulary

ONE `#[derive(Debug, thiserror::Error)] pub enum LivechatError`, stable `code() -> &'static str`
wire strings, private `status()`, one `IntoResponse` emitting `{"error":{"code","message"}}`
(+ `Retry-After` on throttle); `From<sqlx::Error> → Database`, `From<anyhow::Error> → Internal`;
`pub type LivechatResult<T>`. Internal shapes never leak text (tracing + generic body). Status
classes copied from events: 404 not-found family / 401 credential / 409 busy-conflict / 422
domain+validation / 429 throttle / 503 unconfigured / 500 infra.

| code | status | the arm |
|---|---|---|
| `livechat_session_not_found` | 404 | the uniform capability/session family (no oracle) |
| `livechat_channel_not_found` | 404 | channel/website misses on gated verbs |
| `livechat_guest_token_invalid` | 401 | presented token at open failed — no mint |
| `livechat_throttled` | 429 | + `retry_after_secs` |
| `livechat_capability_secret_not_configured` | 503 | never mint under an empty key |
| `livechat_website_bridge_not_composed` | 503 | the bridge refusing default |
| `livechat_carrier_not_composed` | 503 | mail carrier refusing (messages/chatbot park) |
| `livechat_transcript_not_composed` | 503 | transcript mailer refusing |
| `livechat_operator_busy` | 409 | take/assign first-wins loser |
| `livechat_rating_already_submitted` | 409 | the UNIQUE made typed |
| `livechat_tag_name_conflict` | 409 | canonical tag/expertise/script names |
| `livechat_rule_regex_invalid` | 422 | save-time regex validation |
| `livechat_answer_invalid` | 422 | selection outside the step's options |
| `livechat_input_invalid` | 422 | email/phone/free-input — the ONE contract |
| `livechat_step_not_current` | 422 | pointer race on the answer verb |
| `livechat_validation` | 422 | the general family |
| `livechat_database` / `livechat_internal` | 500 | infra, message-safe |

## 17. Host uptake contract (orchestrator-serialized; workers never touch)

1. Host `Cargo.toml`: `backbone-livechat = { git = ".../backbone-livechat", tag = "v0.1.0" }`
   (tag pins only); workspace `metaphor.yaml` projects[] entry + `depends_on` append;
   `metaphor sync --update`.
2. `HOST/src/infrastructure/seams/livechat_compose.rs` (the seams glob is already user_owned):
   `WebsiteBridgeAdapter` over `PgWebsiteSurface`; `MailCarrierAdapter` over backbone-mail;
   `TranscriptMailerAdapter`; `NotifierAdapter`; refusing digest/RTC defaults;
   `livechat_actor_bridge` (reads `CompanyContext`, parses `tenant.user_id` → `LivechatActor`,
   unresolvable = typed 403 `livechat_actor_unresolved`); `public_router(pool)` +
   `admin_router(pool)` (boot WARN notes when the secret is unset); cron spawners for the two
   sweeps on the jobs loop, per-company `with_company_scope`.
3. `main.rs` nest, copying the website/events shape verbatim in effect:
   `modules_router.nest("/api/v1/livechat", livechat_gated_admin.merge(livechat_public))` with
   `company_auth` OUTSIDE → `ModuleWriteGate("livechat")` → actor bridge innermost-1.
4. Ops: `LIVECHAT_CAPABILITY_SECRET` (required-to-mint) and `LIVECHAT_TRUSTED_PROXY` (optional)
   declared in the matching `.env.*.example` templates; after dev migrations run
   `apps/serpa-service/scripts/rls_app_role.sql` as owner (new schemas lock out `sherpa_app`
   otherwise); prod twin `deployment/scripts/migrate-with-grants.sh`.

## 18. Census disposition register (traceability appendix)

Audit-cohort IDs (metaphora `docs/odoo/website/live-chat/`) → where this spec disposes them:

| ID | Disposition → section |
|---|---|
| LC-1 / LC-R1 | adapted — the ladder as one ranked statement; GC off the read path → §2 |
| LC-1b / LC-R2 | adapted — buffer inside the pool, every path → §2.1 |
| LC-2 / LC-R3 | adapted — ONE window const, one shared predicate → §2.1 |
| LC-3 | ported — dual-duty ledger + constraints → §5 |
| LC-4 / LC-R6 | adapted — declared public surface; token-scoped reads, no search-visible⇒member → §1.3, §8 |
| LC-5 / LC-R7 (split) | refused — the CORS mirror + mint-on-wrong-token; adapted — embed re-keys on Tier A → §1.3, §8.1 |
| LC-6 | refused — join-not-start by construction, no shadow controller → §1.3 |
| LC-7 | adapted — open route throttled per-IP/per-identity; zero message rows at open → §8 |
| LC-8 / LC-R5 | adapted — UNIQUE constraint, operator-path attribution, validated value → §7.7 |
| LC-9 / LC-R4 | adapted — declared state, explicit audited endings, scheduled sweeps → §4, §2.4 |
| LC-9b / LC-R10 | adapted — restart resets-or-preserves explicitly → §4, §6.4 |
| LC-10 / LC-R8/R9/R13 | ported — pointer machine, seven-type closure, one chokepoint → §6 |
| LC-10b | fence — sanitized storage only → §6.4 |
| LC-11 / LC-R15 | refused — per-record derive law → §3 |
| LC-12 / LC-R12 | adapted — one declared verb, not the duplicated cascade → §7.1 (channels), §9 |
| LC-13 | adapted — module-owned columns/verbs; the sudo proxy refused → §7.1 |
| LC-13b | fence — public projection is display_name only → §7.1, §8.1 |
| LC-14 | adapted — canonical non-translated case-insensitive uniques → §7.1, §7 (tags) |
| LC-15 | adapted — bounded windows, no NOW() drift, deterministic fallbacks → §7 (view), §9 |
| LC-16 | split — windowed KPIs port; install flip refused (inert install) → §9 |
| LC-17 / LC-R11 | adapted — derive stays; need_help = explicit audited verbs; serialized take → §4, §9 |
| LC-18 | excluded (unpopulated) |
| LC-19 | adapted — plain FK cascade; the resync hack has no port → §7 (tags) |
| LC-20 | refused — dead data, reserved seams, random color all dropped → §1.3, §7, §9 |
| LC-21 | adapted — both join/quit gated; asymmetry closed → §9 |
| LC-22 | refused — no phantom threads; test verb drives the real surface → §9 |
| LC-R14 | adapted — regex validated at save, empty refused, two-pass match, explicit no-match answer → §7 (rules) |
| WLC-2..5 | adapted — the invite lifecycle (per-visitor binding, cancel-both-sides, visible-after-message, audited expiry, no hard delete) → §10 |
| WLC-6 | ported — the five bridge columns → §10 |
| WLC-7 | ported — message-piggyback heartbeat → §10, §8.1 |
| WLC-8 | ported — relink verb composed with website's merge → §10 |
| WLC-9 | adapted — operator-gated test verb → §9 |
| WLC-10 | adapted — bind every channel; declared auto-config; no shipped popup → §9, §10 |
| WLC-11 | adapted — server-side per-request availability → §8.1 |
| WLC-12 | adapted — derived flag, inert install → §10 |

Declared deltas beyond the census (all deliberate, none silent): rating resubmit refuses (409)
rather than overwrites; rules require a channel; the strict persona trichotomy bans both-NULL;
the four raw widget color strings, the random tag color, the archived bot partner, and
`help_status` on ledger rows (escalation trace lives in the audit trail) do not port; restart is
an admin verb, not a public one; presence is a 60 s heartbeat window (no bus); all seeds refused.

## 19. File tree (the module's specified shape)

```
backbone-livechat/
├── Cargo.toml
├── CLAUDE.md                          # the module-type template
├── README.md                          # orientation + the frozen quality-bar contract
├── SPEC.md                            # this document
├── metaphor.codegen.yaml              # §15, declared before any file lands
├── docs/                              # reserved for later detail notes
├── schema/models/
│   ├── index.model.yaml
│   ├── channel.model.yaml
│   ├── channel_member.model.yaml
│   ├── operator_profile.model.yaml
│   ├── expertise_tag.model.yaml
│   ├── operator_expertise.model.yaml
│   ├── channel_rule.model.yaml
│   ├── session.model.yaml
│   ├── member_history.model.yaml
│   ├── rating.model.yaml
│   ├── conversation_tag.model.yaml
│   ├── session_tag.model.yaml
│   ├── chatbot_script.model.yaml
│   ├── chatbot_step.model.yaml
│   ├── chatbot_answer.model.yaml
│   ├── chatbot_step_trigger.model.yaml
│   ├── chatbot_message.model.yaml
│   └── livechat_audit_log.model.yaml
├── migrations/                        # §13; .up/.down pairs except the up-only triggers
├── src/
│   ├── lib.rs                         # builder + all_crud_routes/readonly_routes + CUSTOM re-exports
│   ├── module.rs                      # generated wiring
│   ├── domain/entity/                 # generated entities
│   ├── application/service/
│   │   ├── livechat_error.rs
│   │   ├── capability.rs              # Tier A, "livechat-capability-v1"
│   │   ├── throttle.rs                # FixedWindows + LivechatRatePolicy
│   │   ├── selection_service.rs       # the ladder
│   │   ├── session_service.rs         # open/close/take/forward/restart/chokepoint/outcome
│   │   ├── chatbot_service.rs         # the pointer machine
│   │   ├── availability_service.rs    # the button answer + rules matching
│   │   ├── rating_service.rs
│   │   ├── report_service.rs
│   │   ├── sweep_service.rs
│   │   ├── website_request_service.rs # the invite lifecycle
│   │   ├── website_bridge.rs          # port + refusing default
│   │   ├── mail_port.rs               # port + refusing default
│   │   ├── transcript_port.rs         # port + refusing default
│   │   ├── notifier_port.rs           # port + unwired (non-blocking) default
│   │   ├── digest_port.rs             # port + refusing default
│   │   └── rtc_port.rs                # port + refusing default (mounted nowhere yet)
│   ├── infrastructure/persistence/    # generated <entity>_repository.rs + the eight hand repos (§12.2)
│   ├── presentation/http/
│   │   ├── public_routes.rs
│   │   └── admin_routes.rs
│   └── exports/services.rs            # CUSTOM re-export block
└── tests/
    ├── livechat_probes.rs
    └── probes/
        ├── common/mod.rs
        ├── fenced_runtime.rs
        ├── ladder_determinism.rs
        ├── outcome_per_record.rs
        ├── ledger_uniques.rs
        ├── chatbot_pointer.rs
        ├── boundary_gate.rs
        ├── throttle_windows.rs
        ├── rating_once.rs
        ├── website_bridge_overlays.rs
        ├── sweep_lifecycle.rs
        └── audit_trail.rs
```
