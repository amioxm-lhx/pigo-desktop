//! Resident tray + window close behavior, ported from Electron
//! `bootstrap/app-lifecycle.ts` (D216) and the frameless close interceptor in
//! `bootstrap/window.ts`: the tray always exists, left click restores, menu
//! is Open/Quit, and the first close goes through ask/tray/quit.

use std::sync::Arc;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use crate::state::{AppState, APP_NAME};

/// Static tray/close strings, resolved by system locale like Electron's
/// `app.getLocale()` path (zh-CN catalog; English fallback).
struct TrayStrings {
    open: &'static str,
    quit: &'static str,
    ask_title: &'static str,
    ask_body: &'static str,
    close_to_tray: &'static str,
    quit_button: &'static str,
}

fn tray_strings() -> TrayStrings {
    let zh = sys_locale::get_locale()
        .map(|l| l.starts_with("zh"))
        .unwrap_or(false);
    if zh {
        TrayStrings {
            open: "打开 pigo-desktop",
            quit: "退出 pigo-desktop",
            ask_title: "关闭后继续在后台运行 pigo-desktop？",
            ask_body: "关闭窗口后，pigo-desktop 可以继续在系统托盘中运行，避免丢失任何内容。你随时可以在设置中更改此选项。",
            close_to_tray: "关闭到托盘",
            quit_button: "退出",
        }
    } else {
        TrayStrings {
            open: "Open pigo-desktop",
            quit: "Quit pigo-desktop",
            ask_title: "Keep pigo-desktop running in the background?",
            ask_body: "pigo-desktop can keep running in the system tray after you close the window, so nothing is lost. You can change this any time in Settings.",
            close_to_tray: "Close to tray",
            quit_button: "Quit",
        }
    }
}

pub fn restore_main_window(app: &AppHandle) {
    // macOS keeps the app (and its Dock icon) alive after the window hides.
    // If the user hid the app itself (Cmd+H) the window is still present but
    // belongs to a hidden app, so unhide the app before showing the window.
    #[cfg(target_os = "macos")]
    let _ = app.show();
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Create the resident tray icon (id: `main-tray`). The Electron build also
/// keeps it alive on every platform, so creating it at boot (not at first
/// close) matches the D216 semantics.
pub fn create_tray(app: &AppHandle) -> tauri::Result<()> {
    let strings = tray_strings();
    let open = MenuItem::with_id(app, "tray-open", strings.open, true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray-quit", strings.quit, true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;

    let icon_bytes = include_bytes!("../../build/icon.png");
    let icon = tauri::image::Image::from_bytes(icon_bytes)?;

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        .tooltip(APP_NAME)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray-open" => restore_main_window(app),
            "tray-quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // Left click restores the window, like Electron. macOS reports
            // both the press and the release; only the release restores so one
            // physical click cannot restore twice.
            if let tauri::tray::TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                log::info!("tray click: restoring main window");
                restore_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Close interception: "tray" hides, "ask" prompts once, "quit" closes.
/// Must run on the main thread (dialog + window ops) — called from
/// `on_window_event`.
pub fn handle_close_request(app: &AppHandle, state: &Arc<AppState>, window: tauri::Window) -> bool {
    let behavior = state.close_behavior.lock().unwrap().clone();
    match behavior.as_str() {
        "tray" => {
            let _ = window.hide();
            false // prevent close
        }
        "quit" => true, // allow close
        _ => {
            // "ask": the window stays open until the user picks a side.
            let app = app.clone();
            let window_clone = window.clone();
            tauri::async_runtime::spawn(async move {
                let strings = tray_strings();
                let dialog = app
                    .dialog()
                    .message(strings.ask_body)
                    .title(strings.ask_title)
                    .kind(MessageDialogKind::Info)
                    .buttons(MessageDialogButtons::OkCancelCustom(
                        strings.close_to_tray.to_string(),
                        strings.quit_button.to_string(),
                    ));
                let to_tray = tauri::async_runtime::spawn_blocking(move || dialog.blocking_show())
                    .await
                    .unwrap_or(false);
                let state = app.state::<Arc<AppState>>();
                *state.close_behavior.lock().unwrap() = if to_tray {
                    "tray".into()
                } else {
                    "quit".into()
                };
                if to_tray {
                    let _ = window_clone.hide();
                } else {
                    // The user picked quit: destroy the window; the app exits
                    // with the last window (Windows/Linux default).
                    let _ = window_clone.destroy();
                }
            });
            false // prevent this close; the async path decides
        }
    }
}

pub fn on_window_event(
    app: &AppHandle,
    state: &Arc<AppState>,
    window: &tauri::Window,
    event: &WindowEvent,
) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        // The tray/ask behavior belongs to the app's own window. Auxiliary
        // windows (staff sign-in, the hidden site collector) close directly;
        // intercepting them would leave a prompt nobody asked for and keep the
        // collector window alive after its sweep.
        if window.label() != "main" {
            return;
        }
        let allowed = handle_close_request(app, state, window.clone());
        if !allowed {
            api.prevent_close();
        }
    }
}
