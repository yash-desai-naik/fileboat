// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod desktop;
mod settings;
mod window;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        // One process for every window: a second launch focuses the running app instead of
        // starting another WebView browser process.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.webview_windows().values().next() {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .setup(|app| {
            // Fit the screen, centered; the UI then maximizes and shows the window once it has
            // painted (no white flash), so "Restore Down" returns to this size and place.
            if let Some(main) = app.get_webview_window("main") {
                window::place_centered(&main, None);
            }
            app.manage(settings::SettingsStore::load()?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::update_settings,
            settings::open_settings_file,
            window::window_fit_restore,
            desktop::app_paths,
            desktop::open_app_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Fileboat");
}
