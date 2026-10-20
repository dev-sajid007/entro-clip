import { describe, expect, it } from "vitest";
import type { Clip, Track } from "./tauri";
import {
  clipEnd,
  findSnap,
  freeVideoTrack,
  formatRulerLabel,
  frameIndex,
  overlapsTrack,
  pxToUs,
  snapCandidates,
  snapToFrame,
  tickInterval,
  timelineEnd,
  trackAccepts,
  usToPx,
} from "./timeline";

const SEC = 1_000_000;

function clip(id: string, start: number, inUs: number, outUs: number): Clip {
  return {
    id, assetId: "a", timelineStartUs: start, sourceInUs: inUs, sourceOutUs: outUs, enabled: true, text: null,
    audio: { gainDb: 0, fadeInUs: 0, fadeOutUs: 0 },
  };
}

function track(kind: Track["kind"], clips: Clip[]): Track {
  return { id: `t-${kind}`, name: kind, kind, order: 0, muted: false, hidden: false, gainDb: 0, clips };
}

describe("pixel/time conversion", () => {
  it("round-trips at any zoom", () => {
    for (const zoom of [5, 37, 100, 400]) {
      expect(pxToUs(usToPx(2_500_000, zoom), zoom)).toBe(2_500_000);
    }
  });

  it("is linear in zoom", () => {
    expect(usToPx(SEC, 100)).toBe(100);
    expect(usToPx(SEC, 50)).toBe(50);
    expect(pxToUs(25, 50)).toBe(SEC / 2);
  });
});

describe("frame snapping", () => {
  it("snaps to integer frame rates", () => {
    const fps30 = { numerator: 30, denominator: 1 };
    expect(snapToFrame(1_010_000, fps30)).toBe(1_000_000);
    expect(snapToFrame(1_020_000, fps30)).toBe(1_033_333);
    expect(frameIndex(1_000_000, fps30)).toBe(30);
  });

  it("snaps to NTSC frame rates", () => {
    const ntsc = { numerator: 30000, denominator: 1001 };
    expect(snapToFrame(SEC, ntsc)).toBe(1_001_000);
    expect(frameIndex(1_001_000, ntsc)).toBe(30);
  });
});

describe("findSnap", () => {
  it("returns the nearest candidate within the threshold", () => {
    expect(findSnap(1_040_000, [0, SEC, 2 * SEC], 50_000)).toBe(SEC);
    expect(findSnap(1_500_000, [0, SEC, 2 * SEC], 50_000)).toBeNull();
  });
});

describe("track helpers", () => {
  const t = track("video", [clip("a", 0, 0, 2 * SEC), clip("b", 3 * SEC, SEC, 4 * SEC)]);

  it("computes clip and timeline ends", () => {
    expect(clipEnd(t.clips[1])).toBe(6 * SEC);
    expect(timelineEnd([t, track("audio", [])])).toBe(6 * SEC);
  });

  it("detects overlap but allows touching clips", () => {
    expect(overlapsTrack(t, 2 * SEC, SEC)).toBe(false);
    expect(overlapsTrack(t, 1.5 * SEC, SEC)).toBe(true);
    expect(overlapsTrack(t, 0, SEC, "a")).toBe(false);
  });

  it("collects snap candidates excluding the dragged clip", () => {
    expect(snapCandidates([t], 7 * SEC, "a")).toEqual([0, 7 * SEC, 3 * SEC, 6 * SEC]);
  });

  it("matches backend track compatibility", () => {
    expect(trackAccepts(track("video", []), "image", false)).toBe(true);
    expect(trackAccepts(track("video", []), "audio", true)).toBe(false);
    expect(trackAccepts(track("audio", []), "video", true)).toBe(true);
    expect(trackAccepts(track("audio", []), "video", false)).toBe(false);
    expect(trackAccepts(track("video", []), "text", false)).toBe(true);
    expect(trackAccepts(track("audio", []), "text", false)).toBe(false);
  });

  it("finds the top-most video track with room", () => {
    const busy = { ...track("video", [clip("a", 0, 0, 5 * SEC)]), id: "busy" };
    const free = { ...track("video", []), id: "free" };
    expect(freeVideoTrack([track("audio", []), busy, free], SEC, SEC)?.id).toBe("free");
    expect(freeVideoTrack([busy, free], 6 * SEC, SEC)?.id).toBe("busy");
    expect(freeVideoTrack([busy], 4 * SEC, SEC)).toBeNull();
  });
});

describe("ruler", () => {
  it("chooses wider ticks when zoomed out", () => {
    expect(tickInterval(100)).toBe(SEC);
    expect(tickInterval(10)).toBe(10 * SEC);
    expect(tickInterval(400)).toBe(250_000);
  });

  it("formats labels", () => {
    expect(formatRulerLabel(65 * SEC)).toBe("1:05");
    expect(formatRulerLabel(1_500_000)).toBe("0:01.50");
  });
});
