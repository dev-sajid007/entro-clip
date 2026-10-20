/** Canonical timeline time: integer microseconds (see project doc §6.1). */
export type Microseconds = number;

export const US_PER_SECOND = 1_000_000;

export interface Rational {
  numerator: number;
  denominator: number;
}

/** Formats microseconds as HH:MM:SS:FF for the given frame rate. */
export function formatTimecode(us: Microseconds, frameRate: Rational): string {
  if (!Number.isFinite(us) || us < 0) throw new RangeError(`invalid time: ${us}`);
  if (frameRate.numerator <= 0 || frameRate.denominator <= 0) {
    throw new RangeError("frame rate must be positive");
  }

  const fps = Math.round(frameRate.numerator / frameRate.denominator);
  const totalFrames = Math.floor(
    (us * frameRate.numerator) / (US_PER_SECOND * frameRate.denominator),
  );
  const frames = totalFrames % fps;
  const totalSeconds = Math.floor(totalFrames / fps);
  const pad = (n: number) => String(n).padStart(2, "0");

  return [
    Math.floor(totalSeconds / 3600),
    Math.floor(totalSeconds / 60) % 60,
    totalSeconds % 60,
    frames,
  ]
    .map(pad)
    .join(":");
}

/** Formats a duration as M:SS or H:MM:SS for display in lists. */
export function formatDuration(us: Microseconds): string {
  const totalSeconds = Math.floor(us / US_PER_SECOND);
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor(totalSeconds / 60) % 60;
  const s = String(totalSeconds % 60).padStart(2, "0");
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${s}` : `${m}:${s}`;
}

/** Formats a rational frame rate, e.g. 30/1 → "30", 30000/1001 → "29.97". */
export function formatFrameRate(rate: Rational): string {
  const fps = rate.numerator / rate.denominator;
  return Number.isInteger(fps) ? String(fps) : fps.toFixed(2);
}
