mod commands;
mod domain;
mod jobs;
mod services;
mod state;

use tauri::{Manager, RunEvent};
use tracing_subscriber::EnvFilter;

fn init_logging() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,entrocut_lib=debug"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_logging();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "starting EntroCut");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(state::AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::system::system_check,
            commands::project::get_project,
            commands::project::create_project,
            commands::project::open_project,
            commands::project::save_project,
            commands::media::import_media,
            commands::media::generate_thumbnail,
            commands::edit::apply_edit,
            commands::edit::undo_edit,
            commands::edit::redo_edit,
            commands::preview::render_preview_frame,
            commands::export::start_export,
            commands::export::cancel_job,
            commands::export::get_job_status,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            // Kill FFmpeg rather than leave it rendering after the window closes.
            if let RunEvent::Exit = event {
                app.state::<state::AppState>().jobs.cancel_all();
            }
        });
}
