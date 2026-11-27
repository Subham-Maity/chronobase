//! Pure-Rust cryptography for the ChronoBase VPS Pull Bridge.
//!
//! Security properties:
//! - 256-bit CSPRNG pairing keys & per-session nonces via `getrandom`.
//! - Mutual challenge-response authentication using HMAC-SHA256 verified with
//!   constant-time comparison (`subtle::ConstantTimeEq`).
//! - Anti-replay protection: ±60s timestamp window + 120s seen-nonce cache.
//! - Per-IP exponential rate limiting / lockout after 5 consecutive auth failures.
//! - Per-session AEAD stream encryption using `ChaCha20-Poly1305` with unique
//!   directional monotonic nonces (`[dir, 0, 0, 0, counter_be...]`).

use std::collections::{HashMap, VecDeque};
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::bridge::BridgeError;
use crate::secret::Secret;

type HmacSha256 = Hmac<Sha256>;

/// Maximum allowed clock skew between client and VPS agent during handshake.
pub const MAX_CLOCK_SKEW_SECS: u64 = 60;

/// Minimum length for user-supplied pairing keys (when not 64-char hex).
pub const MIN_PAIRING_KEY_CHARS: usize = 24;

const MAX_REPLAY_ENTRIES: usize = 4096;
const NONCE_TTL: Duration = Duration::from_secs(120);
const MAX_AUTH_FAILURES: u32 = 5;
const LOCKOUT_DURATION: Duration = Duration::from_secs(60);

/// Generates a fresh 256-bit CSPRNG pairing key formatted as `cb_<64 hex chars>`.
pub fn generate_pairing_key() -> Result<Secret, BridgeError> {
    let mut raw = [0u8; 32];
    getrandom::fill(&mut raw).map_err(|e| BridgeError::Protocol(format!("CSPRNG failed: {e}")))?;
    Ok(Secret::new(format!("cb_{}", hex_encode(&raw))))
}

/// Generates 32 cryptographically random bytes for a handshake nonce.
pub fn random_nonce32() -> Result<[u8; 32], BridgeError> {
    let mut nonce = [0u8; 32];
    getrandom::fill(&mut nonce)
        .map_err(|e| BridgeError::Protocol(format!("CSPRNG failed: {e}")))?;
    Ok(nonce)
}

/// Returns the current UNIX timestamp in seconds.
pub fn unix_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Computes a safe display fingerprint (`sha256:<first 16 hex chars>`) of the pairing key.
pub fn key_fingerprint(key: &Secret) -> String {
    let Ok(derived) = derive_master_key(key) else {
        return "invalid-key".to_string();
    };
    let digest = Sha256::digest(derived);
    format!("sha256:{}", &hex_encode(&digest)[..16])
}

/// Derives a 32-byte master key from a `Secret` pairing key.
///
/// Accepts `cb_<64 hex>` or `<64 hex>` directly, or derives via SHA-256 for
/// user-entered passphrases of at least [`MIN_PAIRING_KEY_CHARS`] characters.
pub fn derive_master_key(secret: &Secret) -> Result<[u8; 32], BridgeError> {
    let trimmed = secret.expose().trim();
    let hex_candidate = trimmed.strip_prefix("cb_").unwrap_or(trimmed);
    if hex_candidate.len() == 64 && hex_candidate.bytes().all(|b| b.is_ascii_hexdigit()) {
        let mut out = [0u8; 32];
        for (i, slot) in out.iter_mut().enumerate() {
            let hi = hex_nibble(hex_candidate.as_bytes()[i * 2])?;
            let lo = hex_nibble(hex_candidate.as_bytes()[i * 2 + 1])?;
            *slot = (hi << 4) | lo;
        }
        return Ok(out);
    }

    if trimmed.len() < MIN_PAIRING_KEY_CHARS {
        return Err(BridgeError::Validation(format!(
            "Pairing key must be at least {MIN_PAIRING_KEY_CHARS} characters (or use Generate Key)"
        )));
    }

    let mut hasher = Sha256::new();
    hasher.update(b"CBV1-KEY-DERIVE:");
    hasher.update(trimmed.as_bytes());
    Ok(hasher.finalize().into())
}

/// Computes `HMAC-SHA256(key, "CBV1-CLIENT-AUTH" || server_nonce || client_nonce || ts_be)`.
pub fn compute_client_auth_mac(
    key: &[u8; 32],
    server_nonce: &[u8; 32],
    client_nonce: &[u8; 32],
    client_ts: u64,
) -> Result<[u8; 32], BridgeError> {
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(key)
        .map_err(|_| BridgeError::Protocol("invalid HMAC key length".into()))?;
    mac.update(b"CBV1-CLIENT-AUTH");
    mac.update(server_nonce);
    mac.update(client_nonce);
    mac.update(&client_ts.to_be_bytes());
    Ok(mac.finalize().into_bytes().into())
}

/// Computes `HMAC-SHA256(key, "CBV1-SERVER-AUTH" || client_nonce || server_nonce)`.
pub fn compute_server_auth_mac(
    key: &[u8; 32],
    client_nonce: &[u8; 32],
    server_nonce: &[u8; 32],
) -> Result<[u8; 32], BridgeError> {
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(key)
        .map_err(|_| BridgeError::Protocol("invalid HMAC key length".into()))?;
    mac.update(b"CBV1-SERVER-AUTH");
    mac.update(client_nonce);
    mac.update(server_nonce);
    Ok(mac.finalize().into_bytes().into())
}

/// Constant-time verification of two 32-byte MACs to prevent timing side-channels.
pub fn verify_mac_ct(expected: &[u8; 32], provided: &[u8; 32]) -> bool {
    bool::from(expected.ct_eq(provided))
}

/// Derives an ephemeral per-session `ChaCha20-Poly1305` AEAD cipher from the
/// shared master key and both handshake nonces.
pub fn derive_session_cipher(
    key: &[u8; 32],
    server_nonce: &[u8; 32],
    client_nonce: &[u8; 32],
) -> Result<ChaCha20Poly1305, BridgeError> {
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(key)
        .map_err(|_| BridgeError::Protocol("invalid HMAC key length".into()))?;
    mac.update(b"CBV1-SESSION-KEY");
    mac.update(server_nonce);
    mac.update(client_nonce);
    let session_key_bytes: [u8; 32] = mac.finalize().into_bytes().into();
    let cipher = ChaCha20Poly1305::new(&Key::from(session_key_bytes));
    Ok(cipher)
}

/// Builds a unique 96-bit AEAD nonce from direction (`1` = C->S, `2` = S->C) and monotonic counter.
fn frame_nonce(direction: u8, counter: u64) -> [u8; 12] {
    let mut n = [0u8; 12];
    n[0] = direction;
    n[4..12].copy_from_slice(&counter.to_be_bytes());
    n
}

/// Encrypts a plaintext payload with `ChaCha20-Poly1305`, binding direction & counter as AAD.
pub fn encrypt_payload(
    cipher: &ChaCha20Poly1305,
    direction: u8,
    counter: u64,
    plaintext: &[u8],
) -> Result<Vec<u8>, BridgeError> {
    let nonce_bytes = frame_nonce(direction, counter);
    let nonce = Nonce::from(nonce_bytes);
    cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad: &nonce_bytes,
            },
        )
        .map_err(|_| BridgeError::Protocol("AEAD encryption failed".into()))
}

/// Decrypts and authenticates an AEAD ciphertext frame.
pub fn decrypt_payload(
    cipher: &ChaCha20Poly1305,
    direction: u8,
    counter: u64,
    ciphertext: &[u8],
) -> Result<Vec<u8>, BridgeError> {
    let nonce_bytes = frame_nonce(direction, counter);
    let nonce = Nonce::from(nonce_bytes);
    cipher
        .decrypt(
            &nonce,
            Payload {
                msg: ciphertext,
                aad: &nonce_bytes,
            },
        )
        .map_err(|_| BridgeError::AuthFailed)
}

/// Anti-replay nonce tracker rejecting replayed client nonces and stale timestamps.
pub struct ReplayGuard {
    inner: Mutex<ReplayState>,
}

struct ReplayState {
    seen: HashMap<[u8; 32], Instant>,
    order: VecDeque<([u8; 32], Instant)>,
}

impl Default for ReplayGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplayGuard {
    /// Creates an empty [`ReplayGuard`].
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(ReplayState {
                seen: HashMap::new(),
                order: VecDeque::new(),
            }),
        }
    }

    /// Checks that `client_ts` is within ±60s of `now_ts` and `client_nonce` has not been seen.
    pub fn check_and_record(
        &self,
        client_nonce: [u8; 32],
        client_ts: u64,
        now_ts: u64,
    ) -> Result<(), BridgeError> {
        let skew = now_ts.abs_diff(client_ts);
        if skew > MAX_CLOCK_SKEW_SECS {
            return Err(BridgeError::AuthFailed);
        }

        let now = Instant::now();
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        while let Some((oldest_nonce, ts)) = guard.order.front().copied() {
            if now.duration_since(ts) > NONCE_TTL || guard.seen.len() >= MAX_REPLAY_ENTRIES {
                guard.order.pop_front();
                guard.seen.remove(&oldest_nonce);
            } else {
                break;
            }
        }

        if guard.seen.contains_key(&client_nonce) {
            return Err(BridgeError::AuthFailed);
        }

        guard.seen.insert(client_nonce, now);
        guard.order.push_back((client_nonce, now));
        Ok(())
    }
}

/// Per-IP rate limiter locking out an IP for 60s after 5 consecutive authentication failures.
pub struct AuthRateLimiter {
    inner: Mutex<HashMap<IpAddr, FailureRecord>>,
}

struct FailureRecord {
    count: u32,
    locked_until: Option<Instant>,
}

impl Default for AuthRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl AuthRateLimiter {
    /// Creates a new [`AuthRateLimiter`].
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Returns `Err(BridgeError::AccessDenied)` if `ip` is currently locked out.
    pub fn check(&self, ip: IpAddr) -> Result<(), BridgeError> {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        if let Some(rec) = guard.get_mut(&ip)
            && let Some(until) = rec.locked_until
        {
            if now < until {
                return Err(BridgeError::AccessDenied);
            }
            // Lockout expired — reset count.
            rec.count = 0;
            rec.locked_until = None;
        }
        Ok(())
    }

    /// Records a failed auth attempt for `ip`, locking out after [`MAX_AUTH_FAILURES`].
    pub fn record_failure(&self, ip: IpAddr) {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let now = Instant::now();
        let rec = guard.entry(ip).or_insert(FailureRecord {
            count: 0,
            locked_until: None,
        });
        rec.count = rec.count.saturating_add(1);
        if rec.count >= MAX_AUTH_FAILURES {
            rec.locked_until = Some(now + LOCKOUT_DURATION);
        }
    }

    /// Clears failure history after a successful authentication.
    pub fn record_success(&self, ip: IpAddr) {
        let mut guard = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        guard.remove(&ip);
    }
}

/// Encodes bytes as lowercase hexadecimal.
pub fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn hex_nibble(b: u8) -> Result<u8, BridgeError> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(BridgeError::Validation(
            "Invalid hex character in key".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    #[test]
    fn generated_pairing_key_derives_and_fingerprints() {
        let k = generate_pairing_key().expect("key");
        assert!(k.expose().starts_with("cb_"));
        assert_eq!(k.expose().len(), 67);
        assert!(key_fingerprint(&k).starts_with("sha256:"));
        assert!(derive_master_key(&k).is_ok());
    }

    #[test]
    fn short_weak_key_is_rejected() {
        let weak = Secret::new("too-short");
        assert!(matches!(
            derive_master_key(&weak),
            Err(BridgeError::Validation(_))
        ));
    }

    #[test]
    fn mutual_hmac_and_aead_round_trip() {
        let secret = generate_pairing_key().expect("key");
        let master = derive_master_key(&secret).expect("master");
        let s_nonce = random_nonce32().expect("s_nonce");
        let c_nonce = random_nonce32().expect("c_nonce");
        let ts = unix_timestamp_secs();

        let c_mac = compute_client_auth_mac(&master, &s_nonce, &c_nonce, ts).expect("cmac");
        assert!(verify_mac_ct(&c_mac, &c_mac));
        let mut tampered = c_mac;
        tampered[0] ^= 0xff;
        assert!(!verify_mac_ct(&c_mac, &tampered));

        let cipher_c = derive_session_cipher(&master, &s_nonce, &c_nonce).expect("c");
        let cipher_s = derive_session_cipher(&master, &s_nonce, &c_nonce).expect("s");

        let ct = encrypt_payload(&cipher_c, 1, 0, b"postgresql://u:p@localhost:5432/db")
            .expect("encrypt");
        let pt = decrypt_payload(&cipher_s, 1, 0, &ct).expect("decrypt");
        assert_eq!(pt, b"postgresql://u:p@localhost:5432/db");

        // Reusing wrong counter or direction fails AEAD tag verification.
        assert!(matches!(
            decrypt_payload(&cipher_s, 2, 0, &ct),
            Err(BridgeError::AuthFailed)
        ));
    }

    #[test]
    fn replay_guard_blocks_duplicate_nonce_and_stale_clock() {
        let guard = ReplayGuard::new();
        let nonce = [7u8; 32];
        let now = 1_700_000_000;
        assert!(guard.check_and_record(nonce, now, now).is_ok());
        // Replay rejected
        assert!(matches!(
            guard.check_and_record(nonce, now, now),
            Err(BridgeError::AuthFailed)
        ));
        // Skew > 60s rejected
        assert!(matches!(
            guard.check_and_record([8u8; 32], now - 120, now),
            Err(BridgeError::AuthFailed)
        ));
    }

    #[test]
    fn rate_limiter_locks_out_after_five_failures() {
        let limiter = AuthRateLimiter::new();
        let ip = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10));
        for _ in 0..5 {
            assert!(limiter.check(ip).is_ok());
            limiter.record_failure(ip);
        }
        assert!(matches!(limiter.check(ip), Err(BridgeError::AccessDenied)));
    }
}
