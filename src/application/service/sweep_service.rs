//! The scheduled passes (hand-written; user-owned; see
//! `metaphor.codegen.yaml`): GC lives HERE and in the host's jobs
//! loop — never on a read path.
//!
//! Two declared passes, both run under the ambient org scope the
//! composing service's tenancy decorator installs for the acting
//! unit — the module itself scopes nothing (ADR-0029):
//! - the IDLE-CLOSE sweep: open sessions with `last_interest_at`
//!   older than `LIVECHAT_IDLE_CLOSE_HOURS` (default 24) close with
//!   reason `expired`, audited;
//! - the INVITE-EXPIRY sweep: pending invites older than
//!   `LIVECHAT_INVITE_EXPIRY_HOURS` (default 24) clear the flag and
//!   close (`expired`), audited — rows survive, nothing is deleted.
//!
//! The 1-hour message-less unlink of upstream is REFUSED: no
//! untraced hard delete exists anywhere in the module.

use chrono::{DateTime, Duration, Utc};

use super::livechat_error::LivechatError;
use crate::infrastructure::persistence::{SweepOutcome, SweepRepository};

/// The env var holding the idle-close horizon (hours; default 24).
pub const LIVECHAT_IDLE_CLOSE_HOURS_ENV: &str = "LIVECHAT_IDLE_CLOSE_HOURS";

/// The env var holding the invite-expiry horizon (hours; default 24).
pub const LIVECHAT_INVITE_EXPIRY_HOURS_ENV: &str = "LIVECHAT_INVITE_EXPIRY_HOURS";

/// Default idle-close horizon: 24 hours.
pub const DEFAULT_IDLE_CLOSE_HOURS: i64 = 24;

/// Default invite-expiry horizon: 24 hours (the invite is a pending
/// REQUEST — it lapses after a day of no acceptance either way).
pub const DEFAULT_INVITE_EXPIRY_HOURS: i64 = 24;

pub struct SweepService {
    sweeps: SweepRepository,
    idle_hours: i64,
    invite_hours: i64,
}

impl SweepService {
    /// Compose with the declared horizons (tolerant-truth env parse;
    /// non-positive or unparseable values fall back to the declared
    /// defaults — the sweep never widens on a bad env).
    pub fn from_env(pool: sqlx::PgPool) -> Self {
        Self::new(pool, idle_hours_from_env(), invite_hours_from_env())
    }

    /// Compose with explicit horizons (the probe entry).
    pub fn new(pool: sqlx::PgPool, idle_hours: i64, invite_hours: i64) -> Self {
        Self {
            sweeps: SweepRepository::new(pool),
            idle_hours: idle_hours.max(1),
            invite_hours: invite_hours.max(1),
        }
    }

    /// Run both passes as of `now` (the host jobs loop's entry
    /// point; the composing service's jobs loop binds the ambient org
    /// scope per acting unit — ADR-0029).
    pub async fn sweep_at(&self, now: DateTime<Utc>) -> Result<SweepOutcome, LivechatError> {
        self.sweeps
            .sweep(
                now - Duration::hours(self.idle_hours),
                now - Duration::hours(self.invite_hours),
            )
            .await
    }

    /// [`Self::sweep_at`] at the current clock.
    pub async fn sweep(&self) -> Result<SweepOutcome, LivechatError> {
        self.sweep_at(Utc::now()).await
    }

    /// The configured horizons (the jobs loop's log line).
    pub fn horizons(&self) -> (i64, i64) {
        (self.idle_hours, self.invite_hours)
    }
}

fn env_hours(name: &str) -> Option<i64> {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|h| *h > 0)
}

/// The idle-close horizon (env or default).
pub fn idle_hours_from_env() -> i64 {
    env_hours(LIVECHAT_IDLE_CLOSE_HOURS_ENV).unwrap_or(DEFAULT_IDLE_CLOSE_HOURS)
}

/// The invite-expiry horizon (env or default).
pub fn invite_hours_from_env() -> i64 {
    env_hours(LIVECHAT_INVITE_EXPIRY_HOURS_ENV).unwrap_or(DEFAULT_INVITE_EXPIRY_HOURS)
}
