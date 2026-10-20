import { describe, expect, it } from "vitest";
import { formatTimecode } from "./time";

const fps30 = { numerator: 30, denominator: 1 };

describe("formatTimecode", () => {
  it("formats zero", () => {
    expect(formatTimecode(0, fps30)).toBe("00:00:00:00");
  });

  it("formats seconds and frames", () => {
    expect(formatTimecode(2_500_000, fps30)).toBe("00:00:02:15");
  });

  it("formats hours and minutes", () => {
    expect(formatTimecode(3_661_000_000, fps30)).toBe("01:01:01:00");
  });

  it("rejects negative time and invalid frame rates", () => {
    expect(() => formatTimecode(-1, fps30)).toThrow(RangeError);
    expect(() => formatTimecode(0, { numerator: 0, denominator: 1 })).toThrow(RangeError);
  });
});
