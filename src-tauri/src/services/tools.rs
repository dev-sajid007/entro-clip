use std::io::ErrorKind;
use std::process::Command;

use serde::Serialize;

use crate::domain::errors::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub name: String,
    pub available: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

/// Runs `<tool> -version` and returns the first line of its output.
pub fn tool_version(tool: &str) -> AppResult<String> {
    let output = Command::new(tool).arg("-version").output().map_err(|e| {
        if e.kind() == ErrorKind::NotFound {
            AppError::ToolNotFound(tool.to_string())
        } else {
            AppError::Io(e)
        }
    })?;

    if !output.status.success() {
        return Err(AppError::ToolFailed {
            tool: tool.to_string(),
            message: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(first_line(&String::from_utf8_lossy(&output.stdout)))
}

pub fn check_tool(tool: &str) -> ToolStatus {
    match tool_version(tool) {
        Ok(version) => ToolStatus {
            name: tool.to_string(),
            available: true,
            version: Some(version),
            error: None,
        },
        Err(err) => {
            tracing::warn!(tool, error = %err, "media tool check failed");
            ToolStatus {
                name: tool.to_string(),
                available: false,
                version: None,
                error: Some(err.to_string()),
            }
        }
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_line_takes_only_first_line() {
        assert_eq!(
            first_line("ffmpeg version 8.1\nbuilt with gcc\n"),
            "ffmpeg version 8.1"
        );
        assert_eq!(first_line(""), "");
    }

    #[test]
    fn missing_tool_reports_not_found() {
        let status = check_tool("entrocut-definitely-missing-tool");
        assert!(!status.available);
        assert!(status.error.unwrap().contains("not found"));
    }
}
