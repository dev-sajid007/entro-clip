import type { Clip, Track } from "./tauri";
import type { Microseconds } from "./time";
import { clipEnd } from "./timeline";

/** Mirrors `MIN_GAIN_DB` / `MAX_GAIN_DB` in src-tauri/src/domain/project.rs. */
export const MIN_GAIN_DB = -60;
export const MAX_GAIN_DB = 12;

export const dbToGain = (db: number) => 10 ** (db / 20);

export function formatDb(db: number): string {
  if (db <= MIN_GAIN_DB) return "−∞ dB";
  const rounded = Math.round(db * 10) / 10;
  return `${rounded > 0 ? "+" : rounded < 0 ? "−" : ""}${Math.abs(rounded).toFixed(1)} dB`;
}

/**
 * Linear gain of a clip at a timeline time: clip and track gain plus linear
 * fades, matching the export's `volume` and `afade` filters. Can exceed 1.
 */
export function clipGainAt(clip: Clip, track: Track, us: Microseconds): number {
  const { gainDb, fadeInUs, fadeOutUs } = clip.audio;
  let gain = dbToGain(gainDb + track.gainDb);
  const fromStart = us - clip.timelineStartUs;
  const toEnd = clipEnd(clip) - us;
  if (fadeInUs > 0) gain *= Math.min(1, Math.max(0, fromStart / fadeInUs));
  if (fadeOutUs > 0) gain *= Math.min(1, Math.max(0, toEnd / fadeOutUs));
  return gain;
}

/** Media element volume for a gain; elements can't amplify above 1. */
export const elementVolume = (gain: number) => Math.min(1, Math.max(0, gain));
