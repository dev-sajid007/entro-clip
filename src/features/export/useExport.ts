import { useCallback, useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ask, save } from "@tauri-apps/plugin-dialog";
import {
  EXPORT_COMPLETED,
  EXPORT_PROGRESS,
  JOB_FAILED,
  cancelJob,
  errorMessage,
  getJobStatus,
  startExport,
  type AppError,
  type ExportCompletedEvent,
  type ExportProgressEvent,
  type ExportQuality,
  type ExportRequest,
  type JobFailedEvent,
  type JobStatus,
} from "../../lib/tauri";
import { applyJobStatus, exportFileName, type ExportPreset, type ExportState } from "../../lib/export";

const MP4_FILTER = { name: "MP4 video", extensions: ["mp4"] };

function isAlreadyExists(err: unknown): boolean {
  return typeof err === "object" && err !== null && (err as AppError).kind === "alreadyExists";
}

/**
 * Export job state for the UI. The backend runs the render and reports
 * progress through events; this hook follows the one job it started.
 */
export function useExport() {
  const [state, setState] = useState<ExportState>({ phase: "idle" });
  const stateRef = useRef(state);
  stateRef.current = state;

  useEffect(() => {
    const apply = (jobId: string, status: JobStatus) => setState((s) => applyJobStatus(s, jobId, status));
    const subscriptions = [
      listen<ExportProgressEvent>(EXPORT_PROGRESS, ({ payload }) =>
        apply(payload.jobId, { state: "running", progress: payload.progress }),
      ),
      listen<ExportCompletedEvent>(EXPORT_COMPLETED, ({ payload }) =>
        apply(payload.jobId, { state: "completed", outputPath: payload.outputPath }),
      ),
      listen<JobFailedEvent>(JOB_FAILED, ({ payload }) => apply(payload.jobId, { state: "failed", error: payload.error })),
    ];
    return () => {
      for (const unlisten of subscriptions) unlisten.then((fn) => fn());
    };
  }, []);

  /** Asks for a destination, then starts rendering the current project. */
  const start = useCallback(async (projectName: string, preset: ExportPreset, quality: ExportQuality) => {
    const path = await save({ defaultPath: exportFileName(projectName), filters: [MP4_FILTER] });
    if (!path) return;
    setState({ phase: "starting" });

    // The save dialog already confirmed replacing this exact path. If the
    // backend had to add ".mp4", the final name still needs confirming.
    const request: ExportRequest = {
      outputPath: path,
      shortSide: preset.shortSide,
      frameRate: preset.frameRate,
      quality,
      overwrite: /\.mp4$/i.test(path),
    };
    try {
      let jobId: string;
      try {
        jobId = await startExport(request);
      } catch (err) {
        if (!isAlreadyExists(err)) throw err;
        const replace = await ask(`${errorMessage(err)}. Replace it?`, {
          title: "Replace file",
          kind: "warning",
          okLabel: "Replace",
          cancelLabel: "Cancel",
        });
        if (!replace) {
          setState({ phase: "idle" });
          return;
        }
        jobId = await startExport({ ...request, overwrite: true });
      }
      setState({ phase: "running", jobId, progress: 0 });
      // Events sent before the job id was known were ignored; catch up.
      const status = await getJobStatus(jobId).catch(() => null);
      if (status) setState((s) => applyJobStatus(s, jobId, status));
    } catch (err) {
      setState({ phase: "failed", message: errorMessage(err) });
    }
  }, []);

  /** Requests cancellation; the job-failed event confirms it. */
  const cancel = useCallback(async () => {
    const current = stateRef.current;
    if (current.phase !== "running") return;
    try {
      await cancelJob(current.jobId);
    } catch (err) {
      setState({ phase: "failed", message: errorMessage(err) });
    }
  }, []);

  const reset = useCallback(() => {
    setState((s) => (s.phase === "running" || s.phase === "starting" ? s : { phase: "idle" }));
  }, []);

  /** Confirms quitting during an export, cancelling it if the user agrees. */
  const confirmQuit = useCallback(async () => {
    const current = stateRef.current;
    if (current.phase !== "running") return true;
    const quit = await ask("An export is still running. Cancel it and quit?", {
      title: "Export in progress",
      kind: "warning",
      okLabel: "Cancel export and quit",
      cancelLabel: "Keep exporting",
    });
    if (quit) await cancelJob(current.jobId).catch(() => undefined);
    return quit;
  }, []);

  return { state, start, cancel, reset, confirmQuit };
}
