import type { AssetKind, Clip, Track } from "./tauri";
import { US_PER_SECOND, type Microseconds, type Rational } from "./time";

/** All pixel/time conversion goes through these helpers. */
export const usToPx = (us: Microseconds, pxPerSecond: number) => (us / US_PER_SECOND) * pxPerSecond;
export const pxToUs = (px: number, pxPerSecond: number): Microseconds =>
  Math.round((px / pxPerSecond) * US_PER_SECOND);

export const MIN_ZOOM = 5;
export const MAX_ZOOM = 400;
export const clampZoom = (pxPerSecond: number) => Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, pxPerSecond));

export const clipDuration = (clip: Clip): Microseconds => clip.sourceOutUs - clip.sourceInUs;
export const clipEnd = (clip: Clip): Microseconds => clip.timelineStartUs + clipDuration(clip);

/** Duration of one frame at `rate`, in microseconds (not rounded). */
export const frameDurationUs = (rate: Rational) => (US_PER_SECOND * rate.denominator) / rate.numerator;

/** Rounds a time to the nearest frame boundary of the project frame rate. */
export function snapToFrame(us: Microseconds, rate: Rational): Microseconds {
  const frames = Math.round(us / frameDurationUs(rate));
  return Math.round(frames * frameDurationUs(rate));
}

/** Converts a time to its frame index (frames that have fully started). */
export function frameIndex(us: Microseconds, rate: Rational): number {
  return Math.floor((us * rate.numerator) / (US_PER_SECOND * rate.denominator) + 1e-9);
}

/** Returns the candidate closest to `us` within `thresholdUs`, if any. */
export function findSnap(
  us: Microseconds,
  candidates: Iterable<Microseconds>,
  thresholdUs: Microseconds,
): Microseconds | null {
  let best: Microseconds | null = null;
  let bestDistance = thresholdUs;
  for (const c of candidates) {
    const d = Math.abs(c - us);
    if (d <= bestDistance) {
      best = c;
      bestDistance = d;
    }
  }
  return best;
}

/** Clip edges and the playhead, used as snap targets. */
export function snapCandidates(tracks: Track[], playheadUs: Microseconds, excludeClipId?: string) {
  const points = [0, playheadUs];
  for (const track of tracks) {
    for (const clip of track.clips) {
      if (clip.id === excludeClipId) continue;
      points.push(clip.timelineStartUs, clipEnd(clip));
    }
  }
  return points;
}

/** Whether a span would overlap any clip on the track (other than `excludeClipId`). */
export function overlapsTrack(
  track: Track,
  startUs: Microseconds,
  durationUs: Microseconds,
  excludeClipId?: string,
): boolean {
  const end = startUs + durationUs;
  return track.clips.some(
    (c) => c.id !== excludeClipId && startUs < clipEnd(c) && c.timelineStartUs < end,
  );
}

/** Mirrors `DEFAULT_TEXT_DURATION_US` in src-tauri/src/domain/edit.rs. */
export const DEFAULT_TEXT_DURATION_US = 3_000_000;

/** What a clip shows: its asset's kind, or text. */
export type ClipKind = AssetKind | "text";

/** Mirrors `Track::accepts` (and the text-clip rule) in the Rust domain model. */
export function trackAccepts(track: Track, kind: ClipKind, hasAudio: boolean): boolean {
  if (kind === "text") return track.kind === "video";
  return track.kind === "video" ? kind === "video" || kind === "image" : kind === "audio" || hasAudio;
}

/**
 * The first video track, top-most first, with room for a clip of
 * `durationUs` at `startUs`.
 */
export function freeVideoTrack(tracks: Track[], startUs: Microseconds, durationUs: Microseconds): Track | null {
  return tracks.find((t) => t.kind === "video" && !overlapsTrack(t, startUs, durationUs)) ?? null;
}

/** End time of the last clip on the track. */
export const trackEnd = (track: Track): Microseconds =>
  track.clips.reduce((end, c) => Math.max(end, clipEnd(c)), 0);

export const timelineEnd = (tracks: Track[]): Microseconds =>
  tracks.reduce((end, t) => Math.max(end, trackEnd(t)), 0);

const TICK_STEPS_SECONDS = [0.1, 0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600];

/** Picks a ruler tick interval so labels are at least `minPx` apart. */
export function tickInterval(pxPerSecond: number, minPx = 80): Microseconds {
  const step = TICK_STEPS_SECONDS.find((s) => s * pxPerSecond >= minPx) ?? 600;
  return Math.round(step * US_PER_SECOND);
}

export function formatRulerLabel(us: Microseconds): string {
  const totalSeconds = us / US_PER_SECOND;
  const m = Math.floor(totalSeconds / 60);
  const s = totalSeconds - m * 60;
  const secText = Number.isInteger(s) ? String(s).padStart(2, "0") : s.toFixed(2).padStart(5, "0");
  return `${m}:${secText}`;
}
