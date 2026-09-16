import { act, renderHook } from "@testing-library/react";
import { vi } from "vitest";

import type { CatalogPage, Query } from "../../lib/contracts";
import { useCatalog } from "./useCatalog";

const baseQuery: Query = {
  text: "",
  sourceIds: [],
  labels: [],
  selectedOnly: false,
  trash: false,
};

const page = (name: string, selected = false): CatalogPage => ({
  items: [{
    id: name.toLowerCase(),
    sourceId: "source",
    name,
    sourceName: "Pack",
    originalFolder: name,
    description: null,
    labels: [],
    selected,
    trashedAt: null,
  }],
  total: 1,
  revision: 1,
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((accept) => { resolve = accept; });
  return { promise, resolve };
}

test("debounces text while applying direct filters immediately and resetting the page", async () => {
  vi.useFakeTimers();
  const queryCatalog = vi.fn().mockResolvedValue(page("Result"));
  const { result, rerender } = renderHook(
    ({ query }) => useCatalog({ queryCatalog }, query, { pageSize: 40, debounceMs: 250 }),
    { initialProps: { query: baseQuery } },
  );
  await act(async () => {});
  expect(queryCatalog).toHaveBeenCalledWith(baseQuery, { offset: 0, limit: 40 });

  act(() => result.current.setOffset(80));
  await act(async () => {});
  expect(queryCatalog).toHaveBeenLastCalledWith(baseQuery, { offset: 80, limit: 40 });

  rerender({ query: { ...baseQuery, sourceIds: ["source-2"] } });
  await act(async () => {});
  expect(queryCatalog).toHaveBeenLastCalledWith(
    { ...baseQuery, sourceIds: ["source-2"] },
    { offset: 0, limit: 40 },
  );

  rerender({ query: { ...baseQuery, sourceIds: ["source-2"], text: "elf" } });
  expect(queryCatalog).toHaveBeenCalledTimes(3);
  await act(async () => { await vi.advanceTimersByTimeAsync(249); });
  expect(queryCatalog).toHaveBeenCalledTimes(3);
  await act(async () => { await vi.advanceTimersByTimeAsync(1); });
  expect(queryCatalog).toHaveBeenLastCalledWith(
    { ...baseQuery, sourceIds: ["source-2"], text: "elf" },
    { offset: 0, limit: 40 },
  );
  vi.useRealTimers();
});

test("an out-of-order response cannot replace the current query results", async () => {
  vi.useFakeTimers();
  const oldResponse = deferred<CatalogPage>();
  const newResponse = deferred<CatalogPage>();
  const queryCatalog = vi.fn()
    .mockReturnValueOnce(oldResponse.promise)
    .mockReturnValueOnce(newResponse.promise);
  const { result, rerender } = renderHook(
    ({ query }) => useCatalog({ queryCatalog }, query, { debounceMs: 200 }),
    { initialProps: { query: baseQuery } },
  );
  await act(async () => {});
  expect(queryCatalog).toHaveBeenCalledTimes(1);

  rerender({ query: { ...baseQuery, text: "current" } });
  await act(async () => { await vi.advanceTimersByTimeAsync(200); });
  expect(queryCatalog).toHaveBeenCalledTimes(2);
  await act(async () => newResponse.resolve(page("Current", true)));
  expect(result.current.data?.items[0].name).toBe("Current");

  await act(async () => oldResponse.resolve(page("Stale")));
  expect(result.current.data?.items[0].name).toBe("Current");
  expect(result.current.data?.items[0].selected).toBe(true);
  vi.useRealTimers();
});

test("keeps the current selected result while a changed query is pending", async () => {
  vi.useFakeTimers();
  const pending = deferred<CatalogPage>();
  const queryCatalog = vi.fn()
    .mockResolvedValueOnce(page("Selected", true))
    .mockReturnValueOnce(pending.promise);
  const { result, rerender } = renderHook(
    ({ query }) => useCatalog({ queryCatalog }, query, { debounceMs: 100 }),
    { initialProps: { query: baseQuery } },
  );
  await act(async () => {});
  expect(result.current.data?.items[0].selected).toBe(true);

  rerender({ query: { ...baseQuery, text: "new" } });
  await act(async () => { await vi.advanceTimersByTimeAsync(100); });
  expect(queryCatalog).toHaveBeenCalledTimes(2);
  expect(result.current.data?.items[0].selected).toBe(true);
  vi.useRealTimers();
});

test("marks a direct filter change pending before its request effect can paint retained rows", async () => {
  const first = deferred<CatalogPage>();
  const second = deferred<CatalogPage>();
  const queryCatalog = vi.fn().mockReturnValueOnce(first.promise).mockReturnValueOnce(second.promise);
  const { result, rerender } = renderHook(
    ({ query }) => useCatalog({ queryCatalog }, query),
    { initialProps: { query: baseQuery } },
  );
  await act(async () => first.resolve(page("Old ranger")));
  expect(result.current.pending).toBe(false);

  rerender({ query: { ...baseQuery, sourceIds: ["new-source"] } });
  expect(result.current.pending).toBe(true);
  await act(async () => second.resolve(page("New ranger")));
  expect(result.current.pending).toBe(false);
});

test("value-equivalent filter arrays do not restart a logical request", async () => {
  const queryCatalog = vi.fn(() => new Promise<CatalogPage>(() => {}));
  renderHook(() => useCatalog(
    { queryCatalog },
    {
      ...baseQuery,
      sourceIds: [...baseQuery.sourceIds],
      labels: baseQuery.labels.map((label) => ({ ...label })),
    },
  ));

  await act(async () => {});
  expect(queryCatalog).toHaveBeenCalledOnce();
});

test("applies direct filters with the last debounced text while new text is pending", async () => {
  vi.useFakeTimers();
  const queryCatalog = vi.fn().mockResolvedValue(page("Result"));
  const { rerender } = renderHook(
    ({ query }) => useCatalog({ queryCatalog }, query, { debounceMs: 200 }),
    { initialProps: { query: baseQuery } },
  );
  await act(async () => {});

  rerender({ query: { ...baseQuery, text: "elf" } });
  expect(queryCatalog).toHaveBeenCalledTimes(1);
  rerender({ query: { ...baseQuery, text: "elf", sourceIds: ["source-2"] } });
  await act(async () => {});
  expect(queryCatalog).toHaveBeenCalledTimes(2);
  expect(queryCatalog).toHaveBeenLastCalledWith(
    { ...baseQuery, sourceIds: ["source-2"], text: "" },
    { offset: 0, limit: 100 },
  );

  await act(async () => { await vi.advanceTimersByTimeAsync(200); });
  expect(queryCatalog).toHaveBeenCalledTimes(3);
  expect(queryCatalog).toHaveBeenLastCalledWith(
    { ...baseQuery, sourceIds: ["source-2"], text: "elf" },
    { offset: 0, limit: 100 },
  );
  vi.useRealTimers();
});
