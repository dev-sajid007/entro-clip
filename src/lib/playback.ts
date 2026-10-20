import type { Clip, MediaAsset, Project, Track } from "./tauri";
import { US_PER_SECOND, type Microseconds } from "./time";
import { clipEnd } from "./timeline";

export interface ActiveClip {
  clip: Clip;
  track: Track;
  asset: MediaAsset;
}

export const containsTime = (clip: Clip, us: Microseconds) =>
  us >= clip.timelineStartUs && us < clipEnd(clip);

/** Source position in seconds for a timeline time inside the clip. */
export const sourceSecondsAt = (clip: Clip, us: Microseconds) =>
  (clip.sourceInUs + (us - clip.timelineStartUs)) / US_PER_SECOND;

/**
 * What is seen and heard at a time. Mirrors the backend rules: the top-most
 * visible video track wins the picture; audio comes from unmuted audio
 * tracks and from the embedded sound of clips on visible video tracks.
 */
export function activeAt(project: Project, us: Microseconds, missing: Set<string>) {
  const tracks = [...project.tracks].sort((a, b) => a.order - b.order);
  const assets = new Map(project.assets.map((a) => [a.id, a]));
  let visual: ActiveClip | null = null;
  const audio: ActiveClip[] = [];

  for (const track of tracks) {
    const clip = track.clips.find((c) => c.enabled && containsTime(c, us));
    const asset = clip && assets.get(clip.assetId);
    if (!clip || !asset || missing.has(asset.id)) continue;

    if (track.kind === "video") {
      if (track.hidden) continue;
      if (!visual) visual = { clip, track, asset };
      else if (asset.hasAudio) audio.push({ clip, track, asset });
    } else if (!track.muted) {
      audio.push({ clip, track, asset });
    }
  }
  return { visual, audio };
}
