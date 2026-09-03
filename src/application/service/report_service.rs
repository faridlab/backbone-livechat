//! The bounded-window report service (hand-written; user-owned; see
//! `metaphor.codegen.yaml`).
//!
//! The reads behind [`Self::session_summary`] are the ONLY
//! callers of the `session_report` view, every call REQUIRES explicit
//! `from`/`to` bounds (the typed 422 without them), and the window is
//! capped at 366 days — the unbounded every-session-ever scan of the
//! upstream report has no caller at all. The install-time digest flip
//! is likewise refused: installs are inert (no seeds ship, no
//! YourWebsite.com channel, no Welcome Bot, no auto-popup rule).
//!
//! Every aggregate is the repository's ONE set-based
//! statement; the service validates and composes, never folds a
//! fetched recordset in a loop.
//!
//! The happiness KPI is WINDOWED (the rating mix over sessions
//! opened inside the window), never a lifetime average. Week buckets
//! derive from the EXPLICIT `week_start` argument (0=Sunday …
//! 6=Saturday) — never a locale.

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::livechat_error::LivechatError;
use crate::infrastructure::persistence::{
    OutcomeMixRow, ReportRepository, SeriesBucketRow, SessionSummaryRow,
};

/// The report window's hard cap (days).
pub const REPORT_WINDOW_MAX_DAYS: i64 = 366;

/// The composed session summary (one request's whole answer).
#[derive(Debug, Clone, Serialize)]
pub struct SessionSummaryReport {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub summary: SessionSummaryRow,
    pub outcome_mix: Vec<OutcomeMixRow>,
    /// `day` or `week` buckets (the week anchored at the explicit
    /// `week_start` weekday).
    pub series: Vec<SeriesBucketRow>,
    pub week_start: i16,
}

pub struct ReportService {
    reports: ReportRepository,
}

impl ReportService {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self {
            reports: ReportRepository::new(pool),
        }
    }

    /// The session summary over an EXPLICIT, BOUNDED window.
    /// `bucket` = `day` (default) or `week`; `week_start` =
    /// 0 (Sunday) … 6 (Saturday) — the explicit anchor, no locale.
    pub async fn session_summary(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        bucket: Option<&str>,
        week_start: Option<i16>,
    ) -> Result<SessionSummaryReport, LivechatError> {
        if from >= to {
            return Err(LivechatError::Validation(
                "report bounds are required and from must precede to".into(),
            ));
        }
        let days = (to - from).num_days();
        if days > REPORT_WINDOW_MAX_DAYS {
            return Err(LivechatError::Validation(format!(
                "report window capped at {REPORT_WINDOW_MAX_DAYS} days (asked {days})"
            )));
        }
        let bucket = bucket.unwrap_or("day");
        if !matches!(bucket, "day" | "week") {
            return Err(LivechatError::Validation(
                "bucket must be 'day' or 'week'".into(),
            ));
        }
        let week_start = week_start.unwrap_or(1);
        if !(0..=6).contains(&week_start) {
            return Err(LivechatError::Validation(
                "week_start must be 0 (Sunday) .. 6 (Saturday)".into(),
            ));
        }
        let summary = self.reports.summary(from, to).await?;
        let outcome_mix = self.reports.outcome_mix(from, to).await?;
        let series = self.reports.series(from, to, bucket, week_start).await?;
        Ok(SessionSummaryReport {
            from,
            to,
            summary,
            outcome_mix,
            series,
            week_start,
        })
    }
}
