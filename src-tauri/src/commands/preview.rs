use std::sync::atomic::Ordering;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::domain::errors::{AppError, AppResult};
use crate::domain::project::AssetKind;
use crate::services::{frames, preview};
use crate::state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewFrame {
    pub path: String,
    pub clip_id: Uuid,
}

/// Renders the still frame shown at `time_us`. Returns `None` when nothing
/// is visible there. A request superseded by a newer one before it starts
/// returns a `cancelled` error, which the frontend ignores.
#[tauri::command]
pub async fn render_preview_frame(
    app: AppHandle,
    state: State<'_, AppState>,
    time_us: u64,
) -> AppResult<Option<PreviewFrame>> {
    let generation = state.preview_generation.fetch_add(1, Ordering::SeqCst) + 1;

    let (clip_id, source, source_time, size) = {
        let session = state.session();
        let project = &session.project;
        let Some((clip, asset)) = project.visual_clip_at(time_us) else {
            return Ok(None);
        };
        let source_time = match asset.kind {
            AssetKind::Image => 0,
            _ => clip.source_time_at(time_us),
        };
        (
            clip.id,
            asset.path.clone(),
            source_time,
            preview::preview_size(&project.settings),
        )
    };

    let _permit = state
        .preview_jobs
        .acquire()
        .await
        .map_err(|e| AppError::InvalidRequest(e.to_string()))?;
    if state.preview_generation.load(Ordering::SeqCst) != generation {
        return Err(AppError::Cancelled);
    }

    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::InvalidRequest(e.to_string()))?
        .join("frames");

    let path = tauri::async_runtime::spawn_blocking(move || {
        let out = preview::render_frame(&cache_dir, &source, source_time, size)?;
        if let Err(e) = frames::prune(&cache_dir, preview::PREVIEW_CACHE_FILES) {
            tracing::debug!(error = %e, "preview cache prune failed");
        }
        Ok::<_, AppError>(out)
    })
    .await
    .map_err(|e| AppError::InvalidRequest(e.to_string()))??;

    Ok(Some(PreviewFrame {
        path: path.display().to_string(),
        clip_id,
    }))
}
