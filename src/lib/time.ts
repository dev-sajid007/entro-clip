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
