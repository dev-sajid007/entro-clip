import type { MediaAsset, ProjectSettings } from "../../lib/tauri";
import { formatDuration, formatFrameRate } from "../../lib/time";

interface Props {
  asset: MediaAsset | null;
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

export function Inspector({ asset, missing, settings }: Props) {
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
