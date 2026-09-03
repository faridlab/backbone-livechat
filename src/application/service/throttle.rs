//! The public-surface throttle posture (hand-written; user-owned;
//! see `metaphor.codegen.yaml`).
//!
//! `FixedWindows` is the events shape verbatim (a
//! `Mutex<HashMap<key,(window_start,count)>>`; a poisoned lock fails
//! CLOSED via `into_inner`); `LivechatRatePolicy` carries the
//! declared per-route windows as consts. Identity buckets key on the
//! VISITOR DIGEST, never the IP (a NAT of visitors is many
//! identities; one visitor behind rotating exits is one identity).
//!
//! The client-IP posture is the website/events one:
//! `LIVECHAT_TRUSTED_PROXY` tolerant-truth env (`true`/`1`/`yes`/`on`
//! arm it), the RIGHTMOST `X-Forwarded-For` hop only when armed (the
//! entry the nearest trusted proxy appended; every hop to its left
//! is client-supplied text), the connection's bare IP (never
//! `ip:port` — the port is per-connection and would fragment a
//! bucket per reconnect) otherwise. The IP feeds rate shaping and
//! digests ONLY, never authorization.

use std::collections::HashMap;
use std::sync::Mutex;

use axum::http::HeaderMap;

/// The env var arming the trusted-proxy posture.
pub const LIVECHAT_TRUSTED_PROXY_ENV: &str = "LIVECHAT_TRUSTED_PROXY";

/// The in-memory fixed-window throttle (per-process; the declared
/// posture at this pin — the windows are shaping, not security).
#[derive(Debug, Default)]
pub struct FixedWindows {
    inner: Mutex<HashMap<String, (u64, u64)>>, // key -> (window_start_unix, count)
}

impl FixedWindows {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a hit; returns false when the key is over budget in the
    /// current window.
    pub fn allow(&self, key: &str, max: u64, window_secs: u64) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let mut guard = match self.inner.lock() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };
        let open = match guard.get_mut(key) {
            Some((start, count)) => {
                if now.saturating_sub(*start) < window_secs {
                    true
                } else {
                    // Stale window: reset in place.
                    *start = now;
                    *count = 0;
                    true
                }
            }
            None => {
                guard.insert(key.to_string(), (now, 0));
                true
            }
        };
        if !open {
            return false;
        }
        let Some((_, count)) = guard.get_mut(key) else {
            return false;
        };
        if *count >= max {
            return false;
        }
        *count += 1;
        true
    }
}

/// One declared window: (max, window_secs).
pub type Window = (u64, u64);

/// The declared per-route windows (consts, documented; the host can
/// tighten at mount by replacing the policy).
#[derive(Debug, Clone)]
pub struct LivechatRatePolicy {
    /// POST /public/sessions — open/resume, per-IP arm.
    pub open_ip: Window,
    /// POST /public/sessions — open/resume, per-identity arm.
    pub open_identity: Window,
    /// POST messages — per-identity arm.
    pub message_identity: Window,
    /// POST messages — per-IP arm.
    pub message_ip: Window,
    /// GET messages (cursor poll).
    pub poll_identity: Window,
    /// GET /public/availability — per-IP arm.
    pub availability_ip: Window,
    /// POST answers — per-identity arm.
    pub answers_identity: Window,
    /// POST rating — per-identity arm.
    pub rating_identity: Window,
}

impl Default for LivechatRatePolicy {
    fn default() -> Self {
        Self {
            open_ip: (6, 3600),
            open_identity: (6, 3600),
            message_identity: (30, 60),
            message_ip: (60, 60),
            poll_identity: (120, 60),
            availability_ip: (240, 60),
            answers_identity: (30, 60),
            rating_identity: (3, 3600),
        }
    }
}

/// The trusted-proxy posture (bool-tolerant, fail-closed): `true` /
/// `1` / `yes` / `on` (any case) arm it; unset or anything else keeps
/// the direct-connection posture (the forwarded header is
/// client-controlled text and never read).
pub fn trusted_proxy_from_env() -> bool {
    matches!(
        std::env::var(LIVECHAT_TRUSTED_PROXY_ENV)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "true" | "1" | "yes" | "on"
    )
}

/// Resolve the caller address for rate shaping and visitor digests
/// (never authorization): the RIGHTMOST forwarded hop ONLY under the
/// trusted-proxy posture; every hop is ignored otherwise and the
/// connection's socket IP wins. Falls back to `"unknown"` when no
/// socket address is available. The socket arm is the bare IP, never
/// the `ip:port` pair.
pub fn caller_ip(headers: &HeaderMap, socket_ip: Option<&str>, trusted_proxy: bool) -> String {
    if trusted_proxy {
        if let Some(fwd) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(last) = fwd.rsplit(',').next() {
                let trimmed = last.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
    }
    socket_ip.unwrap_or("unknown").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn over_budget_refuses_then_rolls() {
        let w = FixedWindows::new();
        for i in 0..3 {
            assert!(w.allow("k", 3, 60), "hit {i} must pass");
        }
        assert!(!w.allow("k", 3, 60), "the 4th hit in-window must refuse");
        // A second key has its own bucket.
        assert!(w.allow("other", 3, 60));
    }

    #[test]
    fn identity_bucket_is_not_the_ip_bucket() {
        let w = FixedWindows::new();
        // The identity arm keys the digest, not the IP: same IP, two
        // identities — both pass.
        assert!(w.allow("identity:digest-a", 1, 60));
        assert!(w.allow("identity:digest-b", 1, 60));
        // The same identity is exhausted at max=1.
        assert!(!w.allow("identity:digest-a", 1, 60));
    }

    #[test]
    fn caller_ip_ignores_forwarded_header_unless_trusted() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            "1.2.3.4, 5.6.7.8"
                .parse::<axum::http::HeaderValue>()
                .unwrap(),
        );
        // Untrusted: the socket IP wins, the header is ignored.
        assert_eq!(caller_ip(&headers, Some("9.9.9.9"), false), "9.9.9.9");
        // Trusted: the RIGHTMOST hop (the nearest proxy's entry).
        assert_eq!(caller_ip(&headers, Some("9.9.9.9"), true), "5.6.7.8");
        // No socket address: the unknown arm, never a panic.
        assert_eq!(caller_ip(&HeaderMap::new(), None, false), "unknown");
    }
}
