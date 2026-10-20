import { useEffect, useMemo, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { errorMessage, renderPreviewFrame, type AppError, type Project } from "../../lib/tauri";
import { activeAt, sourceSecondsAt, type ActiveClip } from "../../lib/playback";
import { clipGainAt, elementVolume } from "../../lib/audio";
import type { Microseconds } from "../../lib/time";

/** Scrubbing requests wait this long for the playhead to settle. */
const STILL_DEBOUNCE_MS = 60;
/** Media elements are re-seeked when they drift further than this. */
const MAX_DRIFT_S = 0.2;

interface Props {
  project: Project;
  playheadUs: Microseconds;
  playing: boolean;
  missingIds: Set<string>;
}

function syncElement(el: HTMLMediaElement | null | undefined, active: ActiveClip, us: Microseconds) {
  if (!el) return;
  // Boosts above 0 dB can't be heard here; export applies them.
  el.volume = elementVolume(clipGainAt(active.clip, active.track, us));
  const target = sourceSecondsAt(active.clip, us);
  if (el.readyState >= 1 && Math.abs(el.currentTime - target) > MAX_DRIFT_S) {
    el.currentTime = target;
  }
  if (el.paused) void el.play().catch(() => undefined);
}

export function PreviewMonitor({ project, playheadUs, playing, missingIds }: Props) {
  const [still, setStill] = useState<string | null>(null);
  const [stillError, setStillError] = useState<string | null>(null);
  const [mediaError, setMediaError] = useState<string | null>(null);
  const requestSeq = useRef(0);
  const videoRef = useRef<HTMLVideoElement>(null);
  const audioRefs = useRef(new Map<string, HTMLAudioElement>());
  const monitorRef = useRef<HTMLDivElement>(null);
  const [monitorHeight, setMonitorHeight] = useState(0);

  // Text is sized relative to the frame height, like the export.
  useEffect(() => {
    const el = monitorRef.current;
    if (!el) return;
    const observer = new ResizeObserver(([entry]) => setMonitorHeight(entry.contentRect.height));
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const active = useMemo(() => activeAt(project, playheadUs, missingIds), [project, playheadUs, missingIds]);
  const visualId = active.visual?.clip.id ?? null;

  // Paused: render an exact still frame for the playhead.
  useEffect(() => {
    if (playing) return;
    if (!visualId) {
      setStill(null);
      setStillError(null);
      return;
    }
    const seq = ++requestSeq.current;
    const timer = window.setTimeout(() => {
      renderPreviewFrame(playheadUs)
        .then((frame) => {
          if (seq !== requestSeq.current) return;
          setStill(frame ? convertFileSrc(frame.path) : null);
          setStillError(null);
        })
        .catch((err: AppError) => {
          if (seq !== requestSeq.current || err?.kind === "cancelled") return;
          setStillError(errorMessage(err));
        });
    }, STILL_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [playing, playheadUs, project, visualId]);

  // Playing: keep media elements in step with the playback clock.
  useEffect(() => {
    if (!playing) return;
    if (active.visual?.asset.kind === "video") syncElement(videoRef.current, active.visual, playheadUs);
    for (const a of active.audio) syncElement(audioRefs.current.get(a.clip.id), a, playheadUs);
  }, [playing, playheadUs, active]);

  useEffect(() => {
    if (!playing) setMediaError(null);
  }, [playing]);

  const { width, height } = project.settings;
  const visual = playing ? active.visual : null;
  const onMediaError = () =>
    setMediaError("This file can't play in the live preview. Pause to see exact frames.");

  return (
    <div className="monitor" ref={monitorRef} style={{ aspectRatio: `${width} / ${height}` }}>
      {still && <img className="monitor-layer" src={still} alt="" />}
      {visual?.asset.kind === "video" && (
        <video
          key={visual.clip.id}
          ref={videoRef}
          className="monitor-layer live"
          src={convertFileSrc(visual.asset.path)}
          preload="auto"
          onLoadedMetadata={(e) => {
            e.currentTarget.currentTime = sourceSecondsAt(visual.clip, playheadUs);
            void e.currentTarget.play().catch(() => undefined);
          }}
          onError={onMediaError}
        />
      )}
      {visual?.asset.kind === "image" && (
        <img className="monitor-layer live" src={convertFileSrc(visual.asset.path)} alt="" />
      )}
      {playing && !visual && <div className="monitor-layer live blank" />}
      {playing &&
        active.audio.map((a) => (
          <audio
            key={a.clip.id}
            ref={(el) => {
              if (el) audioRefs.current.set(a.clip.id, el);
              else audioRefs.current.delete(a.clip.id);
            }}
            src={convertFileSrc(a.asset.path)}
            preload="auto"
            onError={onMediaError}
          />
        ))}
      {active.texts.map(({ clip, text }) => (
        <div
          key={clip.id}
          className={`monitor-text ${text.background ? "boxed" : ""}`}
          style={{
            left: `${text.x * 100}%`,
            top: `${text.y * 100}%`,
            color: text.color,
            fontSize: text.size * monitorHeight,
          }}
        >
          {text.content}
        </div>
      ))}
      {(mediaError || (!playing && stillError)) && (
        <div className="monitor-message">{mediaError ?? stillError}</div>
      )}
    </div>
  );
}
