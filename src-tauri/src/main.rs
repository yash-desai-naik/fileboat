// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod clipboard;
mod desktop;
mod dnd;
mod folders;
mod jobs;
mod launch;
mod navigate;
mod ops_cmd;
mod places_cmd;
mod settings;
mod state;
mod tools_cmd;
mod update;
mod watch;
mod window;

use tauri::{Manager, WindowEvent};

fn main() {
    let cwd = std::env::current_dir().unwrap_or_default();
    let launch_path = fileboat_core::fs::path_arg(&std::env::args().collect::<Vec<_>>(), &cwd);
    tauri::Builder::default()
        // One process for every window: a second launch focuses the running app instead of
        // starting another WebView browser process, and opens the path it was given in a new tab.
        .plugin(tauri_plugin_single_instance::init(launch::second_launch))
        .manage(launch::LaunchPath(std::sync::Mutex::new(launch_path)))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(update::UpdateState::default())
        .manage(folders::Folders::default())
        .manage(fileboat_core::ops::undo::UndoLog::default())
        .manage(jobs::Jobs::default())
        .manage(clipboard::AppClipboard::default())
        .setup(|app| {
            // Fit the screen, centered; the UI then maximizes and shows the window once it has
            // painted (no white flash), so "Restore Down" returns to this size and place.
            if let Some(main) = app.get_webview_window("main") {
                window::place_centered(&main, None);
            }
            app.manage(settings::SettingsStore::load()?);
            update::start_background_checks(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // A window's open folders close with it.
            if let WindowEvent::Destroyed = event {
                window.app_handle().state::<folders::Folders>().close_window(window.label());
            }
        })
        .invoke_handler(tauri::generate_handler![
            settings::get_settings,
            settings::update_settings,
            settings::open_settings_file,
            window::window_fit_restore,
            desktop::app_paths,
            desktop::open_app_folder,
            desktop::open_releases_page,
            desktop::open_path,
            folders::folder_open,
            folders::folder_rows,
            folders::folder_names,
            folders::folder_selection_info,
            folders::folder_view,
            folders::folder_row_of,
            folders::folder_seek,
            folders::folder_close,
            folders::folders_reset,
            folders::folder_refresh,
            ops_cmd::op_new_folder,
            ops_cmd::op_new_file,
            ops_cmd::op_rename,
            ops_cmd::op_check_name,
            ops_cmd::op_trash,
            ops_cmd::op_delete,
            ops_cmd::op_undo,
            jobs::job_transfer,
            jobs::job_cancel,
            jobs::job_resolve,
            clipboard::clipboard_set,
            clipboard::clipboard_get,
            clipboard::clipboard_clear_cut,
            dnd::drag_start,
            dnd::job_drop,
            tools_cmd::tools_list,
            tools_cmd::open_in_editor,
            tools_cmd::open_terminal,
            navigate::path_resolve,
            navigate::path_complete,
            places_cmd::places_list,
            launch::take_launch_path,
            places_cmd::drive_space,
            state::state_read,
            state::state_write,
            update::update_check,
            update::update_status,
            update::update_install,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Fileboat");
}
