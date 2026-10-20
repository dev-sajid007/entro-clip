//! Timeline edit commands. Each edit is applied to a copy of the project and
//! the result is validated before it can replace the canonical state.

use serde::Deserialize;
use uuid::Uuid;

use super::errors::{AppError, AppResult};
use super::project::{AssetKind, Clip, ClipAudio, Project, Track, TrackKind};

/// Default length of a still image placed on the timeline.
pub const DEFAULT_IMAGE_DURATION_US: u64 = 5_000_000;

#[derive(Debug, Clone, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Edit {
    /// Places the whole source range of an asset on a track.
    AddClip {
        track_id: Uuid,
        asset_id: Uuid,
        timeline_start_us: u64,
    },
    MoveClip {
        clip_id: Uuid,
        track_id: Uuid,
        timeline_start_us: u64,
    },
    TrimClip {
        clip_id: Uuid,
        timeline_start_us: u64,
        source_in_us: u64,
        source_out_us: u64,
    },
    SplitClip {
        clip_id: Uuid,
        at_us: u64,
    },
    DeleteClip {
        clip_id: Uuid,
    },
    /// Changes a clip's sound; omitted fields keep their value.
    UpdateClipAudio {
        clip_id: Uuid,
        gain_db: Option<f64>,
        fade_in_us: Option<u64>,
        fade_out_us: Option<u64>,
    },
    AddTrack {
        kind: TrackKind,
    },
    DeleteTrack {
        track_id: Uuid,
    },
    UpdateTrack {
        track_id: Uuid,
        name: Option<String>,
        muted: Option<bool>,
        hidden: Option<bool>,
        gain_db: Option<f64>,
    },
}

#[derive(Debug)]
pub struct EditOutcome {
    pub project: Project,
    /// The clip created by `AddClip` or the right half of `SplitClip`.
    pub created_clip_id: Option<Uuid>,
}

pub fn apply(project: &Project, edit: &Edit) -> AppResult<EditOutcome> {
    let mut next = project.clone();
    let mut created_clip_id = None;

    match *edit {
        Edit::AddClip {
            track_id,
            asset_id,
            timeline_start_us,
        } => {
            let asset = project
                .asset(asset_id)
                .ok_or_else(|| not_found("asset", asset_id))?;
            let source_out_us = match (asset.kind, asset.duration_us) {
                (AssetKind::Image, _) => DEFAULT_IMAGE_DURATION_US,
                (_, Some(duration)) => duration,
                (_, None) => {
                    return Err(AppError::InvalidEdit(format!(
                        "{} has no known duration",
                        asset.name
                    )));
                }
            };
            let clip = Clip {
                id: Uuid::new_v4(),
                asset_id,
                timeline_start_us,
                source_in_us: 0,
                source_out_us,
                enabled: true,
                audio: ClipAudio::default(),
            };
            created_clip_id = Some(clip.id);
            track_mut(&mut next, track_id)?.clips.push(clip);
        }

        Edit::MoveClip {
            clip_id,
            track_id,
            timeline_start_us,
        } => {
            let mut clip = take_clip(&mut next, clip_id)?;
            clip.timeline_start_us = timeline_start_us;
            track_mut(&mut next, track_id)?.clips.push(clip);
        }

        Edit::TrimClip {
            clip_id,
            timeline_start_us,
            source_in_us,
            source_out_us,
        } => {
            let clip = clip_mut(&mut next, clip_id)?;
            clip.timeline_start_us = timeline_start_us;
            clip.source_in_us = source_in_us;
            clip.source_out_us = source_out_us;
            clip.fit_fades();
        }

        Edit::SplitClip { clip_id, at_us } => {
            let (track_index, clip_index) = locate_clip(&next, clip_id)?;
            let clip = &mut next.tracks[track_index].clips[clip_index];
            let end = clip.timeline_start_us + clip.duration_us();
            if at_us <= clip.timeline_start_us || at_us >= end {
                return Err(AppError::InvalidEdit(
                    "the split point must be inside the clip".into(),
                ));
            }
            let split_source = clip.source_in_us + (at_us - clip.timeline_start_us);
            // The fade-in stays on the left half and the fade-out on the right.
            let mut right = Clip {
                id: Uuid::new_v4(),
                timeline_start_us: at_us,
                source_in_us: split_source,
                ..clip.clone()
            };
            right.audio.fade_in_us = 0;
            right.fit_fades();
            clip.source_out_us = split_source;
            clip.audio.fade_out_us = 0;
            clip.fit_fades();
            created_clip_id = Some(right.id);
            next.tracks[track_index].clips.push(right);
        }

        Edit::DeleteClip { clip_id } => {
            take_clip(&mut next, clip_id)?;
        }

        Edit::UpdateClipAudio {
            clip_id,
            gain_db,
            fade_in_us,
            fade_out_us,
        } => {
            let audio = &mut clip_mut(&mut next, clip_id)?.audio;
            if let Some(gain_db) = gain_db {
                audio.gain_db = gain_db;
            }
            if let Some(fade_in_us) = fade_in_us {
                audio.fade_in_us = fade_in_us;
            }
            if let Some(fade_out_us) = fade_out_us {
                audio.fade_out_us = fade_out_us;
            }
        }

        Edit::AddTrack { kind } => {
            let count = next.tracks.iter().filter(|t| t.kind == kind).count() + 1;
            let name = match kind {
                TrackKind::Video => format!("Video {count}"),
                TrackKind::Audio => format!("Audio {count}"),
            };
            let track = Track::new(&name, kind, 0);
            // New video tracks go above existing video; new audio tracks go last.
            match kind {
                TrackKind::Video => next.tracks.insert(0, track),
                TrackKind::Audio => next.tracks.push(track),
            }
            renumber(&mut next);
        }

        Edit::DeleteTrack { track_id } => {
            let before = next.tracks.len();
            next.tracks.retain(|t| t.id != track_id);
            if next.tracks.len() == before {
                return Err(not_found("track", track_id));
            }
            renumber(&mut next);
        }

        Edit::UpdateTrack {
            track_id,
            ref name,
            muted,
            hidden,
            gain_db,
        } => {
            let track = track_mut(&mut next, track_id)?;
            if let Some(gain_db) = gain_db {
                track.gain_db = gain_db;
            }
            if let Some(name) = name {
                track.name = name.trim().to_string();
            }
            if let Some(muted) = muted {
                track.muted = muted;
            }
            if let Some(hidden) = hidden {
                track.hidden = hidden;
            }
        }
    }

    for track in &mut next.tracks {
        track.clips.sort_by_key(|c| c.timeline_start_us);
    }
    next.validate()
        .map_err(|problems| AppError::InvalidEdit(problems.join("; ")))?;

    Ok(EditOutcome {
        project: next,
        created_clip_id,
    })
}

fn not_found(what: &str, id: Uuid) -> AppError {
    AppError::NotFound(format!("{what} {id}"))
}

/// Keeps tracks in display order (video above audio) with sequential `order`.
/// The vector is already in display order, so a stable sort by kind only
/// moves tracks across the video/audio boundary.
fn renumber(project: &mut Project) {
    project.tracks.sort_by_key(|t| t.kind == TrackKind::Audio);
    for (i, track) in project.tracks.iter_mut().enumerate() {
        track.order = i as u32;
    }
}

fn track_mut(project: &mut Project, id: Uuid) -> AppResult<&mut Track> {
    project
        .tracks
        .iter_mut()
        .find(|t| t.id == id)
        .ok_or_else(|| not_found("track", id))
}

fn locate_clip(project: &Project, id: Uuid) -> AppResult<(usize, usize)> {
    project
        .tracks
        .iter()
        .enumerate()
        .find_map(|(ti, t)| t.clips.iter().position(|c| c.id == id).map(|ci| (ti, ci)))
        .ok_or_else(|| not_found("clip", id))
}

fn clip_mut(project: &mut Project, id: Uuid) -> AppResult<&mut Clip> {
    let (ti, ci) = locate_clip(project, id)?;
    Ok(&mut project.tracks[ti].clips[ci])
}

fn take_clip(project: &mut Project, id: Uuid) -> AppResult<Clip> {
    let (ti, ci) = locate_clip(project, id)?;
    Ok(project.tracks[ti].clips.remove(ci))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::domain::project::{MediaAsset, ProjectSettings};

    const SEC: u64 = 1_000_000;

    fn asset(kind: AssetKind, duration_us: Option<u64>) -> MediaAsset {
        MediaAsset {
            id: Uuid::new_v4(),
            kind,
            path: PathBuf::from("/media/file"),
            name: "file".into(),
            duration_us,
            width: Some(1920),
            height: Some(1080),
            frame_rate: None,
            has_audio: kind != AssetKind::Image,
        }
    }

    struct Fixture {
        project: Project,
        video: Uuid,
        audio_track: Uuid,
        video_asset: Uuid,
    }

    fn fixture() -> Fixture {
        let mut project = Project::new("Test", ProjectSettings::default());
        let video_asset = asset(AssetKind::Video, Some(10 * SEC));
        let id = video_asset.id;
        project.assets.push(video_asset);
        Fixture {
            video: project.tracks[0].id,
            audio_track: project.tracks[1].id,
            video_asset: id,
            project,
        }
    }

    fn add_clip(f: &Fixture, project: &Project, start: u64) -> (Project, Uuid) {
        let out = apply(
            project,
            &Edit::AddClip {
                track_id: f.video,
                asset_id: f.video_asset,
                timeline_start_us: start,
            },
        )
        .unwrap();
        (out.project, out.created_clip_id.unwrap())
    }

    #[test]
    fn adds_full_source_range() {
        let f = fixture();
        let (project, id) = add_clip(&f, &f.project, 2 * SEC);
        let clip = &project.tracks[0].clips[0];
        assert_eq!(clip.id, id);
        assert_eq!((clip.source_in_us, clip.source_out_us), (0, 10 * SEC));
        assert_eq!(clip.timeline_start_us, 2 * SEC);
    }

    #[test]
    fn images_get_default_duration() {
        let mut f = fixture();
        let image = asset(AssetKind::Image, None);
        let image_id = image.id;
        f.project.assets.push(image);
        let out = apply(
            &f.project,
            &Edit::AddClip {
                track_id: f.video,
                asset_id: image_id,
                timeline_start_us: 0,
            },
        )
        .unwrap();
        assert_eq!(
            out.project.tracks[0].clips[0].duration_us(),
            DEFAULT_IMAGE_DURATION_US
        );
    }

    #[test]
    fn rejects_overlapping_add_and_leaves_input_untouched() {
        let f = fixture();
        let (project, _) = add_clip(&f, &f.project, 0);
        let err = apply(
            &project,
            &Edit::AddClip {
                track_id: f.video,
                asset_id: f.video_asset,
                timeline_start_us: 5 * SEC,
            },
        )
        .unwrap_err();
        assert!(matches!(err, AppError::InvalidEdit(m) if m.contains("overlap")));
        assert_eq!(project.tracks[0].clips.len(), 1);
    }

    #[test]
    fn moves_clip_between_compatible_tracks() {
        let f = fixture();
        let (project, id) = add_clip(&f, &f.project, 0);
        let moved = apply(
            &project,
            &Edit::MoveClip {
                clip_id: id,
                track_id: f.audio_track,
                timeline_start_us: 3 * SEC,
            },
        )
        .unwrap()
        .project;
        assert!(moved.tracks[0].clips.is_empty());
        assert_eq!(moved.tracks[1].clips[0].timeline_start_us, 3 * SEC);
    }

    #[test]
    fn trims_within_source_bounds() {
        let f = fixture();
        let (project, id) = add_clip(&f, &f.project, 0);
        let trim = |start, src_in, src_out| {
            apply(
                &project,
                &Edit::TrimClip {
                    clip_id: id,
                    timeline_start_us: start,
                    source_in_us: src_in,
                    source_out_us: src_out,
                },
            )
        };
        let trimmed = trim(SEC, SEC, 4 * SEC).unwrap().project;
        assert_eq!(trimmed.tracks[0].clips[0].duration_us(), 3 * SEC);
        assert!(trim(0, 0, 11 * SEC).is_err());
        assert!(trim(0, 4 * SEC, 4 * SEC).is_err());
    }

    #[test]
    fn splits_clip_into_contiguous_halves() {
        let f = fixture();
        let (project, id) = add_clip(&f, &f.project, 2 * SEC);
        let out = apply(
            &project,
            &Edit::SplitClip {
                clip_id: id,
                at_us: 5 * SEC,
            },
        )
        .unwrap();
        let clips = &out.project.tracks[0].clips;
        assert_eq!(clips.len(), 2);
        assert_eq!(
            (clips[0].timeline_start_us, clips[0].source_out_us),
            (2 * SEC, 3 * SEC)
        );
        assert_eq!(clips[1].id, out.created_clip_id.unwrap());
        assert_eq!(
            (
                clips[1].timeline_start_us,
                clips[1].source_in_us,
                clips[1].source_out_us
            ),
            (5 * SEC, 3 * SEC, 10 * SEC)
        );
        assert!(
            apply(
                &project,
                &Edit::SplitClip {
                    clip_id: id,
                    at_us: 2 * SEC
                }
            )
            .is_err()
        );
    }

    #[test]
    fn deletes_clip() {
        let f = fixture();
        let (project, id) = add_clip(&f, &f.project, 0);
        let out = apply(&project, &Edit::DeleteClip { clip_id: id }).unwrap();
        assert!(out.project.tracks[0].clips.is_empty());
        assert!(apply(&out.project, &Edit::DeleteClip { clip_id: id }).is_err());
    }

    #[test]
    fn adds_tracks_in_display_order() {
        let f = fixture();
        let p = apply(
            &f.project,
            &Edit::AddTrack {
                kind: TrackKind::Video,
            },
        )
        .unwrap()
        .project;
        let p = apply(
            &p,
            &Edit::AddTrack {
                kind: TrackKind::Audio,
            },
        )
        .unwrap()
        .project;
        let names: Vec<_> = p
            .tracks
            .iter()
            .map(|t| (t.name.as_str(), t.order))
            .collect();
        assert_eq!(
            names,
            [
                ("Video 2", 0),
                ("Video 1", 1),
                ("Audio 1", 2),
                ("Audio 2", 3)
            ]
        );
    }

    fn set_audio(
        project: &Project,
        clip_id: Uuid,
        fade_in: u64,
        fade_out: u64,
    ) -> AppResult<Project> {
        apply(
            project,
            &Edit::UpdateClipAudio {
                clip_id,
                gain_db: Some(-3.0),
                fade_in_us: Some(fade_in),
                fade_out_us: Some(fade_out),
            },
        )
        .map(|o| o.project)
    }

    #[test]
    fn updates_clip_audio_within_limits() {
        let f = fixture();
        let (p, id) = add_clip(&f, &f.project, 0);
        let p = set_audio(&p, id, 2 * SEC, 3 * SEC).unwrap();
        let audio = p.tracks[0].clips[0].audio;
        assert_eq!(audio.gain_db, -3.0);
        assert_eq!((audio.fade_in_us, audio.fade_out_us), (2 * SEC, 3 * SEC));
        // 10 s clip: 6 s + 5 s of fades don't fit.
        assert!(matches!(
            set_audio(&p, id, 6 * SEC, 5 * SEC),
            Err(AppError::InvalidEdit(_))
        ));
    }

    #[test]
    fn split_and_trim_keep_fades_inside_clips() {
        let f = fixture();
        let (p, id) = add_clip(&f, &f.project, 0);
        let p = set_audio(&p, id, 2 * SEC, 3 * SEC).unwrap();

        let out = apply(
            &p,
            &Edit::SplitClip {
                clip_id: id,
                at_us: SEC,
            },
        )
        .unwrap();
        let clips = &out.project.tracks[0].clips;
        // Left half is 1 s long, so its 2 s fade-in shrinks; the right half
        // keeps the fade-out and loses the fade-in.
        assert_eq!(
            (clips[0].audio.fade_in_us, clips[0].audio.fade_out_us),
            (SEC, 0)
        );
        assert_eq!(
            (clips[1].audio.fade_in_us, clips[1].audio.fade_out_us),
            (0, 3 * SEC)
        );

        let trimmed = apply(
            &p,
            &Edit::TrimClip {
                clip_id: id,
                timeline_start_us: 0,
                source_in_us: 0,
                source_out_us: 4 * SEC,
            },
        )
        .unwrap()
        .project;
        let audio = trimmed.tracks[0].clips[0].audio;
        assert_eq!((audio.fade_in_us, audio.fade_out_us), (2 * SEC, 2 * SEC));
    }

    #[test]
    fn updates_and_deletes_tracks() {
        let f = fixture();
        let p = apply(
            &f.project,
            &Edit::UpdateTrack {
                track_id: f.audio_track,
                name: None,
                muted: Some(true),
                hidden: None,
                gain_db: Some(-6.0),
            },
        )
        .unwrap()
        .project;
        assert!(p.tracks[1].muted);
        assert_eq!(p.tracks[1].gain_db, -6.0);
        let p = apply(&p, &Edit::DeleteTrack { track_id: f.video })
            .unwrap()
            .project;
        assert_eq!(p.tracks.len(), 1);
        assert_eq!(p.tracks[0].order, 0);
    }

    #[test]
    fn deserializes_from_frontend_json() {
        let json = format!(
            r#"{{"type":"moveClip","clipId":"{}","trackId":"{}","timelineStartUs":5}}"#,
            Uuid::nil(),
            Uuid::nil()
        );
        assert!(matches!(
            serde_json::from_str::<Edit>(&json).unwrap(),
            Edit::MoveClip {
                timeline_start_us: 5,
                ..
            }
        ));
    }
}
