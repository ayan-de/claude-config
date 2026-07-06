pub mod claude_md;
pub mod credentials;
pub mod keyring;
pub mod marketplaces;
pub mod providers;
pub mod settings;

#[allow(unused_imports)] // credentials_path is used by tests + future UI surfacing
pub use credentials::{credentials_path, read_credentials_oauth, write_credentials_oauth};
pub use keyring::{KeyringStatus, KeyringStore};
pub use marketplaces::{scan_marketplaces, MarketplaceSummary};
pub use providers::{load_providers_file, save_providers_file};
pub use settings::{discover_claude_dir, read_settings, settings_path, write_settings_atomic};
