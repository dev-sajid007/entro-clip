import { useEffect, useState } from "react";
import { systemCheck, type SystemCheck } from "../lib/tauri";
import { formatTimecode } from "../lib/time";

function ToolBadge({ label, ok, title }: { label: string; ok: boolean; title: string }) {
  return (
    <span className={`badge ${ok ? "ok" : "bad"}`} title={title}>
      {label} {ok ? "ready" : "missing"}
    </span>
  );
}

export default function App() {
  const [check, setCheck] = useState<SystemCheck | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    systemCheck()
      .then(setCheck)
      .catch((e) => setError(String(e?.message ?? e)));
  }, []);

  return (
    <div className="editor">
      <header className="topbar">
        <strong>EntroCut</strong>
        <span className="muted">Untitled project</span>
        <div className="spacer" />
        {check && (
          <>
            <ToolBadge label="FFmpeg" ok={check.ffmpeg.available} title={check.ffmpeg.version ?? check.ffmpeg.error ?? ""} />
            <ToolBadge label="FFprobe" ok={check.ffprobe.available} title={check.ffprobe.version ?? check.ffprobe.error ?? ""} />
            <span className="muted">v{check.appVersion}</span>
          </>
        )}
        {error && <span className="badge bad">{error}</span>}
        <button type="button" disabled>Export</button>
      </header>

      <aside className="panel library" aria-label="Media library">
        <h2>Media</h2>
        <p className="muted">Import is coming in Milestone 1.</p>
      </aside>

      <section className="panel preview" aria-label="Preview">
        <div className="monitor" />
        <div className="timecode">{formatTimecode(0, { numerator: 30, denominator: 1 })}</div>
      </section>

      <aside className="panel inspector" aria-label="Inspector">
        <h2>Properties</h2>
        <p className="muted">Select a clip to edit its properties.</p>
      </aside>

      <section className="panel timeline" aria-label="Timeline">
        <div className="track"><span>Video 1</span></div>
        <div className="track"><span>Audio 1</span></div>
      </section>
    </div>
  );
}
