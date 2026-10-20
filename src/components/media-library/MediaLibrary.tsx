import type { MediaAsset } from "../../lib/tauri";
import { formatDuration } from "../../lib/time";
import type { Thumbnail } from "../../features/project/useProject";

interface Props {
  assets: MediaAsset[];
  missingIds: Set<string>;
  thumbnails: Record<string, Thumbnail>;
  selectedId: string | null;
  busy: boolean;
  onSelect: (id: string) => void;
  onImport: () => void;
}

const KIND_LABEL = { video: "Video", audio: "Audio", image: "Image" } as const;

function describe(asset: MediaAsset): string {
  const parts: string[] = [KIND_LABEL[asset.kind]];
  if (asset.durationUs !== null) parts.push(formatDuration(asset.durationUs));
  if (asset.width && asset.height) parts.push(`${asset.width}×${asset.height}`);
  return parts.join(" · ");
}

function Preview({ asset, thumb, missing }: { asset: MediaAsset; thumb?: Thumbnail; missing: boolean }) {
  if (!missing && thumb?.status === "ready") {
    return <img className="thumb" src={thumb.src} alt="" draggable={false} />;
  }
  const label = missing
    ? "Missing"
    : asset.kind === "audio"
      ? "♪"
      : thumb?.status === "loading"
        ? "…"
        : thumb?.status === "error"
          ? "No preview"
          : "";
  return (
    <div className="thumb placeholder" title={thumb?.status === "error" ? thumb.message : undefined}>
      {label}
    </div>
  );
}

export function MediaLibrary({ assets, missingIds, thumbnails, selectedId, busy, onSelect, onImport }: Props) {
  return (
    <div className="media-library">
      <div className="panel-header">
        <h2>Media</h2>
        <button type="button" onClick={onImport} disabled={busy}>
          Import
        </button>
      </div>

      {assets.length === 0 ? (
        <p className="muted">Import video, audio, or image files to get started.</p>
      ) : (
        <ul className="asset-list">
          {assets.map((asset) => {
            const missing = missingIds.has(asset.id);
            return (
              <li key={asset.id}>
                <button
                  type="button"
                  className={`asset ${asset.id === selectedId ? "selected" : ""}`}
                  onClick={() => onSelect(asset.id)}
                  title={asset.path}
                >
                  <Preview asset={asset} thumb={thumbnails[asset.id]} missing={missing} />
                  <span className="asset-text">
                    <span className="asset-name">{asset.name}</span>
                    <span className="muted">{describe(asset)}</span>
                    {missing && <span className="badge bad">File missing</span>}
                  </span>
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
