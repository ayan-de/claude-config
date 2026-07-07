//! Reads Claude Code's marketplace registry.
//!
//! Marketplaces live at `<claude_dir>/plugins/marketplaces/<name>/.claude-plugin/marketplace.json`.
//! Each subfolder of `plugins/marketplaces/` is one marketplace; we read its
//! manifest and extract a small summary for the UI. Per-marketplace plugin
//! inspection and add/remove are deferred — this module is read-only for now.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::models::{AppError, AppResult};
use crate::storage::settings::read_settings;

const MARKETPLACES_DIR: &str = "plugins/marketplaces";
const MARKETPLACES_MANIFEST: &str = ".claude-plugin/marketplace.json";

/// Subset of `marketplace.json` we care about. Be lenient — Claude Code's
/// manifest may grow fields over time, and a partial/older copy on disk
/// should still surface the marketplace instead of being dropped silently.
#[derive(Debug, Deserialize, Default)]
struct Manifest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    owner: Option<Owner>,
    #[serde(default)]
    metadata: Option<Metadata>,
    #[serde(default)]
    plugins: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize, Default)]
struct Owner {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize, Default)]
struct Metadata {
    #[serde(default)]
    description: String,
}

/// One row for the UI list. All fields optional except `name`, which falls
/// back to the directory name when the manifest is malformed or missing it.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MarketplaceSummary {
    pub name: String,
    pub owner: String,
    pub description: String,
    pub plugin_count: usize,
    /// Number of plugins from this marketplace that are currently enabled in
    /// `settings.json` (`enabledPlugins`). Mirrors `installed_plugins.len()` —
    /// kept as a separate field so the header pill doesn't recompute.
    pub installed_count: usize,
    /// Plugin names from this marketplace that are currently enabled in
    /// `settings.json`. Sorted alphabetically for stable rendering.
    pub installed_plugins: Vec<String>,
    /// Plugin names declared in the manifest that are NOT yet enabled.
    /// Order follows the manifest (Claude Code preserves author intent —
    /// featured/recommended plugins appear first).
    pub available_plugins: Vec<String>,
    pub source: String,
}

/// Scans `<claude_dir>/plugins/marketplaces/*` and returns one summary per
/// subdirectory. Missing marketplaces dir → empty vec (not an error —
/// no marketplaces is a valid state).
pub fn scan_marketplaces(claude_dir: &Path) -> AppResult<Vec<MarketplaceSummary>> {
    let marketplaces_dir = claude_dir.join(MARKETPLACES_DIR);

    // Missing dir is the "no marketplaces yet" case, not an error.
    if !marketplaces_dir.exists() {
        return Ok(Vec::new());
    }

    // Per-marketplace installed plugin names come from settings.json
    // `enabledPlugins`. Missing/malformed settings is not fatal — we just
    // report zero installed.
    let installed_by_marketplace = read_installed_plugins_by_marketplace(claude_dir);

    let entries = fs::read_dir(&marketplaces_dir).map_err(|e| {
        AppError::Io(std::io::Error::new(
            e.kind(),
            format!(
                "reading marketplaces dir {}: {e}",
                marketplaces_dir.display()
            ),
        ))
    })?;

    let mut out = Vec::new();
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                log::warn!("marketplaces: skipping unreadable entry: {e}");
                continue;
            }
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Source = where the dir came from. We can't reliably tell git vs
        // manual paste from disk state alone, so use the dir name as the
        // canonical identifier and report the manifest path the row was
        // derived from. Future: read `extraKnownMarketplaces` from
        // settings.json for the true source.
        let dir_name = entry
            .file_name()
            .to_string_lossy()
            .into_owned();

        let manifest_path = path.join(MARKETPLACES_MANIFEST);
        let summary = match fs::read_to_string(&manifest_path) {
            Ok(raw) => match serde_json::from_str::<Manifest>(&raw) {
                Ok(m) => {
                    let (mut s, manifest_names) =
                        summary_from_manifest(m, &dir_name, &manifest_path);
                    let installed_set = installed_by_marketplace
                        .get(&s.name)
                        .cloned()
                        .unwrap_or_default();
                    split_installed_available(&mut s, &manifest_names, &installed_set);
                    s
                }
                Err(e) => {
                    log::warn!(
                        "marketplaces: skipping {} (manifest parse error: {e})",
                        dir_name
                    );
                    continue;
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Subdir exists but no manifest yet — partial clone.
                // Surface it as a row so the user knows it's there.
                MarketplaceSummary {
                    name: dir_name.clone(),
                    owner: String::new(),
                    description: "(manifest not yet available)".into(),
                    plugin_count: 0,
                    installed_count: 0,
                    installed_plugins: Vec::new(),
                    available_plugins: Vec::new(),
                    source: manifest_path.display().to_string(),
                }
            }
            Err(e) => {
                log::warn!(
                    "marketplaces: skipping {} (manifest read error: {e})",
                    dir_name
                );
                continue;
            }
        };
        out.push(summary);
    }

    // Stable order so the UI doesn't shuffle between renders.
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

/// Reads `enabledPlugins` from `<claude_dir>/settings.json` and groups
/// installed plugin names by their `<marketplace>` half of each key.
///
/// Returns an empty map on any failure (missing/malformed settings) — the
/// caller should treat that as "we just don't know what's installed" and
/// surface empty lists rather than failing the whole marketplace scan.
fn read_installed_plugins_by_marketplace(
    claude_dir: &Path,
) -> HashMap<String, HashSet<String>> {
    let Ok(Some(settings)) = read_settings(&claude_dir.join("settings.json")) else {
        return HashMap::new();
    };
    let Some(entries) = settings.get("enabledPlugins").and_then(Value::as_object) else {
        return HashMap::new();
    };

    let mut by_marketplace: HashMap<String, HashSet<String>> = HashMap::new();
    for (key, value) in entries {
        // Claude Code accepts both `true` and `{enabled: true}`. Treat any
        // truthy / enabled entry as installed; ignore entries without an `@`.
        let Some((plugin, marketplace)) = key.split_once('@') else {
            log::warn!("enabledPlugins: skipping key without '@': {key}");
            continue;
        };
        if !is_enabled(value) {
            continue;
        }
        by_marketplace
            .entry(marketplace.to_string())
            .or_default()
            .insert(plugin.to_string());
    }
    by_marketplace
}

fn is_enabled(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Object(obj) => obj
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        _ => false,
    }
}

/// Build a `MarketplaceSummary` from a parsed manifest and return the
/// manifest's plugin name list (in author-declared order). The plugin list
/// is needed later to split installed vs available; returning it together
/// avoids re-parsing the manifest.
fn summary_from_manifest(
    m: Manifest,
    dir_name: &str,
    manifest_path: &Path,
) -> (MarketplaceSummary, Vec<String>) {
    let plugin_names = extract_plugin_names(&m.plugins);
    let summary = MarketplaceSummary {
        // Manifest name wins, but fall back to the dir so a malformed /
        // older manifest still shows up.
        name: if m.name.is_empty() {
            dir_name.to_string()
        } else {
            m.name
        },
        owner: m.owner.map(|o| o.name).unwrap_or_default(),
        description: m.metadata.map(|md| md.description).unwrap_or_default(),
        plugin_count: plugin_names.len(),
        installed_count: 0,
        installed_plugins: Vec::new(),
        available_plugins: Vec::new(),
        source: manifest_path.display().to_string(),
    };
    (summary, plugin_names)
}

/// Pull the `name` field from each plugin object in manifest order.
/// Skips entries without a non-empty `name` — they're malformed and
/// can't be cross-referenced to settings.json anyway.
fn extract_plugin_names(plugins: &[serde_json::Value]) -> Vec<String> {
    plugins
        .iter()
        .filter_map(|p| {
            p.get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .collect()
}

/// Populate `installed_plugins`, `available_plugins`, and `installed_count`
/// against the manifest's plugin list.
///
/// Order semantics:
/// - `installed_plugins`: alphabetical (stable rendering across refreshes).
/// - `available_plugins`: manifest order (preserves featured/recommended
///   placement that the marketplace author chose).
///
/// Installed names that don't appear in the manifest (e.g. from a removed
/// marketplace or a typo) are NOT shown here — settings.json remains the
/// authoritative record; this view only surfaces what the marketplace
/// declares.
fn split_installed_available(
    summary: &mut MarketplaceSummary,
    manifest_plugin_names: &[String],
    installed_set: &HashSet<String>,
) {
    let mut installed_sorted: Vec<String> = manifest_plugin_names
        .iter()
        .filter(|n| installed_set.contains(*n))
        .cloned()
        .collect();
    installed_sorted.sort();

    let available_plugins: Vec<String> = manifest_plugin_names
        .iter()
        .filter(|n| !installed_set.contains(*n))
        .cloned()
        .collect();

    summary.installed_count = installed_sorted.len();
    summary.installed_plugins = installed_sorted;
    summary.available_plugins = available_plugins;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, content).unwrap();
    }

    #[test]
    fn missing_dir_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let out = scan_marketplaces(tmp.path()).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn reads_multiple_marketplaces_with_plugin_counts() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "plugins/marketplaces/awesome/.claude-plugin/marketplace.json",
            r#"{
                "name": "awesome",
                "owner": {"name": "Alice"},
                "metadata": {"description": "Cool plugins"},
                "plugins": [
                    {"name": "a"}, {"name": "b"}, {"name": "c"}
                ]
            }"#,
        );
        write(
            tmp.path(),
            "plugins/marketplaces/b-market/.claude-plugin/marketplace.json",
            r#"{
                "name": "b-market",
                "owner": {"name": "Bob"},
                "metadata": {"description": "More stuff"},
                "plugins": [{"name": "x"}]
            }"#,
        );
        let out = scan_marketplaces(tmp.path()).unwrap();
        assert_eq!(out.len(), 2);
        // Sorted alphabetically (case-insensitive).
        assert_eq!(out[0].name, "awesome");
        assert_eq!(out[0].owner, "Alice");
        assert_eq!(out[0].description, "Cool plugins");
        assert_eq!(out[0].plugin_count, 3);
        assert_eq!(out[0].installed_count, 0);
        assert!(out[0].installed_plugins.is_empty());
        assert_eq!(out[0].available_plugins, vec!["a", "b", "c"]);
        assert_eq!(out[1].name, "b-market");
        assert_eq!(out[1].plugin_count, 1);
        assert_eq!(out[1].installed_count, 0);
        assert_eq!(out[1].available_plugins, vec!["x"]);
    }

    #[test]
    fn installed_count_reflects_enabled_plugins_per_marketplace() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "plugins/marketplaces/awesome/.claude-plugin/marketplace.json",
            r#"{
                "name": "awesome",
                "plugins": [{"name": "a"}, {"name": "b"}, {"name": "c"}]
            }"#,
        );
        write(
            tmp.path(),
            "plugins/marketplaces/b-market/.claude-plugin/marketplace.json",
            r#"{
                "name": "b-market",
                "plugins": [{"name": "x"}]
            }"#,
        );
        // 2 enabled in awesome, 1 in b-market, 1 disabled, 1 in a third
        // marketplace that doesn't exist on disk yet (still counted in
        // settings — not the scanner's job to filter).
        write(
            tmp.path(),
            "settings.json",
            r#"{
                "enabledPlugins": {
                    "a@awesome": true,
                    "b@awesome": {"enabled": true},
                    "x@b-market": true,
                    "old@awesome": false,
                    "future@ghost": true
                }
            }"#,
        );

        let out = scan_marketplaces(tmp.path()).unwrap();
        let by_name: std::collections::HashMap<_, _> =
            out.iter().map(|m| (m.name.as_str(), m)).collect();
        assert_eq!(by_name["awesome"].installed_count, 2);
        assert_eq!(by_name["b-market"].installed_count, 1);
    }

    #[test]
    fn installed_plugins_are_sorted_alphabetically() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "plugins/marketplaces/awesome/.claude-plugin/marketplace.json",
            r#"{
                "name": "awesome",
                "plugins": [
                    {"name": "zebra"}, {"name": "alpha"}, {"name": "mango"}
                ]
            }"#,
        );
        write(
            tmp.path(),
            "settings.json",
            r#"{
                "enabledPlugins": {
                    "mango@awesome": true,
                    "zebra@awesome": true,
                    "alpha@awesome": true
                }
            }"#,
        );
        let out = scan_marketplaces(tmp.path()).unwrap();
        assert_eq!(out[0].installed_plugins, vec!["alpha", "mango", "zebra"]);
        // Available preserves manifest order.
        assert_eq!(out[0].available_plugins, Vec::<String>::new());
    }

    #[test]
    fn available_plugins_preserve_manifest_order() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "plugins/marketplaces/awesome/.claude-plugin/marketplace.json",
            r#"{
                "name": "awesome",
                "plugins": [
                    {"name": "featured"},
                    {"name": "middle"},
                    {"name": "another"},
                    {"name": "first"}
                ]
            }"#,
        );
        write(
            tmp.path(),
            "settings.json",
            r#"{
                "enabledPlugins": {
                    "featured@awesome": true
                }
            }"#,
        );
        let out = scan_marketplaces(tmp.path()).unwrap();
        assert_eq!(out[0].installed_plugins, vec!["featured"]);
        // Manifest order preserved (not alphabetical).
        assert_eq!(
            out[0].available_plugins,
            vec!["middle", "another", "first"]
        );
    }

    #[test]
    fn installed_names_not_in_manifest_are_ignored_in_summary() {
        // enabledPlugins may reference plugins no longer in the manifest
        // (marketplace removed, renamed, or stale entry). Those shouldn't
        // appear under the marketplace row — settings.json is the source
        // of truth and still authoritatively enables them.
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "plugins/marketplaces/awesome/.claude-plugin/marketplace.json",
            r#"{
                "name": "awesome",
                "plugins": [{"name": "a"}]
            }"#,
        );
        write(
            tmp.path(),
            "settings.json",
            r#"{
                "enabledPlugins": {
                    "a@awesome": true,
                    "ghost@awesome": true,
                    "phantom@other": true
                }
            }"#,
        );
        let out = scan_marketplaces(tmp.path()).unwrap();
        // Only `a` shows up; ghost is silently dropped from this row.
        assert_eq!(out[0].installed_plugins, vec!["a"]);
        assert_eq!(out[0].installed_count, 1);
        assert_eq!(out[0].available_plugins, Vec::<String>::new());
    }

    #[test]
    fn missing_settings_means_zero_installed_not_error() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "plugins/marketplaces/awesome/.claude-plugin/marketplace.json",
            r#"{"name": "awesome", "plugins": [{"name": "a"}]}"#,
        );
        // No settings.json at all.
        let out = scan_marketplaces(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].installed_count, 0);
        assert!(out[0].installed_plugins.is_empty());
        assert_eq!(out[0].available_plugins, vec!["a"]);
    }

    #[test]
    fn malformed_settings_does_not_break_scan() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "plugins/marketplaces/awesome/.claude-plugin/marketplace.json",
            r#"{"name": "awesome", "plugins": [{"name": "a"}]}"#,
        );
        write(tmp.path(), "settings.json", "{not json");
        let out = scan_marketplaces(tmp.path()).unwrap();
        // Settings read fails → counts default to zero, scan still succeeds.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].installed_count, 0);
    }

    #[test]
    fn skips_subdirs_without_manifest_but_lists_them() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("plugins/marketplaces/empty")).unwrap();
        // not-json: dir + .claude-plugin/marketplace.json with invalid JSON
        write(
            tmp.path(),
            "plugins/marketplaces/not-json/.claude-plugin/marketplace.json",
            "not json",
        );

        let out = scan_marketplaces(tmp.path()).unwrap();
        // The empty dir is surfaced (no manifest); the malformed one is dropped.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "empty");
        assert!(out[0].description.contains("not yet available"));
        assert!(out[0].installed_plugins.is_empty());
        assert!(out[0].available_plugins.is_empty());
    }

    #[test]
    fn missing_name_falls_back_to_dirname() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "plugins/marketplaces/orphan/.claude-plugin/marketplace.json",
            r#"{"metadata": {"description": "no name field"}, "plugins": []}"#,
        );
        let out = scan_marketplaces(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "orphan");
        assert_eq!(out[0].description, "no name field");
    }

    #[test]
    fn non_directories_in_marketplaces_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("plugins/marketplaces")).unwrap();
        fs::write(
            tmp.path().join("plugins/marketplaces/stray.txt"),
            "ignore me",
        )
        .unwrap();
        let out = scan_marketplaces(tmp.path()).unwrap();
        assert!(out.is_empty());
    }
}