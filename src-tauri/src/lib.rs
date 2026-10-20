mod commands;
mod domain;
mod services;

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
        .invoke_handler(tauri::generate_handler![commands::system::system_check])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
