//! GitHub OAuth Device Flow.
//!
//! OAuth App must have Device Flow enabled in its settings on
//! github.com. Scopes requested: `repo` (we need to read/write a
//! private repo on the user's account).
//!
//! Endpoint behaviour:
//! - POST /login/device/code with `client_id` returns a one-time
//!   `device_code` (for polling) and a `user_code` + `verification_uri`
//!   we show to the user.
//! - POST /login/oauth/access_token polls for completion. While
//!   pending, GitHub returns `authorization_pending`; once granted it
//!   returns the access_token; on expiry it returns `expired_token`.
//!
//! Both endpoints are unauthenticated.

use serde::{Deserialize, Serialize};

use crate::github::client::{GitHubClient, GitHubError};
use crate::models::GitHubDeviceFlowStart;

/// GitHub OAuth App client_id. NOT a secret — device-flow public clients
/// are designed to ship in binaries.
///
/// **TEMP / maintainer-personal:** This is currently the maintainer's
/// personal GitHub OAuth App. Before publishing a release that ships to
/// end users, register a proper OAuth App under the project's GitHub
/// organization (e.g. `anthropics/claude-config`) and replace this
/// constant. Until then, the consent screen users see will show the
/// maintainer's personal OAuth App name.
pub const GITHUB_OAUTH_CLIENT_ID: &str = "Ov23lix1ebX5eSK1Qcm5";

/// Scopes we request. `repo` covers private repo read/write; no need
/// for `user`/`delete_repo` etc.
pub const GITHUB_OAUTH_SCOPES: &str = "repo";

#[derive(Debug, Clone, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum AccessTokenResponse {
    Ok {
        access_token: String,
        #[serde(default)]
        token_type: Option<String>,
        #[serde(default)]
        scope: Option<String>,
    },
    Err {
        error: String,
        #[serde(default)]
        error_description: Option<String>,
        #[serde(default)]
        error_uri: Option<String>,
    },
}

/// Outcome of a single poll attempt. The frontend translates these into
/// UI state — "still waiting", "success", "denied", "expired".
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "status")]
pub enum DeviceFlowOutcome {
    Pending,
    Authorized {
        access_token: String,
        username: String,
        avatar_url: Option<String>,
    },
    Denied,
    Expired,
    SlowDown,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubUser {
    pub login: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

/// Kick off the device flow.
pub fn start_device_flow() -> Result<GitHubDeviceFlowStart, GitHubError> {
    let url = format!(
        "{}/login/device/code",
        crate::github::client::GITHUB_OAUTH_BASE
    );
    let resp: DeviceCodeResponse = GitHubClient::post_form_unauth(
        &url,
        &[
            ("client_id", GITHUB_OAUTH_CLIENT_ID),
            ("scope", GITHUB_OAUTH_SCOPES),
        ],
    )?;
    Ok(GitHubDeviceFlowStart {
        device_code: resp.device_code,
        user_code: resp.user_code,
        verification_uri: resp.verification_uri,
        expires_in: resp.expires_in,
        interval: resp.interval,
    })
}

/// One poll. The frontend is responsible for pacing (sleep `interval`
/// between calls) and for enforcing the `expires_in` deadline.
pub fn poll_device_flow(device_code: &str) -> Result<DeviceFlowOutcome, GitHubError> {
    let url = format!(
        "{}/login/oauth/access_token",
        crate::github::client::GITHUB_OAUTH_BASE
    );
    let resp: AccessTokenResponse = GitHubClient::post_form_unauth(
        &url,
        &[
            ("client_id", GITHUB_OAUTH_CLIENT_ID),
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ],
    )?;

    match resp {
        AccessTokenResponse::Ok {
            access_token,
            token_type: _,
            scope: _,
        } => {
            // Fetch username + avatar once we have the token — UI
            // shows username in the "Connected as ..." banner and
            // avatar in the top bar.
            let user = fetch_user(&access_token)?;
            Ok(DeviceFlowOutcome::Authorized {
                access_token,
                username: user.login,
                avatar_url: user.avatar_url,
            })
        }
        AccessTokenResponse::Err { error, .. } => Ok(match error.as_str() {
            "authorization_pending" => DeviceFlowOutcome::Pending,
            "slow_down" => DeviceFlowOutcome::SlowDown,
            "access_denied" => DeviceFlowOutcome::Denied,
            "expired_token" | "device_flow_disabled" => DeviceFlowOutcome::Expired,
            _ => {
                return Err(GitHubError::Parse(format!(
                    "unknown OAuth error: {error}"
                )));
            }
        }),
    }
}

fn fetch_user(token: &str) -> Result<GitHubUser, GitHubError> {
    GitHubClient::get_json(&format!("{}/user", crate::github::client::GITHUB_API_BASE), token)
}