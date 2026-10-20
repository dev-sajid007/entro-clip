import { describe, expect, it } from "vitest";
import { applyJobStatus, exportFileName, type ExportState } from "./export";

const running: ExportState = { phase: "running", jobId: "a", progress: 0.4 };

describe("exportFileName", () => {
  it("strips unsafe characters and adds .mp4", () => {
    expect(exportFileName("My: Edit/v2")).toBe("My Editv2.mp4");
  });

  it("falls back for empty names", () => {
    expect(exportFileName("  ")).toBe("Untitled.mp4");
  });
});

describe("applyJobStatus", () => {
  it("updates progress without going backwards", () => {
    expect(applyJobStatus(running, "a", { state: "running", progress: 0.6 })).toEqual({ ...running, progress: 0.6 });
    expect(applyJobStatus(running, "a", { state: "running", progress: 0.1 })).toEqual(running);
  });

  it("ignores other jobs", () => {
    expect(applyJobStatus(running, "b", { state: "completed", outputPath: "/x.mp4" })).toBe(running);
  });

  it("finishes the job", () => {
    expect(applyJobStatus(running, "a", { state: "completed", outputPath: "/x.mp4" })).toEqual({
      phase: "completed",
      jobId: "a",
      outputPath: "/x.mp4",
    });
  });

  it("treats a cancelled error as cancellation", () => {
    const status = { state: "failed", error: { kind: "cancelled", message: "cancelled" } } as const;
    expect(applyJobStatus(running, "a", status)).toEqual({ phase: "cancelled" });
  });

  it("reports failures", () => {
    const status = { state: "failed", error: { kind: "toolFailed", message: "boom" } } as const;
    expect(applyJobStatus(running, "a", status)).toEqual({ phase: "failed", message: "boom" });
  });

  it("never reopens a finished job", () => {
    const done: ExportState = { phase: "completed", jobId: "a", outputPath: "/x.mp4" };
    expect(applyJobStatus(done, "a", { state: "running", progress: 0.5 })).toBe(done);
  });
});
