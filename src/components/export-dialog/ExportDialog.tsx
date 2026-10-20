import { useEffect, useRef, useState } from "react";
import type { ExportQuality } from "../../lib/tauri";
import { EXPORT_PRESETS, EXPORT_QUALITIES, type ExportPreset, type ExportState } from "../../lib/export";
import { formatTimecode, type Microseconds, type Rational } from "../../lib/time";

interface Props {
  state: ExportState;
  durationUs: Microseconds;
  frameRate: Rational;
  onStart: (preset: ExportPreset, quality: ExportQuality) => void;
  onCancel: () => void;
  onClose: () => void;
}

export function ExportDialog({ state, durationUs, frameRate, onStart, onCancel, onClose }: Props) {
  const ref = useRef<HTMLDialogElement>(null);
  const [presetId, setPresetId] = useState(EXPORT_PRESETS[0].id);
  const [quality, setQuality] = useState<ExportQuality>("standard");
  const working = state.phase === "starting" || state.phase === "running";

  useEffect(() => {
    const dialog = ref.current;
    if (dialog && !dialog.open) dialog.showModal();
  }, []);

  const preset = EXPORT_PRESETS.find((p) => p.id === presetId) ?? EXPORT_PRESETS[0];
  const percent = state.phase === "running" ? Math.round(state.progress * 100) : 0;

  return (
    <dialog
      ref={ref}
      className="export-dialog"
      aria-labelledby="export-title"
      onCancel={(e) => {
        // Escape closes the dialog, but never abandons a running export.
        e.preventDefault();
        if (!working) onClose();
      }}
    >
      <h2 id="export-title">Export</h2>

      {state.phase === "idle" && (
        <form
          method="dialog"
          className="export-form"
          onSubmit={(e) => {
            e.preventDefault();
            onStart(preset, quality);
          }}
        >
          <label>
            Format
            <select value={presetId} onChange={(e) => setPresetId(e.target.value)}>
              {EXPORT_PRESETS.map((p) => (
                <option key={p.id} value={p.id}>{p.label}</option>
              ))}
            </select>
          </label>
          <label>
            Quality
            <select value={quality} onChange={(e) => setQuality(e.target.value as ExportQuality)}>
              {EXPORT_QUALITIES.map((q) => (
                <option key={q.id} value={q.id}>{q.label}</option>
              ))}
            </select>
          </label>
          <p className="muted">
            MP4 (H.264 + AAC), duration <span className="timecode">{formatTimecode(durationUs, frameRate)}</span>
          </p>
          <div className="dialog-actions">
            <button type="button" className="ghost" onClick={onClose}>Close</button>
            <button type="submit">Export…</button>
          </div>
        </form>
      )}

      {working && (
        <>
          <progress max={100} value={state.phase === "running" ? percent : undefined} aria-label="Export progress" />
          <p className="muted">{state.phase === "starting" ? "Preparing…" : `Rendering… ${percent}%`}</p>
          <div className="dialog-actions">
            <button type="button" className="ghost" onClick={onCancel} disabled={state.phase !== "running"}>
              Cancel export
            </button>
          </div>
        </>
      )}

      {(state.phase === "completed" || state.phase === "failed" || state.phase === "cancelled") && (
        <>
          <p role="status" className={state.phase === "failed" ? "export-error" : undefined}>
            {state.phase === "completed" && <>Exported to <span className="path">{state.outputPath}</span></>}
            {state.phase === "failed" && <>Export failed: {state.message}</>}
            {state.phase === "cancelled" && "Export cancelled."}
          </p>
          <div className="dialog-actions">
            <button type="button" onClick={onClose}>Close</button>
          </div>
        </>
      )}
    </dialog>
  );
}
