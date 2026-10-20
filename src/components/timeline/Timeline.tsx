import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { PointerEvent as ReactPointerEvent, DragEvent } from "react";
import { ASSET_DRAG_TYPE, type Clip, type Edit, type MediaAsset, type Project, type Track } from "../../lib/tauri";
import type { Microseconds } from "../../lib/time";
import {
  MAX_ZOOM,
  MIN_ZOOM,
  clampZoom,
  clipDuration,
  clipEnd,
  findSnap,
  formatRulerLabel,
  frameDurationUs,
  overlapsTrack,
  pxToUs,
  snapCandidates,
  snapToFrame,
  tickInterval,
  timelineEnd,
  trackAccepts,
  usToPx,
} from "../../lib/timeline";
import type { Thumbnail } from "../../features/project/useProject";

const IMAGE_CLIP_US = 5_000_000;
const SNAP_PX = 8;
const DRAG_THRESHOLD_PX = 3;
const PADDING_US = 30_000_000;
const MIN_VISIBLE_US = 60_000_000;

type DragMode = "move" | "trimStart" | "trimEnd";

interface Preview {
  trackId: string;
  startUs: Microseconds;
  inUs: Microseconds;
  outUs: Microseconds;
  valid: boolean;
}

interface DragState {
  mode: DragMode;
  clip: Clip;
  trackId: string;
  asset: MediaAsset;
  originX: number;
  moved: boolean;
  preview: Preview;
}

interface Props {
  project: Project;
  thumbnails: Record<string, Thumbnail>;
  selectedClipId: string | null;
  playheadUs: Microseconds;
  followPlayhead: boolean;
  onSelectClip: (id: string | null) => void;
  onSeek: (us: Microseconds) => void;
  onEdit: (edit: Edit) => Promise<string | null>;
  onNotify: (text: string) => void;
}

export function sortedTracks(project: Project): Track[] {
  return [...project.tracks].sort((a, b) => a.order - b.order);
}

export function Timeline({
  project,
  thumbnails,
  selectedClipId,
  playheadUs,
  followPlayhead,
  onSelectClip,
  onSeek,
  onEdit,
  onNotify,
}: Props) {
  const [zoom, setZoom] = useState(60);
  const [snapping, setSnapping] = useState(true);
  const [drag, setDrag] = useState<DragState | null>(null);

  const scrollRef = useRef<HTMLDivElement>(null);
  const rulerRef = useRef<HTMLDivElement>(null);
  const laneRefs = useRef(new Map<string, HTMLDivElement>());
  const dragRef = useRef<DragState | null>(null);
  const pendingScroll = useRef<number | null>(null);

  const tracks = useMemo(() => sortedTracks(project), [project]);
  const assets = useMemo(() => new Map(project.assets.map((a) => [a.id, a])), [project.assets]);
  const frameRate = project.settings.frameRate;
  const minClipUs = Math.ceil(frameDurationUs(frameRate));

  const contentUs = Math.max(timelineEnd(tracks) + PADDING_US, MIN_VISIBLE_US);
  const contentPx = usToPx(contentUs, zoom);

  // --- coordinate helpers -------------------------------------------------

  const timeAt = (clientX: number): Microseconds => {
    const left = rulerRef.current?.getBoundingClientRect().left ?? 0;
    return Math.max(0, pxToUs(clientX - left, zoom));
  };

  const trackAt = (clientY: number): Track | undefined =>
    tracks.find((t) => {
      const rect = laneRefs.current.get(t.id)?.getBoundingClientRect();
      return rect && clientY >= rect.top && clientY < rect.bottom;
    });

  /** Snaps to clip edges/playhead when enabled, otherwise to frame boundaries. */
  const snapPoint = (us: Microseconds, excludeClipId?: string): Microseconds => {
    if (snapping) {
      const hit = findSnap(us, snapCandidates(tracks, playheadUs, excludeClipId), pxToUs(SNAP_PX, zoom));
      if (hit !== null) return hit;
    }
    return snapToFrame(us, frameRate);
  };

  // --- zoom ------------------------------------------------------------------

  const zoomAround = (nextZoom: number, clientX?: number) => {
    const el = scrollRef.current;
    const clamped = clampZoom(nextZoom);
    if (el && rulerRef.current) {
      const anchorX = clientX ?? rulerRef.current.getBoundingClientRect().left + el.clientWidth / 2;
      const t = timeAt(anchorX);
      pendingScroll.current = el.scrollLeft + usToPx(t, clamped) - usToPx(t, zoom);
    }
    setZoom(clamped);
  };

  useLayoutEffect(() => {
    if (pendingScroll.current !== null && scrollRef.current) {
      scrollRef.current.scrollLeft = Math.max(0, pendingScroll.current);
      pendingScroll.current = null;
    }
  }, [zoom]);

  // React's wheel listener is passive, so Ctrl+wheel zoom needs a native one.
  const zoomRef = useRef(zoomAround);
  zoomRef.current = zoomAround;
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      zoomRef.current(zoom * Math.exp(-e.deltaY * 0.002), e.clientX);
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [zoom]);

  // Keep the playhead in view during playback.
  useEffect(() => {
    const el = scrollRef.current;
    if (!followPlayhead || !el) return;
    const x = usToPx(playheadUs, zoom);
    const visible = el.clientWidth - 140;
    if (x < el.scrollLeft || x > el.scrollLeft + visible - 20) el.scrollLeft = Math.max(0, x - 40);
  }, [followPlayhead, playheadUs, zoom]);

  const fit = () => {
    const width = scrollRef.current?.clientWidth ?? 800;
    const end = Math.max(timelineEnd(tracks), 10_000_000);
    pendingScroll.current = 0;
    setZoom(clampZoom(((width - 160) / end) * 1_000_000));
  };

  // --- clip dragging ---------------------------------------------------------

  const startDrag = (e: ReactPointerEvent, mode: DragMode, clip: Clip, track: Track) => {
    if (e.button !== 0) return;
    const asset = assets.get(clip.assetId);
    if (!asset) return;
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    onSelectClip(clip.id);
    const state: DragState = {
      mode,
      clip,
      trackId: track.id,
      asset,
      originX: e.clientX,
      moved: false,
      preview: {
        trackId: track.id,
        startUs: clip.timelineStartUs,
        inUs: clip.sourceInUs,
        outUs: clip.sourceOutUs,
        valid: true,
      },
    };
    dragRef.current = state;
    setDrag(state);
  };

  const computePreview = (d: DragState, clientX: number, clientY: number): Preview => {
    const { clip, asset } = d;
    const delta = pxToUs(clientX - d.originX, zoom);
    const duration = clipDuration(clip);
    const homeTrack = tracks.find((t) => t.id === d.trackId)!;

    if (d.mode === "move") {
      const raw = Math.max(0, clip.timelineStartUs + delta);
      let start = snapToFrame(raw, frameRate);
      if (snapping) {
        const candidates = snapCandidates(tracks, playheadUs, clip.id);
        const threshold = pxToUs(SNAP_PX, zoom);
        const byStart = findSnap(raw, candidates, threshold);
        const byEnd = findSnap(raw + duration, candidates, threshold);
        if (byStart !== null && (byEnd === null || Math.abs(byStart - raw) <= Math.abs(byEnd - raw - duration))) {
          start = byStart;
        } else if (byEnd !== null) {
          start = Math.max(0, byEnd - duration);
        }
      }
      const target = trackAt(clientY) ?? homeTrack;
      const valid =
        trackAccepts(target, asset.kind, asset.hasAudio) && !overlapsTrack(target, start, duration, clip.id);
      return { trackId: target.id, startUs: start, inUs: clip.sourceInUs, outUs: clip.sourceOutUs, valid };
    }

    if (d.mode === "trimStart") {
      const snapped = snapPoint(clip.timelineStartUs + delta, clip.id);
      let shift = snapped - clip.timelineStartUs;
      shift = Math.max(shift, -clip.sourceInUs, -clip.timelineStartUs);
      shift = Math.min(shift, duration - minClipUs);
      const start = clip.timelineStartUs + shift;
      const inUs = clip.sourceInUs + shift;
      return {
        trackId: homeTrack.id,
        startUs: start,
        inUs,
        outUs: clip.sourceOutUs,
        valid: !overlapsTrack(homeTrack, start, clip.sourceOutUs - inUs, clip.id),
      };
    }

    const snappedEnd = snapPoint(clipEnd(clip) + delta, clip.id);
    let outUs = clip.sourceOutUs + (snappedEnd - clipEnd(clip));
    outUs = Math.max(outUs, clip.sourceInUs + minClipUs);
    if (asset.durationUs !== null) outUs = Math.min(outUs, asset.durationUs);
    return {
      trackId: homeTrack.id,
      startUs: clip.timelineStartUs,
      inUs: clip.sourceInUs,
      outUs,
      valid: !overlapsTrack(homeTrack, clip.timelineStartUs, outUs - clip.sourceInUs, clip.id),
    };
  };

  const onPointerMove = (e: ReactPointerEvent) => {
    const d = dragRef.current;
    if (!d) return;
    const moved = d.moved || Math.abs(e.clientX - d.originX) >= DRAG_THRESHOLD_PX;
    if (!moved) return;
    const next = { ...d, moved, preview: computePreview(d, e.clientX, e.clientY) };
    dragRef.current = next;
    setDrag(next);
  };

  const endDrag = () => {
    const d = dragRef.current;
    dragRef.current = null;
    setDrag(null);
    if (!d || !d.moved || !d.preview.valid) return;

    const { clip, preview } = d;
    if (d.mode === "move") {
      if (preview.trackId === d.trackId && preview.startUs === clip.timelineStartUs) return;
      void onEdit({ type: "moveClip", clipId: clip.id, trackId: preview.trackId, timelineStartUs: preview.startUs });
    } else if (
      preview.startUs !== clip.timelineStartUs ||
      preview.inUs !== clip.sourceInUs ||
      preview.outUs !== clip.sourceOutUs
    ) {
      void onEdit({
        type: "trimClip",
        clipId: clip.id,
        timelineStartUs: preview.startUs,
        sourceInUs: preview.inUs,
        sourceOutUs: preview.outUs,
      });
    }
  };

  // --- library drops ---------------------------------------------------------

  const onDragOver = (e: DragEvent) => {
    if (e.dataTransfer.types.includes(ASSET_DRAG_TYPE)) {
      e.preventDefault();
      e.dataTransfer.dropEffect = "copy";
    }
  };

  const onDrop = async (e: DragEvent, track: Track) => {
    const assetId = e.dataTransfer.getData(ASSET_DRAG_TYPE);
    const asset = assets.get(assetId);
    if (!asset) return;
    e.preventDefault();

    if (!trackAccepts(track, asset.kind, asset.hasAudio)) {
      onNotify(`${asset.name} can't be placed on ${track.kind} track "${track.name}".`);
      return;
    }
    const duration = asset.kind === "image" ? IMAGE_CLIP_US : asset.durationUs;
    if (duration === null) {
      onNotify(`${asset.name} has no known duration.`);
      return;
    }
    const start = snapPoint(timeAt(e.clientX));
    if (overlapsTrack(track, start, duration)) {
      onNotify("That spot is already taken. Drop the clip on an empty part of the track.");
      return;
    }
    const id = await onEdit({ type: "addClip", trackId: track.id, assetId, timelineStartUs: start });
    if (id) onSelectClip(id);
  };

  // --- ruler -----------------------------------------------------------------

  const scrubbing = useRef(false);
  const onRulerDown = (e: ReactPointerEvent) => {
    if (e.button !== 0) return;
    scrubbing.current = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    onSeek(snapToFrame(timeAt(e.clientX), frameRate));
  };
  const onRulerMove = (e: ReactPointerEvent) => {
    if (scrubbing.current) onSeek(snapToFrame(timeAt(e.clientX), frameRate));
  };

  const ticks = useMemo(() => {
    const step = tickInterval(zoom);
    const out: Microseconds[] = [];
    for (let t = 0; t <= contentUs; t += step) out.push(t);
    return out;
  }, [zoom, contentUs]);

  // --- rendering -------------------------------------------------------------

  const selectedClip = tracks.flatMap((t) => t.clips).find((c) => c.id === selectedClipId);
  const canSplit =
    selectedClip !== undefined &&
    playheadUs > selectedClip.timelineStartUs &&
    playheadUs < clipEnd(selectedClip);

  const renderClip = (clip: Clip, track: Track) => {
    const asset = assets.get(clip.assetId);
    const isDragging = drag?.clip.id === clip.id && drag.moved;
    const p = isDragging ? drag.preview : null;
    const start = p?.startUs ?? clip.timelineStartUs;
    const duration = p ? p.outUs - p.inUs : clipDuration(clip);
    const thumb = thumbnails[clip.assetId];

    return (
      <div
        key={clip.id}
        className={[
          "clip",
          asset?.kind ?? "",
          clip.id === selectedClipId ? "selected" : "",
          isDragging ? "dragging" : "",
          p && !p.valid ? "invalid" : "",
          track.hidden || track.muted ? "inactive" : "",
        ].join(" ")}
        style={{
          left: usToPx(start, zoom),
          width: Math.max(2, usToPx(duration, zoom)),
          backgroundImage: thumb?.status === "ready" ? `url("${thumb.src}")` : undefined,
        }}
        onPointerDown={(e) => startDrag(e, "move", clip, track)}
        title={asset?.name}
      >
        <span className="clip-label">{asset?.name ?? "Missing asset"}</span>
        <div className="handle start" onPointerDown={(e) => startDrag(e, "trimStart", clip, track)} />
        <div className="handle end" onPointerDown={(e) => startDrag(e, "trimEnd", clip, track)} />
      </div>
    );
  };

  return (
    <div className="timeline-root">
      <div className="timeline-toolbar">
        <button type="button" className="ghost" onClick={() => onEdit({ type: "addTrack", kind: "video" })}>
          + Video track
        </button>
        <button type="button" className="ghost" onClick={() => onEdit({ type: "addTrack", kind: "audio" })}>
          + Audio track
        </button>
        <span className="divider" />
        <button
          type="button"
          className="ghost"
          disabled={!canSplit}
          title="Split at playhead (S)"
          onClick={() => selectedClip && onEdit({ type: "splitClip", clipId: selectedClip.id, atUs: playheadUs })}
        >
          Split
        </button>
        <button
          type="button"
          className="ghost"
          disabled={!selectedClip}
          title="Delete clip (Delete)"
          onClick={() => {
            if (!selectedClip) return;
            onSelectClip(null);
            void onEdit({ type: "deleteClip", clipId: selectedClip.id });
          }}
        >
          Delete
        </button>
        <div className="spacer" />
        <label className="toggle">
          <input type="checkbox" checked={snapping} onChange={(e) => setSnapping(e.target.checked)} />
          Snapping
        </label>
        <button type="button" className="ghost" onClick={() => zoomAround(zoom / 1.5)} aria-label="Zoom out">−</button>
        <input
          type="range"
          aria-label="Zoom"
          min={Math.log(MIN_ZOOM)}
          max={Math.log(MAX_ZOOM)}
          step={0.01}
          value={Math.log(zoom)}
          onChange={(e) => zoomAround(Math.exp(Number(e.target.value)))}
        />
        <button type="button" className="ghost" onClick={() => zoomAround(zoom * 1.5)} aria-label="Zoom in">+</button>
        <button type="button" className="ghost" onClick={fit}>Fit</button>
      </div>

      <div className="timeline-scroll" ref={scrollRef} onPointerMove={onPointerMove} onPointerUp={endDrag} onPointerCancel={endDrag}>
        <div className="timeline-content" style={{ width: `calc(var(--track-header) + ${contentPx}px)` }}>
          <div className="tl-row ruler-row">
            <div className="tl-head" />
            <div
              className="ruler"
              ref={rulerRef}
              onPointerDown={onRulerDown}
              onPointerMove={onRulerMove}
              onPointerUp={() => (scrubbing.current = false)}
            >
              {ticks.map((t) => (
                <span key={t} className="tick" style={{ left: usToPx(t, zoom) }}>
                  {formatRulerLabel(t)}
                </span>
              ))}
            </div>
          </div>

          {tracks.map((track) => (
            <div key={track.id} className={`tl-row ${track.kind}`}>
              <div className="tl-head">
                <span className="track-name" title={track.name}>{track.name}</span>
                <span className="track-actions">
                  {track.kind === "video" ? (
                    <button
                      type="button"
                      className={`icon ${track.hidden ? "on" : ""}`}
                      aria-pressed={track.hidden}
                      title={track.hidden ? "Show track" : "Hide track"}
                      onClick={() => onEdit({ type: "updateTrack", trackId: track.id, hidden: !track.hidden })}
                    >
                      {track.hidden ? "◌" : "◉"}
                    </button>
                  ) : (
                    <button
                      type="button"
                      className={`icon ${track.muted ? "on" : ""}`}
                      aria-pressed={track.muted}
                      title={track.muted ? "Unmute track" : "Mute track"}
                      onClick={() => onEdit({ type: "updateTrack", trackId: track.id, muted: !track.muted })}
                    >
                      {track.muted ? "M" : "♪"}
                    </button>
                  )}
                  <button
                    type="button"
                    className="icon"
                    title="Delete track"
                    onClick={() => onEdit({ type: "deleteTrack", trackId: track.id })}
                  >
                    ×
                  </button>
                </span>
              </div>
              <div
                className="lane"
                ref={(el) => {
                  if (el) laneRefs.current.set(track.id, el);
                  else laneRefs.current.delete(track.id);
                }}
                onPointerDown={(e) => {
                  if (e.button !== 0) return;
                  onSelectClip(null);
                  onSeek(snapToFrame(timeAt(e.clientX), frameRate));
                }}
                onDragOver={onDragOver}
                onDrop={(e) => void onDrop(e, track)}
              >
                {track.clips
                  .filter((c) => !(drag?.moved && drag.clip.id === c.id && drag.preview.trackId !== track.id))
                  .map((c) => renderClip(c, track))}
                {drag?.moved && drag.mode === "move" && drag.preview.trackId === track.id && drag.trackId !== track.id &&
                  renderClip(drag.clip, track)}
              </div>
            </div>
          ))}

          <div className="playhead" style={{ left: `calc(var(--track-header) + ${usToPx(playheadUs, zoom)}px)` }} />
        </div>
      </div>
    </div>
  );
}
