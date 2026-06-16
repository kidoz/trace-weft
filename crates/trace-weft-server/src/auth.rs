//! API-key authentication and per-tenant resolution.
//!
//! Keys are configured as `raw_key:project_id` pairs; the raw key is hashed
//! with SHA-256 at load time and never retained, so the in-memory store only
//! ever holds hashes. A presented `Authorization: Bearer <key>` header is
//! hashed and compared against the stored hashes in constant time.

use std::collections::HashMap;

use axum::http::HeaderMap;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

/// How a request resolved against the configured keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auth {
    /// A valid key mapped to this project; queries are scoped to it and ingested
    /// spans are tagged with it.
    Project(String),
    /// The env-gated dev bypass is enabled: no key required, queries see every
    /// tenant, and ingested spans are left untagged.
    DevBypass,
}

impl Auth {
    /// The project a query/ingest should be scoped to, or `None` to span all
    /// tenants (dev bypass).
    pub fn project(&self) -> Option<&str> {
        match self {
            Auth::Project(p) => Some(p),
            Auth::DevBypass => None,
        }
    }
}

/// Resolved API-key configuration: a map of `sha256(key)` hex → project id, plus
/// whether the dev bypass is enabled.
#[derive(Clone, Default)]
pub struct AuthConfig {
    keys: HashMap<String, String>,
    dev_mode: bool,
}

impl AuthConfig {
    /// Build from environment:
    /// - `TRACE_WEFT_API_KEYS`: comma-separated `raw_key:project_id` pairs.
    /// - `TRACE_WEFT_DEV_MODE=1` (or `true`): enable the dev bypass (off by
    ///   default).
    pub fn from_env() -> Self {
        let pairs = std::env::var("TRACE_WEFT_API_KEYS").unwrap_or_default();
        let raw_keys = pairs.split(',').filter_map(|pair| {
            let (key, project) = pair.trim().split_once(':')?;
            let (key, project) = (key.trim(), project.trim());
            (!key.is_empty() && !project.is_empty()).then(|| (key.to_string(), project.to_string()))
        });
        let dev_mode = matches!(
            std::env::var("TRACE_WEFT_DEV_MODE").as_deref(),
            Ok("1") | Ok("true")
        );
        let config = Self::new(raw_keys, dev_mode);
        if config.keys.is_empty() && !dev_mode {
            tracing::warn!(
                "No API keys configured and dev mode is off; all requests will be rejected with 401. \
                 Set TRACE_WEFT_API_KEYS or TRACE_WEFT_DEV_MODE=1."
            );
        }
        config
    }

    /// Construct from raw `(key, project_id)` pairs, hashing each key. Raw keys
    /// are dropped after hashing.
    pub fn new(raw_keys: impl IntoIterator<Item = (String, String)>, dev_mode: bool) -> Self {
        let keys = raw_keys
            .into_iter()
            .map(|(key, project)| (hash_key(&key), project))
            .collect();
        Self { keys, dev_mode }
    }

    /// Resolve the `Authorization: Bearer <key>` header to a tenant. Returns
    /// `None` when no valid key is presented and the dev bypass is off — the
    /// caller should answer `401` in that case.
    pub fn authenticate(&self, headers: &HeaderMap) -> Option<Auth> {
        if let Some(project) = bearer_token(headers).and_then(|token| self.lookup(&token)) {
            return Some(Auth::Project(project));
        }
        self.dev_mode.then_some(Auth::DevBypass)
    }

    /// Hash the presented key and compare it against every stored hash without
    /// short-circuiting, so lookup time does not leak which key (if any)
    /// matched.
    fn lookup(&self, presented: &str) -> Option<String> {
        let presented_hash = hash_key(presented);
        let mut matched: Option<String> = None;
        for (stored_hash, project) in &self.keys {
            // Hex SHA-256 strings are always equal length, so ct_eq is valid.
            if bool::from(stored_hash.as_bytes().ct_eq(presented_hash.as_bytes())) {
                matched = Some(project.clone());
            }
        }
        matched
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get("Authorization")?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .map(|token| token.trim().to_string())
        .filter(|token| !token.is_empty())
}

fn hash_key(key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn headers_with(auth: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("Authorization", HeaderValue::from_str(auth).unwrap());
        headers
    }

    fn config() -> AuthConfig {
        AuthConfig::new(
            [
                ("tw-alpha-key".to_string(), "proj_alpha".to_string()),
                ("tw-beta-key".to_string(), "proj_beta".to_string()),
            ],
            false,
        )
    }

    #[test]
    fn valid_key_resolves_to_its_project() {
        let auth = config().authenticate(&headers_with("Bearer tw-alpha-key"));
        assert_eq!(auth, Some(Auth::Project("proj_alpha".to_string())));

        let auth = config().authenticate(&headers_with("Bearer tw-beta-key"));
        assert_eq!(auth, Some(Auth::Project("proj_beta".to_string())));
    }

    #[test]
    fn unknown_key_is_rejected() {
        assert_eq!(
            config().authenticate(&headers_with("Bearer tw-unknown")),
            None
        );
    }

    #[test]
    fn missing_or_malformed_header_is_rejected() {
        assert_eq!(config().authenticate(&HeaderMap::new()), None);
        assert_eq!(config().authenticate(&headers_with("tw-alpha-key")), None);
        assert_eq!(config().authenticate(&headers_with("Bearer ")), None);
    }

    #[test]
    fn dev_bypass_only_works_when_enabled() {
        // Off: unknown/absent key is rejected.
        let strict = AuthConfig::new([], false);
        assert_eq!(strict.authenticate(&HeaderMap::new()), None);

        // On: absent key falls through to the bypass.
        let dev = AuthConfig::new([], true);
        assert_eq!(dev.authenticate(&HeaderMap::new()), Some(Auth::DevBypass));
    }

    #[test]
    fn valid_key_takes_precedence_over_dev_bypass() {
        let dev = AuthConfig::new(
            [("tw-alpha-key".to_string(), "proj_alpha".to_string())],
            true,
        );
        // A recognized key still resolves to its project even in dev mode.
        assert_eq!(
            dev.authenticate(&headers_with("Bearer tw-alpha-key")),
            Some(Auth::Project("proj_alpha".to_string()))
        );
        // An unrecognized key falls back to the bypass rather than 401.
        assert_eq!(
            dev.authenticate(&headers_with("Bearer tw-nope")),
            Some(Auth::DevBypass)
        );
    }

    #[test]
    fn stored_config_holds_hashes_not_raw_keys() {
        let config = config();
        // The raw key never appears as a stored map key.
        assert!(!config.keys.contains_key("tw-alpha-key"));
        // sha256("tw-alpha-key") hex is 64 chars.
        assert!(config.keys.keys().all(|k| k.len() == 64));
    }
}
