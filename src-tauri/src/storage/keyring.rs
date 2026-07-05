//! Thin wrapper over the `keyring` crate with platform fallback.
//!
//! Auth tokens are stored under (service, account) where:
//!   - service = `KEYRING_SERVICE` ("claude-config")
//!   - account = provider.id (uuid v4 string)
//!
//! On Linux this uses the Secret Service (libsecret / GNOME Keyring /
//! KWallet via dbus). If the keyring is unavailable (no daemon, headless
//! env), `KeyringStore::status()` returns `Unavailable` and all writes/reads
//! fail. The UI must surface this prominently and refuse to save new
//! providers.

use std::sync::Arc;

use crate::models::{AppError, ProviderSecret};

pub const KEYRING_SERVICE: &str = "claude-config";

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum KeyringStatus {
    Available,
    Unavailable { message: String },
}

#[derive(Debug)]
pub enum KeyringError {
    Unavailable(String),
    Backend(String),
}

impl std::fmt::Display for KeyringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyringError::Unavailable(m) => {
                write!(f, "OS keyring is unavailable: {m}")
            }
            KeyringError::Backend(m) => write!(f, "keyring backend error: {m}"),
        }
    }
}

impl std::error::Error for KeyringError {}

impl From<KeyringError> for AppError {
    fn from(e: KeyringError) -> Self {
        match e {
            KeyringError::Unavailable(m) => AppError::KeyringUnavailable(m),
            KeyringError::Backend(m) => AppError::Keyring(m),
        }
    }
}

/// Cheap, thread-safe handle to the OS keyring. The underlying crate is
/// already sync; we wrap in Arc so commands can share one instance.
#[derive(Clone)]
pub struct KeyringStore {
    inner: Arc<KeyringInner>,
}

struct KeyringInner {
    status: KeyringStatus,
}

impl KeyringStore {
    pub fn new() -> Self {
        let status = probe_keyring();
        Self {
            inner: Arc::new(KeyringInner { status }),
        }
    }

    pub fn status(&self) -> KeyringStatus {
        self.inner.status.clone()
    }

    pub fn is_available(&self) -> bool {
        matches!(self.inner.status, KeyringStatus::Available)
    }

    /// Write a `ProviderSecret` as a JSON blob into the keyring entry for
    /// `provider_id`. This is the multi-kind-aware storage path. Prefer this
    /// over `set_token` for any new code.
    pub fn set_secret(
        &self,
        provider_id: &str,
        secret: &ProviderSecret,
    ) -> Result<(), KeyringError> {
        self.ensure_available()?;
        let json = serde_json::to_string(secret)
            .map_err(|e| KeyringError::Backend(format!("serialize secret: {e}")))?;
        let entry = keyring::Entry::new(KEYRING_SERVICE, provider_id)
            .map_err(|e| KeyringError::Backend(e.to_string()))?;
        entry
            .set_password(&json)
            .map_err(|e| KeyringError::Backend(e.to_string()))
    }

    /// Read a `ProviderSecret` from the keyring entry for `provider_id`.
    ///
    /// Backwards compatibility: for entries written by the schema-v1 codepath,
    /// the keyring value is a raw token string (not JSON). If parsing as
    /// `ProviderSecret` fails, we fall back to interpreting the value as
    /// `ProviderSecret::Custom { auth_token }` — v1 only ever stored
    /// custom-relay tokens.
    pub fn get_secret(&self, provider_id: &str) -> Result<ProviderSecret, KeyringError> {
        self.ensure_available()?;
        let entry = keyring::Entry::new(KEYRING_SERVICE, provider_id)
            .map_err(|e| KeyringError::Backend(e.to_string()))?;
        let raw = entry
            .get_password()
            .map_err(|e| KeyringError::Backend(e.to_string()))?;
        match serde_json::from_str::<ProviderSecret>(&raw) {
            Ok(secret) => Ok(secret),
            Err(_) => Ok(ProviderSecret::Custom { auth_token: raw }),
        }
    }

    /// Legacy helper: write a raw auth token for a Custom-kind provider.
    /// Wraps `set_secret` with `ProviderSecret::Custom` so callers that only
    /// have a token string (legacy tests) keep working.
    #[allow(dead_code)]
    pub fn set_token(&self, provider_id: &str, token: &str) -> Result<(), KeyringError> {
        self.set_secret(
            provider_id,
            &ProviderSecret::Custom {
                auth_token: token.to_string(),
            },
        )
    }

    /// Legacy helper: read a Custom-kind auth token. Returns an error for
    /// other kinds — callers should migrate to `get_secret`.
    #[allow(dead_code)]
    pub fn get_token(&self, provider_id: &str) -> Result<String, KeyringError> {
        match self.get_secret(provider_id)? {
            ProviderSecret::Custom { auth_token } => Ok(auth_token),
            other => Err(KeyringError::Backend(format!(
                "provider {provider_id} secret is not a custom auth token (kind: {:?})",
                other.kind()
            ))),
        }
    }

    pub fn delete_token(&self, provider_id: &str) -> Result<(), KeyringError> {
        self.ensure_available()?;
        let entry = keyring::Entry::new(KEYRING_SERVICE, provider_id)
            .map_err(|e| KeyringError::Backend(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()), // already gone
            Err(e) => Err(KeyringError::Backend(e.to_string())),
        }
    }

    fn ensure_available(&self) -> Result<(), KeyringError> {
        match &self.inner.status {
            KeyringStatus::Available => Ok(()),
            KeyringStatus::Unavailable { message } => {
                Err(KeyringError::Unavailable(message.clone()))
            }
        }
    }
}

impl Default for KeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

fn probe_keyring() -> KeyringStatus {
    // Probe by trying to write + delete a dummy entry under a probe account.
    // This catches the common failure modes (no Secret Service running, no
    // desktop session, locked keyring) at startup.
    let probe_account = "__claude_config_probe__";
    let entry = match keyring::Entry::new(KEYRING_SERVICE, probe_account) {
        Ok(e) => e,
        Err(e) => {
            return KeyringStatus::Unavailable {
                message: format!("could not init keyring entry: {e}"),
            }
        }
    };
    let probe_value = "probe";
    if let Err(e) = entry.set_password(probe_value) {
        return KeyringStatus::Unavailable {
            message: format!("set_password failed: {e}"),
        };
    }
    if let Err(e) = entry.get_password() {
        let _ = entry.delete_credential();
        return KeyringStatus::Unavailable {
            message: format!("get_password failed: {e}"),
        };
    }
    if let Err(e) = entry.delete_credential() {
        // Non-fatal — the probe value will be re-deleted on next launch.
        log::warn!("probe cleanup failed: {e}");
    }
    KeyringStatus::Available
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    /// This test exercises the real keyring. On CI/headless environments
    /// it will skip via the probe status. Marked ignore by default so it
    /// only runs when explicitly requested (`cargo test keyring -- --ignored`).
    #[test]
    #[ignore = "exercises real OS keyring; run with --ignored"]
    fn roundtrip_token_through_keyring() {
        let store = KeyringStore::new();
        if !store.is_available() {
            eprintln!("keyring unavailable; skipping");
            return;
        }
        let id = Uuid::new_v4().to_string();
        store.set_token(&id, "secret-xyz").unwrap();
        assert_eq!(store.get_token(&id).unwrap(), "secret-xyz");
        store.delete_token(&id).unwrap();
        assert!(store.get_token(&id).is_err());
    }

    #[test]
    fn status_is_deterministic() {
        let a = KeyringStore::new();
        let b = KeyringStore::new();
        assert_eq!(a.is_available(), b.is_available());
    }
}