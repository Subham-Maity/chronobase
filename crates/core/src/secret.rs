//! Secret wrapper and URL masking utilities.
//!
//! Connection URLs contain database credentials and must never appear in
//! `Debug` output, `Display` output, logs, or subprocess command-line arguments.

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use url::Url;

/// Newtype wrapper around sensitive strings (such as database connection URLs).
///
/// Both `Debug` and `Display` output `"[REDACTED]"` so accidental formatting
/// never leaks credentials into logs or error messages.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

impl Secret {
    /// Wraps a raw secret string.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Exposes the underlying secret value only at the explicit point of use
    /// (such as setting `PGPASSWORD` or writing a `0600` temp config file).
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Returns a masked representation safe for UI display (`:***@`).
    pub fn masked(&self) -> String {
        mask_url(&self.0)
    }

    /// Returns `true` if the underlying string is empty or whitespace-only.
    pub fn is_blank(&self) -> bool {
        self.0.trim().is_empty()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl Serialize for Secret {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Serialized directly to the local-only JSON store for v1 compatibility.
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Secret {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ok(Self(raw))
    }
}

/// Masks the password segment of a database connection URL for safe UI display.
///
/// Falls back to a simple `:...@` scan if the string is not a strict RFC-3986 URL.
pub fn mask_url(raw: &str) -> String {
    if let Ok(mut parsed) = Url::parse(raw)
        && parsed.password().is_some()
    {
        // Replace password with "***" via url::Url so special chars are handled cleanly.
        if parsed.set_password(Some("***")).is_ok() {
            return parsed.to_string();
        }
    }

    // Fallback for non-standard connection strings: mask between ':' after scheme and '@'.
    if let Some(scheme_end) = raw.find("://") {
        let rest = &raw[scheme_end + 3..];
        if let Some(at_pos) = rest.find('@') {
            let userinfo = &rest[..at_pos];
            if let Some(colon_pos) = userinfo.find(':') {
                let prefix = &raw[..scheme_end + 3 + colon_pos + 1];
                let suffix = &rest[at_pos..];
                return format!("{prefix}***{suffix}");
            }
        }
    }

    raw.to_string()
}

/// Splits a PostgreSQL URL into `(sanitized_url_without_password, Option<password>)`.
///
/// Security: passing the password in `PGPASSWORD` instead of `argv` prevents
/// other local users from reading the database password via `ps` / `/proc`.
pub fn split_postgres_url(raw: &str) -> Result<(String, Option<String>), url::ParseError> {
    let mut parsed = Url::parse(raw)?;
    let password = parsed.password().map(ToString::to_string);
    if password.is_some() {
        // Intentionally ignore: set_password(None) only fails if cannot_be_a_base(),
        // which is impossible for valid postgres:// / postgresql:// URLs.
        let _ = parsed.set_password(None);
    }
    Ok((parsed.to_string(), password))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_redacts_debug_and_display() {
        let s = Secret::new("postgres://user:supersecret@localhost:5432/db");
        assert_eq!(format!("{s:?}"), "[REDACTED]");
        assert_eq!(format!("{s}"), "[REDACTED]");
        assert!(!format!("{s:?}").contains("supersecret"));
    }

    #[test]
    fn mask_url_hides_password() {
        let masked = mask_url("postgresql://admin:p@ss:word@db.example.com:5432/prod");
        assert!(!masked.contains("p@ss:word"));
        assert!(masked.contains("***@db.example.com:5432/prod"));
    }

    #[test]
    fn mask_url_leaves_passwordless_url_unchanged() {
        let url = "postgresql://localhost:5432/prod";
        assert_eq!(mask_url(url), url);
    }

    #[test]
    fn split_postgres_url_extracts_password_and_strips_from_uri() {
        let (clean, pass) =
            split_postgres_url("postgresql://alice:s3cr3t@localhost:5432/mydb?sslmode=require")
                .expect("valid url");
        assert_eq!(pass.as_deref(), Some("s3cr3t"));
        assert!(!clean.contains("s3cr3t"));
        assert!(clean.contains("alice@localhost:5432/mydb?sslmode=require"));
    }
}
