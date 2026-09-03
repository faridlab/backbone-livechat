//! Tier A capability tokens (hand-written; user-owned; see
//! `metaphor.codegen.yaml`) — ADR-0018, file-shape-copied from the
//! events capability module with a module-local domain-separation
//! context (never events').
//!
//! HMAC-SHA256 over a domain-separated message, base64url-encoded,
//! verified in CONSTANT TIME. The one secret is
//! `LIVECHAT_CAPABILITY_SECRET`; an empty secret is a typed 503 at
//! the routes, never a mint under an empty key (fail-closed — a
//! token minted under "" would be forgeable by anyone with the
//! source).
//!
//! Token shape: `v1.<payload-b64url>.<sig-b64url>` where the payload
//! is compact JSON `{purpose, exp, data}` and the signature is
//! HMAC-SHA256(secret, "livechat-capability-v1\n" + purpose + "\n" +
//! payload-b64url). Verification recomputes the signature and
//! compares with `subtle` (`ConstantTimeEq`) — never `==`.
//!
//! Two purposes at this version, both carrying
//! `data = [session_id, visitor_key]`:
//!  - `livechat-guest-session` — the visitor's session handle,
//!    `exp = now + 24h` (`LIVECHAT_GUEST_TOKEN_TTL_SECS`). Rotates by
//!    construction: every open mints fresh.
//!  - `livechat-invite-accept` — the operator-initiated invite
//!    handoff, `exp = now + 15min`.
//!
//! Fail-closed on EVERY malformed arm: a token that fails to parse,
//! carries the wrong purpose, the wrong version, a signature
//! mismatch, or an expired `exp` maps onto the uniform session-404
//! family at the session-scoped routes (no oracle), and onto
//! `livechat_guest_token_invalid` 401 when PRESENTED at the open
//! verb (a presented capability that fails verify never mints).

use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use super::livechat_error::LivechatError;

/// The visitor's session-handle purpose (24h TTL, rotates every
/// open).
pub const PURPOSE_GUEST_SESSION: &str = "livechat-guest-session";

/// The operator-initiated invite acceptance purpose (15min TTL).
pub const PURPOSE_INVITE_ACCEPT: &str = "livechat-invite-accept";

/// The domain-separation label (first arm of every MAC message).
const CAPABILITY_CONTEXT: &str = "livechat-capability-v1";

type HmacSha256 = Hmac<Sha256>;

/// The env var holding the module's capability secret.
pub const LIVECHAT_CAPABILITY_SECRET_ENV: &str = "LIVECHAT_CAPABILITY_SECRET";

/// Default guest-token lifetime: 24h.
pub const LIVECHAT_GUEST_TOKEN_TTL_SECS: i64 = 24 * 60 * 60;

/// Default invite-accept token lifetime: 15min.
pub const LIVECHAT_INVITE_TTL_SECS: i64 = 15 * 60;

/// Read the capability secret from the environment (empty string when
/// unset — the routes turn that into the typed 503).
pub fn capability_secret_from_env() -> String {
    std::env::var(LIVECHAT_CAPABILITY_SECRET_ENV).unwrap_or_default()
}

fn b64url_encode(bytes: &[u8]) -> String {
    // Standard base64 WITHOUT padding, URL-safe alphabet — the
    // path-segment-safe encoding (tokens ride in URL path segments).
    const ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | (b[2] as u32);
        out.push(ALPHA[(n >> 18) as usize & 63] as char);
        out.push(ALPHA[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHA[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHA[n as usize & 63] as char
        } else {
            '='
        });
    }
    // Trim padding: url-safe no-pad form.
    while out.ends_with('=') {
        out.pop();
    }
    out
}

fn b64url_decode(text: &str) -> Option<Vec<u8>> {
    const ALPHA: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut bits: u32 = 0;
    let mut nbits: u32 = 0;
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for ch in text.bytes() {
        let v = ALPHA.iter().position(|&a| a == ch)? as u32;
        bits = (bits << 6) | v;
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            out.push(((bits >> nbits) & 0xff) as u8);
        }
    }
    Some(out)
}

fn sign(secret: &str, purpose: &str, payload_b64: &str) -> Result<Vec<u8>, LivechatError> {
    if secret.is_empty() {
        return Err(LivechatError::CapabilitySecretNotConfigured);
    }
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .map_err(|e| LivechatError::Internal(format!("capability secret rejected by HMAC: {e}")))?;
    mac.update(CAPABILITY_CONTEXT.as_bytes());
    mac.update(b"\n");
    mac.update(purpose.as_bytes());
    mac.update(b"\n");
    mac.update(payload_b64.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

/// The token payload. `exp` is unix seconds. `data` is
/// `[session_id, visitor_key]` for both livechat purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityClaims {
    pub purpose: String,
    pub exp: i64,
    pub data: Vec<String>,
}

impl CapabilityClaims {
    /// Mint a token for these claims (fails closed on an empty
    /// secret).
    pub fn mint(&self, secret: &str) -> Result<String, LivechatError> {
        let payload = serde_json::to_vec(self)
            .map_err(|e| LivechatError::Internal(format!("capability payload encode: {e}")))?;
        let payload_b64 = b64url_encode(&payload);
        let sig = sign(secret, &self.purpose, &payload_b64)?;
        Ok(format!("v1.{payload_b64}.{}", b64url_encode(&sig)))
    }

    /// Verify a token against the expected purpose (constant-time
    /// signature compare; expiry checked AFTER the signature so a
    /// forged expiry is not distinguishable from a forged anything).
    pub fn verify(
        secret: &str,
        expected_purpose: &str,
        token: &str,
        now: DateTime<Utc>,
    ) -> Result<Self, LivechatError> {
        if secret.is_empty() {
            return Err(LivechatError::CapabilitySecretNotConfigured);
        }
        let bad = || LivechatError::SessionNotFound;
        let mut parts = token.splitn(3, '.');
        let version = parts.next().unwrap_or_default();
        let payload_b64 = parts.next().unwrap_or_default();
        let sig_b64 = parts.next().unwrap_or_default();
        if version != "v1" || payload_b64.is_empty() || sig_b64.is_empty() {
            return Err(bad());
        }
        let expected_sig = sign(secret, expected_purpose, payload_b64)?;
        let given_sig = b64url_decode(sig_b64).ok_or_else(bad)?;
        // Constant-time compare (length included): never `==`.
        if expected_sig.len() != given_sig.len() || expected_sig.ct_eq(&given_sig).unwrap_u8() == 0
        {
            return Err(bad());
        }
        let payload = b64url_decode(payload_b64).ok_or_else(bad)?;
        let claims: Self = serde_json::from_slice(&payload).map_err(|_| bad())?;
        if claims.purpose != expected_purpose {
            return Err(bad());
        }
        if now.timestamp() >= claims.exp {
            return Err(bad());
        }
        Ok(claims)
    }

    /// The claims' session id (first data arm) — `None` when the
    /// payload is not the expected two-string shape.
    pub fn session_id(&self) -> Option<uuid::Uuid> {
        self.data
            .first()
            .and_then(|s| uuid::Uuid::parse_str(s).ok())
    }

    /// The claims' visitor key (second data arm).
    pub fn visitor_key(&self) -> Option<&str> {
        self.data.get(1).map(|s| s.as_str())
    }
}

/// Mint a guest-session capability: `[session_id, visitor_key]`, TTL
/// from `now`.
pub fn mint_guest_capability(
    secret: &str,
    session_id: &uuid::Uuid,
    visitor_key: &str,
    now: DateTime<Utc>,
    ttl_secs: i64,
) -> Result<String, LivechatError> {
    CapabilityClaims {
        purpose: PURPOSE_GUEST_SESSION.to_string(),
        exp: now.timestamp() + ttl_secs,
        data: vec![session_id.to_string(), visitor_key.to_string()],
    }
    .mint(secret)
}

/// Mint an invite-accept capability: `[session_id, visitor_key]`, the
/// short 15-minute handoff TTL from `now`.
pub fn mint_invite_capability(
    secret: &str,
    session_id: &uuid::Uuid,
    visitor_key: &str,
    now: DateTime<Utc>,
) -> Result<String, LivechatError> {
    CapabilityClaims {
        purpose: PURPOSE_INVITE_ACCEPT.to_string(),
        exp: now.timestamp() + LIVECHAT_INVITE_TTL_SECS,
        data: vec![session_id.to_string(), visitor_key.to_string()],
    }
    .mint(secret)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    #[test]
    fn mint_then_verify_round_trips() {
        let sid = uuid::Uuid::new_v4();
        let token =
            mint_guest_capability("probe-secret", &sid, "visitor-digest", now(), 60).unwrap();
        let claims =
            CapabilityClaims::verify("probe-secret", PURPOSE_GUEST_SESSION, &token, now()).unwrap();
        assert_eq!(claims.session_id(), Some(sid));
        assert_eq!(claims.visitor_key(), Some("visitor-digest"));
    }

    #[test]
    fn empty_secret_never_mints() {
        match mint_guest_capability("", &uuid::Uuid::new_v4(), "k", now(), 60) {
            Err(LivechatError::CapabilitySecretNotConfigured) => {}
            other => panic!("empty secret must refuse, got {:?}", other),
        }
    }

    #[test]
    fn expired_token_fails() {
        let sid = uuid::Uuid::new_v4();
        let token = mint_guest_capability("s", &sid, "k", now(), 10).unwrap();
        let later = now() + chrono::Duration::seconds(11);
        assert!(
            CapabilityClaims::verify("s", PURPOSE_GUEST_SESSION, &token, later).is_err(),
            "expired token must fail"
        );
    }

    #[test]
    fn wrong_purpose_fails() {
        let sid = uuid::Uuid::new_v4();
        let token = mint_invite_capability("s", &sid, "k", now()).unwrap();
        assert!(
            CapabilityClaims::verify("s", PURPOSE_GUEST_SESSION, &token, now()).is_err(),
            "purpose mismatch must fail"
        );
    }

    #[test]
    fn tampered_signature_fails() {
        let sid = uuid::Uuid::new_v4();
        let token = mint_guest_capability("s", &sid, "k", now(), 60).unwrap();
        let tampered = match token.rsplit_once('.') {
            Some((head, _)) => format!("{head}.AAAA"),
            None => token.clone(),
        };
        assert!(CapabilityClaims::verify("s", PURPOSE_GUEST_SESSION, &tampered, now()).is_err());
    }

    #[test]
    fn cross_secret_signature_fails() {
        let sid = uuid::Uuid::new_v4();
        let token = mint_guest_capability("secret-a", &sid, "k", now(), 60).unwrap();
        assert!(
            CapabilityClaims::verify("secret-b", PURPOSE_GUEST_SESSION, &token, now()).is_err()
        );
    }
}
