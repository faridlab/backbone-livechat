//! The fail-hard probe suite (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! Every probe runs on its own DISPOSABLE scratch database on the
//! local scratch Postgres (127.0.0.1:5433 — NEVER the live dev
//! database on 5432). A probe that cannot reach the scratch server
//! PANICS — a skipped probe is a failed probe.
//!
//! The named gates this suite closes:
//! - the DETERMINISTIC LADDER — no randomness, the 120s anti-burst
//!   buffer inside the pool on every path, ONE 30-minute ongoing
//!   window, the total-order tie-break, no read-path GC;
//! - the PER-RECORD outcome derive (never a recordset-wide
//!   assignment inside an iteration) + the bounded report reads;
//! - the member-history ledger's three partial uniques + the strict
//!   persona trichotomy;
//! - the chatbot POINTER state machine (one column, forward-only,
//!   the seven-type closure, lazy welcome, sanitized answers);
//! - the capability BOUNDARY (nine-path allowlist, uniform 404
//!   family, wrong-token-never-mints, no CORS mirror, typed
//!   throttles) — through the real router;
//! - the once-per-session rating wall;
//! - the website bridge overlays (invite lifecycle, both sides
//!   visible, merge relink);
//! - the sweep lifecycle (bounded, audited, no untraced deletes);
//! - the audit trail (every decision leaves its row; the event
//!   vocabulary is closed);
//! - the FENCED RUNTIME — the RLS claims held by a NOSUPERUSER
//!   NOBYPASSRLS role, never by a role that bypasses the fence.

mod probes;
