//! Reads Claude Code's marketplace registry.
//!
//! Marketplaces live at `<claude_dir>/plugins/marketplaces/<name>/.claude-plugin/marketplace.json`.
//! Each subfolder of `plugins/marketplaces/` is one marketplace; we read its
//! manifest and extract a small summary for the UI. Per-marketplace plugin
//! inspection and add/remove are deferred — this module is read-only for now.

use std::fs;
use std::path::Path;

use serde::Deserialize;

use crate::models::{AppError, AppResult};

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
                Ok(m) => summary_from_manifest(m, &dir_name, &manifest_path),
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

fn summary_from_manifest(
    m: Manifest,
    dir_name: &str,
    manifest_path: &Path,
) -> MarketplaceSummary {
    MarketplaceSummary {
        // Manifest name wins, but fall back to the dir so a malformed /
        // older manifest still shows up.
        name: if m.name.is_empty() {
            dir_name.to_string()
        } else {
            m.name
        },
        owner: m.owner.map(|o| o.name).unwrap_or_default(),
        description: m.metadata.map(|md| md.description).unwrap_or_default(),
        plugin_count: m.plugins.len(),
        source: manifest_path.display().to_string(),
    }
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
        assert_eq!(out[1].name, "b-market");
        assert_eq!(out[1].plugin_count, 1);
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
