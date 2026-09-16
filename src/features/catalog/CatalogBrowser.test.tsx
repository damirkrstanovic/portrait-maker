import { StrictMode } from "react";
import { createEvent, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { mockConvertFileSrc } from "@tauri-apps/api/mocks";
import { vi } from "vitest";

import type { LibraryApi } from "../../lib/api";
import { assetUrl, CatalogBrowser } from "./CatalogBrowser";

const all = Array.from({ length: 10_000 }, (_, index) => ({
  id: `portrait-${index}`, sourceId: "source", name: `Portrait ${index}`,
  sourceName: "Pack", originalFolder: `portrait-${index}`, description: null,
  labels: [], selected: false, trashedAt: null,
}));

test("navigates a bounded catalog page window beyond the first 200 records", async () => {
  const queryCatalog = vi.fn(async (_query, page) => ({
    items: all.slice(page.offset, page.offset + page.limit), total: all.length, revision: 1,
  }));
  const api = { queryCatalog, getCatalogFacets: async () => ({ sources: [], labels: [] }) } as unknown as LibraryApi;
  render(<CatalogBrowser api={api} onImport={vi.fn()} />);

  expect(await screen.findByText("Portrait 0")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Next page" }));
  expect(await screen.findByText("Portrait 200")).toBeInTheDocument();
  expect(queryCatalog).toHaveBeenLastCalledWith(expect.anything(), { offset: 200, limit: 200 });
  expect(screen.getAllByTestId("portrait-card").length).toBeLessThan(200);
});

test("settles the initial catalog request under the desktop entrypoint's StrictMode", async () => {
  const api = { queryCatalog: vi.fn().mockResolvedValue({ items: [all[0]], total: 1, revision: 1 }), getCatalogFacets: async () => ({ sources: [], labels: [] }) } as unknown as LibraryApi;
  render(<StrictMode><CatalogBrowser api={api} onImport={vi.fn()} /></StrictMode>);
  expect(await screen.findByText("Portrait 0")).toBeInTheDocument();
});

test("uses Tauri's platform URL helper for the percent-encoded protocol route", () => {
  mockConvertFileSrc("windows");
  expect(assetUrl("c058f994-faf8-49d2-bc4f-1f9b7e1eb034", "large")).toBe(
    "http://portrait.localhost/%2Fthumbnail%2Fc058f994-faf8-49d2-bc4f-1f9b7e1eb034%2Flarge%2F360",
  );
});

test("refreshes the page after import without losing the active query", async () => {
  const queryCatalog = vi.fn().mockResolvedValue({ items: [], total: 0, revision: 1 });
  const api = { queryCatalog, getCatalogFacets: vi.fn().mockResolvedValue({ sources: [], labels: [] }) } as unknown as LibraryApi;
  const view = render(<CatalogBrowser api={api} onImport={vi.fn()} />);
  const search = await screen.findByRole("textbox", { name: "Search portraits" });
  fireEvent.change(search, { target: { value: "ranger" } });
  view.rerender(<CatalogBrowser api={api} onImport={vi.fn()} refreshKey={1} />);
  expect(search).toHaveValue("ranger");
  expect(api.getCatalogFacets).toHaveBeenCalledTimes(2);
});

test("opens, navigates, and closes the optional preview without changing selection", async () => {
  const api = {
    queryCatalog: vi.fn().mockResolvedValue({
      items: [
        { ...all[0], id: "elf", name: "Elf", selected: false },
        { ...all[1], id: "dwarf", name: "Dwarf", selected: false },
      ],
      total: 2,
      revision: 1,
    }),
    getCatalogFacets: async () => ({ sources: [], labels: [] }),
  } as unknown as LibraryApi;
  render(<CatalogBrowser api={api} onImport={vi.fn()} />);

  fireEvent.click(await screen.findByRole("button", { name: "Preview Elf" }));
  const preview = await screen.findByRole("complementary", { name: "Portrait preview" });
  expect(preview).toHaveTextContent("Elf");
  expect(screen.getByRole("checkbox", { name: "Select Elf" })).not.toBeChecked();

  fireEvent.keyDown(preview, { key: "ArrowRight" });
  expect(await screen.findByRole("heading", { name: "Dwarf" })).toBeInTheDocument();
  fireEvent.keyDown(screen.getByRole("complementary", { name: "Portrait preview" }), { key: "Escape" });
  expect(screen.queryByRole("complementary", { name: "Portrait preview" })).not.toBeInTheDocument();
});

test("returns focus to the portrait that opened a preview", async () => {
  const api = {
    queryCatalog: vi.fn().mockResolvedValue({ items: [{ ...all[0], id: "elf", name: "Elf" }], total: 1, revision: 1 }),
    getCatalogFacets: async () => ({ sources: [], labels: [] }),
  } as unknown as LibraryApi;
  render(<CatalogBrowser api={api} onImport={vi.fn()} />);

  const trigger = await screen.findByRole("button", { name: "Preview Elf" });
  trigger.focus();
  fireEvent.click(trigger);
  fireEvent.keyDown(await screen.findByRole("complementary", { name: "Portrait preview" }), { key: "Escape" });

  await waitFor(() => expect(trigger).toHaveFocus());
});

test("persists a single card checkbox without changing preview focus", async () => {
  const changeSelection = vi.fn().mockResolvedValue(1);
  const api = {
    queryCatalog: vi.fn().mockResolvedValue({ items: [{ ...all[0], id: "elf", name: "Elf" }], total: 1, revision: 1 }),
    getCatalogFacets: async () => ({ sources: [], labels: [] }),
    changeSelection,
  } as unknown as LibraryApi;
  render(<CatalogBrowser api={api} onImport={vi.fn()} />);

  const checkbox = await screen.findByRole("checkbox", { name: "Select Elf" });
  fireEvent.click(checkbox);

  await waitFor(() => expect(changeSelection).toHaveBeenCalledWith({ ids: ["elf"] }, "add"));
  expect(screen.queryByRole("complementary", { name: "Portrait preview" })).not.toBeInTheDocument();
});

test("edits every persisted selection through bounded catalog pages", async () => {
  const selected = Array.from({ length: 201 }, (_, index) => ({ ...all[index], id: `selected-${index}`, selected: true }));
  const queryCatalog = vi.fn(async (query, page) => query.selectedOnly
    ? { items: selected.slice(page.offset, page.offset + page.limit), total: selected.length, revision: 1 }
    : { items: [selected[0]], total: 1, revision: 1 });
  const editMetadata = vi.fn().mockResolvedValue(undefined);
  const api = { queryCatalog, getCatalogFacets: async () => ({ sources: [], labels: [] }), changeSelection: vi.fn(), editMetadata } as unknown as LibraryApi;
  render(<CatalogBrowser api={api} onImport={vi.fn()} />);

  fireEvent.click(await screen.findByRole("button", { name: "Edit 201 selected" }));
  await screen.findByRole("dialog", { name: "Edit selected metadata" });
  expect(queryCatalog).toHaveBeenCalledWith(expect.objectContaining({ selectedOnly: true }), { offset: 200, limit: 200 });
  fireEvent.change(screen.getByRole("textbox", { name: "Label category" }), { target: { value: "role" } });
  fireEvent.change(screen.getByRole("textbox", { name: "Label value" }), { target: { value: "keeper" } });
  fireEvent.click(screen.getByRole("button", { name: "Add label" }));
  fireEvent.click(screen.getByRole("button", { name: "Save metadata" }));

  await waitFor(() => expect(editMetadata).toHaveBeenCalledWith(selected.map((portrait) => portrait.id), expect.objectContaining({ addLabels: [{ category: "role", value: "keeper" }] })));
});

test("rejects a bulk edit when selected pages drift to a new revision", async () => {
  const selected = Array.from({ length: 201 }, (_, index) => ({ ...all[index], id: `selected-${index}`, selected: true }));
  const queryCatalog = vi.fn(async (query, page) => {
    if (!query.selectedOnly) return { items: [selected[0]], total: 1, revision: 1 };
    if (page.limit === 1) return { items: [selected[0]], total: selected.length, revision: 1 };
    return { items: selected.slice(page.offset, page.offset + page.limit), total: selected.length, revision: page.offset ? 2 : 1 };
  });
  const api = { queryCatalog, getCatalogFacets: async () => ({ sources: [], labels: [] }), changeSelection: vi.fn(), editMetadata: vi.fn() } as unknown as LibraryApi;
  render(<CatalogBrowser api={api} onImport={vi.fn()} />);

  fireEvent.click(await screen.findByRole("button", { name: "Edit 201 selected" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("selection changed while it was being collected");
  expect(screen.queryByRole("dialog", { name: "Edit selected metadata" })).not.toBeInTheDocument();
});

test("surfaces a failed global selection count and does not offer a page-local bulk edit", async () => {
  const queryCatalog = vi.fn(async (query, page) => {
    if (query.selectedOnly && page.limit === 1) throw new Error("Count unavailable");
    return { items: [{ ...all[0], selected: true }], total: 1, revision: 1 };
  });
  const api = { queryCatalog, getCatalogFacets: async () => ({ sources: [], labels: [] }), changeSelection: vi.fn(), editMetadata: vi.fn() } as unknown as LibraryApi;
  render(<CatalogBrowser api={api} onImport={vi.fn()} />);

  expect(await screen.findByRole("alert")).toHaveTextContent("Count unavailable");
  expect(screen.queryByRole("button", { name: /Edit .* selected/ })).not.toBeInTheDocument();
});

test("contains and closes the bulk editor with keyboard focus returned to its trigger", async () => {
  const selected = { ...all[0], id: "selected", selected: true };
  const queryCatalog = vi.fn(async (query, page) => query.selectedOnly
    ? { items: [selected], total: 1, revision: 1 }
    : { items: [selected], total: 1, revision: 1 });
  const api = { queryCatalog, getCatalogFacets: async () => ({ sources: [], labels: [] }), changeSelection: vi.fn(), editMetadata: vi.fn() } as unknown as LibraryApi;
  render(<CatalogBrowser api={api} onImport={vi.fn()} />);

  const trigger = await screen.findByRole("button", { name: "Edit 1 selected" });
  fireEvent.click(trigger);
  const dialog = await screen.findByRole("dialog", { name: "Edit selected metadata" });
  const close = screen.getByRole("button", { name: "Close" });
  const save = screen.getByRole("button", { name: "Save metadata" });
  expect(close).toHaveFocus();
  const backwardTab = createEvent.keyDown(close, { key: "Tab", shiftKey: true });
  fireEvent(close, backwardTab);
  expect(backwardTab.defaultPrevented).toBe(true);
  expect(save).toHaveFocus();
  const forwardTab = createEvent.keyDown(save, { key: "Tab" });
  fireEvent(save, forwardTab);
  expect(forwardTab.defaultPrevented).toBe(true);
  expect(close).toHaveFocus();
  fireEvent.keyDown(screen.getByRole("textbox", { name: "Description" }), { key: "Escape" });
  expect(screen.queryByRole("dialog", { name: "Edit selected metadata" })).not.toBeInTheDocument();
  expect(trigger).toHaveFocus();
});

test("returns focus to Edit selected after a successful bulk save reloads the global count", async () => {
  const selected = { ...all[0], id: "selected", selected: true };
  let countRequests = 0;
  let finishReload: (page: { items: typeof selected[]; total: number; revision: number }) => void = () => { throw new Error("Reload resolver was not assigned"); };
  const queryCatalog = vi.fn((query, page) => {
    if (!query.selectedOnly) return Promise.resolve({ items: [selected], total: 1, revision: 1 });
    if (page.limit !== 1) return Promise.resolve({ items: [selected], total: 1, revision: 1 });
    countRequests += 1;
    if (countRequests === 1) return Promise.resolve({ items: [selected], total: 1, revision: 1 });
    return new Promise<{ items: typeof selected[]; total: number; revision: number }>((resolve) => { finishReload = resolve; });
  });
  const editMetadata = vi.fn().mockResolvedValue(undefined);
  const api = { queryCatalog, getCatalogFacets: async () => ({ sources: [], labels: [] }), changeSelection: vi.fn(), editMetadata } as unknown as LibraryApi;
  render(<CatalogBrowser api={api} onImport={vi.fn()} />);

  fireEvent.click(await screen.findByRole("button", { name: "Edit 1 selected" }));
  fireEvent.change(await screen.findByRole("textbox", { name: "Description" }), { target: { value: "Updated" } });
  fireEvent.click(screen.getByRole("button", { name: "Save metadata" }));

  await waitFor(() => expect(countRequests).toBe(2));
  expect(screen.queryByRole("button", { name: "Edit 1 selected" })).not.toBeInTheDocument();
  finishReload({ items: [selected], total: 1, revision: 1 });
  expect(await screen.findByRole("button", { name: "Edit 1 selected" })).toHaveFocus();
  expect(editMetadata).toHaveBeenCalledWith(["selected"], expect.objectContaining({ description: "Updated" }));
});

test("shows only selected portraits when the selected-only filter is enabled", async () => {
  const queryCatalog = vi.fn(async (query, page) => ({
    items: query.selectedOnly ? [{ ...all[0], id: "selected", name: "Selected", selected: true }] : [{ ...all[1], id: "other", name: "Other" }],
    total: 1,
    revision: 1,
  }));
  const api = { queryCatalog, getCatalogFacets: async () => ({ sources: [], labels: [] }) } as unknown as LibraryApi;
  render(<CatalogBrowser api={api} onImport={vi.fn()} />);

  fireEvent.click(await screen.findByRole("checkbox", { name: "Selected only" }));
  expect(await screen.findByRole("button", { name: "Preview Selected" })).toBeInTheDocument();
  expect(queryCatalog).toHaveBeenLastCalledWith(expect.objectContaining({ selectedOnly: true }), { offset: 0, limit: 200 });
});

test("keeps empty-trash unavailable and surfaces a full-trash-count failure", async () => {
  const queryCatalog = vi.fn(async (query, page) => {
    if (query.trash && page.limit === 1) throw new Error("Trash count unavailable");
    return { items: query.trash ? [{ ...all[0], id: "trash", trashedAt: "2026-09-16T10:00:00Z" }] : [all[0]], total: 1, revision: 1 };
  });
  const api = { queryCatalog, getCatalogFacets: async () => ({ sources: [], labels: [] }), restorePortraits: vi.fn(), startPurge: vi.fn(), getJob: vi.fn(), cancelJob: vi.fn() } as unknown as LibraryApi;
  render(<CatalogBrowser api={api} onImport={vi.fn()} />);

  expect(await screen.findByRole("alert")).toHaveTextContent("Trash count unavailable");
  fireEvent.click(screen.getByRole("radio", { name: "Trash" }));
  expect(await screen.findByRole("button", { name: "Empty trash" })).toBeDisabled();
});
