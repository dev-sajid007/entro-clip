use std::path::{Path, PathBuf};

use crate::domain::errors::AppResult;
use crate::domain::project::ProjectSettings;
use crate::services::frames;

/// Preview frames are rendered at most this wide; export uses full size.
pub const PREVIEW_MAX_WIDTH: u32 = 960;

/// Frame cache entries kept on disk before the oldest are pruned.
pub const PREVIEW_CACHE_FILES: usize = 600;

/// Preview canvas size: project aspect ratio, capped width, even dimensions.
pub fn preview_size(settings: &ProjectSettings) -> (u32, u32) {
    let width = settings.width.min(PREVIEW_MAX_WIDTH);
    let height = u64::from(settings.height) * u64::from(width) / u64::from(settings.width);
    let even = |v: u64| (v.max(2) as u32) & !1;
    (even(u64::from(width)), even(height))
}

/// Filter that fits a source into the canvas and letterboxes the rest,
/// matching how export places clips.
pub fn fit_filter(width: u32, height: u32) -> String {
    format!(
        "scale={width}:{height}:force_original_aspect_ratio=decrease,\
         pad={width}:{height}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1"
    )
}

/// Returns a cached preview frame of `source` at `source_time_us`.
pub fn render_frame(
    cache_dir: &Path,
    source: &Path,
    source_time_us: u64,
    size: (u32, u32),
) -> AppResult<PathBuf> {
    let (w, h) = size;
    let params = format!("preview|{w}x{h}|t{source_time_us}");
    let out = frames::cache_file(cache_dir, source, &params, "jpg")?;
    if out.is_file() {
        return Ok(out);
    }
    frames::extract_frame(source, source_time_us, &fit_filter(w, h), &out)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::time::Rational;
    use crate::services::thumbnail::tests::{ffmpeg_available, make_test_video};

    fn settings(width: u32, height: u32) -> ProjectSettings {
        ProjectSettings {
            width,
            height,
            frame_rate: Rational::new(30, 1),
            audio_sample_rate: 48_000,
        }
    }

    #[test]
    fn preview_size_keeps_aspect_and_is_even() {
        assert_eq!(preview_size(&settings(1920, 1080)), (960, 540));
        assert_eq!(preview_size(&settings(1080, 1920)), (960, 1706));
        assert_eq!(preview_size(&settings(640, 480)), (640, 480));
        assert_eq!(preview_size(&settings(1000, 333)), (960, 318));
    }

    #[test]
    fn renders_and_caches_frames() {
        if !ffmpeg_available() {
            eprintln!("skipping: ffmpeg/ffprobe not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("sample.mp4");
        make_test_video(&video, 2);
        let cache = dir.path().join("frames");

        let a = render_frame(&cache, &video, 500_000, (320, 180)).unwrap();
        let b = render_frame(&cache, &video, 1_500_000, (320, 180)).unwrap();
        assert!(a.is_file() && b.is_file());
        assert_ne!(a, b);
        assert_eq!(
            render_frame(&cache, &video, 500_000, (320, 180)).unwrap(),
            a
        );
    }
}
