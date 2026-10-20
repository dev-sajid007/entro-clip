use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use uuid::Uuid;

use crate::domain::errors::{AppError, AppResult, ErrorInfo};
use crate::services::render::RenderControl;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum JobStatus {
    Running { progress: f64 },
    Completed { output_path: String },
    Failed { error: ErrorInfo },
    Cancelled,
}

impl JobStatus {
    fn is_running(&self) -> bool {
        matches!(self, JobStatus::Running { .. })
    }
}

struct Job {
    status: JobStatus,
    control: Arc<RenderControl>,
}

/// Tracks background jobs by id. Only one export runs at a time so FFmpeg
/// never competes with itself for CPU and memory.
#[derive(Default)]
pub struct JobManager {
    jobs: Mutex<HashMap<Uuid, Job>>,
}

impl JobManager {
    fn jobs(&self) -> MutexGuard<'_, HashMap<Uuid, Job>> {
        self.jobs.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Registers a new running job, or fails if one is already running.
    pub fn start(&self) -> AppResult<(Uuid, Arc<RenderControl>)> {
        let mut jobs = self.jobs();
        if jobs.values().any(|j| j.status.is_running()) {
            return Err(AppError::InvalidRequest(
                "an export is already running".into(),
            ));
        }
        let id = Uuid::new_v4();
        let control = Arc::new(RenderControl::default());
        jobs.insert(
            id,
            Job {
                status: JobStatus::Running { progress: 0.0 },
                control: control.clone(),
            },
        );
        Ok((id, control))
    }

    pub fn set_status(&self, id: Uuid, status: JobStatus) {
        if let Some(job) = self.jobs().get_mut(&id) {
            job.status = status;
        }
    }

    pub fn status(&self, id: Uuid) -> AppResult<JobStatus> {
        self.jobs()
            .get(&id)
            .map(|j| j.status.clone())
            .ok_or_else(|| AppError::NotFound(format!("job {id}")))
    }

    pub fn cancel(&self, id: Uuid) -> AppResult<()> {
        let control = {
            let jobs = self.jobs();
            let job = jobs
                .get(&id)
                .ok_or_else(|| AppError::NotFound(format!("job {id}")))?;
            if !job.status.is_running() {
                return Ok(());
            }
            job.control.clone()
        };
        control.cancel();
        Ok(())
    }

    /// Cancels every running job, e.g. when the app is closing.
    pub fn cancel_all(&self) {
        let controls: Vec<_> = self
            .jobs()
            .values()
            .filter(|j| j.status.is_running())
            .map(|j| j.control.clone())
            .collect();
        for control in controls {
            control.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_one_running_job() {
        let jobs = JobManager::default();
        let (id, _) = jobs.start().unwrap();
        assert!(jobs.start().is_err());
        jobs.set_status(id, JobStatus::Cancelled);
        assert!(jobs.start().is_ok());
    }

    #[test]
    fn cancel_signals_control_and_reports_status() {
        let jobs = JobManager::default();
        let (id, control) = jobs.start().unwrap();
        jobs.cancel(id).unwrap();
        assert!(control.is_cancelled());
        assert!(matches!(
            jobs.status(id).unwrap(),
            JobStatus::Running { .. }
        ));
        assert!(jobs.cancel(Uuid::new_v4()).is_err());
    }
}
