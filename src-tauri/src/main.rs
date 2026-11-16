// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod ai;
mod clipboard;
mod desktop;
mod dnd;
mod folders;
mod icons;
mod indexer;
mod git_cmd;
mod grep_cmd;
mod jobs;
mod launch;
mod native_menu;
mod navigate;
mod ops_cmd;
mod places_cmd;
mod preview_cmd;
mod repos;
mod settings;
mod shell_cmd;
mod sizes;
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
        .manage(preview_cmd::PreviewFiles::default())
        .manage(git_cmd::Syncing::default())
        .manage(std::sync::Arc::new(grep_cmd::Greps::default()))
        .manage(std::sync::Arc::new(shell_cmd::Shells::default()))
        .manage(ai::ClaudeHub::default())
        // A VS Code icon theme's files (from the icon-themes folder only).
        .register_asynchronous_uri_scheme_protocol("fbicon", |_ctx, request, responder| {
            std::thread::spawn(move || responder.respond(icons::serve(&request)));
        })
        // Images, media, PDFs and fonts for the preview, read off the main thread.
        .register_asynchronous_uri_scheme_protocol("fbpreview", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || {
                let files = app.state::<preview_cmd::PreviewFiles>();
                responder.respond(preview_cmd::serve(&files, &request));
            });
        })
        .setup(|app| {
            // Fit the screen, centered; the UI then maximizes and shows the window once it has
            // painted (no white flash), so "Restore Down" returns to this size and place.
            if let Some(main) = app.get_webview_window("main") {
                window::place_centered(&main, None);
            }
            app.manage(settings::SettingsStore::load()?);
            app.state::<folders::Folders>().set_app(app.handle().clone());
            let settings = app.state::<settings::SettingsStore>().current();
            app.manage(indexer::Indexer::start(app.handle().clone(), &settings));
            update::start_background_checks(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // A window's open folders close with it.
            if let WindowEvent::Destroyed = event {
                window.app_handle().state::<folders::Folders>().close_window(window.label());
                // Its Claude Code process goes with it.
                let (app, label) = (window.app_handle().clone(), window.label().to_string());
                tauri::async_runtime::spawn(async move { ai::stop_window(&app.state::<ai::ClaudeHub>(), &label).await });
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
            desktop::open_url,
            desktop::reveal_in_file_manager,
            desktop::show_properties,
            desktop::open_with_dialog,
            native_menu::native_menu,
            preview_cmd::preview_info,
            git_cmd::git_info,
            git_cmd::git_status_run,
            indexer::index_search,
            grep_cmd::grep_start,
            grep_cmd::grep_cancel,
            shell_cmd::shell_run,
            ai::claude_status,
            icons::icon_themes_list,
            icons::icon_theme_load,
            icons::icon_themes_folder,
            icons::icon_image,
            icons::system_icon,
            ai::claude_send,
            ai::claude_stop,
            ai::claude_new,
            ai::claude_set_context,
            ai::claude_permission_reply,
            ai::ui_reply,
            ai::claude_sessions,
            ai::claude_session_messages,
            ai::read_attachment,
            ai::voice_typing,
            shell_cmd::shell_cancel,
            indexer::index_status,
            indexer::index_rebuild,
            indexer::index_pause,
            indexer::index_rules,
            git_cmd::git_status,
            git_cmd::git_paths,
            git_cmd::git_ignore,
            git_cmd::git_commit,
            git_cmd::git_branches,
            git_cmd::git_branch_op,
            git_cmd::git_log,
            git_cmd::git_sync,
            git_cmd::git_diff,
            git_cmd::git_web_url,
            preview_cmd::preview_text,
            preview_cmd::preview_bytes,
            preview_cmd::preview_archive,
            preview_cmd::preview_structured,
            preview_cmd::preview_folder,
            preview_cmd::preview_url,
            folders::folder_open,
            folders::folder_rows,
            folders::folder_names,
            folders::folder_entries,
            folders::folder_groups,
            folders::folder_collapse,
            folders::folder_selection_info,
            folders::folder_view,
            folders::folder_sizes,
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
