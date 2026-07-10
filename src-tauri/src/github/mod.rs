//! GitHub API client for session sync.
//!
//! Layered so device-flow code and repo ops code can be tested/used
//! independently. `client` is the shared `reqwest::blocking::Client`
//! (one per app, connection-pooled). Both submodules depend on it.

pub mod client;
pub mod device_flow;
pub mod repo;