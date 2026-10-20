import { invoke } from "@tauri-apps/api/core";

/** Mirrors `AppError` serialization in src-tauri/src/domain/errors.rs. */
export interface AppError {
  kind: string;
  message: string;
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

export const systemCheck = () => invoke<SystemCheck>("system_check");
