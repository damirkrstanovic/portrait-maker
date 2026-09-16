import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { LibraryApi } from "../../lib/api";
import type { CatalogFacets, Job, Portrait, Query, Role } from "../../lib/contracts";
import { FilterSidebar } from "./FilterSidebar";
import { PortraitPreview } from "./PortraitPreview";
import { MetadataEditor } from "../metadata/MetadataEditor";
import { SelectionToolbar } from "../selection/SelectionToolbar";
import { TrashView } from "../trash/TrashView";
import { PortraitGrid } from "./PortraitGrid";
import { SearchBar } from "./SearchBar";
import { useCatalog } from "./useCatalog";
import { loadCatalogPreferences, saveCatalogPreferences } from "../../lib/preferences";

const emptyFacets: CatalogFacets = { sources: [], labels: [] };
const initialQuery: Query = { text: "", sourceIds: [], labels: [], selectedOnly: false, trash: false };
type SelectionSnapshot = { total: number; revision: number; trash: boolean };
export function assetUrl(id: string, role: Role, variant: "thumbnail" | "original" = "thumbnail") {
  if (import.meta.env.DEV && new URLSearchParams(window.location.search).has("testCatalog")) return "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='692' height='1024'%3E%3Crect width='100%25' height='100%25' fill='%23d8dee4'/%3E%3C/svg%3E";
  const route = variant === "thumbnail" ? `/thumbnail/${encodeURIComponent(id)}/${role}/360` : `/original/${encodeURIComponent(id)}/${role}`;
  if (!("__TAURI_INTERNALS__" in window)) return `http://portrait.localhost${route}`;
  return convertFileSrc(route, "portrait");
}

export function CatalogBrowser({ api, onImport, refreshKey = 0 }: { api: LibraryApi; onImport(): void; refreshKey?: number }) {
  const savedPreferences = loadCatalogPreferences();
  const [query, setQuery] = useState<Query>(initialQuery);
  const [facets, setFacets] = useState<CatalogFacets>(emptyFacets);
  const [role, setRole] = useState<Role>(savedPreferences.gridRole);
  const [focusedId, setFocusedId] = useState<string | null>(null);
  const [previewVisible, setPreviewVisible] = useState(savedPreferences.previewVisible);
  const [selectionSnapshot, setSelectionSnapshot] = useState<SelectionSnapshot | null>(null);
  const [selectionCountPending, setSelectionCountPending] = useState(!!api.changeSelection);
  const [selectionRefresh, setSelectionRefresh] = useState(0);
  const [editingSelection, setEditingSelection] = useState(false);
  const [bulkPortraits, setBulkPortraits] = useState<Portrait[]>([]);
  const [mutationError, setMutationError] = useState<string | null>(null);
  const [trashMarked, setTrashMarked] = useState<Set<string>>(() => new Set());
  const [trashTotal, setTrashTotal] = useState<number | null>(null);
  const [trashCountError, setTrashCountError] = useState<string | null>(null);
  const [purgeJob, setPurgeJob] = useState<Job | null>(null);
  const previewOrigin = useRef<HTMLElement | null>(null);
  const restorePreviewOrigin = useRef(false);
  const bulkEditTrigger = useRef<HTMLButtonElement>(null);
  const bulkDialog = useRef<HTMLDivElement>(null);
  const bulkClose = useRef<HTMLButtonElement>(null);
  const restoreBulkEditTrigger = useRef(false);
  const catalogRefreshKey = refreshKey + selectionRefresh;
  const catalog = useCatalog(api, query, { pageSize: 200, refreshKey: catalogRefreshKey });
  useEffect(() => { void api.getCatalogFacets?.().then(setFacets).catch(() => setFacets(emptyFacets)); }, [api, catalogRefreshKey]);
  useEffect(() => {
    let active = true;
    setTrashTotal(null);
    setTrashCountError(null);
    void api.queryCatalog({ text: "", sourceIds: [], labels: [], selectedOnly: false, trash: true }, { offset: 0, limit: 1 })
      .then((response) => { if (active) setTrashTotal(response.total); })
      .catch((caught) => { if (active) setTrashCountError(caught instanceof Error ? caught.message : "The full trash count could not be loaded."); });
    return () => { active = false; };
  }, [api, catalogRefreshKey]);
  useEffect(() => {
    if (!purgeJob || purgeJob.state !== "running") return;
    let active = true;
    const refresh = async () => {
      try {
        const next = await api.getJob(purgeJob.id);
        if (!active || !next) return;
        setPurgeJob(next);
        if (next.state !== "running") {
          if (next.state === "done") { setTrashMarked(new Set()); refreshMutations(); }
          else if (next.state === "failed") setMutationError(next.message);
        }
      } catch (caught) { if (active) setMutationError(caught instanceof Error ? caught.message : "The permanent deletion status could not be loaded."); }
    };
    void refresh();
    const timer = window.setInterval(() => void refresh(), 300);
    return () => { active = false; window.clearInterval(timer); };
  }, [api, purgeJob?.id, purgeJob?.state]);
  useEffect(() => {
    if (!api.changeSelection) return;
    let active = true;
    setSelectionSnapshot(null);
    setSelectionCountPending(true);
    void api.queryCatalog({ text: "", sourceIds: [], labels: [], selectedOnly: true, trash: false }, { offset: 0, limit: 1 })
      .then((page) => {
        if (!active) return;
        setSelectionSnapshot({ total: page.total, revision: page.revision, trash: false });
        setSelectionCountPending(false);
      })
      .catch((caught) => {
        if (!active) return;
        setSelectionSnapshot(null);
        setSelectionCountPending(false);
        setMutationError(caught instanceof Error ? caught.message : "The persisted selection count could not be loaded.");
      });
    return () => { active = false; };
  }, [api, catalogRefreshKey]);
  useEffect(() => { void api.getDisplayRole?.().then((next) => { setRole(next); saveCatalogPreferences({ gridRole: next }); }).catch(() => {}); }, [api]);
  const page = catalog.data;
  const pageItems = page?.items ?? [];
  const displayItems = query.trash ? pageItems.map((portrait) => ({ ...portrait, selected: trashMarked.has(portrait.id) })) : pageItems;
  const currentSelectionCount = selectionSnapshot?.total ?? null;
  const focusedPortrait = pageItems.find((portrait) => portrait.id === focusedId) ?? null;
  const pageNumber = Math.floor(catalog.offset / 200) + 1;
  const pageCount = Math.max(1, Math.ceil((page?.total ?? 0) / 200));
  useEffect(() => {
    if (previewVisible || !restorePreviewOrigin.current) return;
    restorePreviewOrigin.current = false;
    if (previewOrigin.current?.isConnected) {
      previewOrigin.current.focus();
      return;
    }
    document.querySelector<HTMLButtonElement>(".preview-toggle:not(:disabled)")?.focus();
  }, [previewVisible]);
  useLayoutEffect(() => {
    if (editingSelection) bulkClose.current?.focus();
    if (!editingSelection && !selectionCountPending && restoreBulkEditTrigger.current && bulkEditTrigger.current) {
      bulkEditTrigger.current.focus();
      restoreBulkEditTrigger.current = false;
    }
  }, [editingSelection, selectionCountPending, currentSelectionCount]);
  const rememberPreviewOrigin = () => {
    previewOrigin.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  };
  const focusPortrait = (id: string) => { rememberPreviewOrigin(); setFocusedId(id); setPreviewVisible(true); saveCatalogPreferences({ previewVisible: true }); };
  const closePreview = () => { restorePreviewOrigin.current = true; setPreviewVisible(false); saveCatalogPreferences({ previewVisible: false }); };
  const navigatePreview = (step: -1 | 1) => {
    const index = pageItems.findIndex((portrait) => portrait.id === focusedId);
    if (index < 0) return;
    const next = pageItems[index + step];
    if (next) setFocusedId(next.id);
  };
  const refreshMutations = (count?: number) => {
    setMutationError(null);
    void count;
    setSelectionCountPending(true);
    setSelectionRefresh((current) => current + 1);
  };
  const collectPortraitIds = async (candidate: Query) => {
    const ids: string[] = [];
    let snapshot: { total: number; revision: number } | null = null;
    for (let offset = 0; ; offset += 200) {
      const response = await api.queryCatalog(candidate, { offset, limit: 200 });
      if (!snapshot) snapshot = { total: response.total, revision: response.revision };
      if (snapshot.total !== response.total || snapshot.revision !== response.revision) throw new Error("The trash changed while it was being collected. Try again.");
      ids.push(...response.items.map((portrait) => portrait.id));
      if (ids.length >= response.total || !response.items.length) return ids;
    }
  };
  const runTrashMutation = async (operation: () => Promise<void>) => {
    try { setMutationError(null); await operation(); setTrashMarked(new Set()); refreshMutations(); }
    catch (caught) { setMutationError(caught instanceof Error ? caught.message : "The trash could not be updated."); }
  };
  const moveSelectionToTrash = () => void runTrashMutation(async () => {
    if (!api.trashPortraits) return;
    await api.trashPortraits(await collectPortraitIds({ text: "", sourceIds: [], labels: [], selectedOnly: true, trash: false }));
  });
  const restoreTrash = (ids: string[]) => void runTrashMutation(async () => { await api.restorePortraits?.(ids); });
  const startPurge = (ids: string[]) => void (async () => {
    try { setMutationError(null); const job = await api.startPurge?.(ids); if (!job) throw new Error("Permanent deletion is unavailable."); setPurgeJob(job); }
    catch (caught) { setMutationError(caught instanceof Error ? caught.message : "The permanent deletion could not start."); }
  })();
  const purgeTrash = (ids: string[]) => startPurge(ids);
  const emptyTrash = () => void (async () => {
    try { startPurge(await collectPortraitIds({ text: "", sourceIds: [], labels: [], selectedOnly: false, trash: true })); }
    catch (caught) { setMutationError(caught instanceof Error ? caught.message : "The trash could not be collected for deletion."); }
  })();
  const toggleSelection = (id: string, selected: boolean) => {
    if (!api.changeSelection) return;
    void api.changeSelection({ ids: [id] }, selected ? "add" : "remove").then(refreshMutations, (caught) => setMutationError(caught instanceof Error ? caught.message : "Selection could not be changed."));
  };
  const editPersistedSelection = async () => {
    try {
      const snapshot = selectionSnapshot;
      if (!snapshot) throw new Error("The persisted selection count is not available.");
      const portraits: Portrait[] = [];
      const selectedQuery: Query = { text: "", sourceIds: [], labels: [], selectedOnly: true, trash: snapshot.trash };
      for (let offset = 0; ; offset += 200) {
        const page = await api.queryCatalog(selectedQuery, { offset, limit: 200 });
        if (page.total !== snapshot.total || page.revision !== snapshot.revision) {
          throw new Error("The selection changed while it was being collected. Try again.");
        }
        portraits.push(...page.items);
        if (portraits.length >= page.total || !page.items.length) break;
      }
      setBulkPortraits(portraits);
      setEditingSelection(true);
    } catch (caught) {
      setMutationError(caught instanceof Error ? caught.message : "Selected portraits could not be loaded.");
    }
  };
  const closeBulkEditor = () => {
    restoreBulkEditTrigger.current = true;
    setEditingSelection(false);
  };
  const containBulkDialog = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closeBulkEditor();
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = Array.from(bulkDialog.current?.querySelectorAll<HTMLElement>("button, input, textarea, select") ?? [])
      .filter((element) => !element.hasAttribute("disabled"));
    if (!focusable.length) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if ((event.shiftKey && document.activeElement === first) || (!event.shiftKey && document.activeElement === last)) {
      event.preventDefault();
      (event.shiftKey ? last : first).focus();
    }
  };
  return <section className={`catalog-layout${previewVisible && focusedPortrait ? " is-preview-visible" : ""}`}>
    <FilterSidebar facets={facets} query={query} onQuery={setQuery} />
    <div className="catalog-main">
      <div className="catalog-controls"><SearchBar text={query.text} total={page?.total ?? 0} role={role} previewVisible={previewVisible && !!focusedPortrait} previewAvailable={!!focusedPortrait} onTogglePreview={() => previewVisible ? closePreview() : focusPortrait(focusedPortrait!.id)} onText={(text) => setQuery({ ...query, text })} onRole={(next) => { setRole(next); saveCatalogPreferences({ gridRole: next }); void api.setDisplayRole?.(next); }} />
      {!query.trash && api.changeSelection ? <SelectionToolbar query={query} matchingCount={page?.total ?? 0} selectedCount={currentSelectionCount} changeSelection={api.changeSelection} onChanged={refreshMutations} /> : null}
      {!query.trash && currentSelectionCount !== null && currentSelectionCount > 0 && api.trashPortraits ? <button type="button" className="trash-selected" onClick={moveSelectionToTrash}>Move {currentSelectionCount.toLocaleString()} selected to trash</button> : null}
      {!query.trash && currentSelectionCount !== null && currentSelectionCount > 0 && api.editMetadata ? <button ref={bulkEditTrigger} type="button" className="edit-selected" onClick={() => void editPersistedSelection()}>Edit {currentSelectionCount} selected</button> : null}
      {query.trash && api.restorePortraits && api.startPurge ? <TrashView total={page?.total ?? 0} emptyCount={trashTotal} markedIds={trashMarked} purgeJob={purgeJob} onRestore={restoreTrash} onPurge={purgeTrash} onEmpty={emptyTrash} onCancelPurge={() => { if (purgeJob) void api.cancelJob(purgeJob.id).catch((caught) => setMutationError(caught instanceof Error ? caught.message : "The deletion could not be cancelled.")); }} /> : null}</div>
      {trashCountError ? <p className="catalog-mutation-error" role="alert">{trashCountError}</p> : null}
      {mutationError ? <p className="catalog-mutation-error" role="alert">{mutationError}</p> : null}
      {page?.total === 0 && !catalog.pending && !catalog.error ? <button className="primary-action empty-import" type="button" onClick={onImport}>Import portraits</button> : null}
      <PortraitGrid items={displayItems} total={page?.total ?? 0} role={role} loading={catalog.loading} queryPending={catalog.pending} error={catalog.error} assetUrl={(id, currentRole) => assetUrl(id, currentRole)} onFocus={focusPortrait} onToggleSelection={query.trash ? (id, marked) => setTrashMarked((current) => { const next = new Set(current); if (marked) next.add(id); else next.delete(id); return next; }) : toggleSelection} selectionLabel={query.trash ? (portrait) => `Mark ${portrait.name} for trash action` : undefined} />
      {page && page.total > 200 ? <nav className="catalog-pages" aria-label="Catalog pages"><button type="button" onClick={() => catalog.setOffset(catalog.offset - 200)} disabled={catalog.offset === 0}>Previous page</button><span>Page {pageNumber} of {pageCount}</span><button type="button" onClick={() => catalog.setOffset(catalog.offset + 200)} disabled={catalog.offset + 200 >= page.total}>Next page</button></nav> : null}
    </div>
    <PortraitPreview portrait={focusedPortrait} visible={previewVisible} assetUrl={assetUrl} onClose={closePreview} onNavigate={navigatePreview} editMetadata={api.editMetadata} renameSource={api.renameSource} onMetadataSaved={() => refreshMutations()} />
    {editingSelection && api.editMetadata ? <div className="metadata-backdrop" role="dialog" aria-modal="true" aria-label="Edit selected metadata" onKeyDownCapture={containBulkDialog}><div ref={bulkDialog} className="metadata-dialog"><button ref={bulkClose} type="button" className="close-dialog" onClick={closeBulkEditor}>Close</button><MetadataEditor portraits={bulkPortraits} editMetadata={api.editMetadata} onSaved={() => { closeBulkEditor(); refreshMutations(); }} /></div></div> : null}
  </section>;
}
