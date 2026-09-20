mod batch;
mod commands;
mod error;
mod picker;
pub mod recovery;
mod selection;
mod session;

use session::AppState;
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::choose_inputs,
            commands::add_paths,
            commands::choose_output,
            commands::recover,
            commands::cancel_recovery,
            commands::clear_selection,
            commands::open_result
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.state::<AppState>().is_recovering() {
                    api.prevent_close();
                    let _ = window.emit("recovery-close-requested", ());
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("Unable to start KSD Decrypt")
        .run(|app, event| {
            // The native Quit menu does not necessarily request a window close.
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if app.state::<AppState>().is_recovering() {
                    api.prevent_exit();
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.emit("recovery-close-requested", ());
                    }
                }
            }
        });
}
