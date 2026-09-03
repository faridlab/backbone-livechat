# Metaphor Domain Module

> Type: **`module`** — a bounded-context library crate. 4-layer DDD. Schema YAML is the single source of truth; most code is regenerated.
> This file orients Claude. Skills carry depth; load them on demand.

## What this is

A library crate (no `main.rs`) owning the livechat domain: the visitor
conversation lifecycle, the deterministic operator-selection ladder, the
member-history ledger, the chatbot pointer state machine, operator
profiles/expertise, ratings, and the website-chat bridge overlays. Consumed
by `backend-service` hosts (serpa-service). Exposes a `LivechatModule`
struct built via `builder()`. Twelve standard CRUD endpoints per entity are
auto-wired via `BackboneCrudHandler` — the hand-owned verbs (selection,
session commands, chatbot steps, availability, rating, report, sweeps,
website requests) live in the `metaphor.codegen.yaml` `user_owned` list.

## Golden path

```bash
metaphor schema schema validate                 # check schema YAML
metaphor make entity <Name>              # scaffold from schema
metaphor migration generate <name>         # new migration
metaphor dev test                        # run tests
metaphor lint check
```

## The single source of truth

**`schema/models/<entity>.model.yaml`** defines every entity. From it, the
codegen pipeline produces domain entity structs, DTOs, SQL migrations,
repository newtypes, service type aliases, HTTP handlers, and route
registration.

**Regeneration preserves only code inside `// <<< CUSTOM ... // END CUSTOM`
blocks.** Everything outside those markers is overwritten — hand files
inside generator-owned trees must be declared in `metaphor.codegen.yaml`
`user_owned` **before** they land.

## Rules

- **MUST** edit `schema/models/*.model.yaml` first for any entity change. Never hand-edit generated files outside CUSTOM markers.
- **MUST** put custom logic inside `// <<< CUSTOM` / `// END CUSTOM` blocks, or in a `user_owned`-declared sibling file which is never regenerated.
- **MUST** define services as type aliases: `pub type SessionService = GenericCrudService<...>`. Don't hand-roll `impl` for CRUD.
- **MUST** define repositories as thin newtypes over `GenericCrudRepository`.
- **MUST** bind the company scope in EVERY transactional repository method (`bind_current_company(&mut tx)` after `pool.begin()`); direct-pool statements go through the `backbone_orm::company_scope` `*_scoped` helpers. This is RLS LAW — a missed bind is a fence hole, not a style fault.
- **MUST** audit every decision row (`livechat.livechat_audit_log`) — assignment, refusal, park, expiry, throttle refusal: durable trace over in-memory gossip.
- **MUST** keep the ladder set-based: ONE statement, no per-record compute in a loop, no `random.choice` — capacity, the 120s assignment buffer (on every path), the one 1800s ongoing window, rungs 0..9, total-order tie-breaks.
- **MUST** keep the chatbot vocabulary CLOSED at the seven step types; routing is forward-only (sequence increases); the pointer is one column + message rows.
- **MUST** keep the public surface exactly the nine declared paths — capability tokens + fixed windows are the fence; no CORS-permissive arm exists anywhere in this crate.
- **NEVER** write ad-hoc axum routes; CRUD goes through `BackboneCrudHandler`.
- **NEVER** bypass `GenericCrudRepository` for simple CRUD — extend it via custom methods.
- **NEVER** touch another module's schema YAML.
- **NEVER** add a sibling-module crate dependency (website, mail, events are reached through the port traits in `src/application/service/*_port.rs`, host-composed).
- **MUST** read and follow the target repo's own `CLAUDE.md` when working across repos — before editing in another repo, read its rules; the more local `CLAUDE.md` always wins.

## Four-layer folder cheatsheet

```
src/
├── lib.rs                                # re-exports + LivechatModule + CUSTOM blocks
├── module.rs                             # Module struct + builder
├── domain/
│   ├── entity/                           # generated entities
│   └── repositories/                     # trait definitions (ports)
├── application/
│   └── service/
│       ├── <entity>_service.rs           # type alias to GenericCrudService
│       ├── livechat_error.rs             # user_owned: the typed error enum
│       ├── capability.rs                 # user_owned: Tier A HMAC tokens
│       ├── throttle.rs                   # user_owned: FixedWindows + policy
│       ├── selection_service.rs          # user_owned: the ladder
│       ├── session_service.rs            # user_owned: session verbs + chokepoint
│       ├── chatbot_service.rs            # user_owned: the pointer machine
│       ├── availability_service.rs       # user_owned: the button answer
│       ├── rating_service.rs             # user_owned: once-per-session rating
│       ├── report_service.rs             # user_owned: bounded-window reads
│       ├── sweep_service.rs              # user_owned: idle-close + invite expiry
│       ├── website_request_service.rs    # user_owned: operator invites + relink
│       └── <entity>_port.rs              # user_owned: refusing port defaults
├── infrastructure/persistence/
│   ├── <entity>_repository.rs            # generated newtypes
│   └── *_repository.rs                   # user_owned: hand transactional SQL
├── presentation/http/
│   ├── <entity>_handler.rs               # generated CRUD wiring
│   ├── public_routes.rs                  # user_owned: the nine-path surface
│   └── admin_routes.rs                   # user_owned: the officer tree
└── routes/mod.rs                         # stateless + stateful composers

migrations/                               # the module's OWN sequence (2026042622xxxx)
schema/models/                            # ← SOURCE OF TRUTH
config/                                   # module-local config
tests/probes/                             # the fail-hard probe suite (scratch 5433)
```

## Tech stack (non-negotiable)

- Rust 2021; `[lib]` only.
- Web: Axum 0.7 (the module exports pure routers; the host mounts).
- DB: SQLx 0.8 over PostgreSQL 16+ (the report view needs `security_invoker`).
- Errors: one typed enum (`LivechatError`) with `code()`/HTTP mapping.

## Probes

`tests/probes/` — fail-hard, one disposable scratch database each on
127.0.0.1:5433 (`LIVECHAT_TEST_ADMIN_URL` overrides), generated migrations
applied in order, fenced claims proven under a NOSUPERUSER NOBYPASSRLS
probe role. A missing scratch Postgres panics the suite — probes never
skip. Never point them at 5432 (the live dev database).

## graphify

This project may have a knowledge graph at graphify-out/. If
graphify-out/graph.json exists, prefer `graphify query "<question>"` over
raw grepping; run `graphify update .` after modifying code.
