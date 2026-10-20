import { useCallback, useEffect, useMemo, useState } from "react";
import { systemCheck, type MediaAsset, type SystemCheck } from "../lib/tauri";
import { US_PER_SECOND, formatTimecode } from "../lib/time";
import { frameDurationUs, snapToFrame, timelineEnd, trackAccepts, trackEnd } from "../lib/timeline";
import { useProject } from "../features/project/useProject";
import { MediaLibrary } from "../components/media-library/MediaLibrary";
import { Inspector } from "../components/inspector/Inspector";
import { Timeline, sortedTracks } from "../components/timeline/Timeline";
import { PreviewMonitor } from "../components/preview/PreviewMonitor";
import { usePlayback } from "../features/playback/usePlayback";

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

const DEFAULT_RATE = { numerator: 30, denominator: 1 };

export default function App() {
  const [check, setCheck] = useState<SystemCheck | null>(null);
  const [selectedAssetId, setSelectedAssetId] = useState<string | null>(null);
  const [selectedClipId, setSelectedClipId] = useState<string | null>(null);
  const [playheadUs, setPlayheadUs] = useState(0);
  const project = useProject();
  const { state } = project;
  const frameRate = state?.project.settings.frameRate ?? DEFAULT_RATE;

  useEffect(() => {
    systemCheck().then(setCheck).catch(() => setCheck(null));
  }, []);

  const tracks = useMemo(() => (state ? sortedTracks(state.project) : []), [state]);
  const selectedClip = tracks.flatMap((t) => t.clips).find((c) => c.id === selectedClipId) ?? null;
  const selectedAsset =
    state?.project.assets.find((a) => a.id === (selectedClip?.assetId ?? selectedAssetId)) ?? null;
  const missingIds = useMemo(() => new Set(state?.missingAssetIds ?? []), [state?.missingAssetIds]);
  const fileLabel = state?.path?.split(/[\\/]/).pop();

  const selectAsset = (id: string) => {
    setSelectedAssetId(id);
    setSelectedClipId(null);
  };
  const selectClip = (id: string | null) => {
    setSelectedClipId(id);
    if (id) setSelectedAssetId(null);
  };

  const endUs = timelineEnd(tracks);
  const setPlayhead = useCallback((us: number) => setPlayheadUs(us), []);
  const playback = usePlayback(playheadUs, setPlayhead, endUs);
  const { seek } = playback;

  const addToTimeline = async (asset: MediaAsset) => {
    const track = tracks.find((t) => trackAccepts(t, asset.kind, asset.hasAudio));
    if (!track) {
      project.notify(`Add a ${asset.kind === "audio" ? "audio" : "video"} track first.`);
      return;
    }
    const id = await project.edit({
      type: "addClip",
      trackId: track.id,
      assetId: asset.id,
      timelineStartUs: trackEnd(track),
    });
    if (id) selectClip(id);
  };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isTypingTarget(e.target) || e.altKey) return;
      const key = e.key.toLowerCase();
      const ctrl = e.ctrlKey || e.metaKey;
      let action: (() => unknown) | null = null;

      if (ctrl) {
        if (key === "z") action = e.shiftKey ? project.redo : project.undo;
        else if (key === "y") action = project.redo;
        else if (project.busy) return;
        else if (key === "s") action = e.shiftKey ? project.saveAs : project.save;
        else if (key === "o") action = project.openProject;
        else if (key === "n") action = project.newProject;
        else if (key === "i") action = project.importFiles;
      } else if ((key === "delete" || key === "backspace") && selectedClip) {
        action = () => {
          setSelectedClipId(null);
          return project.edit({ type: "deleteClip", clipId: selectedClip.id });
        };
      } else if (key === "s" && selectedClip) {
        action = () => project.edit({ type: "splitClip", clipId: selectedClip.id, atUs: playheadUs });
      } else if (key === "arrowleft" || key === "arrowright") {
        const step = e.shiftKey ? US_PER_SECOND : frameDurationUs(frameRate);
        const dir = key === "arrowleft" ? -1 : 1;
        action = () => seek(snapToFrame(playheadUs + dir * step, frameRate));
      } else if (key === "home") {
        action = () => seek(0);
      } else if (key === "end") {
        action = () => seek(endUs);
      } else if (key === " " || key === "k") {
        action = playback.toggle;
      } else if (key === "escape") {
        action = () => selectClip(null);
      }

      if (action) {
        e.preventDefault();
        void action();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  return (
    <div className="editor">
      <header className="topbar">
        <strong>EntroCut</strong>
        <div className="toolbar" role="toolbar" aria-label="Project">
          <button type="button" className="ghost" onClick={project.newProject} disabled={project.busy} title="Ctrl+N">New</button>
          <button type="button" className="ghost" onClick={project.openProject} disabled={project.busy} title="Ctrl+O">Open</button>
          <button type="button" className="ghost" onClick={project.save} disabled={project.busy || !state} title="Ctrl+S">Save</button>
          <button type="button" className="ghost" onClick={project.saveAs} disabled={project.busy || !state} title="Ctrl+Shift+S">Save As</button>
          <span className="divider" />
          <button type="button" className="ghost" onClick={project.undo} disabled={!state?.canUndo} title="Ctrl+Z">Undo</button>
          <button type="button" className="ghost" onClick={project.redo} disabled={!state?.canRedo} title="Ctrl+Shift+Z">Redo</button>
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
          selectedId={selectedClip ? null : selectedAssetId}
          busy={project.busy}
          onSelect={selectAsset}
          onImport={project.importFiles}
          onAddToTimeline={addToTimeline}
        />
      </aside>

      <section className="panel preview" aria-label="Preview">
        {project.notice && (
          <div className={`notice ${project.notice.kind}`} role="status">
            <span>{project.notice.text}</span>
            <button type="button" className="ghost" onClick={project.dismissNotice} aria-label="Dismiss">×</button>
          </div>
        )}
        {state && (
          <PreviewMonitor
            project={state.project}
            playheadUs={playheadUs}
            playing={playback.playing}
            missingIds={missingIds}
          />
        )}
        <div className="transport">
          <button type="button" className="ghost" onClick={() => seek(0)} title="Go to start (Home)" aria-label="Go to start">⏮</button>
          <button
            type="button"
            className="play"
            onClick={playback.toggle}
            disabled={endUs === 0}
            title="Play/Pause (Space)"
            aria-label={playback.playing ? "Pause" : "Play"}
          >
            {playback.playing ? "❚❚" : "▶"}
          </button>
          <button type="button" className="ghost" onClick={() => seek(endUs)} title="Go to end (End)" aria-label="Go to end">⏭</button>
          <span className="timecode">
            {formatTimecode(playheadUs, frameRate)}
            <span className="muted"> / {formatTimecode(endUs, frameRate)}</span>
          </span>
        </div>
      </section>

      <aside className="panel inspector" aria-label="Inspector">
        <Inspector
          asset={selectedAsset}
          clip={selectedClip}
          missing={selectedAsset ? missingIds.has(selectedAsset.id) : false}
          settings={state?.project.settings ?? null}
        />
      </aside>

      <section className="panel timeline" aria-label="Timeline">
        {state && (
          <Timeline
            project={state.project}
            thumbnails={project.thumbnails}
            selectedClipId={selectedClip?.id ?? null}
            playheadUs={playheadUs}
            followPlayhead={playback.playing}
            onSelectClip={selectClip}
            onSeek={seek}
            onEdit={project.edit}
            onNotify={project.notify}
          />
        )}
      </section>
    </div>
  );
}

