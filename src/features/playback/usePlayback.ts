import { useCallback, useEffect, useRef, useState } from "react";
import type { Microseconds } from "../../lib/time";

/**
 * Timeline playback clock. Wall-clock time drives the playhead; media
 * elements follow it rather than the other way around.
 */
export function usePlayback(
  playheadUs: Microseconds,
  setPlayheadUs: (us: Microseconds) => void,
  endUs: Microseconds,
) {
  const [playing, setPlaying] = useState(false);
  const origin = useRef({ wallMs: 0, timeUs: 0 });
  const endRef = useRef(endUs);
  endRef.current = endUs;

  const seek = useCallback(
    (us: Microseconds) => {
      const t = Math.max(0, us);
      origin.current = { wallMs: performance.now(), timeUs: t };
      setPlayheadUs(t);
    },
    [setPlayheadUs],
  );

  const play = useCallback(() => {
    if (endRef.current <= 0) return;
    const start = playheadUs >= endRef.current ? 0 : playheadUs;
    origin.current = { wallMs: performance.now(), timeUs: start };
    setPlayheadUs(start);
    setPlaying(true);
  }, [playheadUs, setPlayheadUs]);

  const pause = useCallback(() => setPlaying(false), []);
  const toggle = useCallback(() => (playing ? pause() : play()), [playing, pause, play]);

  useEffect(() => {
    if (!playing) return;
    let frame = 0;
    const tick = () => {
      const t = origin.current.timeUs + (performance.now() - origin.current.wallMs) * 1000;
      if (t >= endRef.current) {
        setPlayheadUs(endRef.current);
        setPlaying(false);
        return;
      }
      setPlayheadUs(Math.round(t));
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [playing, setPlayheadUs]);

  return { playing, play, pause, toggle, seek };
}
