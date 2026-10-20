use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use serde_json::Value;
use uuid::Uuid;

use crate::domain::errors::{AppError, AppResult};
use crate::domain::project::{Project, SCHEMA_VERSION};

pub const PROJECT_EXTENSION: &str = "entrocut";

/// Project files beyond this size are rejected before parsing.
const MAX_PROJECT_BYTES: u64 = 32 * 1024 * 1024;

/// Validates and writes `project` atomically: write a temp file in the same
/// directory, fsync it, then rename it over the destination.
pub fn save(project: &Project, path: &Path) -> AppResult<()> {
    project.validate().map_err(AppError::InvalidProject)?;
    let json = serde_json::to_vec_pretty(project)?;

    let file_name = path
        .file_name()
        .ok_or_else(|| AppError::InvalidRequest("save path has no file name".into()))?;
    let dir = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        _ => Path::new("."),
    };
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        file_name.to_string_lossy(),
        Uuid::new_v4().simple()
    ));

    let result = (|| {
        let mut file = File::create(&tmp)?;
        file.write_all(&json)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();

    if let Err(err) = result {
        let _ = fs::remove_file(&tmp);
        return Err(err.into());
    }
    Ok(())
}

/// Loads, migrates, and validates a project file.
pub fn load(path: &Path) -> AppResult<Project> {
    let size = fs::metadata(path)?.len();
    if size > MAX_PROJECT_BYTES {
        return Err(AppError::InvalidProject(vec![format!(
            "file is too large ({size} bytes)"
        )]));
    }

    let value: Value = serde_json::from_slice(&fs::read(path)?)?;
    let project: Project = serde_json::from_value(migrate(value)?)?;
    project.validate().map_err(AppError::InvalidProject)?;
    Ok(project)
}

/// Upgrades older schema versions to `SCHEMA_VERSION`, one step at a time.
fn migrate(mut value: Value) -> AppResult<Value> {
    loop {
        let version = value
            .get("schemaVersion")
            .and_then(Value::as_u64)
            .ok_or_else(|| AppError::InvalidProject(vec!["missing schemaVersion".into()]))?;

        match version {
            v if v == u64::from(SCHEMA_VERSION) => return Ok(value),
            // v2 added clip audio settings and track gain. Both default to
            // "unchanged" when absent, so only the version moves.
            1 => value["schemaVersion"] = 2.into(),
            v => return Err(AppError::UnsupportedSchema(v)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::ProjectSettings;

    #[test]
    fn round_trips_a_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("edit.entrocut");
        let project = Project::new("Round Trip", ProjectSettings::default());

        save(&project, &path).unwrap();
        assert_eq!(load(&path).unwrap(), project);

        let leftovers: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files left behind");
    }

    #[test]
    fn refuses_to_save_invalid_project() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.entrocut");
        let project = Project::new("", ProjectSettings::default());

        assert!(matches!(
            save(&project, &path),
            Err(AppError::InvalidProject(_))
        ));
        assert!(!path.exists());
    }

    #[test]
    fn rejects_newer_schema() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("future.entrocut");
        let mut json = serde_json::to_value(Project::new("F", ProjectSettings::default())).unwrap();
        json["schemaVersion"] = 99.into();
        fs::write(&path, json.to_string()).unwrap();

        assert!(matches!(load(&path), Err(AppError::UnsupportedSchema(99))));
    }

    #[test]
    fn migrates_v1_projects() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.entrocut");
        let mut json =
            serde_json::to_value(Project::new("Old", ProjectSettings::default())).unwrap();
        json["schemaVersion"] = 1.into();
        for track in json["tracks"].as_array_mut().unwrap() {
            track.as_object_mut().unwrap().remove("gainDb");
        }
        fs::write(&path, json.to_string()).unwrap();

        let project = load(&path).unwrap();
        assert_eq!(project.schema_version, SCHEMA_VERSION);
        assert!(project.tracks.iter().all(|t| t.gain_db == 0.0));
    }

    #[test]
    fn rejects_malformed_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.entrocut");
        fs::write(&path, "{ not json").unwrap();

        assert!(matches!(load(&path), Err(AppError::Json(_))));
    }
}
