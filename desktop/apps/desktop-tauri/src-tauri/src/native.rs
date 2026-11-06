//! Native surface that used to be Electron APIs: window chrome controls,
//! native dialogs, system notifications, shell open/reveal, devtools.

use serde_json::{json, Value};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

fn main_window(app: &tauri::AppHandle) -> Option<tauri::WebviewWindow> {
    app.get_webview_window("main")
}

/// `pi-desktop/window/control` — renderer-drawn chrome buttons.
pub fn window_control(app: &tauri::AppHandle, input: &Value) -> Value {
    let action = input.get("action").and_then(|v| v.as_str()).unwrap_or("");
    let Some(window) = main_window(app) else {
        return crate::envelope::ok(json!({ "maximized": false }));
    };
    match action {
        "minimize" => {
            let _ = window.minimize();
        }
        "toggleMaximize" => {
            if window.is_maximized().unwrap_or(false) {
                let _ = window.unmaximize();
            } else {
                let _ = window.maximize();
            }
        }
        "close" => {
            let _ = window.close();
        }
        "getState" => {}
        other => {
            return crate::envelope::err(
                "INVALID_ARGUMENT",
                format!("unsupported window control action: {other}"),
            )
        }
    }
    crate::envelope::ok(json!({ "maximized": window.is_maximized().unwrap_or(false) }))
}

/// `pi-desktop/menu/nativeAction` — the 15 native menu actions.
pub fn native_menu_action(app: &tauri::AppHandle, action: &str) -> Value {
    let Some(window) = main_window(app) else {
        return crate::envelope::ok(json!({ "handled": false }));
    };
    match action {
        "undo" | "redo" | "cut" | "copy" | "paste" | "selectAll" => {
            let command = match action {
                "selectAll" => "selectAll",
                other => other,
            };
            let _ = window.eval(&format!("document.execCommand('{command}')"));
        }
        "reload" => {
            let _ = window.eval("location.reload()");
        }
        "zoomIn" => zoom_by(&window, 0.1),
        "zoomOut" => zoom_by(&window, -0.1),
        "resetZoom" => {
            let _ = window.set_zoom(1.0);
        }
        "toggleFullScreen" => {
            let next = !window.is_fullscreen().unwrap_or(false);
            let _ = window.set_fullscreen(next);
        }
        "minimize" => {
            let _ = window.minimize();
        }
        "toggleMaximize" => {
            if window.is_maximized().unwrap_or(false) {
                let _ = window.unmaximize();
            } else {
                let _ = window.maximize();
            }
        }
        "close" => {
            let _ = window.close();
        }
        "restoreMainWindow" => {
            let _ = window.show();
            let _ = window.set_focus();
        }
        _ => {}
    }
    crate::envelope::ok(json!({ "handled": true }))
}

/// wry does not expose the live zoom factor, so menu zoom steps between the
/// documented 0.5–2.0 range rather than compounding from the current value.
fn zoom_by(window: &tauri::WebviewWindow, delta: f64) {
    let _ = window.set_zoom((1.0 + delta).clamp(0.5, 2.0));
}

pub fn open_path(app: &tauri::AppHandle, path: &str) -> Result<(), String> {
    app.opener()
        .open_path(path.to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

pub fn reveal_item(app: &tauri::AppHandle, path: &str) -> Result<(), String> {
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|e| e.to_string())
}

pub fn open_url(app: &tauri::AppHandle, url: &str) -> Result<(), String> {
    app.opener()
        .open_url(url.to_string(), None::<&str>)
        .map_err(|e| e.to_string())
}

pub fn pick_folder(app: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    app.dialog()
        .file()
        .blocking_pick_folder()
        .and_then(|p| p.into_path().ok())
}

pub fn pick_folders(app: &tauri::AppHandle) -> Vec<std::path::PathBuf> {
    app.dialog()
        .file()
        .blocking_pick_folders()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .collect()
}

pub fn pick_file(
    app: &tauri::AppHandle,
    filter_name: &str,
    extensions: &[&str],
) -> Option<std::path::PathBuf> {
    let mut builder = app.dialog().file();
    if !extensions.is_empty() {
        builder = builder.add_filter(filter_name, extensions);
    }
    builder
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
}

pub fn save_file(app: &tauri::AppHandle, file_name: &str) -> Option<std::path::PathBuf> {
    app.dialog()
        .file()
        .set_file_name(file_name)
        .blocking_save_file()
        .and_then(|p| p.into_path().ok())
}

pub fn show_notification(app: &tauri::AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

pub fn toggle_devtools(app: &tauri::AppHandle) {
    if let Some(window) = main_window(app) {
        if window.is_devtools_open() {
            window.close_devtools();
        } else {
            window.open_devtools();
        }
    }
}

pub fn focus_main_window(app: &tauri::AppHandle) {
    if let Some(window) = main_window(app) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
