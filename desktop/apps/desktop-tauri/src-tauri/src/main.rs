//! pigo-desktop Tauri shell entry. Replaces the Electron main
//! process: window + renderer host, backend process supervision, channel
//! router, native surface. See ../README.md for the porting map.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod boot;
mod browser;
mod catalog;
mod collaboration;
mod composer;
mod desktop_control;
mod envelope;
mod events;
mod extension_import;
mod fsops;
mod git_clone;
mod importer;
mod importer_opencode_sqlite;
mod launch;
mod market;
mod mcp;
mod model_config_import;
mod models;
mod native;
mod oauth;
mod plugin_bridge;
mod plugin_host;
mod plugin_scaffold;
mod plugin_surface;
mod plugins;
mod preview;
mod proxy;
mod proxy_test;
mod pulls;
mod queue;
mod router;
mod rpc;
mod session_export;
mod shell;
mod staff_driver;
mod staff_site_scripts;
mod staff_sites;
mod staff_watch;
mod state;
mod stocks;
mod subagents;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::Manager;

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let data_dir = boot::resolve_data_dir();
    let _ = std::fs::create_dir_all(&data_dir);

    // Notification channels must exist before the clients: the reader tasks
    // push (method, params) into them from the first spawned line.
    let (host_tx, host_rx) = tokio::sync::mpsc::channel(2048);
    let (sidecar_tx, sidecar_rx) = tokio::sync::mpsc::channel(2048);

    let host_binary =
        boot::resolve_host_binary().unwrap_or_else(|| PathBuf::from("pi-desktop-host-core"));
    let sidecar_binary =
        boot::resolve_sidecar_binary().unwrap_or_else(|| PathBuf::from("pi-go-desktop-agent"));

    let host_env_remove: Vec<String> = [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let host = rpc::RpcClient::new(
        "host",
        host_binary,
        vec![(
            "PI_DESKTOP_DATA_DIR".to_string(),
            data_dir.to_string_lossy().to_string(),
        )],
        host_env_remove,
        true,
        host_tx,
    );
    let sidecar = rpc::RpcClient::new("sidecar", sidecar_binary, vec![], vec![], false, sidecar_tx);

    // The plugin runtime talks to host-core for registry rows and settings.
    let surfaces = plugin_surface::PluginSurfaceServer::start()
        .ok()
        .map(Arc::new);
    // One MCP runtime holds both user-owned servers and plugin-declared ones;
    // the plugin host prunes its entries when a plugin unloads.
    let mcp = mcp::new_runtime();
    let plugins = std::sync::Arc::new(plugin_host::PluginHost::new(
        data_dir.clone(),
        host.clone(),
        surfaces.clone(),
        mcp.clone(),
    ));

    let stocks_engine = stocks::StocksEngine::new(data_dir.clone());
    let app_state = Arc::new(state::AppState {
        data_dir,
        host,
        sidecar,
        viewing_session: std::sync::Mutex::new(None),
        // Electron default (index.ts): the first close asks tray vs quit.
        close_behavior: std::sync::Mutex::new("ask".to_string()),
        active_turns: std::sync::Mutex::new(std::collections::HashMap::new()),
        active_executions: std::sync::Mutex::new(std::collections::HashMap::new()),
        active_tools: std::sync::Mutex::new(std::collections::HashMap::new()),
        composer_tokens: std::sync::Mutex::new(std::collections::HashMap::new()),
        catalog: Arc::new(catalog::Catalog::new()),
        oauth: Arc::new(oauth::OAuthManager::new()),
        plugins,
        mcp,
        browser: browser::BrowserHost::new(),
        market: market::new_runtime(),
        preview: preview::PreviewServer::start().ok().map(Arc::new),
        surfaces,
        provider_bindings: std::sync::Mutex::new(std::collections::HashMap::new()),
        session_import_cache: std::sync::Mutex::new(std::collections::HashMap::new()),
        model_import_cache: std::sync::Mutex::new(std::collections::HashMap::new()),
        staff: staff_driver::Resident::new(),
        watch: staff_watch::Watch::new(),
        stocks: stocks_engine,
    });
    let app_state_for_events = app_state.clone();

    tauri::Builder::default()
        // Theme assets are served read-only from an enabled plugin's declared
        // asset list. The handler checks the plugin grant and canonical path.
        .register_asynchronous_uri_scheme_protocol("plugin-asset", {
            let state = app_state.clone();
            move |_context, request, responder| {
                let state = state.clone();
                tauri::async_runtime::spawn(async move {
                    responder.respond(plugins::asset_response(&state, request).await);
                });
            }
        })
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            native::focus_main_window(app);
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .manage(app_state.clone())
        .setup(move |app| {
            let handle = app.handle().clone();
            app_state.plugins.attach_app(handle.clone());
            // Resident tray (D216 parity): exists from boot, left click
            // restores the window, menu is Open/Quit.
            if let Err(e) = shell::create_tray(&handle) {
                log::warn!("tray creation failed: {e}");
            }
            events::install_sidecar_proxy(handle.clone(), &app_state);
            events::dispatch(handle.clone(), app_state.clone(), host_rx, sidecar_rx);
            // The resident staff driver starts only after the handshakes: a
            // claim against a booting host would just fill the error slot.
            let boot_handle = handle.clone();
            let boot_state = app_state.clone();
            let driver_handle = handle.clone();
            let driver_state = app_state.clone();
            tauri::async_runtime::spawn(async move {
                boot::boot(boot_handle, boot_state).await;
                staff_driver::start(driver_handle.clone(), driver_state.clone());
                staff_watch::start(driver_handle, driver_state);
            });
            Ok(())
        })
        .on_window_event(move |window, event| {
            let app = window.app_handle().clone();
            shell::on_window_event(&app, &app_state_for_events, window, event);
        })
        .invoke_handler(tauri::generate_handler![router::pi_desktop_invoke])
        .build(tauri::generate_context!())
        .expect("error while building pigo-desktop")
        .run(|app_handle, event| {
            // macOS: clicking the Dock icon of a running app asks the app to
            // reopen. A window hidden to the tray stays hidden unless this
            // restores it, so the app looked dead after closing to tray.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                shell::restore_main_window(app_handle);
            }
            #[cfg(not(target_os = "macos"))]
            {
                let _ = (app_handle, event);
            }
        });
}
