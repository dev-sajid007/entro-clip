import { useEffect, useRef, useState } from "react";
import type { Clip, Edit, MediaAsset, ProjectSettings, Track } from "../../lib/tauri";
import { MAX_GAIN_DB, MIN_GAIN_DB, formatDb } from "../../lib/audio";
import { US_PER_SECOND, formatDuration, formatFrameRate, formatTimecode, type Microseconds } from "../../lib/time";

interface Props {
  asset: MediaAsset | null;
  clip: Clip | null;
  /** The track holding `clip`. */
  track: Track | null;
  missing: boolean;
  settings: ProjectSettings | null;
  onEdit: (edit: Edit) => Promise<unknown>;
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </>
  );
}

/**
 * Volume slider that previews locally while dragging and commits one edit on
 * release, so a drag is a single undo step. Double-click resets to 0 dB.
 */
function GainSlider({ label, value, onCommit }: { label: string; value: number; onCommit: (db: number) => Promise<unknown> }) {
  const [draft, setDraft] = useState<number | null>(null);
  // Pointer-up and blur can both fire for one drag; send its edit once.
  const pending = useRef<number | null>(null);
  const shown = draft ?? value;
  const commit = (db: number | null) => {
    if (db === pending.current) return;
    if (db === null || db === value) {
      setDraft(null);
      return;
    }
    pending.current = db;
    void onCommit(db).finally(() => {
      pending.current = null;
      setDraft(null);
    });
  };

  return (
    <label className="control" title="Double-click to reset">
      <span>{label}</span>
      <input
        type="range"
        min={MIN_GAIN_DB}
        max={MAX_GAIN_DB}
        step={0.5}
        value={shown}
        onChange={(e) => setDraft(Number(e.target.value))}
        onPointerUp={() => commit(draft)}
        onKeyUp={() => commit(draft)}
        onBlur={() => commit(draft)}
        onDoubleClick={() => commit(0)}
      />
      <output>{formatDb(shown)}</output>
    </label>
  );
}

/** Seconds field that commits on Enter or blur, clamped to `[0, maxUs]`. */
function SecondsInput({ label, valueUs, maxUs, onCommit }: {
  label: string;
  valueUs: Microseconds;
  maxUs: Microseconds;
  onCommit: (us: Microseconds) => void;
}) {
  const [draft, setDraft] = useState<string | null>(null);
  // Show the committed value again when it changes (e.g. undo).
  useEffect(() => setDraft(null), [valueUs]);

  const commit = () => {
    if (draft === null) return;
    setDraft(null);
    const seconds = Number(draft);
    if (draft.trim() === "" || !Number.isFinite(seconds)) return;
    const us = Math.round(Math.min(maxUs / US_PER_SECOND, Math.max(0, seconds)) * US_PER_SECOND);
    if (us !== valueUs) onCommit(us);
  };

  return (
    <label className="control">
      <span>{label}</span>
      <input
        type="number"
        min={0}
        max={maxUs / US_PER_SECOND}
        step={0.1}
        value={draft ?? (valueUs / US_PER_SECOND).toFixed(2)}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit();
          else if (e.key === "Escape") setDraft(null);
        }}
      />
      <span className="muted">s</span>
    </label>
  );
}

function AudioControls({ clip, track, onEdit }: { clip: Clip; track: Track; onEdit: Props["onEdit"] }) {
  const duration = clip.sourceOutUs - clip.sourceInUs;
  const { gainDb, fadeInUs, fadeOutUs } = clip.audio;
  const editAudio = (change: { gainDb?: number; fadeInUs?: Microseconds; fadeOutUs?: Microseconds }) =>
    onEdit({ type: "updateClipAudio", clipId: clip.id, ...change });

  return (
    <>
      <h2 className="section">Audio</h2>
      <div className="controls">
        <GainSlider label="Clip volume" value={gainDb} onCommit={(db) => editAudio({ gainDb: db })} />
        <SecondsInput label="Fade in" valueUs={fadeInUs} maxUs={duration - fadeOutUs} onCommit={(us) => editAudio({ fadeInUs: us })} />
        <SecondsInput label="Fade out" valueUs={fadeOutUs} maxUs={duration - fadeInUs} onCommit={(us) => editAudio({ fadeOutUs: us })} />
        <GainSlider
          label={`Track volume (${track.name})`}
          value={track.gainDb}
          onCommit={(db) => onEdit({ type: "updateTrack", trackId: track.id, gainDb: db })}
        />
      </div>
      {(gainDb + track.gainDb > 0) && (
        <p className="muted hint">Boosts above 0 dB are heard in the export, not the live preview.</p>
      )}
    </>
  );
}

export function Inspector({ asset, clip, track, missing, settings, onEdit }: Props) {
  const fps = settings?.frameRate ?? { numerator: 30, denominator: 1 };
  const tc = (us: number) => formatTimecode(us, fps);

  if (clip) {
    const duration = clip.sourceOutUs - clip.sourceInUs;
    return (
      <>
        <h2>Clip properties</h2>
        {missing && <p className="badge bad">Source file is missing</p>}
        <dl className="props">
          <Row label="Source" value={asset?.name ?? "Missing asset"} />
          <Row label="Start" value={tc(clip.timelineStartUs)} />
          <Row label="End" value={tc(clip.timelineStartUs + duration)} />
          <Row label="Duration" value={tc(duration)} />
          <Row label="Source in" value={tc(clip.sourceInUs)} />
          <Row label="Source out" value={tc(clip.sourceOutUs)} />
        </dl>
        {track && asset && (asset.kind === "audio" || asset.hasAudio) && (
          <AudioControls key={clip.id} clip={clip} track={track} onEdit={onEdit} />
        )}
      </>
    );
  }

  if (asset) {
    return (
      <>
        <h2>Media properties</h2>
        {missing && <p className="badge bad">Source file is missing</p>}
        <dl className="props">
          <Row label="Name" value={asset.name} />
          <Row label="Type" value={asset.kind} />
          {asset.durationUs !== null && <Row label="Duration" value={formatDuration(asset.durationUs)} />}
          {asset.width && asset.height && <Row label="Resolution" value={`${asset.width}×${asset.height}`} />}
          {asset.frameRate && <Row label="Frame rate" value={`${formatFrameRate(asset.frameRate)} fps`} />}
          {asset.kind !== "image" && <Row label="Audio" value={asset.hasAudio ? "Yes" : "No"} />}
          <Row label="Path" value={asset.path} />
        </dl>
      </>
    );
  }

  return (
    <>
      <h2>Project settings</h2>
      {settings && (
        <dl className="props">
          <Row label="Resolution" value={`${settings.width}×${settings.height}`} />
          <Row label="Frame rate" value={`${formatFrameRate(settings.frameRate)} fps`} />
          <Row label="Sample rate" value={`${settings.audioSampleRate} Hz`} />
        </dl>
      )}
    </>
  );
}
