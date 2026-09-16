import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CatalogPage, Page, Query } from "../../lib/contracts";

type CatalogClient = { queryCatalog(query: Query, page: Page): Promise<CatalogPage> };
type Options = { pageSize?: number; debounceMs?: number; refreshKey?: number };
type CatalogState = { data: CatalogPage | null; loading: boolean; error: unknown; pending: boolean; offset: number; setOffset(offset: number): void };
const DEFAULT_PAGE_SIZE = 100;
const DEFAULT_DEBOUNCE_MS = 250;

export function useCatalog(api: CatalogClient, query: Query, options: Options = {}): CatalogState {
  const pageSize = options.pageSize ?? DEFAULT_PAGE_SIZE;
  const debounceMs = options.debounceMs ?? DEFAULT_DEBOUNCE_MS;
  const refreshKey = options.refreshKey ?? 0;
  const queryCatalog = api.queryCatalog;
  const directKey = JSON.stringify({ sourceIds: [...query.sourceIds].sort(), labels: query.labels.map((label) => ({ ...label })).sort((a, b) => a.category.localeCompare(b.category) || a.value.localeCompare(b.value)), selectedOnly: query.selectedOnly, trash: query.trash });
  const queryKey = JSON.stringify([query.text, directKey]);
  const [debouncedText, setDebouncedText] = useState(query.text);
  const [windowState, setWindowState] = useState({ queryKey, offset: 0 });
  const [data, setData] = useState<CatalogPage | null>(null);
  const [committed, setCommitted] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const generation = useRef(0);
  const lastStarted = useRef<string | null>(null);
  const offset = windowState.queryKey === queryKey ? windowState.offset : 0;
  const directFilters = useMemo(() => JSON.parse(directKey) as Omit<Query, "text">, [directKey]);
  const desired = JSON.stringify([query.text, directKey, offset, pageSize, refreshKey]);
  const request = JSON.stringify([debouncedText, directKey, offset, pageSize, refreshKey]);
  const effectiveQuery = useMemo<Query>(() => ({ text: debouncedText, ...directFilters }), [debouncedText, directFilters]);

  useEffect(() => { generation.current += 1; setWindowState((current) => current.queryKey === queryKey ? current : { queryKey, offset: 0 }); }, [queryKey]);
  useEffect(() => { if (query.text === debouncedText) return; const timer = window.setTimeout(() => setDebouncedText(query.text), debounceMs); return () => window.clearTimeout(timer); }, [query.text, debouncedText, debounceMs]);
  useEffect(() => {
    if (committed === request) return;
    if (lastStarted.current === request) return;
    if (query.text !== debouncedText && committed === request) return;
    lastStarted.current = request;
    const current = ++generation.current;
    setLoading(true); setError(null);
    void queryCatalog(effectiveQuery, { offset, limit: pageSize }).then((response) => {
      if (generation.current === current) { setData(response); setCommitted(request); setLoading(false); }
    }, (reason) => { if (generation.current === current) { setError(reason); setCommitted(request); setLoading(false); } });
  }, [queryCatalog, effectiveQuery, offset, pageSize, query.text, debouncedText, request, committed]);
  useEffect(() => () => { generation.current += 1; lastStarted.current = null; }, []);
  const setOffset = useCallback((next: number) => setWindowState({ queryKey, offset: Math.max(0, Math.trunc(next)) }), [queryKey]);
  return { data, loading, error, pending: committed !== desired, offset, setOffset };
}
