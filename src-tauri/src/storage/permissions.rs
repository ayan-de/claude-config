//! Pure logic for the `permissions` block in `~/.claude/settings.json`.
//! I/O lives in `set()` (see Step 8 below) which delegates to
//! `storage::settings::write_settings_atomic` so the same lock + backup
//! semantics as `load_provider_cmd` apply.

use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{AppError, AppResult};
use crate::storage::settings;

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Permissions {
    #[serde(rename = "defaultMode", default, skip_serializing_if = "Option::is_none")]
    pub default_mode: Option<String>,
}

/// Pure: what should the `permissions` block look like for the requested state?
pub fn block_for(enabled: bool) -> Permissions {
    if enabled {
        Permissions {
            default_mode: Some("bypassPermissions".into()),
        }
    } else {
        Permissions::default()
    }
}

/// Read current state. Conservative: returns `false` if `settings.json` is
/// missing, malformed, has no `permissions` block, or `defaultMode` is
/// anything other than the literal string `"bypassPermissions"`.
pub fn read(settings: &Value) -> bool {
    settings
        .get("permissions")
        .and_then(|p| p.get("defaultMode"))
        .and_then(|v| v.as_str())
        == Some("bypassPermissions")
}

/// Locked + backed-up atomic write. Reuses `settings::write_settings_atomic`
/// so the same sidecar lock and timestamped backup as `load_provider_cmd`
/// apply — no race between the two writers, no backup-policy divergence.
///
/// If `settings.json` doesn't exist yet, this creates it with the `permissions`
/// block as the only key. Other top-level keys (env, hooks, plugins, etc.) are
/// preserved verbatim — the closure mutates only the `permissions` field.
pub fn set(path: &Path, backups_dir: &Path, enabled: bool) -> AppResult<()> {
    let mut value = match settings::read_settings(path)? {
        Some(v) => v,
        None => Value::Object(Default::default()),
    };
    value["permissions"] = serde_json::to_value(block_for(enabled))
        .map_err(|e| AppError::Internal(format!("serialize permissions: {e}")))?;
    settings::write_settings_atomic(path, &value, backups_dir)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn block_for_on_emits_bypass_permissions() {
        let p = block_for(true);
        assert_eq!(p.default_mode.as_deref(), Some("bypassPermissions"));
    }

    #[test]
    fn block_for_off_is_empty() {
        let p = block_for(false);
        assert!(p.default_mode.is_none());
        // Serialized form must be `{}` — proves skip_serializing_if works.
        assert_eq!(serde_json::to_value(&p).unwrap(), json!({}));
    }

    #[test]
    fn read_returns_false_on_missing_key() {
        let s = json!({ "env": { "ANTHROPIC_BASE_URL": "x" } });
        assert!(!read(&s));
    }

    #[test]
    fn read_returns_false_on_empty_permissions() {
        assert!(!read(&json!({ "permissions": {} })));
    }

    #[test]
    fn read_returns_true_on_bypass_permissions() {
        assert!(read(&json!({ "permissions": { "defaultMode": "bypassPermissions" } })));
    }

    #[test]
    fn read_ignores_other_permission_keys() {
        // If the user has disableBypassPermissionsMode or future keys, our
        // read must not mistake them for "on".
        let s = json!({
            "permissions": {
                "defaultMode": "default",
                "disableBypassPermissionsMode": "disable"
            }
        });
        assert!(!read(&s));
    }

    #[test]
    fn roundtrip_on_then_off_keeps_empty_object() {
        let mut s = json!({});
        s["permissions"] = serde_json::to_value(block_for(true)).unwrap();
        assert!(read(&s));
        s["permissions"] = serde_json::to_value(block_for(false)).unwrap();
        assert!(!read(&s));
        assert!(s["permissions"].is_object());
    }

    use std::fs;
    use std::path::PathBuf;

    fn fresh_dir(name: &str) -> PathBuf {
        let d = tempfile::tempdir().unwrap().keep().join(name);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn set_writes_and_round_trips() {
        let dir = fresh_dir("claude");
        let path = dir.join("settings.json");
        let backups = fresh_dir("backups");
        fs::write(&path, "{}").unwrap();

        super::set(&path, &backups, true).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        // write_settings_atomic uses to_vec_pretty, so the colon is
        // followed by a space — match that.
        assert!(after.contains("\"defaultMode\": \"bypassPermissions\""));

        super::set(&path, &backups, false).unwrap();
        let after = fs::read_to_string(&path).unwrap();
        assert!(after.contains("\"permissions\": {}"));
    }

    #[test]
    fn set_preserves_unrelated_keys() {
        let dir = fresh_dir("claude");
        let path = dir.join("settings.json");
        let backups = fresh_dir("backups");
        fs::write(
            &path,
            serde_json::to_string_pretty(&json!({
                "env": { "ANTHROPIC_BASE_URL": "https://x" },
                "hooks": { "Stop": [] },
                "extraKnownMarketplaces": { "foo": 1 }
            }))
            .unwrap(),
        )
        .unwrap();

        super::set(&path, &backups, true).unwrap();
        let after: Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(after["env"]["ANTHROPIC_BASE_URL"], "https://x");
        assert_eq!(after["hooks"]["Stop"], json!([]));
        assert_eq!(after["extraKnownMarketplaces"]["foo"], 1);
        assert_eq!(after["permissions"]["defaultMode"], "bypassPermissions");
    }

    #[test]
    fn set_creates_file_when_missing() {
        let dir = fresh_dir("claude");
        let path = dir.join("settings.json");
        let backups = fresh_dir("backups");
        // No file written — simulates first run on a fresh machine.
        super::set(&path, &backups, true).unwrap();
        assert!(path.exists());
        let after: Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(after["permissions"]["defaultMode"], "bypassPermissions");
    }
}