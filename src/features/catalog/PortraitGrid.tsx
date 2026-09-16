import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useMemo, useRef, useState } from "react";
import type { Portrait, Role } from "../../lib/contracts";
import { PortraitCard } from "./PortraitCard";
type Props = { items: Portrait[]; total: number; role: Role; zoom?: number; loading?: boolean; queryPending?: boolean; error?: unknown; assetUrl(id: string, role: Role): string; onFocus(id: string): void; onToggleSelection(id: string, selected: boolean): void; selectionLabel?: (portrait: Portrait) => string };
const aspectRatio: Record<Role, number> = { small: 242 / 185, medium: 432 / 330, large: 1024 / 692 };
const message = (error: unknown) => error instanceof Error ? error.message : "Portraits could not be loaded.";
export function PortraitGrid({ items, total, role, zoom = 100, loading = false, queryPending = false, error, assetUrl, onFocus, onToggleSelection, selectionLabel }: Props) {
 const viewport = useRef<HTMLDivElement>(null);
 const [width, setWidth] = useState(760);
 const [scrollTop, setScrollTop] = useState(0);
 useEffect(() => {
   const element = viewport.current;
   if (!element) return;
   const update = () => {
     const style = getComputedStyle(element);
     const available = element.clientWidth - parseFloat(style.paddingLeft || "0") - parseFloat(style.paddingRight || "0");
     if (available > 0) setWidth(available);
   };
   update();
   const observer = typeof ResizeObserver !== "undefined" ? new ResizeObserver(update) : null;
   observer?.observe(element);
   window.addEventListener("resize", update);
   return () => { observer?.disconnect(); window.removeEventListener("resize", update); };
 }, [queryPending, error, total, loading]);
 const cardWidth = 144 * zoom / 100;
 const columns = Math.max(1, Math.floor((width + 12) / (cardWidth + 12)));
 const actualWidth = (width - (columns - 1) * 12) / columns;
 const rowHeight = Math.ceil((actualWidth - 14) * aspectRatio[role] + 68);
 const rows = useMemo(() => Array.from({ length: Math.ceil(items.length / columns) }, (_, row) => items.slice(row * columns, row * columns + columns)), [items, columns]);
 const virtualizer = useVirtualizer({ count: rows.length, getScrollElement: () => viewport.current, estimateSize: () => rowHeight, overscan: 2 });
 useEffect(() => { virtualizer.measure(); }, [virtualizer, rowHeight, columns]);
 const virtualRows = virtualizer.getVirtualItems();
 const renderedRows = virtualRows.length > 0 ? virtualRows : Array.from({ length: Math.min(5, rows.length) }, (_, offset) => {
   const index = Math.min(rows.length - 1, Math.floor(scrollTop / rowHeight) + offset);
   return { key: index, index, start: index * rowHeight };
 });
 if (error) return <section className="catalog-state" role="alert">{message(error)}</section>;
 if (queryPending) return <section className="catalog-state" aria-live="polite">Updating results…</section>;
 if (!loading && total === 0) return <section className="catalog-state">No portraits match these filters.</section>;
 return <div className="portrait-grid-viewport" ref={viewport} role="region" aria-label="Portrait results" tabIndex={0} onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}><div className="portrait-grid-space" style={{ height: virtualizer.getTotalSize() }}>{renderedRows.map((virtualRow) => <div className="portrait-grid-row" data-index={virtualRow.index} ref={virtualizer.measureElement} key={virtualRow.key} style={{ transform: `translateY(${virtualRow.start}px)`, height: rowHeight, gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}>{rows[virtualRow.index].map((portrait) => <PortraitCard key={portrait.id} portrait={portrait} role={role} assetUrl={assetUrl} onFocus={onFocus} onToggleSelection={onToggleSelection} selectionLabel={selectionLabel} />)}</div>)}</div></div>;
}
