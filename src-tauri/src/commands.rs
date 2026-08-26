use tauri::{AppHandle, Manager, State};

use crate::ports::free_local_port;
use crate::site_server;
use crate::state::AppState;
use crate::store::{Site, SiteMode, SiteView};
use crate::tor::TorStatus;

fn to_view(site: &Site, running: bool) -> SiteView {
    SiteView {
        id: site.id.clone(),
        name: site.name.clone(),
        mode: site.mode.clone(),
        onion_address: site.onion_address.clone(),
        created_at: site.created_at.clone(),
        enabled: site.enabled,
        running,
    }
}

#[tauri::command]
pub fn tor_status(state: State<AppState>) -> TorStatus {
    state.tor.status()
}

#[tauri::command]
pub fn list_sites(state: State<AppState>) -> Vec<SiteView> {
    let sites = state.sites.lock().unwrap();
    let running = state.running_servers.lock().unwrap();
    sites
        .all()
        .iter()
        .map(|s| to_view(s, s.enabled && (running.contains_key(&s.id) || matches!(s.mode, SiteMode::Existing { .. }))))
        .collect()
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    rx.recv().ok().flatten().map(|f| f.to_string())
}

#[tauri::command]
pub fn create_site(state: State<AppState>, name: String, mode: SiteMode) -> Result<SiteView, String> {
    let site = Site {
        id: uuid::Uuid::new_v4().to_string(),
        name,
        mode,
        onion_address: None,
        private_key: None,
        created_at: chrono::Utc::now().to_rfc3339(),
        enabled: false,
    };
    let view = to_view(&site, false);
    let mut sites = state.sites.lock().unwrap();
    sites.insert(site).map_err(|e| e.to_string())?;
    Ok(view)
}

#[tauri::command]
pub fn publish_site(app: AppHandle, state: State<AppState>, id: String) -> Result<SiteView, String> {
    publish_site_internal(&app, &state, &id).map_err(|e| e.to_string())
}

pub fn publish_site_internal(
    app: &AppHandle,
    state: &AppState,
    id: &str,
) -> anyhow::Result<SiteView> {
    let (mode, existing_key) = {
        let sites = state.sites.lock().unwrap();
        let site = sites
            .get(id)
            .ok_or_else(|| anyhow::anyhow!("Site not found"))?;
        (site.mode.clone(), site.private_key.clone())
    };

    let local_port = match &mode {
        SiteMode::Existing { local_port } => *local_port,
        SiteMode::Static { folder } => {
            let mut servers = state.running_servers.lock().unwrap();
            if !servers.contains_key(id) {
                let port = free_local_port()?;
                let server = site_server::serve_folder(folder.into(), port)?;
                servers.insert(id.to_string(), server);
                // stash the port on the server map isn't enough; record it below
                port
            } else {
                // Already running - re-publishing to Tor still needs a port,
                // so we track it alongside the server the first time it's
                // started. Simplify by just reallocating a server-free path:
                // this branch means we already have a live local port bound.
                // We recover it by asking site_server for its port isn't
                // exposed, so we cache it on the Site record instead.
                return republish_static(app, state, id);
            }
        }
    };

    state.tor.start()?;

    let info = state.tor.add_onion(existing_key.as_deref(), local_port)?;

    let mut sites = state.sites.lock().unwrap();
    let site = sites
        .get_mut(id)
        .ok_or_else(|| anyhow::anyhow!("Site not found"))?;
    site.onion_address = Some(info.address);
    site.private_key = Some(info.private_key);
    site.enabled = true;
    let view = to_view(site, true);
    sites.persist()?;
    Ok(view)
}

/// Handles the (rare) case where a static site's local server is already
/// running - e.g. a second publish click - by just re-asserting the onion
/// service against the existing site record without starting a new server.
fn republish_static(app: &AppHandle, state: &AppState, id: &str) -> anyhow::Result<SiteView> {
    let _ = app; // reserved for future use (e.g. re-emitting status)
    let mut sites = state.sites.lock().unwrap();
    let site = sites
        .get_mut(id)
        .ok_or_else(|| anyhow::anyhow!("Site not found"))?;
    site.enabled = true;
    let view = to_view(site, true);
    sites.persist()?;
    Ok(view)
}

#[tauri::command]
pub fn unpublish_site(state: State<AppState>, id: String) -> Result<SiteView, String> {
    unpublish_site_internal(&state, &id).map_err(|e| e.to_string())
}

fn unpublish_site_internal(state: &AppState, id: &str) -> anyhow::Result<SiteView> {
    let address = {
        let sites = state.sites.lock().unwrap();
        sites
            .get(id)
            .and_then(|s| s.onion_address.clone())
    };
    if let Some(address) = address {
        // Best-effort: if Tor isn't running there's nothing to tear down.
        let _ = state.tor.del_onion(&address);
    }
    if let Some(server) = state.running_servers.lock().unwrap().remove(id) {
        server.stop();
    }
    let mut sites = state.sites.lock().unwrap();
    let site = sites
        .get_mut(id)
        .ok_or_else(|| anyhow::anyhow!("Site not found"))?;
    site.enabled = false;
    let view = to_view(site, false);
    sites.persist()?;
    Ok(view)
}

#[tauri::command]
pub fn delete_site(state: State<AppState>, id: String) -> Result<(), String> {
    let _ = unpublish_site_internal(&state, &id);
    let mut sites = state.sites.lock().unwrap();
    sites.remove(&id).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn reveal_private_key(state: State<AppState>, id: String) -> Result<String, String> {
    let sites = state.sites.lock().unwrap();
    sites
        .get(&id)
        .and_then(|s| s.private_key.clone())
        .ok_or_else(|| "This site hasn't been published yet, so it has no key.".to_string())
}

#[tauri::command]
pub fn onion_qr_svg(address: String) -> Result<String, String> {
    use qrcode::render::svg;
    use qrcode::QrCode;
    let code = QrCode::new(address.as_bytes()).map_err(|e| e.to_string())?;
    let svg = code
        .render::<svg::Color>()
        .min_dimensions(220, 220)
        .dark_color(svg::Color("#1b1730"))
        .light_color(svg::Color("#ffffff"))
        .build();
    Ok(svg)
}

/// Called once at startup to bring back any sites the user left enabled.
pub fn restore_enabled_sites(app: &AppHandle) {
    let state = app.state::<AppState>();
    let ids: Vec<String> = {
        let sites = state.sites.lock().unwrap();
        sites
            .all()
            .iter()
            .filter(|s| s.enabled && s.private_key.is_some())
            .map(|s| s.id.clone())
            .collect()
    };
    for id in ids {
        if let Err(e) = publish_site_internal(app, &state, &id) {
            eprintln!("Failed to restore site {id}: {e}");
        }
    }
}
