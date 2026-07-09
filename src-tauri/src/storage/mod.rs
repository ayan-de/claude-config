pub mod claude_md;
pub mod credentials;
pub mod keyring;
pub mod marketplaces;
pub mod mcp;
pub mod permissions;
pub mod providers;
pub mod sessions;
pub mod settings;
pub mod skills;
pub mod tracker;

#[allow(unused_imports)] // credentials_path is used by tests + future UI surfacing
pub use credentials::{credentials_path, read_credentials_oauth, write_credentials_oauth};
pub use keyring::{KeyringStatus, KeyringStore};
pub use marketplaces::{scan_marketplaces, MarketplaceSummary};
pub use mcp::{scan_mcp_servers, McpServerSummary};
pub use sessions::{scan_sessions, SessionSummary};
pub use skills::{scan_skills, SkillSummary};
pub use providers::{load_providers_file, save_providers_file};
pub use settings::{discover_claude_dir, read_settings, settings_path, write_settings_atomic};
pub use tracker::{load_trackers_file, save_trackers_file, TrackerConfig};
