//! Typed render plan: the bridge between the project model and FFmpeg.
//! Building the plan and its FFmpeg arguments is pure, so it is tested
//! without running FFmpeg.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::errors::{AppError, AppResult};
use super::project::{AssetKind, Project, ProjectSettings, TrackKind, validate_settings};
use super::time::{Rational, US_PER_SECOND, format_us_as_seconds};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    High,
    Standard,
    Small,
}

impl Quality {
    /// x264 constant rate factor; lower is higher quality.
    pub fn crf(self) -> u8 {
        match self {
            Quality::High => 18,
            Quality::Standard => 21,
            Quality::Small => 26,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportSettings {
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub sample_rate: u32,
    pub quality: Quality,
}

const AUDIO_BITRATE: &str = "192k";

/// Resolves export settings from the project. `short_side` scales the
/// canvas so its shorter edge has that many pixels (720p, 1080p, ...),
/// keeping the project's aspect ratio for both landscape and portrait.
pub fn resolve_settings(
    project: &ProjectSettings,
    short_side: Option<u32>,
    frame_rate: Option<Rational>,
    quality: Quality,
) -> AppResult<ExportSettings> {
    let (width, height) = match short_side {
        None => (project.width, project.height),
        Some(target) => {
            let short = u64::from(project.width.min(project.height));
            let scale = |v: u32| {
                let scaled = (u64::from(v) * u64::from(target) + short / 2) / short;
                ((scaled as u32) + 1) & !1
            };
            (scale(project.width), scale(project.height))
        }
    };
    let settings = ExportSettings {
        width,
        height,
        frame_rate: frame_rate.unwrap_or(project.frame_rate),
        sample_rate: project.audio_sample_rate,
        quality,
    };

    let mut problems = Vec::new();
    validate_settings(
        &ProjectSettings {
            width,
            height,
            frame_rate: settings.frame_rate,
            audio_sample_rate: settings.sample_rate,
        },
        &mut problems,
    );
    if problems.is_empty() {
        Ok(settings)
    } else {
        Err(AppError::InvalidRequest(problems.join("; ")))
    }
}

/// One FFmpeg input: a trimmed range of a source file.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderInput {
    pub path: PathBuf,
    pub image: bool,
    pub seek_us: u64,
    pub duration_us: u64,
}

/// A source placed on the timeline at `start_us`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub input: usize,
    pub start_us: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderPlan {
    pub settings: ExportSettings,
    pub duration_us: u64,
    pub inputs: Vec<RenderInput>,
    /// Video layers from bottom to top.
    pub video_layers: Vec<Placement>,
    pub audio_sources: Vec<Placement>,
}

/// Builds a render plan using the same rules as the preview: visible video
/// tracks are stacked with the first track on top, video clips contribute
/// their own sound, and muted audio tracks are skipped.
pub fn build(project: &Project, settings: ExportSettings) -> AppResult<RenderPlan> {
    let duration_us = project
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .filter(|c| c.enabled)
        .map(|c| c.end_us())
        .max()
        .unwrap_or(0);
    if duration_us == 0 {
        return Err(AppError::InvalidRequest("the timeline is empty".into()));
    }

    let mut plan = RenderPlan {
        settings,
        duration_us,
        inputs: Vec::new(),
        video_layers: Vec::new(),
        audio_sources: Vec::new(),
    };

    let tracks = project.tracks_in_order();
    let video = tracks
        .iter()
        .rev()
        .filter(|t| t.kind == TrackKind::Video && !t.hidden);
    let audio = tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Audio && !t.muted);

    for track in video.chain(audio) {
        for clip in track.clips.iter().filter(|c| c.enabled) {
            let asset = project
                .asset(clip.asset_id)
                .ok_or_else(|| AppError::NotFound(format!("asset {}", clip.asset_id)))?;
            let image = asset.kind == AssetKind::Image;
            let input = plan.inputs.len();
            plan.inputs.push(RenderInput {
                path: asset.path.clone(),
                image,
                seek_us: if image { 0 } else { clip.source_in_us },
                duration_us: clip.duration_us(),
            });
            let placement = Placement {
                input,
                start_us: clip.timeline_start_us,
            };
            if track.kind == TrackKind::Video {
                plan.video_layers.push(placement);
            }
            if asset.has_audio {
                plan.audio_sources.push(placement);
            }
        }
    }
    Ok(plan)
}

fn rate(r: Rational) -> String {
    format!("{}/{}", r.numerator, r.denominator)
}

impl RenderPlan {
    pub fn filter_graph(&self) -> String {
        let ExportSettings {
            width: w,
            height: h,
            frame_rate,
            sample_rate: sr,
            ..
        } = self.settings;
        let fps = rate(frame_rate);
        let dur = format_us_as_seconds(self.duration_us);
        let mut parts = vec![format!("color=c=black:s={w}x{h}:r={fps}:d={dur}[base]")];

        let mut last = "base".to_string();
        for (i, layer) in self.video_layers.iter().enumerate() {
            let start = format_us_as_seconds(layer.start_us);
            parts.push(format!(
                "[{}:v]fps={fps},scale={w}:{h}:force_original_aspect_ratio=decrease,\
                 format=yuva420p,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black@0,setsar=1,\
                 setpts=PTS-STARTPTS+{start}/TB[l{i}]",
                layer.input
            ));
            parts.push(format!(
                "[{last}][l{i}]overlay=eof_action=pass:format=auto[v{i}]"
            ));
            last = format!("v{i}");
        }
        parts.push(format!("[{last}]format=yuv420p[vout]"));

        if self.audio_sources.is_empty() {
            parts.push(format!(
                "anullsrc=r={sr}:cl=stereo,atrim=duration={dur}[aout]"
            ));
        } else {
            let mut labels = String::new();
            for (j, src) in self.audio_sources.iter().enumerate() {
                let delay = u128::from(src.start_us) * u128::from(sr) / u128::from(US_PER_SECOND);
                parts.push(format!(
                    "[{}:a]aresample={sr},aformat=sample_fmts=fltp:channel_layouts=stereo,\
                     asetpts=PTS-STARTPTS,adelay=delays={delay}S:all=1[a{j}]",
                    src.input
                ));
                labels.push_str(&format!("[a{j}]"));
            }
            let n = self.audio_sources.len();
            let mix = if n == 1 {
                labels
            } else {
                format!("{labels}amix=inputs={n}:duration=longest:normalize=0,")
            };
            parts.push(format!(
                "{mix}apad=whole_dur={dur},atrim=duration={dur}[aout]"
            ));
        }
        parts.join(";")
    }

    /// Complete FFmpeg argument list. Paths are separate arguments; nothing
    /// is interpreted by a shell.
    pub fn ffmpeg_args(&self, output: &Path) -> Vec<OsString> {
        let s = &self.settings;
        let fps = rate(s.frame_rate);
        let mut args: Vec<OsString> = [
            "-hide_banner",
            "-nostdin",
            "-v",
            "error",
            "-progress",
            "pipe:1",
            "-nostats",
            "-y",
        ]
        .map(OsString::from)
        .into();

        for input in &self.inputs {
            let dur = format_us_as_seconds(input.duration_us);
            if input.image {
                args.extend(["-loop", "1", "-framerate", &fps, "-t", &dur].map(OsString::from));
            } else {
                let seek = format_us_as_seconds(input.seek_us);
                args.extend(["-ss", &seek, "-t", &dur].map(OsString::from));
            }
            args.push("-i".into());
            args.push(input.path.clone().into_os_string());
        }

        let crf = s.quality.crf().to_string();
        let sr = s.sample_rate.to_string();
        let total = format_us_as_seconds(self.duration_us);
        let graph = self.filter_graph();
        args.extend(
            [
                "-filter_complex",
                &graph,
                "-map",
                "[vout]",
                "-map",
                "[aout]",
                "-c:v",
                "libx264",
                "-preset",
                "medium",
                "-crf",
                &crf,
                "-pix_fmt",
                "yuv420p",
                "-r",
                &fps,
                "-c:a",
                "aac",
                "-b:a",
                AUDIO_BITRATE,
                "-ar",
                &sr,
                "-ac",
                "2",
                "-t",
                &total,
                "-movflags",
                "+faststart",
                "-f",
                "mp4",
            ]
            .map(OsString::from),
        );
        args.push(output.as_os_str().to_owned());
        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::{Clip, MediaAsset, Track};
    use uuid::Uuid;

    const SEC: u64 = 1_000_000;

    fn asset(kind: AssetKind, has_audio: bool, path: &str) -> MediaAsset {
        MediaAsset {
            id: Uuid::new_v4(),
            kind,
            path: PathBuf::from(path),
            name: path.into(),
            duration_us: (kind != AssetKind::Image).then_some(60 * SEC),
            width: Some(1920),
            height: Some(1080),
            frame_rate: None,
            has_audio,
        }
    }

    fn clip(asset: &MediaAsset, start: u64, src_in: u64, len: u64) -> Clip {
        Clip {
            id: Uuid::new_v4(),
            asset_id: asset.id,
            timeline_start_us: start,
            source_in_us: src_in,
            source_out_us: src_in + len,
            enabled: true,
        }
    }

    fn settings() -> ExportSettings {
        resolve_settings(&ProjectSettings::default(), None, None, Quality::Standard).unwrap()
    }

    /// Main video track with a clip, an overlay track with an image, and music.
    fn sample_project() -> Project {
        let mut p = Project::new("P", ProjectSettings::default());
        let video = asset(AssetKind::Video, true, "/m/a b.mp4");
        let image = asset(AssetKind::Image, false, "/m/logo.png");
        let music = asset(AssetKind::Audio, true, "/m/music.mp3");
        let mut overlay = Track::new("Video 2", TrackKind::Video, 0);
        p.tracks[0].order = 1;
        p.tracks[1].order = 2;
        p.tracks[0].clips.push(clip(&video, 0, 2 * SEC, 5 * SEC));
        overlay.clips.push(clip(&image, SEC, 0, 2 * SEC));
        p.tracks[1].clips.push(clip(&music, 500_000, 0, 10 * SEC));
        p.tracks.push(overlay);
        p.assets = vec![video, image, music];
        p.validate().unwrap();
        p
    }

    #[test]
    fn scales_short_side_for_both_orientations() {
        let landscape = ProjectSettings::default();
        let portrait = ProjectSettings {
            width: 1080,
            height: 1920,
            ..ProjectSettings::default()
        };
        let size = |p: &ProjectSettings, s| {
            let e = resolve_settings(p, Some(s), None, Quality::High).unwrap();
            (e.width, e.height)
        };
        assert_eq!(size(&landscape, 720), (1280, 720));
        assert_eq!(size(&landscape, 1080), (1920, 1080));
        assert_eq!(size(&portrait, 720), (720, 1280));
    }

    #[test]
    fn rejects_invalid_export_settings() {
        let err = resolve_settings(
            &ProjectSettings::default(),
            None,
            Some(Rational::new(0, 1)),
            Quality::High,
        );
        assert!(matches!(err, Err(AppError::InvalidRequest(_))));
    }

    #[test]
    fn rejects_empty_timeline() {
        let p = Project::new("P", ProjectSettings::default());
        assert!(matches!(
            build(&p, settings()),
            Err(AppError::InvalidRequest(_))
        ));
    }

    #[test]
    fn stacks_layers_bottom_to_top_and_collects_audio() {
        let plan = build(&sample_project(), settings()).unwrap();
        assert_eq!(plan.duration_us, 10_500_000);
        assert_eq!(plan.inputs.len(), 3);
        // Bottom layer is the main track's video, then the overlay image.
        assert_eq!(
            plan.inputs[plan.video_layers[0].input].path,
            PathBuf::from("/m/a b.mp4")
        );
        assert_eq!(
            plan.inputs[plan.video_layers[1].input].path,
            PathBuf::from("/m/logo.png")
        );
        assert_eq!(plan.inputs[0].seek_us, 2 * SEC);
        assert!(plan.inputs[1].image);
        // Video sound and the music are mixed; the image has no audio.
        assert_eq!(plan.audio_sources.len(), 2);
    }

    #[test]
    fn hidden_and_muted_tracks_are_skipped() {
        let mut p = sample_project();
        for t in &mut p.tracks {
            t.hidden = t.name == "Video 2";
            t.muted = t.kind == TrackKind::Audio;
        }
        let plan = build(&p, settings()).unwrap();
        assert_eq!(plan.video_layers.len(), 1);
        assert_eq!(plan.audio_sources.len(), 1);
    }

    #[test]
    fn filter_graph_places_layers_and_delays_audio() {
        let graph = build(&sample_project(), settings()).unwrap().filter_graph();
        assert!(graph.starts_with("color=c=black:s=1920x1080:r=30/1:d=10.500000[base]"));
        assert!(graph.contains("setpts=PTS-STARTPTS+1.000000/TB[l1]"));
        assert!(graph.contains("[v0][l1]overlay=eof_action=pass"));
        // 0.5 s at 48 kHz = 24000 samples.
        assert!(graph.contains("adelay=delays=24000S:all=1[a1]"));
        assert!(graph.contains("amix=inputs=2:duration=longest:normalize=0"));
        assert!(graph.ends_with("atrim=duration=10.500000[aout]"));
    }

    #[test]
    fn silent_timeline_gets_null_audio() {
        let mut p = sample_project();
        p.tracks.retain(|t| t.name == "Video 2");
        let graph = build(&p, settings()).unwrap().filter_graph();
        assert!(graph.contains("anullsrc=r=48000:cl=stereo,atrim=duration=3.000000[aout]"));
    }

    #[test]
    fn ffmpeg_args_keep_paths_as_single_arguments() {
        let plan = build(&sample_project(), settings()).unwrap();
        let args = plan.ffmpeg_args(Path::new("/out/my video.mp4"));
        let args: Vec<String> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let pos = |s: &str| args.iter().position(|a| a == s).unwrap();

        assert!(args.contains(&"/m/a b.mp4".to_string()));
        assert_eq!(args.last().unwrap(), "/out/my video.mp4");
        assert_eq!(args[pos("-crf") + 1], "21");
        assert_eq!(args[pos("-r") + 1], "30/1");
        // The image input loops for exactly its clip duration.
        let logo = pos("/m/logo.png");
        assert_eq!(
            args[logo - 7..logo],
            ["-loop", "1", "-framerate", "30/1", "-t", "2.000000", "-i"]
        );
    }
}
