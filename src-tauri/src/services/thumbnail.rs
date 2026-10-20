use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use uuid::Uuid;

use crate::domain::errors::{AppError, AppResult};
use crate::domain::project::AssetKind;
use crate::domain::time::format_us_as_seconds;
use crate::services::tools::run_tool;

pub const THUMBNAIL_WIDTH: u32 = 320;

/// Video thumbnails are taken at 1s, or halfway through shorter clips.
const PREFERRED_SEEK_US: u64 = 1_000_000;

/// Deterministic cache path for a source file. The key includes the file's
/// size and modification time, so editing the source produces a new thumbnail.
pub fn thumbnail_path(cache_dir: &Path, source: &Path) -> AppResult<PathBuf> {
    let meta = fs::metadata(source)?;
    let mtime = meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let key = format!(
        "{}|{}|{}|w{}",
        source.display(),
        meta.len(),
        mtime,
        THUMBNAIL_WIDTH
    );
    let id = Uuid::new_v5(&Uuid::NAMESPACE_OID, key.as_bytes());
    Ok(cache_dir.join(format!("{}.jpg", id.simple())))
}

/// Returns the cached thumbnail for `source`, generating it with FFmpeg if needed.
pub fn generate(
    cache_dir: &Path,
    source: &Path,
    kind: AssetKind,
    duration_us: Option<u64>,
) -> AppResult<PathBuf> {
    if kind == AssetKind::Audio {
        return Err(AppError::UnsupportedMedia(
            "audio assets have no thumbnail".into(),
        ));
    }

    let out = thumbnail_path(cache_dir, source)?;
    if out.is_file() {
        return Ok(out);
    }
    fs::create_dir_all(cache_dir)?;

    let seek_us = match (kind, duration_us) {
        (AssetKind::Video, Some(d)) => PREFERRED_SEEK_US.min(d / 2),
        _ => 0,
    };
    let tmp = cache_dir.join(format!(".{}.tmp.jpg", Uuid::new_v4().simple()));
    let scale = format!("scale={THUMBNAIL_WIDTH}:-2");
    let seek = format_us_as_seconds(seek_us);

    let mut args: Vec<&OsStr> = ["-hide_banner", "-v", "error"].map(OsStr::new).into();
    if seek_us > 0 {
        args.extend([OsStr::new("-ss"), OsStr::new(&seek)]);
    }
    args.push(OsStr::new("-i"));
    args.push(source.as_os_str());
    args.extend(["-frames:v", "1", "-vf", &scale, "-q:v", "4", "-y"].map(OsStr::new));
    args.push(tmp.as_os_str());

    let result = run_tool("ffmpeg", args).and_then(|_| {
        if !tmp.is_file() {
            return Err(AppError::UnsupportedMedia(
                "FFmpeg produced no frame".into(),
            ));
        }
        fs::rename(&tmp, &out).map_err(AppError::from)
    });

    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map(|_| out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::media_probe;
    use crate::services::tools::tool_version;

    fn ffmpeg_available() -> bool {
        tool_version("ffmpeg").is_ok() && tool_version("ffprobe").is_ok()
    }

    #[test]
    fn cache_path_is_stable_and_changes_with_content() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("a.bin");
        fs::write(&source, b"one").unwrap();
        let first = thumbnail_path(dir.path(), &source).unwrap();
        assert_eq!(first, thumbnail_path(dir.path(), &source).unwrap());

        fs::write(&source, b"longer content").unwrap();
        assert_ne!(first, thumbnail_path(dir.path(), &source).unwrap());
    }

    #[test]
    fn audio_has_no_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let result = generate(
            dir.path(),
            &dir.path().join("x.wav"),
            AssetKind::Audio,
            None,
        );
        assert!(matches!(result, Err(AppError::UnsupportedMedia(_))));
    }

    /// End-to-end: synthesize a short clip with FFmpeg, probe it, and thumbnail it.
    #[test]
    fn probes_and_thumbnails_generated_video() {
        if !ffmpeg_available() {
            eprintln!("skipping: ffmpeg/ffprobe not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("sample.mp4");
        run_tool(
            "ffmpeg",
            [
                "-v".as_ref(),
                "error".as_ref(),
                "-f".as_ref(),
                "lavfi".as_ref(),
                "-i".as_ref(),
                "testsrc=duration=2:size=320x240:rate=25".as_ref(),
                "-f".as_ref(),
                "lavfi".as_ref(),
                "-i".as_ref(),
                "sine=duration=2".as_ref(),
                "-shortest".as_ref(),
                "-pix_fmt".as_ref(),
                "yuv420p".as_ref(),
                video.as_os_str(),
            ],
        )
        .unwrap();

        let media = media_probe::probe(&video).unwrap();
        assert_eq!(media.kind, AssetKind::Video);
        assert_eq!((media.width, media.height), (Some(320), Some(240)));
        assert!(media.has_audio);
        let duration = media.duration_us.unwrap();
        assert!((1_900_000..=2_100_000).contains(&duration), "{duration}");

        let cache = dir.path().join("thumbs");
        let thumb = generate(&cache, &video, media.kind, media.duration_us).unwrap();
        assert!(thumb.is_file());
        assert_eq!(
            generate(&cache, &video, media.kind, media.duration_us).unwrap(),
            thumb
        );
    }
}
