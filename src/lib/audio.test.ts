import { describe, expect, it } from "vitest";
import type { Clip, Track } from "./tauri";
import { clipGainAt, dbToGain, elementVolume, formatDb } from "./audio";

const SEC = 1_000_000;

function clip(audio: Partial<Clip["audio"]> = {}): Clip {
  return {
    id: "c", assetId: "a", timelineStartUs: 10 * SEC, sourceInUs: 0, sourceOutUs: 4 * SEC, enabled: true, text: null,
    audio: { gainDb: 0, fadeInUs: 0, fadeOutUs: 0, ...audio },
  };
}

const track = (gainDb = 0): Track => ({
  id: "t", name: "t", kind: "audio", order: 0, muted: false, hidden: false, gainDb, clips: [],
});

describe("clipGainAt", () => {
  it("is unity with default settings", () => {
    expect(clipGainAt(clip(), track(), 11 * SEC)).toBe(1);
  });

  it("adds clip and track gain in decibels", () => {
    expect(clipGainAt(clip({ gainDb: -3 }), track(-3), 11 * SEC)).toBeCloseTo(dbToGain(-6));
  });

  it("ramps linearly through fades", () => {
    const c = clip({ fadeInUs: SEC, fadeOutUs: 2 * SEC });
    expect(clipGainAt(c, track(), 10 * SEC)).toBe(0);
    expect(clipGainAt(c, track(), 10.5 * SEC)).toBeCloseTo(0.5);
    expect(clipGainAt(c, track(), 12 * SEC)).toBe(1);
    // Clip ends at 14 s; 1 s before the end is halfway through a 2 s fade.
    expect(clipGainAt(c, track(), 13 * SEC)).toBeCloseTo(0.5);
  });
});

describe("helpers", () => {
  it("caps element volume at 1", () => {
    expect(elementVolume(dbToGain(6))).toBe(1);
    expect(elementVolume(0.25)).toBe(0.25);
  });

  it("formats decibels", () => {
    expect(formatDb(0)).toBe("0.0 dB");
    expect(formatDb(3.25)).toBe("+3.3 dB");
    expect(formatDb(-6)).toBe("−6.0 dB");
    expect(formatDb(-60)).toBe("−∞ dB");
  });
});
