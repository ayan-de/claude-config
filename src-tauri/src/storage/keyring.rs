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

use crate::models::AppError;

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

    pub fn set_token(&self, provider_id: &str, token: &str) -> Result<(), KeyringError> {
        self.ensure_available()?;
        let entry = keyring::Entry::new(KEYRING_SERVICE, provider_id)
            .map_err(|e| KeyringError::Backend(e.to_string()))?;
        entry
            .set_password(token)
            .map_err(|e| KeyringError::Backend(e.to_string()))
    }

    pub fn get_token(&self, provider_id: &str) -> Result<String, KeyringError> {
        self.ensure_available()?;
        let entry = keyring::Entry::new(KEYRING_SERVICE, provider_id)
            .map_err(|e| KeyringError::Backend(e.to_string()))?;
        entry
            .get_password()
            .map_err(|e| KeyringError::Backend(e.to_string()))
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