//! Pure logic for the `permissions` block in `~/.claude/settings.json`.
//! I/O lives in `set()` (see Step 8 below) which delegates to
//! `storage::settings::write_settings_atomic` so the same lock + backup
//! semantics as `load_provider_cmd` apply.

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
}