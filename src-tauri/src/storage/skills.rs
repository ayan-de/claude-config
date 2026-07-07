//! Reads Claude Code's installed skills.
//!
//! Skills are `SKILL.md` files Claude Code loads on demand. Two sources:
//!
//! - **User-authored**: `<claude_dir>/skills/<skill-name>/SKILL.md`. Recursive
//!   walk so multi-folder layouts work, but in practice each skill is one
//!   folder with one `SKILL.md`. Symlinks are followed once and deduped by
//!   canonical path so the same skill installed in two locations (e.g. via
//!   stow) doesn't show up twice.
//!
//! - **Plugin-bundled**: SKILL.md files shipped inside plugins the user has
//!   installed. Claude Code records the install record at
//!   `<claude_dir>/plugins/installed_plugins.json` (schema v2) — that's the
//!   authoritative source for *which plugins are installed and where they
//!   live on disk*. For each record we look inside the plugin at a small
//!   set of well-known skill directories (in order, first-hit wins per
//!   skill-name) so that mirrors under `.agents/`, `.cursor/` etc. don't
//!   duplicate the canonical entry under `skills/`.
//!
//! Frontmatter is parsed leniently — a missing or malformed YAML block is
//! not an error; we just surface an empty description.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::models::{AppError, AppResult};
use crate::storage::settings::read_settings;

const USER_SKILLS_DIR: &str = "skills";
const SKILL_FILENAME: &str = "SKILL.md";

/// Plugin-side candidate dirs to scan, in priority order. First hit wins for
/// a given skill-name so we don't show the same skill twice when it lives in
/// both the canonical `skills/` and a clone under `.agents/` / `.cursor/`.
const PLUGIN_SKILL_DIRS: &[&str] = &[
    "skills",
    ".claude/skills",
    ".agents/skills",
    ".cursor/skills",
];

const INSTALLED_PLUGINS_FILE: &str = "plugins/installed_plugins.json";

/// Where a skill came from. Drives grouping in the UI: all `User` rows
/// render in one section; `Plugin` rows are grouped by `plugin@marketplace`.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum SkillSource {
    User,
    Plugin {
        plugin: String,
        marketplace: String,
        version: String,
    },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SkillSummary {
    pub name: String,
    pub description: String,
    pub source: SkillSource,
    /// Absolute path to the SKILL.md file. Drives the tooltip and any
    /// future "reveal in file manager" action.
    pub path: String,
    /// Always `true` for user skills. For plugin skills, mirrors
    /// `enabledPlugins["<plugin>@<marketplace>"]` in `settings.json`;
    /// missing key defaults to `true` (Claude Code treats absent as
    /// enabled).
    pub enabled: bool,
}

/// Scans user and plugin-bundled skills, returning a single merged list.
///
/// Order: user skills first (alphabetical), then plugin skills grouped by
/// `plugin@marketplace` (groups alphabetical, skills within alphabetical).
///
/// All inputs are best-effort. A missing skills dir, a missing
/// `installed_plugins.json`, a malformed settings file, or a malformed
/// SKILL.md never bubbles up as an error — the scanner just returns
/// whatever it could read.
pub fn scan_skills(claude_dir: &Path) -> AppResult<Vec<SkillSummary>> {
    let enabled_map = read_enabled_plugins(claude_dir);
    let plugin_records = read_installed_plugins(claude_dir);

    let mut out = Vec::new();

    // User-authored.
    let user_dir = claude_dir.join(USER_SKILLS_DIR);
    if user_dir.is_dir() {
        let mut seen_canonical: std::collections::HashSet<PathBuf> =
            std::collections::HashSet::new();
        walk_skill_files(&user_dir, &mut |path| {
            // De-dupe symlinks by canonical path so `~/.claude/skills/foo`
            // -> `~/.agents/skills/foo` doesn't double-list.
            let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            if seen_canonical.contains(&canonical) {
                return;
            }
            seen_canonical.insert(canonical);
            if let Some(skill) = build_user_skill(path) {
                out.push(skill);
            }
        });
        out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    }

    let user_count = out.len();

    // Plugin-bundled.
    for record in plugin_records {
        let plugin_key = format!("{}@{}", record.plugin, record.marketplace);
        let enabled = enabled_map.get(&plugin_key).copied().unwrap_or(true);
        let mut plugin_skills = Vec::new();
        // First-hit-wins per skill name across ALL candidate subdirs for
        // this plugin — so a clone under `.claude/skills/foo/` doesn't
        // double-list when the canonical lives at `skills/foo/`.
        let mut local_seen: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for sub in PLUGIN_SKILL_DIRS {
            let candidate_root = record.install_path.join(sub);
            if !candidate_root.is_dir() {
                continue;
            }
            let entries = match fs::read_dir(&candidate_root) {
                Ok(e) => e,
                Err(e) => {
                    log::warn!(
                        "skills: cannot read plugin dir {}: {e}",
                        candidate_root.display()
                    );
                    continue;
                }
            };
            for entry in entries.flatten() {
                let child = entry.path();
                if !child.is_dir() {
                    continue;
                }
                let skill_md = child.join(SKILL_FILENAME);
                if !skill_md.is_file() {
                    continue;
                }
                let name = match entry.file_name().into_string() {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if !local_seen.insert(name.clone()) {
                    continue;
                }
                let (description, _other_keys) = read_frontmatter(&skill_md);
                plugin_skills.push(SkillSummary {
                    name,
                    description,
                    source: SkillSource::Plugin {
                        plugin: record.plugin.clone(),
                        marketplace: record.marketplace.clone(),
                        version: record.version.clone(),
                    },
                    path: skill_md.display().to_string(),
                    enabled,
                });
            }
        }
        plugin_skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        out.extend(plugin_skills);
    }

    // Re-sort: keep all user skills first (in their original order), then
    // group plugin skills by their source key. Within a plugin group
    // they're already alphabetical; sort groups alphabetically too.
    let mut user_skills: Vec<SkillSummary> = out.drain(..user_count).collect();
    let mut plugin_skills: Vec<SkillSummary> = out;
    plugin_skills.sort_by(|a, b| {
        let ka = plugin_group_key(a);
        let kb = plugin_group_key(b);
        ka.cmp(&kb).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    user_skills.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    user_skills.extend(plugin_skills);
    Ok(user_skills)
}

fn plugin_group_key(s: &SkillSummary) -> String {
    match &s.source {
        SkillSource::Plugin { plugin, marketplace, .. } => {
            format!("{marketplace}/{plugin}")
        }
        SkillSource::User => String::new(),
    }
}

fn build_user_skill(skill_md_path: &Path) -> Option<SkillSummary> {
    let name = skill_md_path
        .parent()?
        .file_name()?
        .to_str()?
        .to_string();
    let (description, _other_keys) = read_frontmatter(skill_md_path);
    Some(SkillSummary {
        name,
        description,
        source: SkillSource::User,
        path: skill_md_path.display().to_string(),
        enabled: true,
    })
}

/// Recursive walker that invokes `cb` for every regular file whose name is
/// `SKILL.md`. Stops descending into `.git` to avoid noise in case the
/// skills dir is itself a checkout.
fn walk_skill_files<F: FnMut(&Path)>(dir: &Path, cb: &mut F) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            log::warn!("skills: cannot read dir {}: {e}", dir.display());
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if file_name == ".git" {
            continue;
        }
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if file_type.is_symlink() || file_type.is_dir() {
            if path.is_dir() {
                walk_skill_files(&path, cb);
            } else if file_name == SKILL_FILENAME && path.is_file() {
                cb(&path);
            }
            continue;
        }
        if file_type.is_file() && file_name == SKILL_FILENAME {
            cb(&path);
        }
    }
}

/// Lenient YAML frontmatter reader. Looks for `---\n…\n---` at the start of
/// the file and extracts `key: value` lines. Returns `(description, keys)`.
/// Never errors — bad frontmatter → empty description.
fn read_frontmatter(path: &Path) -> (String, Vec<String>) {
    let Ok(raw) = fs::read_to_string(path) else {
        return (String::new(), Vec::new());
    };
    let Some(body) = raw.strip_prefix("---\n").or_else(|| raw.strip_prefix("---\r\n")) else {
        return (String::new(), Vec::new());
    };
    let Some(end) = body.find("\n---") else {
        return (String::new(), Vec::new());
    };
    let block = &body[..end];
    let mut description = String::new();
    let mut keys = Vec::new();
    for line in block.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || key.contains(' ') {
            continue;
        }
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        keys.push(key.to_string());
        if key == "description" && !value.is_empty() && description.is_empty() {
            description = value.to_string();
        }
    }
    (description, keys)
}

/// Reads `enabledPlugins` from settings.json and returns a map of
/// `plugin@marketplace` → enabled. Missing settings → empty map.
fn read_enabled_plugins(claude_dir: &Path) -> HashMap<String, bool> {
    let Ok(Some(settings)) = read_settings(&claude_dir.join("settings.json")) else {
        return HashMap::new();
    };
    let Some(entries) = settings.get("enabledPlugins").and_then(Value::as_object) else {
        return HashMap::new();
    };
    entries
        .iter()
        .filter_map(|(k, v)| {
            let enabled = match v {
                Value::Bool(b) => *b,
                Value::Object(obj) => obj
                    .get("enabled")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                _ => false,
            };
            Some((k.clone(), enabled))
        })
        .collect()
}

/// Parses `<claude_dir>/plugins/installed_plugins.json` (schema v2).
/// Returns an empty Vec on missing/malformed input.
#[derive(Debug, Deserialize)]
struct InstalledPluginsFile {
    #[serde(default)]
    plugins: HashMap<String, Vec<PluginInstall>>,
}

// Field names mirror the JSON keys in installed_plugins.json
// (camelCase on disk, snake_case locally would force an extra rename).
#[allow(non_snake_case)]
#[derive(Debug, Deserialize)]
struct PluginInstall {
    #[serde(default)]
    installPath: String,
    #[serde(default)]
    version: String,
}

#[derive(Debug, Clone)]
struct PluginRecord {
    plugin: String,
    marketplace: String,
    install_path: PathBuf,
    version: String,
}

fn read_installed_plugins(claude_dir: &Path) -> Vec<PluginRecord> {
    let path = claude_dir.join(INSTALLED_PLUGINS_FILE);
    if !path.is_file() {
        return Vec::new();
    }
    let raw = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("skills: cannot read {}: {e}", path.display());
            return Vec::new();
        }
    };
    let parsed: InstalledPluginsFile = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("skills: malformed {}: {e}", path.display());
            return Vec::new();
        }
    };

    let mut out = Vec::new();
    for (key, installs) in parsed.plugins {
        let Some((plugin, marketplace)) = key.split_once('@') else {
            log::warn!("skills: installed_plugins entry missing '@': {key}");
            continue;
        };
        // Prefer the first install record (latest). Falls back to first
        // available record if version lookup is empty.
        let Some(install) = installs.into_iter().next() else {
            continue;
        };
        let install_path = PathBuf::from(&install.installPath);
        if !install_path.is_dir() {
            log::warn!(
                "skills: plugin {plugin}@{marketplace} install path missing: {}",
                install_path.display()
            );
            continue;
        }
        out.push(PluginRecord {
            plugin: plugin.to_string(),
            marketplace: marketplace.to_string(),
            install_path,
            version: install.version,
        });
    }
    // Stable order: by marketplace then plugin.
    out.sort_by(|a, b| {
        a.marketplace
            .cmp(&b.marketplace)
            .then_with(|| a.plugin.cmp(&b.plugin))
    });
    out
}

// Unused-import silencer for AppError in callers that don't construct one.
#[allow(dead_code)]
fn _ensure_apperror_in_scope(_: AppError) {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, content).unwrap();
    }

    #[test]
    fn missing_user_skills_dir_returns_no_user_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let out = scan_skills(tmp.path()).unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn reads_user_skills_with_frontmatter_description() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "skills/graphify/SKILL.md",
            "---\ndescription: Build a knowledge graph from any input.\n---\n# body",
        );
        write(
            tmp.path(),
            "skills/find-skills/SKILL.md",
            "---\nname: find-skills\ndescription: Discover installed skills.\nallowed-tools: [Bash]\n---\n",
        );

        let out = scan_skills(tmp.path()).unwrap();
        assert_eq!(out.len(), 2);
        // Sorted alphabetically.
        assert_eq!(out[0].name, "find-skills");
        assert_eq!(out[0].description, "Discover installed skills.");
        assert!(matches!(out[0].source, SkillSource::User));
        assert!(out[0].enabled);
        assert_eq!(out[1].name, "graphify");
        assert_eq!(out[1].description, "Build a knowledge graph from any input.");
    }

    #[test]
    fn user_skill_without_frontmatter_has_empty_description() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "skills/raw/SKILL.md", "# no frontmatter here");
        let out = scan_skills(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "raw");
        assert_eq!(out[0].description, "");
        assert!(out[0].enabled);
    }

    #[test]
    fn symlinked_user_skill_is_deduped() {
        let tmp = tempfile::tempdir().unwrap();
        let real_dir = tmp.path().join("real");
        fs::create_dir_all(&real_dir).unwrap();
        fs::write(
            real_dir.join("SKILL.md"),
            "---\ndescription: real\n---\n",
        )
        .unwrap();
        // Symlink: skills/shared -> real. The parent dir must exist before
        // symlink() — it doesn't follow mkdir-p semantics.
        fs::create_dir_all(tmp.path().join("skills")).unwrap();
        std::os::unix::fs::symlink(&real_dir, tmp.path().join("skills/shared")).unwrap();

        let out = scan_skills(tmp.path()).unwrap();
        assert_eq!(out.len(), 1, "symlinked skill should appear once");
        assert_eq!(out[0].name, "shared");
    }

    #[test]
    fn plugin_skills_are_grouped_with_first_hit_wins_per_subdir() {
        let tmp = tempfile::tempdir().unwrap();
        let install = tmp.path().join("plugins/installed/ecc/0.1.0");
        fs::create_dir_all(&install).unwrap();
        // Canonical `skills/` and a duplicate under `.claude/skills/`.
        write(
            &install,
            "skills/foo/SKILL.md",
            "---\ndescription: from canonical\n---\n",
        );
        write(
            &install,
            ".claude/skills/foo/SKILL.md",
            "---\ndescription: from .claude\n---\n",
        );
        write(
            &install,
            ".agents/skills/bar/SKILL.md",
            "---\ndescription: agents-only\n---\n",
        );
        write(
            tmp.path(),
            "plugins/installed_plugins.json",
            r#"{
                "version": 2,
                "plugins": {
                    "ecc@everything-claude-code": [
                        {"installPath": "PLACEHOLDER", "version": "0.1.0"}
                    ]
                }
            }"#,
        );
        // Rewrite with the absolute path so the scanner can find it.
        let install_json_path = tmp.path().join("plugins/installed_plugins.json");
        let install_path_str = install.to_string_lossy().into_owned();
        let json = fs::read_to_string(&install_json_path)
            .unwrap()
            .replace("PLACEHOLDER", &install_path_str.replace('\\', "\\\\"));
        fs::write(&install_json_path, json).unwrap();

        let out = scan_skills(tmp.path()).unwrap();
        // foo (1) + bar (1) = 2 plugin skills, no duplicate of foo.
        assert_eq!(out.len(), 2);
        let foo = out.iter().find(|s| s.name == "foo").unwrap();
        assert_eq!(foo.description, "from canonical");
        let bar = out.iter().find(|s| s.name == "bar").unwrap();
        assert_eq!(bar.description, "agents-only");
        match &foo.source {
            SkillSource::Plugin { plugin, marketplace, version } => {
                assert_eq!(plugin, "ecc");
                assert_eq!(marketplace, "everything-claude-code");
                assert_eq!(version, "0.1.0");
            }
            _ => panic!("expected Plugin source"),
        }
    }

    #[test]
    fn plugin_enabled_flag_reflects_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let install = tmp.path().join("plugins/installed/ecc/0.1.0");
        fs::create_dir_all(install.join("skills/foo")).unwrap();
        fs::write(
            install.join("skills/foo/SKILL.md"),
            "---\ndescription: foo\n---\n",
        )
        .unwrap();

        write(tmp.path(), "settings.json", "{}");
        write(
            tmp.path(),
            "plugins/installed_plugins.json",
            &format!(
                r#"{{"version":2,"plugins":{{"ecc@m":[
                  {{"installPath":"{}","version":"0.1.0"}}
                ]}}}}"#,
                install.to_string_lossy().replace('\\', "\\\\")
            ),
        );

        // No enabledPlugins entry → default enabled.
        let out = scan_skills(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert!(out[0].enabled);

        // Explicit false → muted.
        write(
            tmp.path(),
            "settings.json",
            r#"{"enabledPlugins":{"ecc@m":false}}"#,
        );
        let out = scan_skills(tmp.path()).unwrap();
        assert!(!out[0].enabled);

        // Object form {enabled: true}.
        write(
            tmp.path(),
            "settings.json",
            r#"{"enabledPlugins":{"ecc@m":{"enabled":true}}}"#,
        );
        let out = scan_skills(tmp.path()).unwrap();
        assert!(out[0].enabled);
    }

    #[test]
    fn missing_installed_plugins_file_yields_no_plugin_skills_but_user_skills_still_listed() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "skills/graphify/SKILL.md",
            "---\ndescription: g\n---\n",
        );
        // No installed_plugins.json, no settings.json.
        let out = scan_skills(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "graphify");
        assert!(matches!(out[0].source, SkillSource::User));
    }

    #[test]
    fn malformed_installed_plugins_does_not_break_scan() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "skills/foo/SKILL.md",
            "---\ndescription: foo\n---\n",
        );
        write(tmp.path(), "plugins/installed_plugins.json", "{not json");
        let out = scan_skills(tmp.path()).unwrap();
        // User skill still surfaces; plugin side is empty.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "foo");
    }

    #[test]
    fn malformed_skill_md_frontmatter_does_not_break_scan() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "skills/bad/SKILL.md",
            "---\nthis is: not: valid: yaml\nstill in block\n---\nbody",
        );
        write(
            tmp.path(),
            "skills/good/SKILL.md",
            "---\ndescription: ok\n---\n",
        );
        let out = scan_skills(tmp.path()).unwrap();
        assert_eq!(out.len(), 2);
        let bad = out.iter().find(|s| s.name == "bad").unwrap();
        // Lenient parser: keeps the line as a key with no real value, but
        // description is empty.
        assert_eq!(bad.description, "");
        let good = out.iter().find(|s| s.name == "good").unwrap();
        assert_eq!(good.description, "ok");
    }

    #[test]
    fn install_path_missing_on_disk_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "plugins/installed_plugins.json",
            r#"{"version":2,"plugins":{"x@y":[{"installPath":"/nope/does/not/exist","version":"1.0"}]}}"#,
        );
        write(
            tmp.path(),
            "skills/solo/SKILL.md",
            "---\ndescription: solo\n---\n",
        );
        let out = scan_skills(tmp.path()).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].name, "solo");
    }

    #[test]
    fn user_skills_listed_before_plugin_skills() {
        let tmp = tempfile::tempdir().unwrap();
        let install = tmp.path().join("plugins/installed/ecc/0.1.0");
        fs::create_dir_all(install.join("skills/zeta")).unwrap();
        fs::write(install.join("skills/zeta/SKILL.md"), "---\ndescription: z\n---\n").unwrap();
        write(
            tmp.path(),
            "skills/alpha/SKILL.md",
            "---\ndescription: a\n---\n",
        );
        write(
            tmp.path(),
            "plugins/installed_plugins.json",
            &format!(
                r#"{{"version":2,"plugins":{{"ecc@m":[
                  {{"installPath":"{}","version":"0.1.0"}}
                ]}}}}"#,
                install.to_string_lossy().replace('\\', "\\\\")
            ),
        );
        let out = scan_skills(tmp.path()).unwrap();
        assert_eq!(out.len(), 2);
        assert!(matches!(out[0].source, SkillSource::User));
        assert!(matches!(out[1].source, SkillSource::Plugin { .. }));
    }

    #[test]
    fn quoted_frontmatter_value_is_unquoted() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "skills/q/SKILL.md",
            "---\ndescription: \"a quoted value\"\n---\n",
        );
        let out = scan_skills(tmp.path()).unwrap();
        assert_eq!(out[0].description, "a quoted value");
    }
}