import { describe, expect, it } from "vitest";
import { formatDuration, formatFrameRate, formatTimecode } from "./time";

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

describe("formatDuration", () => {
  it("formats minutes and seconds", () => {
    expect(formatDuration(0)).toBe("0:00");
    expect(formatDuration(12_500_000)).toBe("0:12");
    expect(formatDuration(125_000_000)).toBe("2:05");
  });

  it("includes hours when needed", () => {
    expect(formatDuration(3_725_000_000)).toBe("1:02:05");
  });
});

describe("formatFrameRate", () => {
  it("formats integer and NTSC rates", () => {
    expect(formatFrameRate({ numerator: 30, denominator: 1 })).toBe("30");
    expect(formatFrameRate({ numerator: 30000, denominator: 1001 })).toBe("29.97");
  });
});
