//! Shared reqwest client + typed error for all GitHub calls.
//!
//! Mirrors the pattern from `commands::tracker::http_client` — one
//! `OnceLock<Client>` for the lifetime of the app, so connections are
//! pooled across calls. Token is passed per-request, not stored on the
//! client, so we don't have to rebuild it after `github_disconnect_cmd`.

use std::sync::OnceLock;

use base64::Engine;
use reqwest::blocking::Client;
use serde::Deserialize;
use thiserror::Error;

pub const GITHUB_API_BASE: &str = "https://api.github.com";
pub const GITHUB_OAUTH_BASE: &str = "https://github.com";
pub const USER_AGENT: &str = concat!(
    "claude-config/",
    env!("CARGO_PKG_VERSION"),
    " (github-sync)"
);

fn http_client() -> &'static Client {
    static C: OnceLock<Client> = OnceLock::new();
    C.get_or_init(|| {
        Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(std::time::Duration::from_secs(5))
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("reqwest client build should not fail with default config")
    })
}

/// Errors that GitHub calls can produce. Mapped to `AppError` in the
/// commands layer — frontend branches on `kind` for user-facing messages.
#[derive(Debug, Error)]
pub enum GitHubError {
    #[error("GitHub HTTP error ({status}): {body}")]
    Http { status: u16, body: String },

    #[error("GitHub rate limited; retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("Network error: {0}")]
    Network(String),

    #[error("Unexpected response: {0}")]
    Parse(String),
}

impl From<reqwest::Error> for GitHubError {
    fn from(e: reqwest::Error) -> Self {
        GitHubError::Network(e.to_string())
    }
}

/// Thin wrapper. We don't add state to `reqwest::Client` itself, so this
/// mostly exists for ergonomics (`client().get_json(...)`).
#[derive(Clone, Copy)]
pub struct GitHubClient;

impl GitHubClient {
    pub fn raw() -> &'static Client {
        http_client()
    }

    /// Authenticated GET. Returns parsed JSON.
    pub fn get_json<T: for<'de> Deserialize<'de>>(
        token: &str,
        url: &str,
    ) -> Result<T, GitHubError> {
        let resp = http_client()
            .get(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()?;
        Self::parse(resp)
    }

    /// Unauthenticated GET (used for OAuth device-flow endpoints).
    pub fn get_json_unauth<T: for<'de> Deserialize<'de>>(
        url: &str,
    ) -> Result<T, GitHubError> {
        let resp = http_client()
            .get(url)
            .header("Accept", "application/json")
            .send()?;
        Self::parse(resp)
    }

    /// Unauthenticated form-urlencoded POST (OAuth device-flow).
    pub fn post_form_unauth<T: for<'de> Deserialize<'de>>(
        url: &str,
        form: &[(&str, &str)],
    ) -> Result<T, GitHubError> {
        let resp = http_client()
            .post(url)
            .header("Accept", "application/json")
            .form(form)
            .send()?;
        Self::parse(resp)
    }

    /// Authenticated JSON POST.
    pub fn post_json<T: for<'de> Deserialize<'de>, B: serde::Serialize>(
        token: &str,
        url: &str,
        body: &B,
    ) -> Result<T, GitHubError> {
        let resp = http_client()
            .post(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(body)
            .send()?;
        Self::parse(resp)
    }

    /// Authenticated JSON PUT (used for repo creation + contents API fallback).
    pub fn put_json<T: for<'de> Deserialize<'de>, B: serde::Serialize>(
        token: &str,
        url: &str,
        body: &B,
    ) -> Result<T, GitHubError> {
        let resp = http_client()
            .put(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(body)
            .send()?;
        Self::parse(resp)
    }

    /// Authenticated JSON PATCH (used for ref updates).
    pub fn patch_json<T: for<'de> Deserialize<'de>, B: serde::Serialize>(
        token: &str,
        url: &str,
        body: &B,
    ) -> Result<T, GitHubError> {
        let resp = http_client()
            .patch(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .json(body)
            .send()?;
        Self::parse(resp)
    }

    /// Authenticated DELETE (used when disconnecting/revoking).
    pub fn delete(token: &str, url: &str) -> Result<(), GitHubError> {
        let resp = http_client()
            .delete(url)
            .header("Authorization", format!("Bearer {token}"))
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2022-11-28")
            .send()?;
        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().unwrap_or_default();
            return Err(GitHubError::Http { status, body });
        }
        Ok(())
    }

    fn parse<T: for<'de> Deserialize<'de>>(
        resp: reqwest::blocking::Response,
    ) -> Result<T, GitHubError> {
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(GitHubError::Http {
                status: 401,
                body: "unauthorized".into(),
            });
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry = resp
                .headers()
                .get("retry-after")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(60);
            return Err(GitHubError::RateLimited {
                retry_after_secs: retry,
            });
        }
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            return Err(GitHubError::Http {
                status: status.as_u16(),
                body,
            });
        }
        let bytes = resp.bytes()?;
        serde_json::from_slice(&bytes).map_err(|e| {
            GitHubError::Parse(format!(
                "decode {} bytes as JSON: {e}",
                bytes.len()
            ))
        })
    }
}

/// Public so callers can encode/decode blobs without re-importing base64.
pub fn b64_encode(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn b64_decode(s: &str) -> Result<Vec<u8>, GitHubError> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .map_err(|e| GitHubError::Parse(format!("base64 decode: {e}")))
}