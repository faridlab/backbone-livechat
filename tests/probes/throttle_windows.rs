//! The throttle probe: the fixed windows key identities separately
//! from IPs, slide in real time (a short window genuinely rolls),
//! the declared policy arms carry the posture, and the client IP
//! resolution ignores forwarded text unless the proxy is trusted.

use axum::http::HeaderMap;

use backbone_livechat::application::service::throttle::{
    caller_ip, FixedWindows, LivechatRatePolicy,
};

#[test]
fn fixed_windows_budget_key_and_slide() {
    let windows = FixedWindows::new();

    // Budget: allow to max, refuse past it — per KEY.
    for hit in 0..4 {
        assert!(windows.allow("probe:a", 4, 60), "hit {hit} must pass");
    }
    assert!(
        !windows.allow("probe:a", 4, 60),
        "the hit past the budget in-window must refuse"
    );
    // A different key has its OWN bucket (identity arms key the
    // visitor digest, never the IP).
    assert!(
        windows.allow("probe:b", 4, 60),
        "a second key is a second bucket"
    );
    assert!(
        windows.allow("ip:1.2.3.4", 1, 60),
        "the ip arm is its own bucket"
    );
    assert!(
        !windows.allow("ip:1.2.3.4", 1, 60),
        "the ip bucket refuses past ITS budget"
    );

    // The slide is REAL: a 1-second window rolls and reopens.
    for _ in 0..2 {
        assert!(
            windows.allow("probe:slide", 1, 1),
            "the first hit in the window passes"
        );
        assert!(
            !windows.allow("probe:slide", 1, 1),
            "the in-window second hit refuses"
        );
        // Wait the window out; the next hit opens a fresh one.
        std::thread::sleep(std::time::Duration::from_millis(1100));
    }
    assert!(
        windows.allow("probe:slide", 1, 1),
        "a rolled window reopens (the throttle shapes, never bans)"
    );
}

#[test]
fn the_declared_policy_arms() {
    let policy = LivechatRatePolicy::default();
    // The open arms: 6/hour on BOTH the ip and the identity.
    assert_eq!(policy.open_ip, (6, 3600));
    assert_eq!(policy.open_identity, (6, 3600));
    // Messages: 30/min per identity, 60/min per ip.
    assert_eq!(policy.message_identity, (30, 60));
    assert_eq!(policy.message_ip, (60, 60));
    // The cursor poll: 120/min.
    assert_eq!(policy.poll_identity, (120, 60));
    // Availability: 240/min per ip (the anonymous arm).
    assert_eq!(policy.availability_ip, (240, 60));
    // Answers: 30/min per identity.
    assert_eq!(policy.answers_identity, (30, 60));
    // Ratings: 3/hour per identity (a session rates once; the arm
    // only shapes retries).
    assert_eq!(policy.rating_identity, (3, 3600));
    // Every window is a sane shape: some budget inside some horizon.
    for (max, secs) in [
        policy.open_ip,
        policy.open_identity,
        policy.message_identity,
        policy.message_ip,
        policy.poll_identity,
        policy.availability_ip,
        policy.answers_identity,
        policy.rating_identity,
    ] {
        assert!(
            max > 0 && secs > 0,
            "a declared window must be a real window ({max}/{secs})"
        );
    }
}

#[test]
fn client_ip_resolution_posture() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-forwarded-for",
        "1.1.1.1, 2.2.2.2, 3.3.3.3"
            .parse::<axum::http::HeaderValue>()
            .unwrap(),
    );
    // Untrusted (the default posture): the socket IP wins, the
    // client-controlled header is IGNORED entirely.
    assert_eq!(caller_ip(&headers, Some("9.9.9.9"), false), "9.9.9.9");
    // Trusted: the RIGHTMOST hop (the entry the nearest proxy
    // appended) — never the leftmost client-supplied text.
    assert_eq!(caller_ip(&headers, Some("9.9.9.9"), true), "3.3.3.3");
    // A single-hop forwarded value under trust.
    let mut one = HeaderMap::new();
    one.insert(
        "x-forwarded-for",
        "7.7.7.7".parse::<axum::http::HeaderValue>().unwrap(),
    );
    assert_eq!(caller_ip(&one, Some("9.9.9.9"), true), "7.7.7.7");
    // No socket address, untrusted: the explicit unknown arm —
    // never a panic, never the header.
    assert_eq!(caller_ip(&headers, None, false), "unknown");
    // The bare-IP law is the call site's (the router passes
    // `ConnectInfo::ip()`, never the ip:port pair — a per-connection
    // port would fragment the bucket per reconnect); the resolver
    // passes the given address through untouched.
    assert_eq!(
        caller_ip(&HeaderMap::new(), Some("10.0.0.7"), false),
        "10.0.0.7"
    );
}
