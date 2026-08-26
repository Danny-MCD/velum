mod commands;
mod ports;
mod site_server;
mod state;
mod store;
mod tor;

use std::collections::HashMap;
use std::sync::Mutex;

use tauri::Manager;

use state::AppState;
use store::SiteStore;
use tor::TorManager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir()?;
            let sites = SiteStore::load(&data_dir)?;

            app.manage(AppState {
                tor: TorManager::new(handle.clone()),
                sites: Mutex::new(sites),
                running_servers: Mutex::new(HashMap::new()),
            });

            // Start Tor immediately so it's bootstrapped (or close to it) by
            // the time the user publishes their first site.
            let state = app.state::<AppState>();
            state.tor.start()?;

            // Bring back anything the user had published last time.
            tauri::async_runtime::spawn(async move {
                commands::restore_enabled_sites(&handle);
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::tor_status,
            commands::list_sites,
            commands::pick_folder,
            commands::create_site,
            commands::publish_site,
            commands::unpublish_site,
            commands::delete_site,
            commands::reveal_private_key,
            commands::onion_qr_svg,
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                let state = window.state::<AppState>();
                state.tor.stop();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Velum");
}
