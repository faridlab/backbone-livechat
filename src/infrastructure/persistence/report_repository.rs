//! The report repository (hand-written; user-owned; see
//! `metaphor.codegen.yaml`): bounded-window reads over the
//! `livechat.session_report` view.
//!
//! The view itself carries NO date predicate; these reads are
//! the ONLY callers, every one requires explicit `from`/`to` bounds,
//! and the service layer caps the window at 366 days before reaching
//! here — the unbounded every-session-ever scan of the upstream
//! report has no caller at all.
//!
//! Every aggregate is ONE set-based statement over the window
//! (percentiles via `percentile_cont`, mixes via GROUP BY) — never a
//! fetched recordset folded in an application loop.
//!
//! Determinism: week buckets derive from an EXPLICIT `week_start`
//! weekday argument (0=Sunday … 6=Saturday), offset-arithmetic on
//! `EXTRACT(DOW)` — never the database's or host's locale.

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::application::service::livechat_error::LivechatError;

/// The windowed headline aggregates (one row).
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct SessionSummaryRow {
    pub sessions_total: i64,
    pub open_sessions: i64,
    pub closed_sessions: i64,
    pub handled_by_agent: i64,
    pub handled_by_bot: i64,
    pub escalated: i64,
    pub avg_time_to_answer_secs: Option<f64>,
    pub duration_p50_secs: Option<f64>,
    pub duration_p90_secs: Option<f64>,
    pub duration_p99_secs: Option<f64>,
    pub rated_count: i64,
    pub avg_rating: Option<f64>,
    pub happy_count: i64,
    pub neutral_count: i64,
    pub unhappy_count: i64,
}

/// One outcome-mix bucket.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct OutcomeMixRow {
    pub outcome: Option<String>,
    pub sessions: i64,
}

/// One time-bucket of the series.
#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct SeriesBucketRow {
    pub bucket: DateTime<Utc>,
    pub sessions: i64,
    pub handled_by_agent: i64,
    pub handled_by_bot: i64,
    pub avg_time_to_answer_secs: Option<f64>,
}

pub struct ReportRepository {
    pool: PgPool,
}

impl ReportRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// The windowed headline: counts, average time-to-answer, duration
    /// percentiles, and the windowed happiness KPI (the rating mix
    /// over sessions OPENED inside the window — not a lifetime
    /// average; the windowed computation).
    pub async fn summary(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<SessionSummaryRow, LivechatError> {
        let row = backbone_orm::company_scope::fetch_optional_scoped(
            &self.pool,
            sqlx::query_as::<_, SessionSummaryRow>(
                r#"SELECT
                       count(*)                                                AS sessions_total,
                       count(*) FILTER (WHERE is_open)                         AS open_sessions,
                       count(*) FILTER (WHERE NOT is_open)                     AS closed_sessions,
                       count(*) FILTER (WHERE handled_by_agent)                AS handled_by_agent,
                       count(*) FILTER (WHERE handled_by_bot)                   AS handled_by_bot,
                       count(*) FILTER (WHERE escalated)                        AS escalated,
                       avg(time_to_answer_secs)::float8                         AS avg_time_to_answer_secs,
                       (percentile_cont(0.5) WITHIN GROUP
                           (ORDER BY duration_secs))::float8                  AS duration_p50_secs,
                       (percentile_cont(0.9) WITHIN GROUP
                           (ORDER BY duration_secs))::float8                  AS duration_p90_secs,
                       (percentile_cont(0.99) WITHIN GROUP
                           (ORDER BY duration_secs))::float8                  AS duration_p99_secs,
                       count(rating_value)                                     AS rated_count,
                       avg(rating_value)::float8                                AS avg_rating,
                       count(*) FILTER (WHERE rating_text = 'happy')           AS happy_count,
                       count(*) FILTER (WHERE rating_text = 'neutral')         AS neutral_count,
                       count(*) FILTER (WHERE rating_text = 'unhappy')         AS unhappy_count
                     FROM livechat.session_report
                    WHERE opened_at >= $1 AND opened_at < $2"#,
            )
            .bind(from)
            .bind(to),
        )
        .await?;
        Ok(row.unwrap_or(SessionSummaryRow {
            sessions_total: 0,
            open_sessions: 0,
            closed_sessions: 0,
            handled_by_agent: 0,
            handled_by_bot: 0,
            escalated: 0,
            avg_time_to_answer_secs: None,
            duration_p50_secs: None,
            duration_p90_secs: None,
            duration_p99_secs: None,
            rated_count: 0,
            avg_rating: None,
            happy_count: 0,
            neutral_count: 0,
            unhappy_count: 0,
        }))
    }

    /// The outcome mix over the window (GROUP BY; the stored
    /// per-record derive — escalation stays a live derive, counted in
    /// the headline).
    pub async fn outcome_mix(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<OutcomeMixRow>, LivechatError> {
        let rows = backbone_orm::company_scope::fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, OutcomeMixRow>(
                r#"SELECT session_outcome AS outcome, count(*) AS sessions
                     FROM livechat.session_report
                    WHERE opened_at >= $1 AND opened_at < $2
                 GROUP BY session_outcome
                 ORDER BY session_outcome NULLS LAST"#,
            )
            .bind(from)
            .bind(to),
        )
        .await?;
        Ok(rows)
    }

    /// The per-day or per-week series. `week_start` (0=Sunday …
    /// 6=Saturday) is EXPLICIT — the bucket is
    /// `opened_at::date - ((dow - week_start + 7) % 7)`; no locale.
    pub async fn series(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        bucket: &str,
        week_start: i16,
    ) -> Result<Vec<SeriesBucketRow>, LivechatError> {
        let grouping = if bucket == "week" {
            // Deterministic week bucket anchored at the caller's
            // explicit week start (never the instance's locale).
            "(date_trunc('day', opened_at) \
              - (((EXTRACT(DOW FROM opened_at)::int - $3::int) + 7) % 7) * interval '1 day')"
        } else {
            "date_trunc('day', opened_at)"
        };
        let rows = backbone_orm::company_scope::fetch_all_scoped(
            &self.pool,
            sqlx::query_as::<_, SeriesBucketRow>(&format!(
                r#"SELECT
                       {grouping}                                            AS bucket,
                       count(*)                                              AS sessions,
                       count(*) FILTER (WHERE handled_by_agent)              AS handled_by_agent,
                       count(*) FILTER (WHERE handled_by_bot)                 AS handled_by_bot,
                       avg(time_to_answer_secs)::float8                       AS avg_time_to_answer_secs
                     FROM livechat.session_report
                    WHERE opened_at >= $1 AND opened_at < $2
                 GROUP BY 1
                 ORDER BY 1"#
            ))
            .bind(from)
            .bind(to)
            .bind(week_start),
        )
        .await?;
        Ok(rows)
    }
}
