# backbone-livechat

The livechat domain module: the visitor conversation lifecycle behind
Tier A guest capabilities, the deterministic operator-selection ladder,
the member-history ledger, the chatbot pointer state machine, operator
profiles and expertise, ratings, and the website-chat bridge overlays.
Schema YAML is the single source of truth; the tree is generator-emitted
with hand-owned verbs declared in `metaphor.codegen.yaml`.

## Documentation map

| doc | what it holds |
|---|---|
| `SPEC.md` | the binding contract: models, the ladder, the ledger constraints, the chatbot machine, the port family, error codes, migration list, probe list |
| `metaphor.codegen.yaml` | the regen-safety contract: every hand file, with the reason it is hand-owned |

## Model inventory (18 schema models)

Channels and membership: `channel`, `channel_member`, `channel_rule`.
Chatbot: `chatbot_script`, `chatbot_step`, `chatbot_answer`,
`chatbot_message`, `chatbot_step_trigger`. Conversation: `session`,
`rating`, `conversation_tag`, `session_tag`. Participants:
`member_history`, `operator_profile`, `operator_expertise`,
`expertise_tag`. Trace: `livechat_audit_log`.

One hand DDL object is NOT a model: the `livechat.session_report` view
(security_invoker — the composing service's tenancy fence flows
through the view).

**Gate-order note (deliberate, do not "harmonize"):** while
`LIVECHAT_CAPABILITY_SECRET` is unset, session-open answers the UNIFORM
typed 503 `livechat_capability_secret_not_configured` BEFORE host
resolution — on every host identically. The open verb is the anonymous
WRITE / capability-mint entry; resolving the host first would answer 404
for unbound vs 503 for bound hosts during the unset window and leak the
binding map (an enumeration oracle). backbone-blog deliberately orders
the other way (host resolution first) — its visit verb serves public
reads on bound hosts, so the website-donor idiom fits there and the
fail-closed events-donor idiom fits here. The divergence is the design.

## The verb surface (hand services)

| service | verbs |
|---|---|
| `selection_service` | the ONE set-based ladder: capacity, the 120s anti-burst buffer inside the pool, the one 1800s ongoing window, rungs 0..9, total-order tie-breaks, the audited first-wins assignment write |
| `session_service` | open/resume, the message chokepoint (member counts, first response, interest stamps, ledger deltas), close/reopen, forward, restart, need-help, the per-record outcome derive |
| `chatbot_service` | lazy welcome materialization, the pointer machine over the seven closed step types, answer/free-input contracts, the forward handoff, bot-only completion |
| `availability_service` | the website button answer (host→website→channel, two-pass rule match, operators/chatbot decision, pending-invite surface) |
| `rating_service` | the once-per-session wall, 1/5/10 scale, persona-attributed ratings |
| `report_service` | bounded-window reads over the view (required from/to, 366-day cap, windowed happiness KPI) |
| `sweep_service` | idle-close + invite-expiry passes (SKIP LOCKED batches, per-row audits) |
| `website_request_service` | operator-initiated invites, per-visitor binding, invisible-until-operator-message, cancel, audited expiry, merge relink |

## Host ports (compose-time wiring)

`LivechatWebsiteBridge` (website surface), `LivechatMailCarrier`
(message transport), `LivechatTranscriptMailer` (transcript mail),
`LivechatDigestQueue` (operator digest), `LivechatRtcCarrier` (realtime
— mounted nowhere at this pin). Every blocking port ships a refusing
default that parks loudly — an unwired host is a typed 503 failure,
never a silent skip. `LivechatNotifier` (cancel/rating notices) is the
one non-blocking port: its default WARNs and answers `notified=false`.

## Probes

`tests/probes/` — fail-hard, one disposable scratch database each on
127.0.0.1:5433 (`LIVECHAT_TEST_ADMIN_URL` overrides), migrations
applied in order, claims proven under a NOSUPERUSER NOBYPASSRLS probe
role the suite mints itself. The suite covers ladder determinism, the
per-record outcome derive, ledger uniques + trichotomy, the chatbot
pointer machine, the capability boundary, throttle windows, rating-once,
website overlays, sweep lifecycle, the audit trail, and RLS fencing.
