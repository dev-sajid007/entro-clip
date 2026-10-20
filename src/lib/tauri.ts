import { invoke } from "@tauri-apps/api/core";
import type { Microseconds, Rational } from "./time";

/** Mirrors `AppError` serialization in src-tauri/src/domain/errors.rs. */
export interface AppError {
  kind: string;
  message: string;
}

export function errorMessage(err: unknown): string {
  if (typeof err === "object" && err !== null && "message" in err) {
    return String((err as AppError).message);
  }
  return String(err);
}

export interface ToolStatus {
  name: string;
  available: boolean;
  version: string | null;
  error: string | null;
}

export interface SystemCheck {
  appVersion: string;
  ffmpeg: ToolStatus;
  ffprobe: ToolStatus;
}

export type AssetKind = "video" | "audio" | "image";
export type TrackKind = "video" | "audio";

export interface ProjectSettings {
  width: number;
  height: number;
  frameRate: Rational;
  audioSampleRate: number;
}

export interface MediaAsset {
  id: string;
  kind: AssetKind;
  path: string;
  name: string;
  durationUs: Microseconds | null;
  width: number | null;
  height: number | null;
  frameRate: Rational | null;
  hasAudio: boolean;
}

export interface Clip {
  id: string;
  /** Source media; null for text clips. Exactly one of assetId and text is set. */
  assetId: string | null;
  text: TextOverlay | null;
  timelineStartUs: Microseconds;
  sourceInUs: Microseconds;
  sourceOutUs: Microseconds;
  enabled: boolean;
  audio: ClipAudio;
}

/** Mirrors `TextOverlay` in src-tauri/src/domain/project.rs. */
export interface TextOverlay {
  content: string;
  /** Font size as a fraction of the frame height. */
  size: number;
  /** `#rrggbb` */
  color: string;
  /** Centre of the text block, as fractions of the frame width and height. */
  x: number;
  y: number;
  background: boolean;
}

/** Mirrors `ClipAudio` in src-tauri/src/domain/project.rs. */
export interface ClipAudio {
  gainDb: number;
  fadeInUs: Microseconds;
  fadeOutUs: Microseconds;
}

export interface Track {
  id: string;
  name: string;
  kind: TrackKind;
  order: number;
  muted: boolean;
  hidden: boolean;
  gainDb: number;
  clips: Clip[];
}

export interface Project {
  schemaVersion: number;
  id: string;
  name: string;
  settings: ProjectSettings;
  assets: MediaAsset[];
  tracks: Track[];
}

export interface ProjectState {
  project: Project;
  path: string | null;
  dirty: boolean;
  canUndo: boolean;
  canRedo: boolean;
  missingAssetIds: string[];
}

export type ImportResult =
  | { status: "imported"; path: string; assetId: string }
  | { status: "duplicate"; path: string; assetId: string }
  | { status: "failed"; path: string; error: AppError };

export interface ImportReport {
  state: ProjectState;
  results: ImportResult[];
}

/** Mirrors `Edit` in src-tauri/src/domain/edit.rs. */
export type Edit =
  | { type: "addClip"; trackId: string; assetId: string; timelineStartUs: Microseconds }
  | { type: "moveClip"; clipId: string; trackId: string; timelineStartUs: Microseconds }
  | {
      type: "trimClip";
      clipId: string;
      timelineStartUs: Microseconds;
      sourceInUs: Microseconds;
      sourceOutUs: Microseconds;
    }
  | { type: "splitClip"; clipId: string; atUs: Microseconds }
  | { type: "deleteClip"; clipId: string }
  | { type: "addText"; trackId: string; timelineStartUs: Microseconds }
  | { type: "updateText"; clipId: string; text: TextOverlay }
  | { type: "updateClipAudio"; clipId: string; gainDb?: number; fadeInUs?: Microseconds; fadeOutUs?: Microseconds }
  | { type: "addTrack"; kind: TrackKind }
  | { type: "deleteTrack"; trackId: string }
  | {
      type: "updateTrack";
      trackId: string;
      name?: string;
      muted?: boolean;
      hidden?: boolean;
      gainDb?: number;
    };

export interface EditResult {
  state: ProjectState;
  createdClipId: string | null;
}

/** Drag-and-drop payload type for assets dragged from the media library. */
export const ASSET_DRAG_TYPE = "application/x-entrocut-asset";

export const PROJECT_EXTENSION = "entrocut";

export const systemCheck = () => invoke<SystemCheck>("system_check");
export const getProject = () => invoke<ProjectState>("get_project");
export const createProject = (name?: string) => invoke<ProjectState>("create_project", { name });
export const openProject = (path: string) => invoke<ProjectState>("open_project", { path });
export const saveProject = (path?: string) => invoke<ProjectState>("save_project", { path });
export const importMedia = (paths: string[]) => invoke<ImportReport>("import_media", { paths });
export const generateThumbnail = (assetId: string) =>
  invoke<string>("generate_thumbnail", { assetId });
export const applyEdit = (edit: Edit) => invoke<EditResult>("apply_edit", { edit });
export const undoEdit = () => invoke<ProjectState>("undo_edit");
export const redoEdit = () => invoke<ProjectState>("redo_edit");

export interface PreviewFrame {
  path: string;
  clipId: string;
}

export const renderPreviewFrame = (timeUs: Microseconds) =>
  invoke<PreviewFrame | null>("render_preview_frame", { timeUs });

/** Mirrors `Quality` in src-tauri/src/domain/render_plan.rs. */
export type ExportQuality = "high" | "standard" | "small";

export interface ExportRequest {
  outputPath: string;
  /** Shorter edge in pixels; null keeps the project size. */
  shortSide: number | null;
  /** Null keeps the project frame rate. */
  frameRate: Rational | null;
  quality: ExportQuality;
  overwrite: boolean;
}

/** Mirrors `JobStatus` in src-tauri/src/jobs/manager.rs. */
export type JobStatus =
  | { state: "running"; progress: number }
  | { state: "completed"; outputPath: string }
  | { state: "failed"; error: AppError }
  | { state: "cancelled" };

/** Event names and payloads from src-tauri/src/jobs/events.rs. */
export const EXPORT_PROGRESS = "export-progress";
export const EXPORT_COMPLETED = "export-completed";
/** Also sent on cancellation, with error kind `cancelled`. */
export const JOB_FAILED = "job-failed";

export interface ExportProgressEvent {
  jobId: string;
  progress: number;
  outTimeUs: Microseconds;
}

export interface ExportCompletedEvent {
  jobId: string;
  outputPath: string;
}

export interface JobFailedEvent {
  jobId: string;
  error: AppError;
}

export const startExport = (request: ExportRequest) => invoke<string>("start_export", { request });
export const cancelJob = (jobId: string) => invoke<void>("cancel_job", { jobId });
export const getJobStatus = (jobId: string) => invoke<JobStatus>("get_job_status", { jobId });
