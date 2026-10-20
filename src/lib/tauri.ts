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
  assetId: string;
  timelineStartUs: Microseconds;
  sourceInUs: Microseconds;
  sourceOutUs: Microseconds;
  enabled: boolean;
}

export interface Track {
  id: string;
  name: string;
  kind: TrackKind;
  order: number;
  muted: boolean;
  hidden: boolean;
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

export const PROJECT_EXTENSION = "entrocut";

export const systemCheck = () => invoke<SystemCheck>("system_check");
export const getProject = () => invoke<ProjectState>("get_project");
export const createProject = (name?: string) => invoke<ProjectState>("create_project", { name });
export const openProject = (path: string) => invoke<ProjectState>("open_project", { path });
export const saveProject = (path?: string) => invoke<ProjectState>("save_project", { path });
export const importMedia = (paths: string[]) => invoke<ImportReport>("import_media", { paths });
export const generateThumbnail = (assetId: string) =>
  invoke<string>("generate_thumbnail", { assetId });
