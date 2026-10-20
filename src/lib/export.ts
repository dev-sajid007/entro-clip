import type { ExportQuality, JobStatus } from "./tauri";
import type { Rational } from "./time";

export interface ExportPreset {
  id: string;
  label: string;
  shortSide: number | null;
  frameRate: Rational | null;
}

/** Initial presets from the project doc §10.5, plus the project's own settings. */
export const EXPORT_PRESETS: ExportPreset[] = [
  { id: "project", label: "Project settings", shortSide: null, frameRate: null },
  { id: "720p30", label: "720p, 30 fps", shortSide: 720, frameRate: { numerator: 30, denominator: 1 } },
  { id: "1080p30", label: "1080p, 30 fps", shortSide: 1080, frameRate: { numerator: 30, denominator: 1 } },
  { id: "1080p60", label: "1080p, 60 fps", shortSide: 1080, frameRate: { numerator: 60, denominator: 1 } },
];

export const EXPORT_QUALITIES: { id: ExportQuality; label: string }[] = [
  { id: "high", label: "High" },
  { id: "standard", label: "Standard" },
  { id: "small", label: "Small file" },
];

/** Suggested output file name: the project name without path-unsafe characters. */
export function exportFileName(projectName: string): string {
  const base = projectName.replace(/[\\/:*?"<>|]/g, "").trim() || "Untitled";
  return `${base}.mp4`;
}

export type ExportState =
  | { phase: "idle" }
  | { phase: "starting" }
  | { phase: "running"; jobId: string; progress: number }
  | { phase: "completed"; jobId: string; outputPath: string }
  | { phase: "failed"; message: string }
  | { phase: "cancelled" };

/**
 * Applies a job status (from an event or a status query) to the export state.
 * Only the running job is updated, and a finished job never goes back to
 * running, so late or duplicate updates are harmless.
 */
export function applyJobStatus(state: ExportState, jobId: string, status: JobStatus): ExportState {
  if (state.phase !== "running" || state.jobId !== jobId) return state;
  switch (status.state) {
    case "running":
      return { ...state, progress: Math.max(state.progress, Math.min(1, status.progress)) };
    case "completed":
      return { phase: "completed", jobId, outputPath: status.outputPath };
    case "cancelled":
      return { phase: "cancelled" };
    case "failed":
      return status.error.kind === "cancelled"
        ? { phase: "cancelled" }
        : { phase: "failed", message: status.error.message };
  }
}
