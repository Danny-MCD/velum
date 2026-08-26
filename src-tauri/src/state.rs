use std::collections::HashMap;
use std::sync::Mutex;

use crate::site_server::RunningServer;
use crate::store::SiteStore;
use crate::tor::TorManager;

/// Everything shared across Tauri commands, held in `tauri::State`.
pub struct AppState {
    pub tor: TorManager,
    pub sites: Mutex<SiteStore>,
    /// Embedded static-file servers for currently-published "static folder"
    /// sites, keyed by site id. Only sites in `SiteMode::Static` show up here;
    /// `SiteMode::Existing` sites point straight at the user's own server.
    pub running_servers: Mutex<HashMap<String, RunningServer>>,
}
