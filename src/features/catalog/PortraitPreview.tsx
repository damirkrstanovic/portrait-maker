import { useEffect, useRef } from "react";

import type { MetadataPatch, Portrait, Role } from "../../lib/contracts";
import { MetadataEditor } from "../metadata/MetadataEditor";
import { PreviewImage } from "./PreviewImage";

type Props = {
  portrait: Portrait | null;
  visible: boolean;
  assetUrl(id: string, role: Role, variant: "thumbnail" | "original"): string;
  onClose(): void;
  onNavigate(step: -1 | 1): void;
  editMetadata?: (ids: string[], patch: MetadataPatch) => Promise<void>;
  renameSource?: (id: string, name: string) => Promise<void>;
  onMetadataSaved?(): void;
};

export function PortraitPreview({ portrait, visible, assetUrl, onClose, onNavigate, editMetadata, renameSource, onMetadataSaved }: Props) {
  const preview = useRef<HTMLElement>(null);

  useEffect(() => {
    if (visible && portrait) preview.current?.focus();
  }, [portrait, visible]);

  if (!visible || !portrait) return null;

  return <aside
    ref={preview}
    className="portrait-preview"
    role="complementary"
    aria-label="Portrait preview"
    aria-keyshortcuts="Escape ArrowLeft ArrowRight"
    tabIndex={-1}
    onKeyDown={(event) => {
      if (event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement || event.target instanceof HTMLSelectElement) return;
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        onNavigate(-1);
      }
      if (event.key === "ArrowRight") {
        event.preventDefault();
        onNavigate(1);
      }
    }}
  >
    <header className="portrait-preview-heading">
      <div><p className="preview-kicker">Focused portrait</p><h2>{portrait.name}</h2><p>{portrait.sourceName}</p></div>
      <button type="button" className="close-preview" onClick={onClose}>Close preview</button>
    </header>
    {portrait.description ? <p className="portrait-preview-description">{portrait.description}</p> : null}
    <div className="portrait-preview-labels" aria-label="Portrait labels">
      {portrait.labels.length ? portrait.labels.map((label) => <span key={`${label.category}:${label.value}`}>{label.category}: {label.value}</span>) : <span>No labels yet.</span>}
    </div>
    {editMetadata ? <MetadataEditor portraits={[portrait]} editMetadata={editMetadata} renameSource={renameSource} onSaved={onMetadataSaved} /> : null}
    <p className="preview-shortcuts">Use left and right arrows to move through this page. Escape closes the preview.</p>
    <div className="preview-images">
      {(["small", "medium", "large"] as Role[]).map((role) => <PreviewImage key={role} id={portrait.id} name={portrait.name} role={role} assetUrl={assetUrl} />)}
    </div>
  </aside>;
}
