import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Portrait, Role } from "../../lib/contracts";
import { PortraitCard } from "./PortraitCard";
type Props = { items: Portrait[]; total: number; role: Role; loading?: boolean; queryPending?: boolean; error?: unknown; assetUrl(id: string, role: Role): string; onFocus(id: string): void; onToggleSelection(id: string, selected: boolean): void; selectionLabel?: (portrait: Portrait) => string };
const roleHeight: Record<Role, number> = { small: 250, medium: 295, large: 390 };
const message = (error: unknown) => error instanceof Error ? error.message : "Portraits could not be loaded.";
export function PortraitGrid({ items, total, role, loading = false, queryPending = false, error, assetUrl, onFocus, onToggleSelection, selectionLabel }: Props) {
 const viewport = useRef<HTMLDivElement>(null); const [columns, setColumns] = useState(3); const [scrollTop, setScrollTop] = useState(0);
 useEffect(() => { const update = () => { const width = viewport.current?.clientWidth ?? 900; setColumns(width >= 1200 ? 5 : width >= 900 ? 4 : width >= 620 ? 3 : 2); }; update(); window.addEventListener("resize", update); return () => window.removeEventListener("resize", update); }, []);
 const rows = useMemo(() => Array.from({ length: Math.ceil(items.length / columns) }, (_, row) => items.slice(row * columns, row * columns + columns)), [items, columns]);
 const virtualizer = useVirtualizer({ count: rows.length, getScrollElement: () => viewport.current, estimateSize: () => roleHeight[role], overscan: 2 });
 const virtualRows = virtualizer.getVirtualItems();
 const renderedRows = virtualRows.length > 0 ? virtualRows : Array.from({ length: Math.min(5, rows.length) }, (_, offset) => {
   const index = Math.min(rows.length - 1, Math.floor(scrollTop / roleHeight[role]) + offset);
   return { key: index, index, start: index * roleHeight[role] };
 });
 if (error) return <section className="catalog-state" role="alert">{message(error)}</section>;
 if (queryPending) return <section className="catalog-state" aria-live="polite">Updating results…</section>;
 if (!loading && total === 0) return <section className="catalog-state">No portraits match these filters.</section>;
 return <div className="portrait-grid-viewport" ref={viewport} role="region" aria-label="Portrait results" tabIndex={0} onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}><div className="portrait-grid-space" style={{ height: virtualizer.getTotalSize() }}>{renderedRows.map((virtualRow) => <div className="portrait-grid-row" data-index={virtualRow.index} ref={virtualizer.measureElement} key={virtualRow.key} style={{ transform: `translateY(${virtualRow.start}px)`, gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}>{rows[virtualRow.index].map((portrait) => <PortraitCard key={portrait.id} portrait={portrait} role={role} assetUrl={assetUrl} onFocus={onFocus} onToggleSelection={onToggleSelection} selectionLabel={selectionLabel} />)}</div>)}</div></div>;
}
