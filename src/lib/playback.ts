import type { Clip, MediaAsset, Project, TextOverlay, Track } from "./tauri";
import { US_PER_SECOND, type Microseconds } from "./time";
import { clipEnd } from "./timeline";

export interface ActiveClip {
  clip: Clip;
  track: Track;
  asset: MediaAsset;
}

export interface ActiveText {
  clip: Clip;
  text: TextOverlay;
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
 * Text on tracks above the picture is listed bottom-most first, the order
 * to draw it in; text below the picture is covered by it.
 */
export function activeAt(project: Project, us: Microseconds, missing: Set<string>) {
  const tracks = [...project.tracks].sort((a, b) => a.order - b.order);
  const assets = new Map(project.assets.map((a) => [a.id, a]));
  let visual: ActiveClip | null = null;
  const audio: ActiveClip[] = [];
  const texts: ActiveText[] = [];

  for (const track of tracks) {
    const clip = track.clips.find((c) => c.enabled && containsTime(c, us));
    if (clip?.text) {
      if (track.kind === "video" && !track.hidden && !visual) texts.unshift({ clip, text: clip.text });
      continue;
    }
    const asset = clip?.assetId ? assets.get(clip.assetId) : undefined;
    if (!clip || !asset || missing.has(asset.id)) continue;

    if (track.kind === "video") {
      if (track.hidden) continue;
      if (!visual) visual = { clip, track, asset };
      else if (asset.hasAudio) audio.push({ clip, track, asset });
    } else if (!track.muted) {
      audio.push({ clip, track, asset });
    }
  }
  return { visual, audio, texts };
}
