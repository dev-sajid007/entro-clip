use std::path::{Path, PathBuf};

use crate::domain::errors::{AppError, AppResult};
use crate::domain::project::AssetKind;
use crate::services::frames;

pub const THUMBNAIL_WIDTH: u32 = 320;

/// Video thumbnails are taken at 1s, or halfway through shorter clips.
const PREFERRED_SEEK_US: u64 = 1_000_000;

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

    let out = frames::cache_file(cache_dir, source, &format!("w{THUMBNAIL_WIDTH}"), "jpg")?;
    if out.is_file() {
        return Ok(out);
    }

    let seek_us = match (kind, duration_us) {
        (AssetKind::Video, Some(d)) => PREFERRED_SEEK_US.min(d / 2),
        _ => 0,
    };
    frames::extract_frame(
        source,
        seek_us,
        &format!("scale={THUMBNAIL_WIDTH}:-2"),
        &out,
    )?;
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::services::media_probe;
    use crate::services::tools::{run_tool, tool_version};
    use std::ffi::OsStr;

    pub fn ffmpeg_available() -> bool {
        tool_version("ffmpeg").is_ok() && tool_version("ffprobe").is_ok()
    }

    /// Synthesizes a short test clip (color bars + tone) with FFmpeg.
    pub fn make_test_video(path: &Path, seconds: u32) {
        let video = format!("testsrc=duration={seconds}:size=320x240:rate=25");
        let audio = format!("sine=duration={seconds}");
        let mut args: Vec<&OsStr> = [
            "-v", "error", "-f", "lavfi", "-i", &video, "-f", "lavfi", "-i", &audio,
        ]
        .map(OsStr::new)
        .into();
        args.extend(["-shortest", "-pix_fmt", "yuv420p", "-y"].map(OsStr::new));
        args.push(path.as_os_str());
        run_tool("ffmpeg", args).unwrap();
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
        make_test_video(&video, 2);

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
