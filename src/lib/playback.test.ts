import { describe, expect, it } from "vitest";
import type { Clip, MediaAsset, Project, Track } from "./tauri";
import { activeAt, sourceSecondsAt } from "./playback";

const SEC = 1_000_000;

const asset = (id: string, kind: MediaAsset["kind"], hasAudio: boolean): MediaAsset => ({
  id, kind, path: `/m/${id}`, name: id, durationUs: 60 * SEC, width: null, height: null, frameRate: null, hasAudio,
});
const clip = (id: string, assetId: string, start: number, len: number): Clip => ({
  id, assetId, timelineStartUs: start, sourceInUs: SEC, sourceOutUs: SEC + len, enabled: true,
});
const track = (id: string, kind: Track["kind"], order: number, clips: Clip[], flags: Partial<Track> = {}): Track => ({
  id, name: id, kind, order, muted: false, hidden: false, clips, ...flags,
});

function project(tracks: Track[]): Project {
  return {
    schemaVersion: 1,
    id: "p",
    name: "p",
    settings: { width: 1920, height: 1080, frameRate: { numerator: 30, denominator: 1 }, audioSampleRate: 48000 },
    assets: [asset("v", "video", true), asset("o", "video", true), asset("m", "audio", true)],
    tracks,
  };
}

describe("activeAt", () => {
  const p = project([
    track("overlay", "video", 0, [clip("c-o", "o", 2 * SEC, SEC)]),
    track("main", "video", 1, [clip("c-v", "v", 0, 5 * SEC)]),
    track("music", "audio", 2, [clip("c-m", "m", 0, 10 * SEC)]),
  ]);

  it("picks the top-most visual and mixes the rest", () => {
    const { visual, audio } = activeAt(p, 2.5 * SEC, new Set());
    expect(visual?.clip.id).toBe("c-o");
    expect(audio.map((a) => a.clip.id)).toEqual(["c-v", "c-m"]);
  });

  it("skips hidden, muted and missing media", () => {
    const q = project([
      track("overlay", "video", 0, [clip("c-o", "o", 0, SEC)], { hidden: true }),
      track("main", "video", 1, [clip("c-v", "v", 0, SEC)]),
      track("music", "audio", 2, [clip("c-m", "m", 0, SEC)], { muted: true }),
    ]);
    expect(activeAt(q, 0, new Set()).visual?.clip.id).toBe("c-v");
    expect(activeAt(q, 0, new Set()).audio).toEqual([]);
    expect(activeAt(q, 0, new Set(["v"])).visual).toBeNull();
  });

  it("treats clip end as exclusive", () => {
    expect(activeAt(p, 5 * SEC, new Set()).visual).toBeNull();
  });
});

describe("sourceSecondsAt", () => {
  it("maps timeline time to source time", () => {
    expect(sourceSecondsAt(clip("c", "v", 2 * SEC, SEC), 2.5 * SEC)).toBe(1.5);
  });
});
