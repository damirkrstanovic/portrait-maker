import type { AppError } from "../lib/contracts";

type Props = {
  busy: boolean;
  error: AppError | null;
  lastLibraryPath: string | null;
  onCreate: () => void;
  onOpen: () => void;
  onReopen: () => void;
  onRestore?: () => void;
  desktopRequired?: boolean;
};

function errorTitle(code: string): string {
  if (code === "LIBRARY_LOCKED") return "Library already open";
  if (code === "LIBRARY_FORMAT_TOO_NEW" || code === "LIBRARY_SCHEMA_TOO_NEW") {
    return "App update required";
  }
  if (code === "LIBRARY_DESTINATION_NOT_EMPTY") return "Folder is not empty";
  return "Library could not be opened";
}

export function LibraryChooser({
  busy,
  error,
  lastLibraryPath,
  onCreate,
  onOpen,
  onReopen,
  onRestore,
  desktopRequired = false,
}: Props) {
  return (
    <main className="chooser-shell">
      <section className="chooser-panel" aria-labelledby="chooser-title">
        <div className="brand-mark" aria-hidden="true">
          <span className="portrait portrait-small" />
          <span className="portrait portrait-medium" />
          <span className="portrait portrait-large" />
        </div>
        <p className="product-name">Pathfinder Portrait Manager</p>
        <h1 id="chooser-title">Your portrait library</h1>
        <p className="chooser-intro">
          Keep game-ready portraits, labels, and selections together in a portable folder.
        </p>

        {error ? (
          <div className="error-notice" role="alert">
            <strong>{errorTitle(error.code)}</strong>
            <span>{error.message}</span>
          </div>
        ) : null}

        <div className="chooser-actions">
          <button className="primary-action" type="button" disabled={busy} onClick={onCreate}>
            Create library
          </button>
          <button className="secondary-action" type="button" disabled={busy} onClick={onOpen}>
            Open library
          </button>
        </div>

        {onRestore && <button className="secondary-action restore-library-action" type="button" disabled={busy} onClick={onRestore}>Restore library backup</button>}

        {lastLibraryPath ? (
          <div className="recent-library">
            <div>
              <span>Last library</span>
              <p title={lastLibraryPath}>{lastLibraryPath}</p>
            </div>
            <button type="button" disabled={busy} onClick={onReopen}>
              Reopen library
            </button>
          </div>
        ) : null}

        <p className="privacy-note">All files stay on this computer.</p>
        {desktopRequired ? <p className="browser-note" role="status">Open this app in the Pathfinder Portrait Manager desktop application to choose and manage a library.</p> : null}
      </section>
      <aside className="format-guide" aria-label="Portrait format guide">
        <p>One library. Three game-ready formats.</p>
        <dl>
          <div><dt>Small</dt><dd>185 × 242</dd></div>
          <div><dt>Medium</dt><dd>330 × 432</dd></div>
          <div><dt>Full length</dt><dd>692 × 1024</dd></div>
        </dl>
      </aside>
    </main>
  );
}
