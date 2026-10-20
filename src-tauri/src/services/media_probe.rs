use std::path::Path;

use serde::Deserialize;

use crate::domain::errors::{AppError, AppResult};
use crate::domain::project::AssetKind;
use crate::domain::time::{Rational, parse_seconds_to_us};
use crate::services::tools::run_tool;

/// Metadata extracted from FFprobe for one media file.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbedMedia {
    pub kind: AssetKind,
    pub duration_us: Option<u64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_rate: Option<Rational>,
    pub has_audio: bool,
}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<ProbeStream>,
    format: Option<ProbeFormat>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    avg_frame_rate: Option<String>,
    r_frame_rate: Option<String>,
    duration: Option<String>,
    #[serde(default)]
    disposition: Disposition,
}

#[derive(Debug, Default, Deserialize)]
struct Disposition {
    #[serde(default)]
    attached_pic: u8,
}

#[derive(Debug, Deserialize)]
struct ProbeFormat {
    format_name: Option<String>,
    duration: Option<String>,
}

/// Probes `path` with FFprobe. The path is passed as a separate argument.
pub fn probe(path: &Path) -> AppResult<ProbedMedia> {
    let output = run_tool(
        "ffprobe",
        [
            "-v".as_ref(),
            "error".as_ref(),
            "-show_format".as_ref(),
            "-show_streams".as_ref(),
            "-of".as_ref(),
            "json".as_ref(),
            "-i".as_ref(),
            path.as_os_str(),
        ],
    )?;
    parse_probe_output(&output.stdout)
}

pub fn parse_probe_output(json: &[u8]) -> AppResult<ProbedMedia> {
    let probe: ProbeOutput = serde_json::from_slice(json)?;

    // Embedded cover art in audio files shows up as a video stream; ignore it.
    let video = probe
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video") && s.disposition.attached_pic == 0);
    let has_audio = probe
        .streams
        .iter()
        .any(|s| s.codec_type.as_deref() == Some("audio"));
    let format_name = probe
        .format
        .as_ref()
        .and_then(|f| f.format_name.as_deref())
        .unwrap_or_default();

    let kind = match video {
        Some(_) if is_still_image_format(format_name) => AssetKind::Image,
        Some(_) => AssetKind::Video,
        None if has_audio => AssetKind::Audio,
        None => {
            return Err(AppError::UnsupportedMedia(
                "no video or audio streams found".into(),
            ));
        }
    };

    let duration_us = match kind {
        AssetKind::Image => None,
        _ => probe
            .format
            .as_ref()
            .and_then(|f| f.duration.as_deref())
            .or_else(|| video.and_then(|v| v.duration.as_deref()))
            .and_then(parse_seconds_to_us)
            .filter(|&d| d > 0),
    };

    let frame_rate = match kind {
        AssetKind::Video => video.and_then(|v| {
            v.avg_frame_rate
                .as_deref()
                .and_then(Rational::parse)
                .or_else(|| v.r_frame_rate.as_deref().and_then(Rational::parse))
        }),
        _ => None,
    };

    Ok(ProbedMedia {
        kind,
        duration_us,
        width: video.and_then(|v| v.width).filter(|&w| w > 0),
        height: video.and_then(|v| v.height).filter(|&h| h > 0),
        frame_rate,
        has_audio,
    })
}

/// FFmpeg reports still images through the `image2` demuxer or `*_pipe`
/// demuxers (e.g. `png_pipe`, `jpeg_pipe`).
fn is_still_image_format(format_name: &str) -> bool {
    format_name
        .split(',')
        .any(|f| f == "image2" || f.ends_with("_pipe"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_video_with_audio() {
        let json = br#"{
            "streams": [
                {"codec_type": "video", "width": 1920, "height": 1080,
                 "avg_frame_rate": "30000/1001", "r_frame_rate": "30000/1001"},
                {"codec_type": "audio"}
            ],
            "format": {"format_name": "mov,mp4,m4a,3gp,3g2,mj2", "duration": "12.500000"}
        }"#;
        let media = parse_probe_output(json).unwrap();
        assert_eq!(
            media,
            ProbedMedia {
                kind: AssetKind::Video,
                duration_us: Some(12_500_000),
                width: Some(1920),
                height: Some(1080),
                frame_rate: Some(Rational::new(30000, 1001)),
                has_audio: true,
            }
        );
    }

    #[test]
    fn audio_with_cover_art_is_audio() {
        let json = br#"{
            "streams": [
                {"codec_type": "audio"},
                {"codec_type": "video", "width": 500, "height": 500, "disposition": {"attached_pic": 1}}
            ],
            "format": {"format_name": "mp3", "duration": "180.0"}
        }"#;
        let media = parse_probe_output(json).unwrap();
        assert_eq!(media.kind, AssetKind::Audio);
        assert_eq!(media.duration_us, Some(180_000_000));
        assert_eq!(media.width, None);
    }

    #[test]
    fn png_is_image_without_duration() {
        let json = br#"{
            "streams": [{"codec_type": "video", "width": 640, "height": 480, "avg_frame_rate": "0/0"}],
            "format": {"format_name": "png_pipe", "duration": "0.040000"}
        }"#;
        let media = parse_probe_output(json).unwrap();
        assert_eq!(media.kind, AssetKind::Image);
        assert_eq!(media.duration_us, None);
        assert_eq!(media.frame_rate, None);
    }

    #[test]
    fn missing_duration_is_none() {
        let json = br#"{
            "streams": [{"codec_type": "video", "avg_frame_rate": "0/0", "r_frame_rate": "25/1"}],
            "format": {"format_name": "matroska,webm", "duration": "N/A"}
        }"#;
        let media = parse_probe_output(json).unwrap();
        assert_eq!(media.duration_us, None);
        assert_eq!(media.frame_rate, Some(Rational::new(25, 1)));
    }

    #[test]
    fn rejects_files_without_media_streams() {
        let json = br#"{"streams": [{"codec_type": "data"}], "format": {}}"#;
        assert!(matches!(
            parse_probe_output(json),
            Err(AppError::UnsupportedMedia(_))
        ));
    }
}
