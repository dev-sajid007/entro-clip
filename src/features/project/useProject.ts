import { useCallback, useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ask, open, save } from "@tauri-apps/plugin-dialog";
import {
  PROJECT_EXTENSION,
  createProject,
  errorMessage,
  generateThumbnail,
  getProject,
  importMedia,
  openProject,
  saveProject,
  type ImportResult,
  type ProjectState,
} from "../../lib/tauri";

export type Thumbnail =
  | { status: "loading" }
  | { status: "ready"; src: string }
  | { status: "error"; message: string };

export interface Notice {
  kind: "info" | "error";
  text: string;
}

const PROJECT_FILTER = { name: "EntroCut project", extensions: [PROJECT_EXTENSION] };
const MEDIA_FILTER = {
  name: "Media",
  extensions: [
    "mp4", "mov", "mkv", "webm", "avi", "m4v",
    "mp3", "wav", "aac", "m4a", "flac", "ogg", "opus",
    "png", "jpg", "jpeg", "webp", "bmp", "gif",
  ],
};

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

export function summarizeImport(results: ImportResult[]): Notice | null {
  if (results.length === 0) return null;
  const imported = results.filter((r) => r.status === "imported").length;
  const duplicates = results.filter((r) => r.status === "duplicate").length;
  const failures = results.filter((r) => r.status === "failed");

  const parts = [`Imported ${imported} file${imported === 1 ? "" : "s"}`];
  if (duplicates > 0) parts.push(`${duplicates} already in project`);
  if (failures.length > 0) parts.push(`${failures.length} failed`);

  const lines = [parts.join(", ") + "."];
  for (const f of failures) {
    if (f.status === "failed") lines.push(`${fileName(f.path)}: ${f.error.message}`);
  }
  return { kind: failures.length > 0 ? "error" : "info", text: lines.join("\n") };
}

/**
 * Project session for the editor UI. The Rust backend owns the canonical
 * project; this hook holds the latest snapshot plus transient UI state.
 */
export function useProject() {
  const [state, setState] = useState<ProjectState | null>(null);
  const [thumbnails, setThumbnails] = useState<Record<string, Thumbnail>>({});
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<Notice | null>(null);

  const dirtyRef = useRef(false);
  dirtyRef.current = state?.dirty ?? false;
  const requestedThumbs = useRef(new Set<string>());

  const run = useCallback(async (task: () => Promise<void>) => {
    setBusy(true);
    try {
      await task();
    } catch (err) {
      setNotice({ kind: "error", text: errorMessage(err) });
    } finally {
      setBusy(false);
    }
  }, []);

  const confirmDiscard = useCallback(async () => {
    if (!dirtyRef.current) return true;
    return ask("You have unsaved changes. Discard them?", {
      title: "Unsaved changes",
      kind: "warning",
      okLabel: "Discard",
      cancelLabel: "Cancel",
    });
  }, []);

  useEffect(() => {
    getProject()
      .then(setState)
      .catch((err) => setNotice({ kind: "error", text: errorMessage(err) }));
  }, []);

  useEffect(() => {
    const unlisten = getCurrentWindow().onCloseRequested(async (event) => {
      if (!(await confirmDiscard())) event.preventDefault();
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [confirmDiscard]);

  // Request thumbnails for visual assets that don't have one yet.
  useEffect(() => {
    if (!state) return;
    const missing = new Set(state.missingAssetIds);
    for (const asset of state.project.assets) {
      if (asset.kind === "audio" || missing.has(asset.id)) continue;
      if (requestedThumbs.current.has(asset.id)) continue;
      requestedThumbs.current.add(asset.id);

      setThumbnails((t) => ({ ...t, [asset.id]: { status: "loading" } }));
      generateThumbnail(asset.id)
        .then((path) =>
          setThumbnails((t) => ({ ...t, [asset.id]: { status: "ready", src: convertFileSrc(path) } })),
        )
        .catch((err) =>
          setThumbnails((t) => ({ ...t, [asset.id]: { status: "error", message: errorMessage(err) } })),
        );
    }
  }, [state]);

  const newProject = useCallback(
    () =>
      run(async () => {
        if (!(await confirmDiscard())) return;
        setState(await createProject());
        setNotice(null);
      }),
    [run, confirmDiscard],
  );

  const openProjectFile = useCallback(
    () =>
      run(async () => {
        if (!(await confirmDiscard())) return;
        const path = await open({ multiple: false, directory: false, filters: [PROJECT_FILTER] });
        if (!path) return;
        setState(await openProject(path));
        setNotice(null);
      }),
    [run, confirmDiscard],
  );

  const saveAs = useCallback(
    () =>
      run(async () => {
        const name = state?.project.name ?? "Untitled";
        const path = await save({ defaultPath: `${name}.${PROJECT_EXTENSION}`, filters: [PROJECT_FILTER] });
        if (!path) return;
        setState(await saveProject(path));
        setNotice({ kind: "info", text: "Project saved." });
      }),
    [run, state?.project.name],
  );

  const saveCurrent = useCallback(() => {
    if (!state?.path) return saveAs();
    return run(async () => {
      setState(await saveProject());
      setNotice({ kind: "info", text: "Project saved." });
    });
  }, [run, saveAs, state?.path]);

  const importFiles = useCallback(
    () =>
      run(async () => {
        const picked = await open({
          multiple: true,
          directory: false,
          filters: [MEDIA_FILTER, { name: "All files", extensions: ["*"] }],
        });
        if (!picked || picked.length === 0) return;
        const report = await importMedia(picked);
        setState(report.state);
        setNotice(summarizeImport(report.results));
      }),
    [run],
  );

  return {
    state,
    thumbnails,
    busy,
    notice,
    dismissNotice: () => setNotice(null),
    newProject,
    openProject: openProjectFile,
    save: saveCurrent,
    saveAs,
    importFiles,
  };
}
