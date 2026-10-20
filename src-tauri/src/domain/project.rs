use std::collections::HashSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::time::Rational;

pub const SCHEMA_VERSION: u32 = 1;

const MAX_DIMENSION: u32 = 8192;
const MAX_FPS: u64 = 240;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSettings {
    pub width: u32,
    pub height: u32,
    pub frame_rate: Rational,
    pub audio_sample_rate: u32,
}

impl Default for ProjectSettings {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            frame_rate: Rational::new(30, 1),
            audio_sample_rate: 48_000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Video,
    Audio,
    Image,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Video,
    Audio,
}

/// A source media file referenced by the project. Paths are stored as
/// absolute paths in schema v1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaAsset {
    pub id: Uuid,
    pub kind: AssetKind,
    pub path: PathBuf,
    pub name: String,
    #[serde(default)]
    pub duration_us: Option<u64>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub frame_rate: Option<Rational>,
    #[serde(default)]
    pub has_audio: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub timeline_start_us: u64,
    pub source_in_us: u64,
    pub source_out_us: u64,
    pub enabled: bool,
}

impl Clip {
    /// Timeline duration. Playback rate is fixed at 1.0 until speed changes exist.
    pub fn duration_us(&self) -> u64 {
        self.source_out_us.saturating_sub(self.source_in_us)
    }

    pub fn end_us(&self) -> u64 {
        self.timeline_start_us.saturating_add(self.duration_us())
    }

    /// Whether the clip covers `time_us` (start inclusive, end exclusive).
    pub fn contains(&self, time_us: u64) -> bool {
        time_us >= self.timeline_start_us && time_us < self.end_us()
    }

    /// Maps a timeline time inside this clip to a time in the source media.
    pub fn source_time_at(&self, time_us: u64) -> u64 {
        self.source_in_us + time_us.saturating_sub(self.timeline_start_us)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: Uuid,
    pub name: String,
    pub kind: TrackKind,
    pub order: u32,
    pub muted: bool,
    pub hidden: bool,
    pub clips: Vec<Clip>,
}

impl Track {
    pub fn new(name: &str, kind: TrackKind, order: u32) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.to_string(),
            kind,
            order,
            muted: false,
            hidden: false,
            clips: Vec::new(),
        }
    }

    fn accepts(&self, asset: &MediaAsset) -> bool {
        match self.kind {
            TrackKind::Video => matches!(asset.kind, AssetKind::Video | AssetKind::Image),
            TrackKind::Audio => asset.kind == AssetKind::Audio || asset.has_audio,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub schema_version: u32,
    pub id: Uuid,
    pub name: String,
    pub settings: ProjectSettings,
    pub assets: Vec<MediaAsset>,
    pub tracks: Vec<Track>,
}

impl Project {
    /// A new project with one empty video track and one empty audio track.
    pub fn new(name: &str, settings: ProjectSettings) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            id: Uuid::new_v4(),
            name: name.to_string(),
            settings,
            assets: Vec::new(),
            tracks: vec![
                Track::new("Video 1", TrackKind::Video, 0),
                Track::new("Audio 1", TrackKind::Audio, 1),
            ],
        }
    }

    pub fn asset(&self, id: Uuid) -> Option<&MediaAsset> {
        self.assets.iter().find(|a| a.id == id)
    }

    /// Tracks in display order. Among video tracks, earlier tracks are drawn
    /// on top of later ones.
    pub fn tracks_in_order(&self) -> Vec<&Track> {
        let mut tracks: Vec<&Track> = self.tracks.iter().collect();
        tracks.sort_by_key(|t| t.order);
        tracks
    }

    /// The top-most visible video or image clip at `time_us`, if any.
    pub fn visual_clip_at(&self, time_us: u64) -> Option<(&Clip, &MediaAsset)> {
        self.tracks_in_order()
            .into_iter()
            .filter(|t| t.kind == TrackKind::Video && !t.hidden)
            .find_map(|t| {
                t.clips
                    .iter()
                    .find(|c| c.enabled && c.contains(time_us))
                    .and_then(|c| self.asset(c.asset_id).map(|a| (c, a)))
            })
    }

    /// Checks every invariant from the project doc §6.4 and returns all
    /// problems found, so the user sees the full list at once.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut problems = Vec::new();

        if self.schema_version != SCHEMA_VERSION {
            problems.push(format!(
                "unsupported schema version {}",
                self.schema_version
            ));
        }
        if self.name.trim().is_empty() {
            problems.push("project name is empty".into());
        }
        validate_settings(&self.settings, &mut problems);

        let mut ids = HashSet::new();
        let mut check_id = |id: Uuid, what: &str, problems: &mut Vec<String>| {
            if !ids.insert(id) {
                problems.push(format!("duplicate {what} id {id}"));
            }
        };

        check_id(self.id, "project", &mut problems);
        for asset in &self.assets {
            check_id(asset.id, "asset", &mut problems);
            validate_asset(asset, &mut problems);
        }

        let mut orders = HashSet::new();
        for track in &self.tracks {
            check_id(track.id, "track", &mut problems);
            if !orders.insert(track.order) {
                problems.push(format!("duplicate track order {}", track.order));
            }
            for clip in &track.clips {
                check_id(clip.id, "clip", &mut problems);
            }
            self.validate_track(track, &mut problems);
        }

        if problems.is_empty() {
            Ok(())
        } else {
            Err(problems)
        }
    }

    fn validate_track(&self, track: &Track, problems: &mut Vec<String>) {
        for clip in &track.clips {
            let Some(asset) = self.asset(clip.asset_id) else {
                problems.push(format!(
                    "clip {} references missing asset {}",
                    clip.id, clip.asset_id
                ));
                continue;
            };
            if clip.source_out_us <= clip.source_in_us {
                problems.push(format!("clip {} has non-positive duration", clip.id));
            }
            if let Some(duration) = asset.duration_us
                && clip.source_out_us > duration
            {
                problems.push(format!(
                    "clip {} extends past the end of its source",
                    clip.id
                ));
            }
            if !track.accepts(asset) {
                problems.push(format!(
                    "clip {} on track '{}' uses an incompatible asset",
                    clip.id, track.name
                ));
            }
        }

        // MVP rule: clips on the same track must not overlap.
        let mut spans: Vec<(u64, Option<u64>, Uuid)> = track
            .clips
            .iter()
            .map(|c| {
                (
                    c.timeline_start_us,
                    c.timeline_start_us.checked_add(c.duration_us()),
                    c.id,
                )
            })
            .collect();
        spans.sort_unstable_by_key(|&(start, _, _)| start);
        for pair in spans.windows(2) {
            let (_, end, id) = pair[0];
            let (next_start, _, next_id) = pair[1];
            match end {
                Some(end) if end <= next_start => {}
                _ => problems.push(format!(
                    "clips {id} and {next_id} overlap on track '{}'",
                    track.name
                )),
            }
        }
    }
}

fn validate_settings(settings: &ProjectSettings, problems: &mut Vec<String>) {
    for (label, value) in [("width", settings.width), ("height", settings.height)] {
        if !(2..=MAX_DIMENSION).contains(&value) || value % 2 != 0 {
            problems.push(format!(
                "{label} must be an even number between 2 and {MAX_DIMENSION}"
            ));
        }
    }

    let fr = settings.frame_rate;
    let (n, d) = (u64::from(fr.numerator), u64::from(fr.denominator));
    if !fr.is_positive() || n < d || n > MAX_FPS * d {
        problems.push(format!("frame rate must be between 1 and {MAX_FPS} fps"));
    }

    if !(8_000..=192_000).contains(&settings.audio_sample_rate) {
        problems.push("audio sample rate must be between 8000 and 192000 Hz".into());
    }
}

fn validate_asset(asset: &MediaAsset, problems: &mut Vec<String>) {
    if !asset.path.is_absolute() {
        problems.push(format!("asset {} path must be absolute", asset.id));
    }
    if asset.duration_us == Some(0) {
        problems.push(format!("asset {} has zero duration", asset.id));
    }
    if asset.width == Some(0) || asset.height == Some(0) {
        problems.push(format!("asset {} has zero dimensions", asset.id));
    }
    if asset.frame_rate.is_some_and(|fr| !fr.is_positive()) {
        problems.push(format!("asset {} has an invalid frame rate", asset.id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video_asset() -> MediaAsset {
        MediaAsset {
            id: Uuid::new_v4(),
            kind: AssetKind::Video,
            path: PathBuf::from("/media/intro.mp4"),
            name: "intro.mp4".into(),
            duration_us: Some(10_000_000),
            width: Some(1920),
            height: Some(1080),
            frame_rate: Some(Rational::new(30, 1)),
            has_audio: true,
        }
    }

    fn clip(asset: &MediaAsset, start: u64, src_in: u64, src_out: u64) -> Clip {
        Clip {
            id: Uuid::new_v4(),
            asset_id: asset.id,
            timeline_start_us: start,
            source_in_us: src_in,
            source_out_us: src_out,
            enabled: true,
        }
    }

    fn project_with_asset() -> (Project, MediaAsset) {
        let mut project = Project::new("Test", ProjectSettings::default());
        let asset = video_asset();
        project.assets.push(asset.clone());
        (project, asset)
    }

    fn errors(project: &Project) -> String {
        project.validate().unwrap_err().join("\n")
    }

    #[test]
    fn new_project_is_valid() {
        assert_eq!(
            Project::new("Test", ProjectSettings::default()).validate(),
            Ok(())
        );
    }

    #[test]
    fn valid_clips_pass() {
        let (mut project, asset) = project_with_asset();
        project.tracks[0].clips.push(clip(&asset, 0, 0, 2_000_000));
        project.tracks[0]
            .clips
            .push(clip(&asset, 2_000_000, 1_000_000, 3_000_000));
        assert_eq!(project.validate(), Ok(()));
    }

    #[test]
    fn rejects_bad_settings() {
        let mut project = Project::new("", ProjectSettings::default());
        project.settings.width = 1921;
        project.settings.frame_rate = Rational::new(0, 1);
        project.settings.audio_sample_rate = 0;
        let errs = errors(&project);
        assert!(errs.contains("name is empty"));
        assert!(errs.contains("width must be"));
        assert!(errs.contains("frame rate"));
        assert!(errs.contains("sample rate"));
    }

    #[test]
    fn accepts_fractional_ntsc_frame_rate() {
        let mut project = Project::new("Test", ProjectSettings::default());
        project.settings.frame_rate = Rational::new(30_000, 1001);
        assert_eq!(project.validate(), Ok(()));
    }

    #[test]
    fn rejects_duplicate_ids() {
        let (mut project, asset) = project_with_asset();
        project.assets.push(asset);
        assert!(errors(&project).contains("duplicate asset id"));
    }

    #[test]
    fn rejects_clip_with_missing_asset() {
        let (mut project, _) = project_with_asset();
        let orphan = video_asset();
        project.tracks[0].clips.push(clip(&orphan, 0, 0, 1_000_000));
        assert!(errors(&project).contains("references missing asset"));
    }

    #[test]
    fn rejects_invalid_source_range() {
        let (mut project, asset) = project_with_asset();
        project.tracks[0]
            .clips
            .push(clip(&asset, 0, 5_000_000, 5_000_000));
        project.tracks[0]
            .clips
            .push(clip(&asset, 10_000_000, 0, 20_000_000));
        let errs = errors(&project);
        assert!(errs.contains("non-positive duration"));
        assert!(errs.contains("past the end"));
    }

    #[test]
    fn rejects_overlapping_clips() {
        let (mut project, asset) = project_with_asset();
        project.tracks[0].clips.push(clip(&asset, 0, 0, 3_000_000));
        project.tracks[0]
            .clips
            .push(clip(&asset, 2_000_000, 0, 1_000_000));
        assert!(errors(&project).contains("overlap"));
    }

    #[test]
    fn rejects_incompatible_track() {
        let (mut project, mut asset) = project_with_asset();
        asset.has_audio = false;
        project.assets[0] = asset.clone();
        project.tracks[1].clips.push(clip(&asset, 0, 0, 1_000_000));
        assert!(errors(&project).contains("incompatible asset"));
    }

    #[test]
    fn finds_topmost_visible_clip() {
        let (mut project, asset) = project_with_asset();
        let mut overlay = Track::new("Video 2", TrackKind::Video, 0);
        project.tracks[0].order = 1;
        project.tracks[1].order = 2;
        overlay.clips.push(clip(&asset, 2_000_000, 0, 1_000_000));
        project.tracks[0]
            .clips
            .push(clip(&asset, 0, 1_000_000, 6_000_000));
        let base_id = project.tracks[0].clips[0].id;
        let overlay_id = overlay.clips[0].id;
        project.tracks.push(overlay);

        let at = |t| project.visual_clip_at(t).map(|(c, _)| c.id);
        assert_eq!(at(1_000_000), Some(base_id));
        assert_eq!(at(2_500_000), Some(overlay_id));
        assert_eq!(at(5_000_000), None);
        assert_eq!(
            project.tracks[0].clips[0].source_time_at(1_000_000),
            2_000_000
        );

        project.tracks[2].hidden = true;
        assert_eq!(
            project.visual_clip_at(2_500_000).map(|(c, _)| c.id),
            Some(base_id)
        );
    }

    #[test]
    fn serializes_with_camel_case_fields() {
        let project = Project::new("Test", ProjectSettings::default());
        let json = serde_json::to_value(&project).unwrap();
        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["settings"]["frameRate"]["numerator"], 30);
        assert_eq!(json["tracks"][0]["kind"], "video");
    }
}
