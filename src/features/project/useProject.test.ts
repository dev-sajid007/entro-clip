import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ ask: vi.fn(), open: vi.fn(), save: vi.fn() }));

const { summarizeImport } = await import("./useProject");

describe("summarizeImport", () => {
  it("returns null for an empty import", () => {
    expect(summarizeImport([])).toBeNull();
  });

  it("reports successes as info", () => {
    expect(
      summarizeImport([{ status: "imported", path: "/a.mp4", assetId: "1" }]),
    ).toEqual({ kind: "info", text: "Imported 1 file." });
  });

  it("lists duplicates and failures with file names", () => {
    const notice = summarizeImport([
      { status: "imported", path: "/a.mp4", assetId: "1" },
      { status: "imported", path: "/b.mp4", assetId: "2" },
      { status: "duplicate", path: "/a.mp4", assetId: "1" },
      {
        status: "failed",
        path: "/docs/notes.txt",
        error: { kind: "unsupportedMedia", message: "unsupported media: no streams" },
      },
    ]);
    expect(notice?.kind).toBe("error");
    expect(notice?.text).toBe(
      "Imported 2 files, 1 already in project, 1 failed.\nnotes.txt: unsupported media: no streams",
    );
  });
});
