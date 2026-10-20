use std::collections::{HashSet, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, ErrorKind};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread;

use crate::domain::errors::{AppError, AppResult};
use crate::domain::project::AssetKind;
use crate::domain::render_plan::{RenderPlan, VideoEncoder};
use crate::domain::time::parse_seconds_to_us;
use crate::services::media_probe;
use crate::services::tools::run_tool;

const STDERR_TAIL_LINES: usize = 20;

/// Shared between a running render and whoever may cancel it.
#[derive(Default)]
pub struct RenderControl {
    cancelled: AtomicBool,
    child: Mutex<Option<Child>>,
}

impl RenderControl {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(child) = self
            .child
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_mut()
        {
            let _ = child.kill();
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

static H264_ENCODER: OnceLock<VideoEncoder> = OnceLock::new();
static DRAWTEXT: OnceLock<()> = OnceLock::new();

/// Fails unless the installed FFmpeg has the `drawtext` filter, which text
/// clips need. Builds without libfreetype lack it.
pub fn require_drawtext() -> AppResult<()> {
    if DRAWTEXT.get().is_some() {
        return Ok(());
    }
    let output = run_tool("ffmpeg", ["-hide_banner", "-filters"])?;
    let listing = String::from_utf8_lossy(&output.stdout);
    if !listing
        .lines()
        .any(|line| line.split_whitespace().nth(1) == Some("drawtext"))
    {
        return Err(AppError::ToolFailed {
            tool: "ffmpeg".into(),
            message: "this FFmpeg build can't draw text (no drawtext filter); \
                      install a build with libfreetype to export text clips"
                .into(),
        });
    }
    DRAWTEXT.get_or_init(|| ());
    Ok(())
}

/// Finds an H.264 encoder in the installed FFmpeg. Only a successful lookup
/// is cached, so installing a better FFmpeg build takes effect on retry.
pub fn h264_encoder() -> AppResult<VideoEncoder> {
    if let Some(encoder) = H264_ENCODER.get() {
        return Ok(*encoder);
    }
    let output = run_tool("ffmpeg", ["-hide_banner", "-encoders"])?;
    let encoder = pick_encoder(&String::from_utf8_lossy(&output.stdout)).ok_or_else(|| {
        AppError::ToolFailed {
            tool: "ffmpeg".into(),
            message: "no H.264 encoder found; install an FFmpeg build with libx264 or libopenh264"
                .into(),
        }
    })?;
    tracing::info!(encoder = encoder.ffmpeg_name(), "selected H.264 encoder");
    Ok(*H264_ENCODER.get_or_init(|| encoder))
}

/// Picks the preferred encoder from `ffmpeg -encoders` output.
pub fn pick_encoder(listing: &str) -> Option<VideoEncoder> {
    let names: HashSet<&str> = listing
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect();
    VideoEncoder::ALL
        .into_iter()
        .find(|e| names.contains(e.ffmpeg_name()))
}

/// One line of FFmpeg `-progress` output that matters to us.
#[derive(Debug, PartialEq)]
pub enum ProgressLine {
    OutTime(u64),
    End,
}

pub fn parse_progress_line(line: &str) -> Option<ProgressLine> {
    let (key, value) = line.trim().split_once('=')?;
    match key {
        "out_time_us" => value.parse().ok().map(ProgressLine::OutTime),
        // Some builds only report HH:MM:SS.micro
        "out_time" => parse_clock(value).map(ProgressLine::OutTime),
        "progress" if value == "end" => Some(ProgressLine::End),
        _ => None,
    }
}

fn parse_clock(text: &str) -> Option<u64> {
    let mut parts = text.split(':');
    let (h, m, s) = (parts.next()?, parts.next()?, parts.next()?);
    let h: u64 = h.parse().ok()?;
    let m: u64 = m.parse().ok()?;
    Some((h * 3600 + m * 60) * 1_000_000 + parse_seconds_to_us(s)?)
}

/// Temporary output next to the destination, so the final rename is atomic.
pub fn partial_path(output: &Path) -> PathBuf {
    let name = output
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "export".into());
    output.with_file_name(format!(".{name}.partial.mp4"))
}

/// Renders `plan` to `output`. The file only appears at `output` after
/// FFmpeg succeeds and the result probes as a playable video of the right
/// length; on failure or cancellation the partial file is removed.
pub fn run(
    plan: &RenderPlan,
    output: &Path,
    control: &RenderControl,
    mut on_progress: impl FnMut(u64),
) -> AppResult<()> {
    let tmp = partial_path(output);
    let result = render_to(plan, &tmp, control, &mut on_progress)
        .and_then(|_| verify(&tmp, plan.duration_us))
        .and_then(|_| fs::rename(&tmp, output).map_err(AppError::from));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn render_to(
    plan: &RenderPlan,
    tmp: &Path,
    control: &RenderControl,
    on_progress: &mut impl FnMut(u64),
) -> AppResult<()> {
    if control.is_cancelled() {
        return Err(AppError::Cancelled);
    }
    let mut child = Command::new("ffmpeg")
        .args(plan.ffmpeg_args(tmp))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| match e.kind() {
            ErrorKind::NotFound => AppError::ToolNotFound("ffmpeg".into()),
            _ => AppError::Io(e),
        })?;

    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    *control.child.lock().unwrap_or_else(|e| e.into_inner()) = Some(child);
    // A cancel that raced with spawning saw no child; apply it now.
    if control.is_cancelled() {
        control.cancel();
    }

    let stderr_tail = thread::spawn(move || {
        let mut tail = VecDeque::with_capacity(STDERR_TAIL_LINES);
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tail.len() == STDERR_TAIL_LINES {
                tail.pop_front();
            }
            tail.push_back(line);
        }
        Vec::from(tail).join("\n")
    });

    // Reading until EOF never holds the child lock, so cancel can kill it.
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        match parse_progress_line(&line) {
            Some(ProgressLine::OutTime(us)) => on_progress(us.min(plan.duration_us)),
            Some(ProgressLine::End) => on_progress(plan.duration_us),
            None => {}
        }
    }

    let status = control
        .child
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .expect("child stored above")
        .wait()?;
    let stderr = stderr_tail.join().unwrap_or_default();

    if control.is_cancelled() {
        return Err(AppError::Cancelled);
    }
    if !status.success() {
        let message = if stderr.trim().is_empty() {
            format!("exited with {status}")
        } else {
            stderr
        };
        return Err(AppError::ToolFailed {
            tool: "ffmpeg".into(),
            message,
        });
    }
    Ok(())
}

/// Confirms the rendered file is a readable video close to the expected length.
fn verify(path: &Path, expected_us: u64) -> AppResult<()> {
    let media = media_probe::probe(path)?;
    if media.kind != AssetKind::Video {
        return Err(AppError::UnsupportedMedia(
            "export has no video stream".into(),
        ));
    }
    let actual = media
        .duration_us
        .ok_or_else(|| AppError::UnsupportedMedia("export has no duration".into()))?;
    let tolerance = (expected_us / 50).max(500_000);
    if actual.abs_diff(expected_us) > tolerance {
        return Err(AppError::UnsupportedMedia(format!(
            "export is {actual} µs long, expected about {expected_us} µs"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::{
        Clip, ClipAudio, MediaAsset, Project, ProjectSettings, TextOverlay, Track, TrackKind,
    };
    use crate::domain::render_plan::{Quality, build, resolve_settings};
    use crate::domain::time::Rational;
    use crate::services::thumbnail::tests::{ffmpeg_available, make_test_video};
    use std::ffi::OsStr;
    use uuid::Uuid;

    const SEC: u64 = 1_000_000;

    #[test]
    fn parses_progress_lines() {
        assert_eq!(
            parse_progress_line("out_time_us=1500000"),
            Some(ProgressLine::OutTime(1_500_000))
        );
        assert_eq!(
            parse_progress_line("out_time=00:01:02.500000"),
            Some(ProgressLine::OutTime(62_500_000))
        );
        assert_eq!(parse_progress_line("progress=end"), Some(ProgressLine::End));
        assert_eq!(parse_progress_line("progress=continue"), None);
        assert_eq!(parse_progress_line("out_time_us=N/A"), None);
        assert_eq!(parse_progress_line("frame=12"), None);
    }

    #[test]
    fn prefers_x264_and_falls_back_to_openh264() {
        let both = " V....D libopenh264          OpenH264\n V....D libx264              x264\n";
        assert_eq!(pick_encoder(both), Some(VideoEncoder::X264));
        let fedora = " V....D libopenh264          OpenH264\n V....D h264_vaapi           VAAPI\n";
        assert_eq!(pick_encoder(fedora), Some(VideoEncoder::OpenH264));
        assert_eq!(pick_encoder(" V....D h264_nvenc   NVENC\n"), None);
    }

    #[test]
    fn partial_path_is_hidden_sibling() {
        assert_eq!(
            partial_path(Path::new("/out/final.mp4")),
            PathBuf::from("/out/.final.mp4.partial.mp4")
        );
    }

    fn asset_for(path: &Path, has_audio: bool) -> MediaAsset {
        let media = media_probe::probe(path).unwrap();
        MediaAsset {
            id: Uuid::new_v4(),
            kind: media.kind,
            path: path.to_path_buf(),
            name: "clip".into(),
            duration_us: media.duration_us,
            width: media.width,
            height: media.height,
            frame_rate: media.frame_rate,
            has_audio: has_audio && media.has_audio,
        }
    }

    /// Mean luma of a frame at `seconds`, from a tiny grayscale grab.
    fn brightness_at(video: &Path, seconds: &str) -> f64 {
        let mut args: Vec<&OsStr> = ["-v", "error", "-ss", seconds, "-i"].map(OsStr::new).into();
        args.push(video.as_os_str());
        args.extend(
            [
                "-frames:v",
                "1",
                "-vf",
                "scale=16:16",
                "-pix_fmt",
                "gray",
                "-f",
                "rawvideo",
                "-",
            ]
            .map(OsStr::new),
        );
        let out = run_tool("ffmpeg", args).unwrap().stdout;
        out.iter().map(|&b| f64::from(b)).sum::<f64>() / out.len() as f64
    }

    /// Renders a real timeline: a 1 s gap, then a 2 s clip, with a 1 s overlay.
    #[test]
    fn renders_timeline_with_gap_and_overlay() {
        if !ffmpeg_available() {
            eprintln!("skipping: ffmpeg/ffprobe not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        make_test_video(&source, 4);

        let settings = ProjectSettings {
            width: 320,
            height: 240,
            frame_rate: Rational::new(25, 1),
            audio_sample_rate: 48_000,
        };
        let mut project = Project::new("E2E", settings.clone());
        let asset = asset_for(&source, true);
        let clip = |start, src_in, len| Clip {
            id: Uuid::new_v4(),
            asset_id: Some(asset.id),
            text: None,
            timeline_start_us: start,
            source_in_us: src_in,
            source_out_us: src_in + len,
            enabled: true,
            audio: Default::default(),
        };
        let mut main = clip(SEC, 0, 2 * SEC);
        main.audio = ClipAudio {
            gain_db: -6.0,
            fade_in_us: 500_000,
            fade_out_us: 500_000,
        };
        project.tracks[0].clips.push(main);
        let mut overlay = Track::new("Video 2", TrackKind::Video, 0);
        overlay.clips.push(clip(1_500_000, SEC, SEC));
        project.tracks[0].order = 1;
        project.tracks[1].order = 2;
        project.tracks.push(overlay);
        project.assets.push(asset);
        project.validate().unwrap();

        let export = resolve_settings(
            &settings,
            None,
            None,
            Quality::Small,
            h264_encoder().unwrap(),
        )
        .unwrap();
        let plan = build(&project, export).unwrap();
        let output = dir.path().join("out.mp4");
        let mut last_progress = 0;
        run(&plan, &output, &RenderControl::default(), |us| {
            last_progress = us
        })
        .unwrap();

        assert!(output.is_file());
        assert!(!partial_path(&output).exists());
        assert_eq!(last_progress, 3 * SEC);
        let media = media_probe::probe(&output).unwrap();
        assert_eq!((media.width, media.height), (Some(320), Some(240)));
        assert!(media.has_audio);

        // The gap is black; the clip (colour bars) is bright.
        assert!(brightness_at(&output, "0.5") < 20.0);
        assert!(brightness_at(&output, "2.0") > 60.0);
    }

    /// Text full of filtergraph and drawtext special characters renders, and
    /// only while its clip is on the timeline.
    #[test]
    fn renders_text_with_special_characters() {
        if !ffmpeg_available() || require_drawtext().is_err() {
            eprintln!("skipping: ffmpeg with drawtext not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let settings = ProjectSettings {
            width: 320,
            height: 240,
            frame_rate: Rational::new(25, 1),
            audio_sample_rate: 48_000,
        };
        let mut project = Project::new("Text", settings.clone());
        project.tracks[0].clips.push(Clip {
            id: Uuid::new_v4(),
            asset_id: None,
            text: Some(TextOverlay {
                content: "It's 100%: [a,b]; \\ {x}\nline two".into(),
                size: 0.2,
                ..TextOverlay::default()
            }),
            timeline_start_us: SEC,
            source_in_us: 0,
            source_out_us: SEC,
            enabled: true,
            audio: Default::default(),
        });
        project.validate().unwrap();

        let export = resolve_settings(
            &settings,
            None,
            None,
            Quality::Small,
            h264_encoder().unwrap(),
        )
        .unwrap();
        let plan = build(&project, export).unwrap();
        let output = dir.path().join("text.mp4");
        run(&plan, &output, &RenderControl::default(), |_| {}).unwrap();

        let dark = brightness_at(&output, "0.5");
        let lit = brightness_at(&output, "1.5");
        assert!(lit > dark + 5.0, "text frame {lit} vs black {dark}");
    }

    #[test]
    fn cancelled_render_leaves_no_file() {
        if !ffmpeg_available() {
            eprintln!("skipping: ffmpeg/ffprobe not installed");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.mp4");
        make_test_video(&source, 2);

        let mut project = Project::new("Cancel", ProjectSettings::default());
        let asset = asset_for(&source, true);
        project.tracks[0].clips.push(Clip {
            id: Uuid::new_v4(),
            asset_id: Some(asset.id),
            text: None,
            timeline_start_us: 0,
            source_in_us: 0,
            source_out_us: SEC,
            enabled: true,
            audio: Default::default(),
        });
        project.assets.push(asset);
        let export = resolve_settings(
            &project.settings,
            None,
            None,
            Quality::Small,
            VideoEncoder::X264,
        )
        .unwrap();
        let plan = build(&project, export).unwrap();

        let control = RenderControl::default();
        control.cancel();
        let output = dir.path().join("out.mp4");
        assert!(matches!(
            run(&plan, &output, &control, |_| {}),
            Err(AppError::Cancelled)
        ));
        assert!(!output.exists());
        assert!(!partial_path(&output).exists());
    }
}
