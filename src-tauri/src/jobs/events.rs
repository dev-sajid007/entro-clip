//! Event names and payloads published to the frontend. Every payload
//! carries the job id so overlapping jobs can't be confused.

use serde::Serialize;
use uuid::Uuid;

use crate::domain::errors::ErrorInfo;

pub const EXPORT_PROGRESS: &str = "export-progress";
pub const EXPORT_COMPLETED: &str = "export-completed";
pub const JOB_FAILED: &str = "job-failed";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub job_id: Uuid,
    /// 0.0 to 1.0
    pub progress: f64,
    pub out_time_us: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCompleted {
    pub job_id: Uuid,
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobFailed {
    pub job_id: Uuid,
    pub error: ErrorInfo,
}
