import { useState } from "react";

import type { ImportRequest } from "../../lib/contracts";
import type { LibraryApi } from "../../lib/api";

type Props = {
  api: LibraryApi;
  onClose: () => void;
  onStart: (request: ImportRequest) => Promise<void>;
};

function sourceNameFromPath(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts.at(-1)?.replace(/\.(zip|rar|7z)$/i, "") || "Imported portraits";
}

export function ImportDialog({ api, onClose, onStart }: Props) {
  const [path, setPath] = useState("");
  const [sourceName, setSourceName] = useState("");
  const [kind, setKind] = useState<ImportRequest["kind"]>("folder");
  const [resize, setResize] = useState(false);
  const [starting, setStarting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const choose = async (nextKind: "folder" | "archive") => {
    setError(null);
    try {
      const selected = nextKind === "folder" ? await api.chooseImportFolder() : await api.chooseImportArchive();
      if (selected) {
        setPath(selected);
        setKind(nextKind);
        setSourceName(sourceNameFromPath(selected));
      }
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "The system file picker is unavailable.");
    }
  };

  const submit = async () => {
    if (!path) return;
    setStarting(true);
    setError(null);
    try {
      await onStart({ path, sourceName: sourceName.trim() || sourceNameFromPath(path), kind, resize });
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "The import could not start.");
      setStarting(false);
    }
  };

  return (
    <div className="import-backdrop" role="presentation">
      <section className="import-dialog" role="dialog" aria-modal="true" aria-labelledby="import-title">
        <header>
          <div>
            <p className="section-kicker">Add portraits</p>
            <h2 id="import-title">Import portraits</h2>
          </div>
          <button className="close-dialog" type="button" onClick={onClose} disabled={starting} aria-label="Close import dialog">×</button>
        </header>
        <p className="import-copy">Choose a folder with complete three-image sets, or a ZIP, RAR, or 7z archive.</p>
        <div className="import-choices">
          <button type="button" className="secondary-action" onClick={() => void choose("folder")} disabled={starting}>Choose folder</button>
          <button type="button" className="secondary-action" onClick={() => void choose("archive")} disabled={starting}>Choose archive</button>
        </div>
        <label className="path-field">
          <span>Source</span>
          <input value={path} readOnly placeholder="No source selected" aria-label="Import source" />
        </label>
        <label className="path-field">
          <span>Source name</span>
          <input value={sourceName} onChange={(event) => setSourceName(event.target.value)} aria-label="Source name" placeholder="Imported portraits" />
        </label>
        <label className="resize-option">
          <input aria-label="Fit and pad to game sizes" type="checkbox" checked={resize} onChange={(event) => setResize(event.target.checked)} disabled={starting} />
          <span>
            <strong>Fit and pad to game sizes</strong>
            <small>Resizes managed copies only. Originals stay unchanged.</small>
          </span>
        </label>
        {error ? <p className="import-error" role="alert">{error}</p> : null}
        <footer>
          <button type="button" className="secondary-action" onClick={onClose} disabled={starting}>Cancel</button>
          <button type="button" className="primary-action" onClick={() => void submit()} disabled={!path || starting}>Start import</button>
        </footer>
      </section>
    </div>
  );
}
