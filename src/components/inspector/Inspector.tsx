import { useEffect, useRef, useState } from "react";
import type { Clip, Edit, MediaAsset, ProjectSettings, TextOverlay, Track } from "../../lib/tauri";
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

interface SliderProps {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (value: number) => string;
  /** Value restored by double-clicking. */
  resetTo: number;
  onCommit: (value: number) => Promise<unknown>;
}

/**
 * Slider that previews locally while dragging and commits one edit on
 * release, so a drag is a single undo step. Double-click resets it.
 */
function SliderControl({ label, value, min, max, step, format, resetTo, onCommit }: SliderProps) {
  const [draft, setDraft] = useState<number | null>(null);
  // Pointer-up and blur can both fire for one drag; send its edit once.
  const pending = useRef<number | null>(null);
  const shown = draft ?? value;
  const commit = (next: number | null) => {
    if (next === pending.current) return;
    if (next === null || next === value) {
      setDraft(null);
      return;
    }
    pending.current = next;
    void onCommit(next).finally(() => {
      pending.current = null;
      setDraft(null);
    });
  };

  return (
    <label className="control" title="Double-click to reset">
      <span>{label}</span>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={shown}
        onChange={(e) => setDraft(Number(e.target.value))}
        onPointerUp={() => commit(draft)}
        onKeyUp={() => commit(draft)}
        onBlur={() => commit(draft)}
        onDoubleClick={() => commit(resetTo)}
      />
      <output>{format(shown)}</output>
    </label>
  );
}

function GainSlider({ label, value, onCommit }: Pick<SliderProps, "label" | "value" | "onCommit">) {
  return (
    <SliderControl
      label={label}
      value={value}
      min={MIN_GAIN_DB}
      max={MAX_GAIN_DB}
      step={0.5}
      format={formatDb}
      resetTo={0}
      onCommit={onCommit}
    />
  );
}

const percent = (fraction: number) => `${Math.round(fraction * 100)}%`;

/** Vertical positions offered as one-click presets. */
const TEXT_POSITIONS = [
  { label: "Top", y: 0.15 },
  { label: "Middle", y: 0.5 },
  { label: "Lower third", y: 0.85 },
];

/** Text content that commits on blur or Ctrl+Enter; empty text is discarded. */
function TextContent({ value, onCommit }: { value: string; onCommit: (content: string) => void }) {
  const [draft, setDraft] = useState<string | null>(null);
  useEffect(() => setDraft(null), [value]);
  const commit = () => {
    if (draft === null) return;
    setDraft(null);
    if (draft.trim() !== "" && draft !== value) onCommit(draft);
  };

  return (
    <label className="control">
      <span>Text</span>
      <textarea
        className="text-content"
        rows={3}
        maxLength={500}
        value={draft ?? value}
        onChange={(e) => setDraft(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) commit();
          else if (e.key === "Escape") setDraft(null);
        }}
      />
    </label>
  );
}

function TextControls({ clip, text, onEdit }: { clip: Clip; text: TextOverlay; onEdit: Props["onEdit"] }) {
  const update = (change: Partial<TextOverlay>) =>
    onEdit({ type: "updateText", clipId: clip.id, text: { ...text, ...change } });

  return (
    <>
      <h2 className="section">Text</h2>
      <div className="controls">
        <TextContent value={text.content} onCommit={(content) => void update({ content })} />
        <SliderControl
          label="Size"
          value={text.size}
          min={0.02}
          max={0.3}
          step={0.005}
          format={percent}
          resetTo={0.08}
          onCommit={(size) => update({ size })}
        />
        <label className="control inline">
          <span>Colour</span>
          <input type="color" value={text.color} onChange={(e) => void update({ color: e.target.value })} />
        </label>
        <label className="control inline">
          <span>Background box</span>
          <input type="checkbox" checked={text.background} onChange={(e) => void update({ background: e.target.checked })} />
        </label>
        <div className="control">
          <span>Position</span>
          <div className="segmented">
            {TEXT_POSITIONS.map((p) => (
              <button
                key={p.label}
                type="button"
                className={`ghost ${text.x === 0.5 && text.y === p.y ? "on" : ""}`}
                onClick={() => void update({ x: 0.5, y: p.y })}
              >
                {p.label}
              </button>
            ))}
          </div>
        </div>
        <SliderControl label="Horizontal" value={text.x} min={0} max={1} step={0.01} format={percent} resetTo={0.5} onCommit={(x) => update({ x })} />
        <SliderControl label="Vertical" value={text.y} min={0} max={1} step={0.01} format={percent} resetTo={0.5} onCommit={(y) => update({ y })} />
      </div>
    </>
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
          <Row label="Source" value={clip.text ? "Text" : (asset?.name ?? "Missing asset")} />
          <Row label="Start" value={tc(clip.timelineStartUs)} />
          <Row label="End" value={tc(clip.timelineStartUs + duration)} />
          <Row label="Duration" value={tc(duration)} />
          {!clip.text && <Row label="Source in" value={tc(clip.sourceInUs)} />}
          {!clip.text && <Row label="Source out" value={tc(clip.sourceOutUs)} />}
        </dl>
        {clip.text && <TextControls key={clip.id} clip={clip} text={clip.text} onEdit={onEdit} />}
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
