//! Reads Claude Code's registered MCP servers.
//!
//! Server definitions live in `~/.claude.json` under the top-level
//! `mcpServers` key (NOT inside `~/.claude/`). The file is shared with
//! Claude Code's runtime — we read it but never write to it.
//!
//! Two cache files enrich the rows:
//! - `~/.claude/mcp-health-cache.json` — per-server health status.
//! - `~/.claude/mcp-needs-auth-cache.json` — per-server "needs re-auth".
//!
//! Path resolution: the MCP file is **always** at `${HOME}/.claude.json`.
//! We deliberately do NOT honor `CLAUDE_CONFIG_DIR` here — the runtime
//! reads from the home-relative path unconditionally (confirmed by the
//! `source` field that `~/.claude/mcp-health-cache.json` writes back).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::models::{AppError, AppResult};

const MCP_FILENAME: &str = ".claude.json";
const HEALTH_CACHE: &str = "mcp-health-cache.json";
const NEEDS_AUTH_CACHE: &str = "mcp-needs-auth-cache.json";

/// Transport declared in the MCP server config. Falls back to `stdio`
/// when the entry has no `type` field — that's Claude Code's documented
/// default and matches what runtime would do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum McpTransport {
    Stdio,
    Http,
    Sse,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum McpHealth {
    Healthy,
    Failing {
        last_error: String,
        failure_count: u32,
    },
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct McpServerSummary {
    pub name: String,
    pub transport: McpTransport,
    /// Stdio fields. Empty for http/sse.
    pub command: Option<String>,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    /// Http/SSE fields. Empty for stdio.
    pub url: Option<String>,
    pub headers: HashMap<String, String>,
    /// `None` when no health record exists yet.
    pub health: Option<McpHealth>,
    pub needs_auth: bool,
    /// Diagnostic — the path the row came from.
    pub source: String,
}

/// Scans global MCP server definitions. Best-effort: any single failure
/// (missing file, malformed JSON, malformed per-server entry) is
/// downgraded to a log warning; the scan never returns an error for
/// data-shape problems. Only filesystem-level failures bubble up.
pub fn scan_mcp_servers() -> AppResult<Vec<McpServerSummary>> {
    let claude_json_path = claude_json_path();

    let servers_obj = match read_mcp_servers(&claude_json_path) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("mcp: could not read {}: {e}", claude_json_path.display());
            return Ok(Vec::new());
        }
    };

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let health = read_health_cache(&home.join(HEALTH_CACHE));
    let needs_auth = read_needs_auth_cache(&home.join(NEEDS_AUTH_CACHE));

    let mut out = Vec::new();
    for (name, value) in servers_obj {
        match build_server_summary(
            &name,
            &value,
            &claude_json_path,
            health.get(&name),
            needs_auth.contains(&name),
        ) {
            Ok(s) => out.push(s),
            Err(e) => log::warn!("mcp: skipping server {name}: {e}"),
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

/// Returns `${HOME}/.claude.json`. Does not honor `CLAUDE_CONFIG_DIR`.
pub fn claude_json_path() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(MCP_FILENAME)
}

fn read_mcp_servers(path: &Path) -> AppResult<HashMap<String, Value>> {
    if !path.is_file() {
        return Ok(HashMap::new());
    }
    let raw = fs::read_to_string(path)?;
    let root: Value = serde_json::from_slice(raw.as_bytes())?;
    let Some(obj) = root.get("mcpServers").and_then(Value::as_object) else {
        return Ok(HashMap::new());
    };
    Ok(obj
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect())
}

fn build_server_summary(
    name: &str,
    value: &Value,
    source: &Path,
    health: Option<&HealthRecord>,
    needs_auth: bool,
) -> AppResult<McpServerSummary> {
    let Some(obj) = value.as_object() else {
        return Err(AppError::Validation(format!(
            "server {name}: expected an object"
        )));
    };

    let transport = match obj.get("type").and_then(Value::as_str) {
        Some("http") => McpTransport::Http,
        Some("sse") => McpTransport::Sse,
        Some("stdio") | None => McpTransport::Stdio,
        Some(other) => {
            // Unknown transport type — surface as stdio with a warning
            // so the row still appears rather than being silently dropped.
            log::warn!("mcp: server {name} has unknown transport '{other}', defaulting to stdio");
            McpTransport::Stdio
        }
    };

    let command = obj
        .get("command")
        .and_then(Value::as_str)
        .map(str::to_string);
    let args = obj
        .get("args")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let env = obj
        .get("env")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();
    let url = obj
        .get("url")
        .and_then(Value::as_str)
        .map(str::to_string);
    let headers = obj
        .get("headers")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                .collect()
        })
        .unwrap_or_default();

    let health_field = health.map(|h| match h.status.as_deref() {
        Some("healthy") => McpHealth::Healthy,
        _ => McpHealth::Failing {
            last_error: h.lastError.clone().unwrap_or_default(),
            failure_count: h.failureCount.unwrap_or(0),
        },
    });

    Ok(McpServerSummary {
        name: name.to_string(),
        transport,
        command,
        args,
        env,
        url,
        headers,
        health: health_field,
        needs_auth,
        source: source.display().to_string(),
    })
}

// Field names mirror the JSON keys in mcp-health-cache.json
// (camelCase on disk).
#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Default)]
struct HealthRecord {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    lastError: Option<String>,
    #[serde(default)]
    failureCount: Option<u32>,
}

fn read_health_cache(path: &Path) -> HashMap<String, HealthRecord> {
    if !path.is_file() {
        return HashMap::new();
    }
    let Ok(raw) = fs::read_to_string(path) else {
        log::warn!("mcp: cannot read {}", path.display());
        return HashMap::new();
    };
    let root: Value = match serde_json::from_slice(raw.as_bytes()) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("mcp: malformed {}: {e}", path.display());
            return HashMap::new();
        }
    };
    let Some(servers) = root.get("servers").and_then(Value::as_object) else {
        return HashMap::new();
    };
    servers
        .iter()
        .filter_map(|(k, v)| {
            let rec: HealthRecord = serde_json::from_value(v.clone()).ok()?;
            Some((k.clone(), rec))
        })
        .collect()
}

fn read_needs_auth_cache(path: &Path) -> std::collections::HashSet<String> {
    let mut out = std::collections::HashSet::new();
    if !path.is_file() {
        return out;
    }
    let Ok(raw) = fs::read_to_string(path) else {
        log::warn!("mcp: cannot read {}", path.display());
        return out;
    };
    let root: Value = match serde_json::from_slice(raw.as_bytes()) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("mcp: malformed {}: {e}", path.display());
            return out;
        }
    };
    let Some(obj) = root.as_object() else {
        return out;
    };
    out.extend(obj.keys().cloned());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::sync::Mutex;

    // Tests that pin HOME must serialize — `dirs::home_dir()` reads it on
    // every call, so concurrent tests would see each other's fixtures.
    static HOME_LOCK: Mutex<()> = Mutex::new(());

    fn write(dir: &Path, rel: &str, content: &str) {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(p, content).unwrap();
    }

    /// Point HOME at a temp dir so `claude_json_path()` and the cache
    /// readers look there. Returns a guard that restores HOME on drop.
    fn pin_home(dir: &Path) -> HomeGuard {
        let _guard = HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var("HOME").ok();
        std::env::set_var("HOME", dir);
        HomeGuard { prev, _guard }
    }

    struct HomeGuard {
        prev: Option<String>,
        // Held for the guard's lifetime to keep HOME_LOCK across the test.
        _guard: std::sync::MutexGuard<'static, ()>,
    }
    impl Drop for HomeGuard {
        fn drop(&mut self) {
            match &self.prev {
                Some(v) => std::env::set_var("HOME", v),
                None => std::env::remove_var("HOME"),
            }
        }
    }

    #[test]
    fn missing_claude_json_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        let out = scan_mcp_servers().unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn empty_mcp_servers_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(tmp.path(), ".claude.json", r#"{"mcpServers": {}}"#);
        let out = scan_mcp_servers().unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn missing_mcp_servers_key_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(tmp.path(), ".claude.json", r#"{"other": "stuff"}"#);
        let out = scan_mcp_servers().unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn stdio_server_surfaces_command_args_env() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(
            tmp.path(),
            ".claude.json",
            r#"{
                "mcpServers": {
                    "github": {
                        "type": "stdio",
                        "command": "npx",
                        "args": ["-y", "@modelcontextprotocol/server-github"],
                        "env": {"GITHUB_TOKEN": "secret"}
                    }
                }
            }"#,
        );
        let out = scan_mcp_servers().unwrap();
        assert_eq!(out.len(), 1);
        let s = &out[0];
        assert_eq!(s.name, "github");
        assert_eq!(s.transport, McpTransport::Stdio);
        assert_eq!(s.command.as_deref(), Some("npx"));
        assert_eq!(s.args, vec!["-y", "@modelcontextprotocol/server-github"]);
        assert_eq!(s.env.get("GITHUB_TOKEN").map(String::as_str), Some("secret"));
        assert!(s.url.is_none());
        assert!(s.headers.is_empty());
        assert!(s.health.is_none());
        assert!(!s.needs_auth);
    }

    #[test]
    fn http_server_surfaces_url_only() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(
            tmp.path(),
            ".claude.json",
            r#"{
                "mcpServers": {
                    "figma": {
                        "type": "http",
                        "url": "https://mcp.figma.com/mcp"
                    }
                }
            }"#,
        );
        let out = scan_mcp_servers().unwrap();
        assert_eq!(out.len(), 1);
        let s = &out[0];
        assert_eq!(s.transport, McpTransport::Http);
        assert_eq!(s.url.as_deref(), Some("https://mcp.figma.com/mcp"));
        assert!(s.command.is_none());
        assert!(s.args.is_empty());
        assert!(s.env.is_empty());
    }

    #[test]
    fn sse_transport_recognised() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(
            tmp.path(),
            ".claude.json",
            r#"{
                "mcpServers": {
                    "evt": {"type": "sse", "url": "https://x/sse"}
                }
            }"#,
        );
        let out = scan_mcp_servers().unwrap();
        assert_eq!(out[0].transport, McpTransport::Sse);
    }

    #[test]
    fn missing_type_field_defaults_to_stdio() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(
            tmp.path(),
            ".claude.json",
            r#"{
                "mcpServers": {
                    "n": {"command": "node", "args": ["srv.js"]}
                }
            }"#,
        );
        let out = scan_mcp_servers().unwrap();
        assert_eq!(out[0].transport, McpTransport::Stdio);
    }

    #[test]
    fn health_cache_enriches_matching_server() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(
            tmp.path(),
            ".claude.json",
            r#"{"mcpServers":{"figma":{"type":"http","url":"https://mcp.figma.com/mcp"}}}"#,
        );
        write(
            tmp.path(),
            "mcp-health-cache.json",
            r#"{"version":1,"servers":{"figma":{"status":"healthy","failureCount":0,"lastError":null}}}"#,
        );
        let out = scan_mcp_servers().unwrap();
        assert!(matches!(out[0].health, Some(McpHealth::Healthy)));
    }

    #[test]
    fn health_cache_failing_populates_error_and_count() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(
            tmp.path(),
            ".claude.json",
            r#"{"mcpServers":{"figma":{"type":"http","url":"https://mcp.figma.com/mcp"}}}"#,
        );
        write(
            tmp.path(),
            "mcp-health-cache.json",
            r#"{"version":1,"servers":{"figma":{"status":"failing","lastError":"401 Unauthorized","failureCount":3}}}"#,
        );
        let out = scan_mcp_servers().unwrap();
        match &out[0].health {
            Some(McpHealth::Failing { last_error, failure_count }) => {
                assert_eq!(last_error, "401 Unauthorized");
                assert_eq!(*failure_count, 3);
            }
            other => panic!("expected Failing, got {other:?}"),
        }
    }

    #[test]
    fn health_cache_misses_leave_health_none() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(
            tmp.path(),
            ".claude.json",
            r#"{"mcpServers":{"figma":{"type":"http","url":"https://mcp.figma.com/mcp"}}}"#,
        );
        // No cache file at all.
        let out = scan_mcp_servers().unwrap();
        assert!(out[0].health.is_none());
    }

    #[test]
    fn needs_auth_cache_sets_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(
            tmp.path(),
            ".claude.json",
            r#"{"mcpServers":{"figma":{"type":"http","url":"https://mcp.figma.com/mcp"}}}"#,
        );
        write(
            tmp.path(),
            "mcp-needs-auth-cache.json",
            r#"{"figma":{"timestamp":1775207328341}}"#,
        );
        let out = scan_mcp_servers().unwrap();
        assert!(out[0].needs_auth);
    }

    #[test]
    fn malformed_claude_json_does_not_crash() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(tmp.path(), ".claude.json", "{not json");
        let out = scan_mcp_servers().unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn malformed_cache_does_not_crash() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(
            tmp.path(),
            ".claude.json",
            r#"{"mcpServers":{"figma":{"type":"http","url":"https://x"}}}"#,
        );
        write(tmp.path(), "mcp-health-cache.json", "{not json");
        write(tmp.path(), "mcp-needs-auth-cache.json", "{not json");
        let out = scan_mcp_servers().unwrap();
        // Server still surfaces; caches degraded to empty enrichment.
        assert_eq!(out.len(), 1);
        assert!(out[0].health.is_none());
        assert!(!out[0].needs_auth);
    }

    #[test]
    fn malformed_individual_server_skipped_others_listed() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(
            tmp.path(),
            ".claude.json",
            r#"{
                "mcpServers": {
                    "good": {"type": "http", "url": "https://ok"},
                    "bad": "this should be an object",
                    "alsogood": {"type": "stdio", "command": "node"}
                }
            }"#,
        );
        let out = scan_mcp_servers().unwrap();
        let names: Vec<&str> = out.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["alsogood", "good"]);
    }

    #[test]
    fn servers_sorted_alphabetically_case_insensitive() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(
            tmp.path(),
            ".claude.json",
            r#"{
                "mcpServers": {
                    "Zebra": {"type": "http", "url": "x"},
                    "apple": {"type": "http", "url": "x"},
                    "Mango": {"type": "http", "url": "x"}
                }
            }"#,
        );
        let out = scan_mcp_servers().unwrap();
        let names: Vec<&str> = out.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["apple", "Mango", "Zebra"]);
    }

    #[test]
    fn unknown_transport_type_falls_back_to_stdio() {
        let tmp = tempfile::tempdir().unwrap();
        let _g = pin_home(tmp.path());
        write(
            tmp.path(),
            ".claude.json",
            r#"{"mcpServers":{"x":{"type":"websocket","url":"wss://x"}}}"#,
        );
        let out = scan_mcp_servers().unwrap();
        // Surfaced with stdio default rather than dropped.
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].transport, McpTransport::Stdio);
    }
}