import type { Clip, MediaAsset, ProjectSettings } from "../../lib/tauri";
import { formatDuration, formatFrameRate, formatTimecode } from "../../lib/time";

interface Props {
  asset: MediaAsset | null;
  clip: Clip | null;
  missing: boolean;
  settings: ProjectSettings | null;
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </>
  );
}

export function Inspector({ asset, clip, missing, settings }: Props) {
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
