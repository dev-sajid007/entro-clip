import { useEffect, useMemo, useState } from "react";
import { systemCheck, type SystemCheck } from "../lib/tauri";
import { formatTimecode } from "../lib/time";
import { useProject } from "../features/project/useProject";
import { MediaLibrary } from "../components/media-library/MediaLibrary";
import { Inspector } from "../components/inspector/Inspector";

function ToolBadge({ label, ok, title }: { label: string; ok: boolean; title: string }) {
  return (
    <span className={`badge ${ok ? "ok" : "bad"}`} title={title}>
      {label} {ok ? "ready" : "missing"}
    </span>
  );
}

function isTypingTarget(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLElement &&
    (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))
  );
}

export default function App() {
  const [check, setCheck] = useState<SystemCheck | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const project = useProject();
  const { state } = project;

  useEffect(() => {
    systemCheck().then(setCheck).catch(() => setCheck(null));
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.altKey || isTypingTarget(e.target)) return;
      const key = e.key.toLowerCase();
      const action =
        key === "s" ? (e.shiftKey ? project.saveAs : project.save)
        : key === "o" ? project.openProject
        : key === "n" ? project.newProject
        : key === "i" ? project.importFiles
        : null;
      if (action && !project.busy) {
        e.preventDefault();
        action();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [project]);

  const missingIds = useMemo(() => new Set(state?.missingAssetIds ?? []), [state?.missingAssetIds]);
  const selected = state?.project.assets.find((a) => a.id === selectedId) ?? null;
  const fileLabel = state?.path?.split(/[\\/]/).pop();

  return (
    <div className="editor">
      <header className="topbar">
        <strong>EntroCut</strong>
        <div className="toolbar" role="toolbar" aria-label="Project">
          <button type="button" className="ghost" onClick={project.newProject} disabled={project.busy} title="Ctrl+N">New</button>
          <button type="button" className="ghost" onClick={project.openProject} disabled={project.busy} title="Ctrl+O">Open</button>
          <button type="button" className="ghost" onClick={project.save} disabled={project.busy || !state} title="Ctrl+S">Save</button>
          <button type="button" className="ghost" onClick={project.saveAs} disabled={project.busy || !state} title="Ctrl+Shift+S">Save As</button>
        </div>
        <span className="project-title" title={state?.path ?? "Not saved yet"}>
          {state?.project.name ?? "…"}
          {fileLabel && <span className="muted"> — {fileLabel}</span>}
          {state?.dirty && <span className="dirty" aria-label="Unsaved changes"> •</span>}
        </span>
        <div className="spacer" />
        {check && (
          <>
            <ToolBadge label="FFmpeg" ok={check.ffmpeg.available} title={check.ffmpeg.version ?? check.ffmpeg.error ?? ""} />
            <ToolBadge label="FFprobe" ok={check.ffprobe.available} title={check.ffprobe.version ?? check.ffprobe.error ?? ""} />
          </>
        )}
        <button type="button" disabled>Export</button>
      </header>

      <aside className="panel library" aria-label="Media library">
        <MediaLibrary
          assets={state?.project.assets ?? []}
          missingIds={missingIds}
          thumbnails={project.thumbnails}
          selectedId={selectedId}
          busy={project.busy}
          onSelect={setSelectedId}
          onImport={project.importFiles}
        />
      </aside>

      <section className="panel preview" aria-label="Preview">
        {project.notice && (
          <div className={`notice ${project.notice.kind}`} role="status">
            <span>{project.notice.text}</span>
            <button type="button" className="ghost" onClick={project.dismissNotice} aria-label="Dismiss">×</button>
          </div>
        )}
        <div className="monitor" />
        <div className="timecode">
          {formatTimecode(0, state?.project.settings.frameRate ?? { numerator: 30, denominator: 1 })}
        </div>
      </section>

      <aside className="panel inspector" aria-label="Inspector">
        <Inspector asset={selected} missing={selected ? missingIds.has(selected.id) : false} settings={state?.project.settings ?? null} />
      </aside>

      <section className="panel timeline" aria-label="Timeline">
        {(state?.project.tracks ?? []).map((track) => (
          <div key={track.id} className="track">
            <span>{track.name}</span>
          </div>
        ))}
      </section>
    </div>
  );
}
