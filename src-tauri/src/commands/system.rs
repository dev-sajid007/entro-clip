use serde::Serialize;

use crate::services::tools::{ToolStatus, check_tool};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemCheck {
    pub app_version: String,
    pub ffmpeg: ToolStatus,
    pub ffprobe: ToolStatus,
}

/// Reports app version and whether the media tools are usable.
/// Runs on a blocking thread so process spawning never stalls the UI.
#[tauri::command]
pub async fn system_check() -> SystemCheck {
    tauri::async_runtime::spawn_blocking(|| SystemCheck {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        ffmpeg: check_tool("ffmpeg"),
        ffprobe: check_tool("ffprobe"),
    })
    .await
    .expect("system check task panicked")
}
