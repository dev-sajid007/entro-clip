//! Shared helpers for extracting single frames with FFmpeg into a cache.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use uuid::Uuid;

use crate::domain::errors::{AppError, AppResult};
use crate::domain::time::format_us_as_seconds;
use crate::services::tools::run_tool;

/// Deterministic cache path for a file derived from `source`. The key
/// includes the source's size and modification time plus `params`, so
/// editing the source or changing parameters produces a new entry.
pub fn cache_file(cache_dir: &Path, source: &Path, params: &str, ext: &str) -> AppResult<PathBuf> {
    let meta = fs::metadata(source)?;
    let mtime = meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let key = format!("{}|{}|{}|{}", source.display(), meta.len(), mtime, params);
    let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, key.as_bytes());
    Ok(cache_dir.join(format!("{}.{ext}", id.simple())))
}

/// Writes one JPEG frame of `source` at `seek_us` to `out`, filtered by
/// `filter`. Writes to a temp file first so `out` is never partial.
pub fn extract_frame(source: &Path, seek_us: u64, filter: &str, out: &Path) -> AppResult<()> {
    let dir = out
        .parent()
        .ok_or_else(|| AppError::InvalidRequest("frame path has no parent".into()))?;
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!(".{}.tmp.jpg", Uuid::new_v4().simple()));
    let seek = format_us_as_seconds(seek_us);

    let mut args: Vec<&OsStr> = ["-hide_banner", "-v", "error"].map(OsStr::new).into();
    if seek_us > 0 {
        args.extend([OsStr::new("-ss"), OsStr::new(&seek)]);
    }
    args.push(OsStr::new("-i"));
    args.push(source.as_os_str());
    args.extend(["-frames:v", "1", "-vf", filter, "-q:v", "4", "-y"].map(OsStr::new));
    args.push(tmp.as_os_str());

    let result = run_tool("ffmpeg", args).and_then(|_| {
        if !tmp.is_file() {
            return Err(AppError::UnsupportedMedia(
                "FFmpeg produced no frame".into(),
            ));
        }
        fs::rename(&tmp, out).map_err(AppError::from)
    });
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Deletes the oldest files in `dir` until at most `keep` remain.
pub fn prune(dir: &Path, keep: usize) -> AppResult<usize> {
    let mut files: Vec<_> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            meta.is_file().then(|| (meta.modified().ok(), e.path()))
        })
        .collect();
    if files.len() <= keep {
        return Ok(0);
    }
    files.sort_by_key(|(modified, _)| *modified);
    let excess = files.len() - keep;
    for (_, path) in files.iter().take(excess) {
        let _ = fs::remove_file(path);
    }
    Ok(excess)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_file_depends_on_content_and_params() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("a.bin");
        fs::write(&source, b"one").unwrap();
        let first = cache_file(dir.path(), &source, "w320", "jpg").unwrap();
        assert_eq!(
            first,
            cache_file(dir.path(), &source, "w320", "jpg").unwrap()
        );
        assert_ne!(
            first,
            cache_file(dir.path(), &source, "w640", "jpg").unwrap()
        );

        fs::write(&source, b"longer content").unwrap();
        assert_ne!(
            first,
            cache_file(dir.path(), &source, "w320", "jpg").unwrap()
        );
    }

    #[test]
    fn prune_keeps_newest_files() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..5 {
            fs::write(dir.path().join(format!("{i}.jpg")), b"x").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(prune(dir.path(), 2).unwrap(), 3);
        let mut left: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        left.sort();
        assert_eq!(left, ["3.jpg", "4.jpg"]);
    }
}
