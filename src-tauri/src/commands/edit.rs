use serde::Serialize;
use tauri::State;
use uuid::Uuid;

use crate::domain::edit::{self, Edit};
use crate::domain::errors::AppResult;
use crate::state::{AppState, ProjectState};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditResult {
    pub state: ProjectState,
    pub created_clip_id: Option<Uuid>,
}

/// Applies one validated timeline edit. Invalid edits leave the project unchanged.
#[tauri::command]
pub fn apply_edit(state: State<'_, AppState>, edit: Edit) -> AppResult<EditResult> {
    let mut session = state.session();
    let outcome = edit::apply(&session.project, &edit).inspect_err(|e| {
        tracing::debug!(?edit, error = %e, "edit rejected");
    })?;
    session.commit(outcome.project);
    Ok(EditResult {
        state: session.snapshot(),
        created_clip_id: outcome.created_clip_id,
    })
}

#[tauri::command]
pub fn undo_edit(state: State<'_, AppState>) -> ProjectState {
    let mut session = state.session();
    session.undo();
    session.snapshot()
}

#[tauri::command]
pub fn redo_edit(state: State<'_, AppState>) -> ProjectState {
    let mut session = state.session();
    session.redo();
    session.snapshot()
}
