import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { LibraryApi } from "../../lib/api";
import type { ImportRequest, Job, Portrait } from "../../lib/contracts";
import { DuplicateDialog } from "./DuplicateDialog";

const job: Job = { id: "scan", state: "done", completed: 2, total: 2, message: "Scan complete" };
const request: ImportRequest = { path: "/portraits.zip", kind: "archive", sourceName: "New pack", resize: false };
const portrait = (id: string, name: string): Portrait => ({ id, name, sourceId: id, sourceName: `Source ${id}`, description: `Description ${id}`, originalFolder: name, labels: [], selected: false, trashedAt: null });

test("reviews import duplicates before explicitly choosing skip or import anyway", async () => {
  const onImport = vi.fn().mockResolvedValue(undefined);
  const api = {
    startImportDuplicateScan: vi.fn().mockResolvedValue(job),
    getJob: vi.fn().mockResolvedValue(job),
    getImportDuplicateReport: vi.fn().mockResolvedValue({ matches: [{ folder: "Elf copy", name: "Elf copy", matchingPortraitId: "old", matchingName: "Elf", duplicateOfInBatch: null }], issues: [] }),
  } as unknown as LibraryApi;
  render(<DuplicateDialog api={api} request={request} onImport={onImport} onChanged={vi.fn()} onClose={vi.fn()} />);
  await screen.findByText("1 incoming duplicates");
  expect(onImport).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "Import anyway" })).toBeEnabled();
  fireEvent.click(screen.getByRole("button", { name: "Skip duplicates and import" }));
  await waitFor(() => expect(onImport).toHaveBeenCalledWith({ ...request, duplicatePolicy: "skip" }));
});

test("lets the user change the keeper and requires confirmation before moving duplicates to Trash", async () => {
  const consolidateDuplicates = vi.fn().mockResolvedValue({ trashed: 1 });
  const onChanged = vi.fn();
  const api = {
    startDuplicateScan: vi.fn().mockResolvedValue(job), getJob: vi.fn().mockResolvedValue(job), consolidateDuplicates,
    getDuplicateScanReport: vi.fn().mockResolvedValue({ groups: [{ fingerprint: "equal", members: [{ portrait: portrait("a", "Elf"), sourceNames: ["Pack A"] }, { portrait: portrait("b", "Mage"), sourceNames: ["Pack B"] }], nameConflict: true, descriptionConflict: true }], issues: [] }),
  } as unknown as LibraryApi;
  render(<DuplicateDialog api={api} onChanged={onChanged} onClose={vi.fn()} />);
  await screen.findByText(/Review differing names and descriptions/);
  fireEvent.click(screen.getByRole("radio", { name: /Keep Mage/ }));
  fireEvent.click(screen.getByRole("button", { name: "Move 1 duplicates to Trash" }));
  expect(consolidateDuplicates).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Confirm move to Trash" }));
  await screen.findByRole("status");
  await waitFor(() => expect(consolidateDuplicates).toHaveBeenCalledWith([{ keepId: "b", removeIds: ["a"] }]));
  await waitFor(() => expect(onChanged).toHaveBeenCalledTimes(1));
});
