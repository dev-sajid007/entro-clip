use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use uuid::Uuid;

use crate::domain::errors::{AppError, AppResult};
use crate::domain::project::{MediaAsset, Project};
use crate::services::{media_probe, thumbnail};
use crate::state::{AppState, ProjectState};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase", tag = "status")]
pub enum ImportResult {
    Imported { path: String, asset_id: Uuid },
    Duplicate { path: String, asset_id: Uuid },
    Failed { path: String, error: AppError },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub state: ProjectState,
    pub results: Vec<ImportResult>,
}

/// Validates and probes each path, then registers the successful ones as
/// assets. One bad file never aborts the rest of the import.
#[tauri::command]
pub async fn import_media(
    app: AppHandle,
    state: State<'_, AppState>,
    paths: Vec<String>,
) -> AppResult<ImportReport> {
    let probed = tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|p| {
                let result = probe_asset(&p);
                (p, result)
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|e| AppError::InvalidRequest(e.to_string()))?;

    let mut session = state.session();
    let mut project = session.project.clone();
    let mut results = Vec::with_capacity(probed.len());

    for (path, result) in probed {
        let asset = match result {
            Ok(asset) => asset,
            Err(error) => {
                tracing::warn!(path, %error, "media import failed");
                results.push(ImportResult::Failed { path, error });
                continue;
            }
        };
        if let Some(existing) = project.assets.iter().find(|a| a.path == asset.path) {
            results.push(ImportResult::Duplicate {
                path,
                asset_id: existing.id,
            });
            continue;
        }
        results.push(ImportResult::Imported {
            path,
            asset_id: asset.id,
        });
        project.assets.push(asset);
    }

    if project.assets.len() != session.project.assets.len() {
        project.validate().map_err(AppError::InvalidProject)?;
        allow_media_playback(&app, &project);
        session.commit(project);
    }
    Ok(ImportReport {
        state: session.snapshot(),
        results,
    })
}

fn probe_asset(path: &str) -> AppResult<MediaAsset> {
    let path = PathBuf::from(path)
        .canonicalize()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => AppError::NotFound(path.to_string()),
            _ => AppError::Io(e),
        })?;
    if !path.is_file() {
        return Err(AppError::InvalidRequest(format!(
            "{} is not a regular file",
            path.display()
        )));
    }

    let media = media_probe::probe(&path)?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    Ok(MediaAsset {
        id: Uuid::new_v4(),
        kind: media.kind,
        path,
        name,
        duration_us: media.duration_us,
        width: media.width,
        height: media.height,
        frame_rate: media.frame_rate,
        has_audio: media.has_audio,
    })
}

/// Returns the path of a cached thumbnail for the asset, generating it in
/// the background if needed.
#[tauri::command]
pub async fn generate_thumbnail(
    app: AppHandle,
    state: State<'_, AppState>,
    asset_id: Uuid,
) -> AppResult<String> {
    let (path, kind, duration_us) = {
        let session = state.session();
        let asset = session
            .project
            .asset(asset_id)
            .ok_or_else(|| AppError::NotFound(format!("asset {asset_id}")))?;
        (asset.path.clone(), asset.kind, asset.duration_us)
    };

    let cache_dir = app
        .path()
        .app_cache_dir()
        .map_err(|e| AppError::InvalidRequest(e.to_string()))?
        .join("thumbnails");

    let _permit = state
        .thumbnail_jobs
        .acquire()
        .await
        .map_err(|e| AppError::InvalidRequest(e.to_string()))?;

    let thumb = tauri::async_runtime::spawn_blocking(move || {
        thumbnail::generate(&cache_dir, &path, kind, duration_us)
    })
    .await
    .map_err(|e| AppError::InvalidRequest(e.to_string()))?
    .inspect_err(|e| tracing::warn!(%asset_id, error = %e, "thumbnail failed"))?;

    Ok(thumb.display().to_string())
}

/// Lets the webview stream exactly the project's source files through the
/// asset protocol (for preview playback), without opening up other paths.
pub fn allow_media_playback(app: &AppHandle, project: &Project) {
    let scope = app.asset_protocol_scope();
    for asset in &project.assets {
        if let Err(e) = scope.allow_file(&asset.path) {
            tracing::warn!(path = %asset.path.display(), error = %e, "could not allow media file");
        }
    }
}
