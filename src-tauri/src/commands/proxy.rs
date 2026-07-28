//! Managed CLIProxyAPI lifecycle.
//!
//! CLIProxyAPI is a third-party local relay that turns *subscriptions*
//! (Claude Pro/Max, ChatGPT Plus, Gemini, Grok, Kimi) into an
//! Anthropic-compatible endpoint. Claude Code can then run on any of them.
//!
//! Users should not have to install Go, clone a repo, or hand-write YAML, so
//! this module owns the whole lifecycle: download the release binary for the
//! current platform, write a minimal config, run it, and spawn the per-provider
//! OAuth logins. Everything lives under `<app-data>/cliproxyapi/`.
//!
//! ponytail: archives are unpacked by shelling out to `tar`, which handles both
//! `.tar.gz` and `.zip` on macOS/Linux/Windows 10+ (bsdtar). Saves three crates.
//! Swap in a real archive crate only if a supported platform ships without tar.

use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::models::{AppError, AppResult};
use crate::state::AppState;

/// Upstream repo. Releases are published as
/// `CLIProxyAPI_<version>_<os>_<arch>.<tar.gz|zip>`.
const REPO: &str = "router-for-me/CLIProxyAPI";

/// Port the managed instance listens on. Matches CLIProxyAPI's own default and
/// the `cliproxyapi` provider preset in the frontend — keep all three in sync.
pub const PROXY_PORT: u16 = 8317;

/// OAuth flows the proxy exposes, as (id, CLI flag, display label). The id is
/// what the frontend sends; anything outside this list is rejected rather than
/// interpolated into a command line.
const LOGIN_FLOWS: &[(&str, &str, &str)] = &[
    ("claude", "--claude-login", "Claude"),
    ("codex", "--codex-login", "Codex / ChatGPT"),
    ("antigravity", "--antigravity-login", "Gemini"),
    ("xai", "--xai-login", "Grok"),
    ("kimi", "--kimi-login", "Kimi"),
];

/// Handle to the proxy process this app started. `None` when we never started
/// it — including when a proxy the user launched themselves is already running,
/// which we can detect but must not kill.
pub type ProxyChild = Arc<Mutex<Option<Child>>>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyStatus {
    /// Binary is present on disk.
    pub installed: bool,
    /// Version recorded at install time, if known.
    pub version: Option<String>,
    /// Something is answering `/healthz` on the port.
    pub running: bool,
    /// True when that something is a process we started (so we can stop it).
    pub managed: bool,
    pub port: u16,
    /// Base URL to paste into a provider — matches the bundled preset.
    pub base_url: String,
    /// Subscription ids (see `LOGIN_FLOWS`) that have a stored credential.
    pub connected: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginFlow {
    pub id: String,
    pub label: String,
}

/// The subset of GitHub's release JSON we care about.
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

// ---------- paths ----------

fn proxy_dir(state: &AppState) -> PathBuf {
    state.app_data_dir.join("cliproxyapi")
}

fn binary_path(state: &AppState) -> PathBuf {
    let name = if cfg!(windows) {
        "cli-proxy-api.exe"
    } else {
        "cli-proxy-api"
    };
    proxy_dir(state).join(name)
}

fn config_path(state: &AppState) -> PathBuf {
    proxy_dir(state).join("config.yaml")
}

/// Credential store. Kept inside our own directory rather than the upstream
/// default (`~/.cli-proxy-api`) so a managed install is self-contained and
/// listing connected accounts is unambiguous.
fn auth_dir(state: &AppState) -> PathBuf {
    proxy_dir(state).join("auth")
}

fn version_path(state: &AppState) -> PathBuf {
    proxy_dir(state).join("version.txt")
}

// ---------- commands ----------

/// The OAuth flows the UI should offer. Static, but exposed as a command so the
/// list lives in one place.
#[tauri::command]
pub fn proxy_login_flows_cmd() -> Vec<LoginFlow> {
    LOGIN_FLOWS
        .iter()
        .map(|(id, _, label)| LoginFlow {
            id: (*id).to_string(),
            label: (*label).to_string(),
        })
        .collect()
}

#[tauri::command]
pub fn proxy_status_cmd(
    state: tauri::State<'_, AppState>,
    child: tauri::State<'_, ProxyChild>,
) -> AppResult<ProxyStatus> {
    let installed = binary_path(&state).exists();
    let version = std::fs::read_to_string(version_path(&state))
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());

    // A process we spawned may have exited on its own (bad config, port
    // clash). Reap it so `managed` doesn't lie.
    let managed = {
        let mut guard = child.lock().unwrap();
        match guard.as_mut() {
            Some(c) => match c.try_wait() {
                Ok(Some(_)) => {
                    *guard = None;
                    false
                }
                _ => true,
            },
            None => false,
        }
    };

    Ok(ProxyStatus {
        installed,
        version,
        running: is_running(),
        managed,
        port: PROXY_PORT,
        base_url: format!("http://localhost:{PROXY_PORT}"),
        connected: connected_accounts(&auth_dir(&state)),
    })
}

/// Download the latest release for this platform and unpack it. Also used to
/// update: the binary is simply overwritten. Returns the installed version.
#[tauri::command]
pub fn install_proxy_cmd(state: tauri::State<'_, AppState>) -> AppResult<String> {
    let dir = proxy_dir(&state);
    std::fs::create_dir_all(&dir)?;
    std::fs::create_dir_all(auth_dir(&state))?;

    let release = fetch_latest_release()?;
    let wanted = asset_suffix()?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name.ends_with(&wanted))
        .ok_or_else(|| {
            AppError::Internal(format!(
                "release {} has no asset for this platform ({wanted})",
                release.tag_name
            ))
        })?;

    let archive = dir.join(&asset.name);
    download_to(&asset.browser_download_url, &archive)?;
    extract(&archive, &dir)?;
    let _ = std::fs::remove_file(&archive);

    let bin = binary_path(&state);
    if !bin.exists() {
        return Err(AppError::Internal(format!(
            "archive {} did not contain the expected binary",
            asset.name
        )));
    }
    make_executable(&bin)?;

    write_config_if_missing(&state)?;
    std::fs::write(version_path(&state), &release.tag_name)?;
    Ok(release.tag_name)
}

/// Start the managed proxy. No-op if something already answers on the port,
/// so clicking twice — or clicking while the user's own instance runs — is safe.
#[tauri::command]
pub fn start_proxy_cmd(
    state: tauri::State<'_, AppState>,
    child: tauri::State<'_, ProxyChild>,
) -> AppResult<()> {
    if is_running() {
        return Ok(());
    }
    let bin = binary_path(&state);
    if !bin.exists() {
        return Err(AppError::Validation(
            "CLIProxyAPI is not installed yet".into(),
        ));
    }
    write_config_if_missing(&state)?;

    let spawned = Command::new(&bin)
        .arg("--config")
        .arg(config_path(&state))
        .current_dir(proxy_dir(&state))
        .spawn()
        .map_err(|e| AppError::Internal(format!("could not start CLIProxyAPI: {e}")))?;
    *child.lock().unwrap() = Some(spawned);

    // The server binds in well under a second; give it a moment so the status
    // refresh that follows this call doesn't report "stopped".
    std::thread::sleep(std::time::Duration::from_millis(600));
    Ok(())
}

/// Stop the proxy we started. A proxy the user launched themselves is left
/// alone — we only own our own child.
#[tauri::command]
pub fn stop_proxy_cmd(child: tauri::State<'_, ProxyChild>) -> AppResult<()> {
    let mut guard = child.lock().unwrap();
    if let Some(mut c) = guard.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
    Ok(())
}

/// Run one provider's OAuth login. The proxy opens the system browser and
/// writes a credential into `auth-dir` on success; this call blocks until that
/// finishes, so the caller should treat it as a long operation.
#[tauri::command]
pub fn proxy_login_cmd(
    state: tauri::State<'_, AppState>,
    provider: String,
) -> AppResult<()> {
    let flag = LOGIN_FLOWS
        .iter()
        .find(|(id, _, _)| *id == provider)
        .map(|(_, flag, _)| *flag)
        .ok_or_else(|| AppError::Validation(format!("unknown login flow: {provider}")))?;

    let bin = binary_path(&state);
    if !bin.exists() {
        return Err(AppError::Validation(
            "CLIProxyAPI is not installed yet".into(),
        ));
    }
    write_config_if_missing(&state)?;

    let out = Command::new(&bin)
        .arg("--config")
        .arg(config_path(&state))
        .arg(flag)
        .current_dir(proxy_dir(&state))
        .output()
        .map_err(|e| AppError::Internal(format!("could not run login: {e}")))?;

    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let tail: String = stderr.lines().rev().take(3).collect::<Vec<_>>().join(" ");
        return Err(AppError::Internal(format!(
            "login failed: {}",
            if tail.is_empty() { "no output" } else { &tail }
        )));
    }
    Ok(())
}

// ---------- helpers ----------

/// Probe the proxy's health endpoint. Short timeout: this runs on every status
/// refresh, and "not running" is the common answer.
fn is_running() -> bool {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_millis(800))
        .build()
        .map(|c| {
            c.get(format!("http://127.0.0.1:{PROXY_PORT}/healthz"))
                .send()
                .map(|r| r.status().is_success())
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// Which subscriptions have a stored credential. The proxy names credential
/// files after the provider, so a prefix match is enough.
fn connected_accounts(auth_dir: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(auth_dir) else {
        return Vec::new();
    };
    let names: Vec<String> = entries
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .map(|n| n.to_lowercase())
        .collect();

    LOGIN_FLOWS
        .iter()
        .filter(|(id, _, _)| names.iter().any(|n| n.contains(id)))
        .map(|(id, _, _)| (*id).to_string())
        .collect()
}

/// Archive suffix for the running platform, e.g. `_linux_amd64.tar.gz`.
fn asset_suffix() -> AppResult<String> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        "windows" => "windows",
        other => return Err(AppError::Validation(format!("unsupported OS: {other}"))),
    };
    // Upstream labels arm64 assets `aarch64`.
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "aarch64",
        other => return Err(AppError::Validation(format!("unsupported CPU: {other}"))),
    };
    let ext = if os == "windows" { "zip" } else { "tar.gz" };
    Ok(format!("_{os}_{arch}.{ext}"))
}

fn http_client() -> AppResult<reqwest::blocking::Client> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("claude-config/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(300))
        .build()
        .map_err(|e| AppError::Internal(format!("http client: {e}")))
}

fn fetch_latest_release() -> AppResult<Release> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let res = http_client()?
        .get(&url)
        .header("accept", "application/vnd.github+json")
        .send()
        .map_err(|e| AppError::Internal(format!("could not reach GitHub: {e}")))?;
    if !res.status().is_success() {
        return Err(AppError::Internal(format!(
            "GitHub returned {} for the latest release",
            res.status()
        )));
    }
    res.json::<Release>()
        .map_err(|e| AppError::Internal(format!("unexpected release JSON: {e}")))
}

fn download_to(url: &str, dest: &Path) -> AppResult<()> {
    let mut res = http_client()?
        .get(url)
        .send()
        .map_err(|e| AppError::Internal(format!("download failed: {e}")))?;
    if !res.status().is_success() {
        return Err(AppError::Internal(format!(
            "download returned {}",
            res.status()
        )));
    }
    let mut file = std::fs::File::create(dest)?;
    res.copy_to(&mut file)
        .map_err(|e| AppError::Internal(format!("could not write {}: {e}", dest.display())))?;
    Ok(())
}

/// Unpack with the system `tar`, which reads both gzip tarballs and zips.
fn extract(archive: &Path, into: &Path) -> AppResult<()> {
    let out = Command::new("tar")
        .arg("-xf")
        .arg(archive)
        .arg("-C")
        .arg(into)
        .output()
        .map_err(|e| AppError::Internal(format!("`tar` unavailable: {e}")))?;
    if !out.status.success() {
        return Err(AppError::Internal(format!(
            "could not unpack {}: {}",
            archive.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn make_executable(bin: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(bin)?.permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(bin, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_bin: &Path) -> AppResult<()> {
    Ok(())
}

/// Write a minimal config on first install and never touch it again — the user
/// may have edited it. Loopback-only with no `api-keys`, which means no token
/// to copy around; the proxy is unreachable from other machines.
fn write_config_if_missing(state: &AppState) -> AppResult<()> {
    let path = config_path(state);
    if path.exists() {
        return Ok(());
    }
    std::fs::create_dir_all(proxy_dir(state))?;
    std::fs::write(&path, default_config(&auth_dir(state)))?;
    Ok(())
}

fn default_config(auth_dir: &Path) -> String {
    format!(
        "# Written by Claude Config. Safe to edit — it is never overwritten.\n\
         host: \"127.0.0.1\"\n\
         port: {PROXY_PORT}\n\
         auth-dir: \"{}\"\n\
         debug: false\n",
        auth_dir.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_suffix_matches_release_naming() {
        let s = asset_suffix().expect("test platform should be supported");
        assert!(s.starts_with('_'), "{s}");
        assert!(s.ends_with(".tar.gz") || s.ends_with(".zip"), "{s}");
        // Guards against reintroducing the arm64/aarch64 mismatch.
        assert!(!s.contains("arm64"), "{s}");
        assert!(!s.contains("x86_64"), "{s}");
    }

    #[test]
    fn default_config_has_no_api_keys_and_binds_loopback() {
        let cfg = default_config(Path::new("/tmp/auth"));
        assert!(cfg.contains("127.0.0.1"));
        assert!(cfg.contains(&format!("port: {PROXY_PORT}")));
        assert!(cfg.contains("/tmp/auth"));
        // An empty `api-keys` list leaves the proxy open; the example
        // placeholders would trip its safe mode. Neither may appear.
        assert!(!cfg.contains("api-keys"));
    }

    #[test]
    fn connected_accounts_matches_credential_filenames() {
        let dir = std::env::temp_dir().join(format!("cc-proxy-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("antigravity-user@example.com.json"), "{}").unwrap();
        std::fs::write(dir.join("claude-someone.json"), "{}").unwrap();

        let mut found = connected_accounts(&dir);
        found.sort();
        assert_eq!(found, vec!["antigravity", "claude"]);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn connected_accounts_is_empty_without_auth_dir() {
        assert!(connected_accounts(Path::new("/nonexistent/path")).is_empty());
    }

    #[test]
    fn login_flow_ids_are_unique_and_flagged() {
        let mut ids: Vec<&str> = LOGIN_FLOWS.iter().map(|(id, _, _)| *id).collect();
        let count = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), count);
        assert!(LOGIN_FLOWS.iter().all(|(_, flag, _)| flag.starts_with("--")));
    }
}
