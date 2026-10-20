use std::path::PathBuf;

use tauri::State;

use crate::domain::errors::{AppError, AppResult};
use crate::domain::project::{Project, ProjectSettings};
use crate::services::project_store::{self, PROJECT_EXTENSION};
use crate::state::{AppState, ProjectState, Session, UNTITLED};

#[tauri::command]
pub fn get_project(state: State<'_, AppState>) -> ProjectState {
    state.session().snapshot()
}

/// Replaces the open project with a new empty one. The frontend is
/// responsible for confirming before discarding unsaved changes.
#[tauri::command]
pub fn create_project(
    state: State<'_, AppState>,
    name: Option<String>,
    settings: Option<ProjectSettings>,
) -> AppResult<ProjectState> {
    let name = name
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| UNTITLED.to_string());
    let project = Project::new(&name, settings.unwrap_or_default());
    project.validate().map_err(AppError::InvalidProject)?;

    let mut session = state.session();
    *session = Session::new(project, None);
    tracing::info!(name, "created project");
    Ok(session.snapshot())
}

#[tauri::command]
pub async fn open_project(state: State<'_, AppState>, path: String) -> AppResult<ProjectState> {
    let path = PathBuf::from(path);
    let load_path = path.clone();
    let project = tauri::async_runtime::spawn_blocking(move || project_store::load(&load_path))
        .await
        .map_err(|e| AppError::InvalidRequest(e.to_string()))??;

    let mut session = state.session();
    *session = Session::new(project, Some(path.clone()));
    tracing::info!(path = %path.display(), "opened project");
    Ok(session.snapshot())
}

/// Saves to `path`, or to the current project path when `path` is omitted.
#[tauri::command]
pub async fn save_project(
    state: State<'_, AppState>,
    path: Option<String>,
) -> AppResult<ProjectState> {
    let (project, revision, target) = {
        let session = state.session();
        let target = path
            .map(PathBuf::from)
            .or_else(|| session.path.clone())
            .ok_or_else(|| AppError::InvalidRequest("no save location chosen".into()))?;
        (
            session.project.clone(),
            session.revision,
            with_extension(target),
        )
    };

    let save_path = target.clone();
    tauri::async_runtime::spawn_blocking(move || project_store::save(&project, &save_path))
        .await
        .map_err(|e| AppError::InvalidRequest(e.to_string()))??;

    let mut session = state.session();
    session.path = Some(target.clone());
    // Edits made while the file was being written keep the project dirty.
    session.saved_revision = revision;
    tracing::info!(path = %target.display(), "saved project");
    Ok(session.snapshot())
}

fn with_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some_and(|e| e == PROJECT_EXTENSION) {
        path
    } else {
        let mut name = path.into_os_string();
        name.push(".");
        name.push(PROJECT_EXTENSION);
        PathBuf::from(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_project_extension_once() {
        assert_eq!(
            with_extension("/a/edit".into()),
            PathBuf::from("/a/edit.entrocut")
        );
        assert_eq!(
            with_extension("/a/edit.entrocut".into()),
            PathBuf::from("/a/edit.entrocut")
        );
        assert_eq!(
            with_extension("/a/my.edit".into()),
            PathBuf::from("/a/my.edit.entrocut")
        );
    }
}
