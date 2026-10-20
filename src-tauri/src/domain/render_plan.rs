//! Typed render plan: the bridge between the project model and FFmpeg.
//! Building the plan and its FFmpeg arguments is pure, so it is tested
//! without running FFmpeg.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::errors::{AppError, AppResult};
use super::project::{
    AssetKind, Project, ProjectSettings, TextOverlay, TrackKind, parse_hex_color, validate_settings,
};
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

    /// Target bits per pixel per frame for encoders without a CRF mode.
    fn bits_per_pixel_milli(self) -> u64 {
        match self {
            Quality::High => 150,
            Quality::Standard => 100,
            Quality::Small => 60,
        }
    }
}

/// Software H.264 encoders EntroCut knows how to drive. Builds such as
/// Fedora's `ffmpeg-free` ship OpenH264 but not x264.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoEncoder {
    X264,
    OpenH264,
}

impl VideoEncoder {
    /// Preference order when several are available.
    pub const ALL: [VideoEncoder; 2] = [VideoEncoder::X264, VideoEncoder::OpenH264];

    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            VideoEncoder::X264 => "libx264",
            VideoEncoder::OpenH264 => "libopenh264",
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
    pub encoder: VideoEncoder,
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
    encoder: VideoEncoder,
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
        encoder,
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

/// Font family for text clips, resolved through fontconfig. The preview
/// uses the matching CSS generic family, `sans-serif`.
pub const TEXT_FONT: &str = "Sans";

/// Text drawn over the layers below it from `start_us` until `end_us`.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLayer {
    pub start_us: u64,
    pub end_us: u64,
    pub text: TextOverlay,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VideoLayer {
    Media(Placement),
    Text(TextLayer),
}

/// A sound source placed on the timeline, with its combined clip and track
/// gain and its fades.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AudioSource {
    pub input: usize,
    pub start_us: u64,
    pub duration_us: u64,
    pub gain_db: f64,
    pub fade_in_us: u64,
    pub fade_out_us: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RenderPlan {
    pub settings: ExportSettings,
    pub duration_us: u64,
    pub inputs: Vec<RenderInput>,
    /// Video layers from bottom to top.
    pub video_layers: Vec<VideoLayer>,
    pub audio_sources: Vec<AudioSource>,
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
            if let Some(text) = &clip.text {
                if track.kind == TrackKind::Video {
                    plan.video_layers.push(VideoLayer::Text(TextLayer {
                        start_us: clip.timeline_start_us,
                        end_us: clip.end_us(),
                        text: text.clone(),
                    }));
                }
                continue;
            }
            let asset = clip
                .asset_id
                .and_then(|id| project.asset(id))
                .ok_or_else(|| AppError::NotFound(format!("source for clip {}", clip.id)))?;
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
                plan.video_layers.push(VideoLayer::Media(placement));
            }
            if asset.has_audio {
                plan.audio_sources.push(AudioSource {
                    input,
                    start_us: clip.timeline_start_us,
                    duration_us: clip.duration_us(),
                    gain_db: clip.audio.gain_db + track.gain_db,
                    fade_in_us: clip.audio.fade_in_us,
                    fade_out_us: clip.audio.fade_out_us,
                });
            }
        }
    }
    Ok(plan)
}

/// Gain and fade filters for one source, each followed by a comma, or
/// nothing when the source plays unchanged. Fades are timed from the start
/// of the trimmed source, before it is delayed into place.
fn audio_adjustments(src: &AudioSource) -> String {
    let mut filters = String::new();
    if src.gain_db != 0.0 {
        filters.push_str(&format!("volume={:.2}dB,", src.gain_db));
    }
    if src.fade_in_us > 0 {
        let d = format_us_as_seconds(src.fade_in_us);
        filters.push_str(&format!("afade=t=in:st=0:d={d},"));
    }
    if src.fade_out_us > 0 {
        let st = format_us_as_seconds(src.duration_us.saturating_sub(src.fade_out_us));
        let d = format_us_as_seconds(src.fade_out_us);
        filters.push_str(&format!("afade=t=out:st={st}:d={d},"));
    }
    filters
}

/// Escapes a filter option value for a filtergraph: first for the option
/// parser (`\ ' :`), then for the graph parser (`\ ' [ ] , ;`).
pub fn escape_filter_value(value: &str) -> String {
    let escape = |s: &str, special: &str| {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            if special.contains(c) {
                out.push('\\');
            }
            out.push(c);
        }
        out
    };
    escape(&escape(value, "\\':"), "\\'[],;")
}

/// `drawtext` filter drawing `layer` centred on its position in a `w`×`h`
/// frame, shown from its start until (not including) its end.
fn drawtext_filter(layer: &TextLayer, w: u32, h: u32) -> String {
    let t = &layer.text;
    let size = ((t.size * f64::from(h)).round() as u32).max(1);
    let cx = (t.x * f64::from(w)).round() as u32;
    let cy = (t.y * f64::from(h)).round() as u32;
    let color = parse_hex_color(&t.color).unwrap_or(0xffffff);
    let enable = format!(
        "gte(t,{})*lt(t,{})",
        format_us_as_seconds(layer.start_us),
        format_us_as_seconds(layer.end_us)
    );
    let mut filter = format!(
        "drawtext=font={TEXT_FONT}:expansion=none:text={}:fontcolor=0x{color:06x}:\
         fontsize={size}:text_align=center:x={cx}-text_w/2:y={cy}-text_h/2:enable={}",
        escape_filter_value(&t.content),
        escape_filter_value(&enable),
    );
    if t.background {
        let pad = (size / 4).max(1);
        filter.push_str(&format!(":box=1:boxcolor=black@0.5:boxborderw={pad}"));
    }
    filter
}

fn rate(r: Rational) -> String {
    format!("{}/{}", r.numerator, r.denominator)
}

impl RenderPlan {
    /// Whether rendering needs FFmpeg's `drawtext` filter.
    pub fn has_text(&self) -> bool {
        self.video_layers
            .iter()
            .any(|l| matches!(l, VideoLayer::Text(_)))
    }

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
            match layer {
                VideoLayer::Media(placement) => {
                    let start = format_us_as_seconds(placement.start_us);
                    parts.push(format!(
                        "[{}:v]fps={fps},scale={w}:{h}:force_original_aspect_ratio=decrease,\
                         format=yuva420p,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black@0,setsar=1,\
                         setpts=PTS-STARTPTS+{start}/TB[l{i}]",
                        placement.input
                    ));
                    parts.push(format!(
                        "[{last}][l{i}]overlay=eof_action=pass:format=auto[v{i}]"
                    ));
                }
                // Drawn onto the composite so far, so layers above cover it.
                VideoLayer::Text(text) => {
                    parts.push(format!("[{last}]{}[v{i}]", drawtext_filter(text, w, h)));
                }
            }
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
                     asetpts=PTS-STARTPTS,{}adelay=delays={delay}S:all=1[a{j}]",
                    src.input,
                    audio_adjustments(src),
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
            ]
            .map(OsString::from),
        );
        args.extend(self.video_codec_args());
        args.extend(
            [
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

    fn video_codec_args(&self) -> Vec<OsString> {
        let s = &self.settings;
        let mut args = vec!["-c:v".into(), s.encoder.ffmpeg_name().into()];
        match s.encoder {
            VideoEncoder::X264 => {
                let crf = s.quality.crf().to_string();
                args.extend(["-preset", "medium", "-crf", &crf].map(OsString::from));
            }
            VideoEncoder::OpenH264 => {
                let kbps = self.video_bitrate_kbps().to_string() + "k";
                args.extend(["-rc_mode", "bitrate", "-b:v", &kbps].map(OsString::from));
            }
        }
        args
    }

    /// Bitrate for encoders without a CRF mode, scaled by pixel rate.
    pub fn video_bitrate_kbps(&self) -> u64 {
        let s = &self.settings;
        let pixels_per_second =
            u64::from(s.width) * u64::from(s.height) * u64::from(s.frame_rate.numerator)
                / u64::from(s.frame_rate.denominator);
        pixels_per_second * s.quality.bits_per_pixel_milli() / 1_000_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::project::{Clip, ClipAudio, MediaAsset, Track};
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
            asset_id: Some(asset.id),
            text: None,
            timeline_start_us: start,
            source_in_us: src_in,
            source_out_us: src_in + len,
            enabled: true,
            audio: Default::default(),
        }
    }

    fn settings() -> ExportSettings {
        resolve_settings(
            &ProjectSettings::default(),
            None,
            None,
            Quality::Standard,
            VideoEncoder::X264,
        )
        .unwrap()
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
            let e = resolve_settings(p, Some(s), None, Quality::High, VideoEncoder::X264).unwrap();
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
            VideoEncoder::X264,
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
        let path = |i: usize| match &plan.video_layers[i] {
            VideoLayer::Media(p) => plan.inputs[p.input].path.clone(),
            VideoLayer::Text(_) => panic!("unexpected text layer"),
        };
        assert_eq!(path(0), PathBuf::from("/m/a b.mp4"));
        assert_eq!(path(1), PathBuf::from("/m/logo.png"));
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
    fn applies_clip_and_track_gain_and_fades() {
        let mut p = sample_project();
        let music = &mut p.tracks[1];
        music.gain_db = -6.0;
        music.clips[0].audio = ClipAudio {
            gain_db: 2.5,
            fade_in_us: SEC,
            fade_out_us: 2 * SEC,
        };
        let plan = build(&p, settings()).unwrap();
        let music = plan.audio_sources[1];
        assert_eq!(music.gain_db, -3.5);

        let graph = plan.filter_graph();
        // The 10 s music clip fades in over 1 s and out over its last 2 s.
        assert!(graph.contains(
            "asetpts=PTS-STARTPTS,volume=-3.50dB,afade=t=in:st=0:d=1.000000,\
             afade=t=out:st=8.000000:d=2.000000,adelay=delays=24000S:all=1[a1]"
        ));
        // The video's sound is unchanged.
        assert!(graph.contains("asetpts=PTS-STARTPTS,adelay=delays=0S:all=1[a0]"));
    }

    #[test]
    fn escapes_filter_values_twice() {
        // The example from the FFmpeg filtergraph escaping documentation.
        assert_eq!(
            escape_filter_value("this is a 'string': may contain one, or more, special characters"),
            r"this is a \\\'string\\\'\\: may contain one\, or more\, special characters"
        );
        assert_eq!(escape_filter_value(r"a\b[c];d"), r"a\\\\b\[c\]\;d");
    }

    #[test]
    fn text_layers_draw_in_stack_order() {
        let mut p = sample_project();
        let mut title = Track::new("Titles", TrackKind::Video, 0);
        for t in &mut p.tracks {
            t.order += 1;
        }
        title.clips.push(Clip {
            id: Uuid::new_v4(),
            asset_id: None,
            text: Some(TextOverlay {
                content: "Hi, it's me".into(),
                color: "#ff8000".into(),
                y: 0.25,
                background: true,
                ..TextOverlay::default()
            }),
            timeline_start_us: SEC,
            source_in_us: 0,
            source_out_us: 2 * SEC,
            enabled: true,
            audio: Default::default(),
        });
        p.tracks.push(title);
        p.validate().unwrap();

        let plan = build(&p, settings()).unwrap();
        assert!(plan.has_text());
        // The titles track is on top, so its text is the last layer drawn.
        assert!(matches!(
            plan.video_layers.last(),
            Some(VideoLayer::Text(_))
        ));
        let graph = plan.filter_graph();
        assert!(graph.contains(
            "[v1]drawtext=font=Sans:expansion=none:text=Hi\\, it\\\\\\'s me:fontcolor=0xff8000:\
             fontsize=86:text_align=center:x=960-text_w/2:y=270-text_h/2:\
             enable=gte(t\\,1.000000)*lt(t\\,3.000000):box=1:boxcolor=black@0.5:boxborderw=21[v2]"
        ));
        assert!(graph.contains("[v2]format=yuv420p[vout]"));
        assert!(!build(&sample_project(), settings()).unwrap().has_text());
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

    #[test]
    fn openh264_uses_bitrate_instead_of_crf() {
        let settings = ExportSettings {
            encoder: VideoEncoder::OpenH264,
            ..settings()
        };
        let plan = build(&sample_project(), settings).unwrap();
        // 1920x1080 at 30 fps, 0.1 bits per pixel.
        assert_eq!(plan.video_bitrate_kbps(), 6220);
        let args: Vec<String> = plan
            .ffmpeg_args(Path::new("/out/a.mp4"))
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        let pos = |s: &str| args.iter().position(|a| a == s);
        assert_eq!(args[pos("-c:v").unwrap() + 1], "libopenh264");
        assert_eq!(args[pos("-b:v").unwrap() + 1], "6220k");
        assert!(pos("-crf").is_none());
    }
}
