use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::domain::errors::{AppError, AppResult, ErrorInfo};
use crate::domain::project::Project;
use crate::domain::render_plan::{self, Quality, RenderPlan, resolve_settings};
use crate::domain::time::Rational;
use crate::jobs::events::{self, ExportCompleted, ExportProgress, JobFailed};
use crate::jobs::manager::JobStatus;
use crate::services::render::{self, RenderControl};
use crate::state::AppState;

/// Progress events are sent at most this often so FFmpeg can't flood the UI.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportRequest {
    pub output_path: String,
    /// Shorter edge in pixels; `None` keeps the project size.
    pub short_side: Option<u32>,
    /// `None` keeps the project frame rate.
    pub frame_rate: Option<Rational>,
    pub quality: Quality,
    /// Set once the user has confirmed replacing an existing file.
    #[serde(default)]
    pub overwrite: bool,
}

/// Validates the project and output, then renders in the background.
/// Progress and the outcome arrive as events tagged with the returned job id.
#[tauri::command]
pub async fn start_export(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ExportRequest,
) -> AppResult<Uuid> {
    let project = state.session().project.clone();
    let output = resolve_output(Path::new(&request.output_path), &project, request.overwrite)?;
    check_sources(&project)?;
    let encoder = tauri::async_runtime::spawn_blocking(render::h264_encoder)
        .await
        .map_err(|e| AppError::InvalidRequest(e.to_string()))??;
    let settings = resolve_settings(
        &project.settings,
        request.short_side,
        request.frame_rate,
        request.quality,
        encoder,
    )?;
    let plan = render_plan::build(&project, settings)?;

    let (job_id, control) = state.jobs.start()?;
    tracing::info!(%job_id, output = %output.display(), "starting export");
    thread::spawn(move || run_export(app, job_id, plan, output, control));
    Ok(job_id)
}

#[tauri::command]
pub fn cancel_job(state: State<'_, AppState>, job_id: Uuid) -> AppResult<()> {
    state.jobs.cancel(job_id)
}

#[tauri::command]
pub fn get_job_status(state: State<'_, AppState>, job_id: Uuid) -> AppResult<JobStatus> {
    state.jobs.status(job_id)
}

fn run_export(
    app: AppHandle,
    job_id: Uuid,
    plan: RenderPlan,
    output: PathBuf,
    control: Arc<RenderControl>,
) {
    let state = app.state::<AppState>();
    let duration_us = plan.duration_us;
    let mut last_sent: Option<Instant> = None;

    let result = render::run(&plan, &output, &control, |out_time_us| {
        let progress = out_time_us as f64 / duration_us as f64;
        state
            .jobs
            .set_status(job_id, JobStatus::Running { progress });
        let due = last_sent.is_none_or(|t| t.elapsed() >= PROGRESS_INTERVAL);
        if due || out_time_us == duration_us {
            last_sent = Some(Instant::now());
            emit(
                &app,
                events::EXPORT_PROGRESS,
                ExportProgress {
                    job_id,
                    progress,
                    out_time_us,
                },
            );
        }
    });

    match result {
        Ok(()) => {
            let output_path = output.display().to_string();
            tracing::info!(%job_id, output_path, "export completed");
            state.jobs.set_status(
                job_id,
                JobStatus::Completed {
                    output_path: output_path.clone(),
                },
            );
            emit(
                &app,
                events::EXPORT_COMPLETED,
                ExportCompleted {
                    job_id,
                    output_path,
                },
            );
        }
        Err(err) => {
            let error = ErrorInfo::from(&err);
            if matches!(err, AppError::Cancelled) {
                tracing::info!(%job_id, "export cancelled");
                state.jobs.set_status(job_id, JobStatus::Cancelled);
            } else {
                tracing::warn!(%job_id, error = %err, "export failed");
                state.jobs.set_status(
                    job_id,
                    JobStatus::Failed {
                        error: error.clone(),
                    },
                );
            }
            // Cancellation is reported as a failure with kind `cancelled`.
            emit(&app, events::JOB_FAILED, JobFailed { job_id, error });
        }
    }
}

fn emit<T: Serialize + Clone>(app: &AppHandle, event: &str, payload: T) {
    if let Err(e) = app.emit(event, payload) {
        tracing::warn!(event, error = %e, "could not send event");
    }
}

/// Checks the destination and normalizes it to an `.mp4` path. An existing
/// file is only replaced when `overwrite` is set, and a source file is never
/// a valid destination.
fn resolve_output(path: &Path, project: &Project, overwrite: bool) -> AppResult<PathBuf> {
    if !path.is_absolute() {
        return Err(AppError::InvalidRequest(
            "export path must be absolute".into(),
        ));
    }
    let mut output = path.to_path_buf();
    let is_mp4 = output
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mp4"));
    if !is_mp4 {
        output.as_mut_os_string().push(".mp4");
    }

    let parent = output
        .parent()
        .filter(|p| p.is_dir())
        .ok_or_else(|| AppError::NotFound(format!("folder for {}", output.display())))?;
    let file_name = output.file_name().expect("path ends in a file name");
    let canonical = parent.canonicalize()?.join(file_name);
    let is_source = project
        .assets
        .iter()
        .any(|a| a.path == output || a.path.canonicalize().is_ok_and(|p| p == canonical));
    if is_source {
        return Err(AppError::InvalidRequest(
            "the export would overwrite a source file".into(),
        ));
    }

    if output.is_dir() {
        return Err(AppError::InvalidRequest(format!(
            "{} is a folder",
            output.display()
        )));
    }
    if output.exists() && !overwrite {
        return Err(AppError::AlreadyExists(output.display().to_string()));
    }
    Ok(output)
}

/// Fails if any file used by an enabled clip is missing.
fn check_sources(project: &Project) -> AppResult<()> {
    let missing: Vec<_> = project
        .assets
        .iter()
        .filter(|a| {
            project
                .tracks
                .iter()
                .flat_map(|t| &t.clips)
                .any(|c| c.enabled && c.asset_id == a.id)
        })
        .filter(|a| !a.path.is_file())
        .map(|a| a.name.as_str())
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(AppError::NotFound(format!(
            "source media missing: {}",
            missing.join(", ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::{AssetKind, Clip, MediaAsset, ProjectSettings};

    fn project_with_asset(path: &Path) -> Project {
        let mut project = Project::new("Export", ProjectSettings::default());
        let asset = MediaAsset {
            id: Uuid::new_v4(),
            kind: AssetKind::Video,
            path: path.to_path_buf(),
            name: "source.mp4".into(),
            duration_us: Some(2_000_000),
            width: Some(1920),
            height: Some(1080),
            frame_rate: None,
            has_audio: false,
        };
        project.tracks[0].clips.push(Clip {
            id: Uuid::new_v4(),
            asset_id: asset.id,
            timeline_start_us: 0,
            source_in_us: 0,
            source_out_us: 1_000_000,
            enabled: true,
        });
        project.assets.push(asset);
        project
    }

    #[test]
    fn adds_mp4_extension() {
        let dir = tempfile::tempdir().unwrap();
        let project = Project::new("Export", ProjectSettings::default());
        let out = resolve_output(&dir.path().join("movie"), &project, false).unwrap();
        assert_eq!(out, dir.path().join("movie.mp4"));
        let out = resolve_output(&dir.path().join("movie.MP4"), &project, false).unwrap();
        assert_eq!(out, dir.path().join("movie.MP4"));
    }

    #[test]
    fn existing_file_needs_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("out.mp4");
        std::fs::write(&target, b"old").unwrap();
        let project = Project::new("Export", ProjectSettings::default());
        assert!(matches!(
            resolve_output(&target, &project, false),
            Err(AppError::AlreadyExists(_))
        ));
        assert_eq!(resolve_output(&target, &project, true).unwrap(), target);
    }

    #[test]
    fn rejects_source_file_relative_and_missing_folder() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        std::fs::write(&source, b"media").unwrap();
        let project = project_with_asset(&source);
        assert!(matches!(
            resolve_output(&source, &project, true),
            Err(AppError::InvalidRequest(_))
        ));
        assert!(resolve_output(Path::new("out.mp4"), &project, true).is_err());
        assert!(matches!(
            resolve_output(&dir.path().join("nope/out.mp4"), &project, true),
            Err(AppError::NotFound(_))
        ));
    }

    #[test]
    fn missing_sources_block_export() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        let mut project = project_with_asset(&source);
        assert!(matches!(
            check_sources(&project),
            Err(AppError::NotFound(_))
        ));

        std::fs::write(&source, b"media").unwrap();
        assert!(check_sources(&project).is_ok());

        // Missing files only matter when an enabled clip uses them.
        std::fs::remove_file(&source).unwrap();
        project.tracks[0].clips[0].enabled = false;
        assert!(check_sources(&project).is_ok());
    }
}
