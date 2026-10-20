use serde::Serialize;

/// Errors returned across the IPC boundary. Serialized as `{ kind, message }`
/// so the frontend can branch on `kind` without parsing message text.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("required tool not found: {0}")]
    ToolNotFound(String),
    #[error("failed to run {tool}: {message}")]
    ToolFailed { tool: String, message: String },
    #[error("invalid project: {}", .0.join("; "))]
    InvalidProject(Vec<String>),
    #[error("project schema version {0} is not supported by this version of EntroCut")]
    UnsupportedSchema(u64),
    #[error("{0}")]
    InvalidEdit(String),
    #[error("unsupported media: {0}")]
    UnsupportedMedia(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("cancelled")]
    Cancelled,
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl AppError {
    fn kind(&self) -> &'static str {
        match self {
            AppError::ToolNotFound(_) => "toolNotFound",
            AppError::ToolFailed { .. } => "toolFailed",
            AppError::InvalidProject(_) => "invalidProject",
            AppError::UnsupportedSchema(_) => "unsupportedSchema",
            AppError::InvalidEdit(_) => "invalidEdit",
            AppError::UnsupportedMedia(_) => "unsupportedMedia",
            AppError::NotFound(_) => "notFound",
            AppError::InvalidRequest(_) => "invalidRequest",
            AppError::Cancelled => "cancelled",
            AppError::Json(_) => "json",
            AppError::Io(_) => "io",
        }
    }
}

impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

pub type AppResult<T> = Result<T, AppError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_kind_and_message() {
        let err = AppError::ToolNotFound("ffmpeg".into());
        let json = serde_json::to_value(&err).unwrap();
        assert_eq!(json["kind"], "toolNotFound");
        assert_eq!(json["message"], "required tool not found: ffmpeg");
    }

    #[test]
    fn joins_project_problems() {
        let err = AppError::InvalidProject(vec!["a".into(), "b".into()]);
        assert_eq!(err.to_string(), "invalid project: a; b");
    }
}
