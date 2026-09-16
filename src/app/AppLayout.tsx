import { useState, useRef, type ReactNode, type RefObject } from "react";

import type { LibraryInfo } from "../lib/api";

type Props = {
  library: LibraryInfo;
  busy: boolean;
  onImport(): void;
  onExport?(): void;
  onDestinations(): void;
  destinationsTriggerRef?: RefObject<HTMLButtonElement | null>;
  onBackup(): void;
  onCloseLibrary(): void;
  children: ReactNode;
};

export function AppLayout({ library, busy, onImport, onExport, onDestinations, destinationsTriggerRef, onBackup, onCloseLibrary, children }: Props) {
  const [menu, setMenu] = useState(false);
  const menuTrigger = useRef<HTMLButtonElement>(null);
  return <main className="library-shell">
    <header className="library-header">
      <div>
        <p className="product-name">Pathfinder Portrait Manager</p>
        <h1>{library.name}</h1>
        <p className="library-path" title={library.path}>{library.path}</p>
      </div>
      <div className="library-actions">
        <button type="button" className="primary-action" disabled={busy} onClick={onImport}>Import portraits</button>
        <button type="button" className="secondary-action" disabled={busy} onClick={onExport}>Export portraits</button>
        <button ref={destinationsTriggerRef} type="button" className="secondary-action" disabled={busy} onClick={onDestinations}>Game destinations</button>
        <div className="library-menu" onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget)) setMenu(false); }} onKeyDown={event => { if (event.key === "Escape") { setMenu(false); menuTrigger.current?.focus(); } }}>
          <button ref={menuTrigger} type="button" className="secondary-action" disabled={busy} aria-expanded={menu} aria-controls="library-menu-actions" onClick={() => setMenu(!menu)}>Library ▾</button>
          {menu && <div className="library-menu-actions" id="library-menu-actions">
            <button type="button" onClick={() => { setMenu(false); onBackup(); }}>Back up library</button>
            <button type="button" onClick={() => { setMenu(false); onCloseLibrary(); }}>Close library</button>
          </div>}
        </div>
      </div>
    </header>
    {children}
  </main>;
}
