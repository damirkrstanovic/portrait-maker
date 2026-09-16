import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { vi } from "vitest";

import { App } from "./App";
import type { LibraryApi, LibraryInfo } from "../lib/api";

function api(overrides: Partial<LibraryApi> = {}): LibraryApi {
  return {
    chooseCreateLocation: vi.fn().mockResolvedValue(null),
    chooseOpenLocation: vi.fn().mockResolvedValue(null),
    createLibrary: vi.fn(),
    openLibrary: vi.fn(),
    closeLibrary: vi.fn().mockResolvedValue(undefined),
    getLastLibraryPath: vi.fn().mockResolvedValue(null),
    chooseImportFolder: vi.fn().mockResolvedValue(null),
    chooseImportArchive: vi.fn().mockResolvedValue(null),
    startImport: vi.fn(),
    getJob: vi.fn(),
    cancelJob: vi.fn(),
    getImportReport: vi.fn(),
    queryCatalog: vi.fn().mockResolvedValue({ items: [], total: 0, revision: 0 }),
    ...overrides,
  };
}

const library: LibraryInfo = {
  id: "c058f994-faf8-49d2-bc4f-1f9b7e1eb034",
  name: "Pathfinder portraits",
  path: "/tmp/Pathfinder portraits",
};

test("creates a library at the chosen location and can close it", async () => {
  const client = api({
    chooseCreateLocation: vi.fn().mockResolvedValue(library.path),
    createLibrary: vi.fn().mockResolvedValue(library),
  });
  render(<App api={client} />);

  fireEvent.click(screen.getByRole("button", { name: "Create library" }));

  expect(await screen.findByRole("heading", { name: library.name })).toBeInTheDocument();
  expect(client.createLibrary).toHaveBeenCalledWith(library.path);
  expect(screen.getByText(library.path)).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: "Library ▾" }));
  fireEvent.click(screen.getByRole("button", { name: "Back up library" }));
  expect(screen.getByRole("dialog", { name: "Back up library" })).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  fireEvent.click(screen.getByRole("button", { name: "Library ▾" }));
  fireEvent.click(screen.getByRole("button", { name: "Close library" }));
  await waitFor(() => expect(client.closeLibrary).toHaveBeenCalledOnce());
  expect(await screen.findByRole("heading", { name: "Your portrait library" })).toBeInTheDocument();
});

test("offers the last library and opens it directly", async () => {
  const client = api({
    getLastLibraryPath: vi.fn().mockResolvedValue(library.path),
    openLibrary: vi.fn().mockResolvedValue(library),
  });
  render(<App api={client} />);

  expect(await screen.findByText(library.path)).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Reopen library" }));

  expect(await screen.findByRole("heading", { name: library.name })).toBeInTheDocument();
  expect(client.openLibrary).toHaveBeenCalledWith(library.path);
});

test("shows an actionable lock error and keeps the chooser available", async () => {
  const client = api({
    chooseOpenLocation: vi.fn().mockResolvedValue(library.path),
    openLibrary: vi.fn().mockRejectedValue({
      code: "LIBRARY_LOCKED",
      message: "This portrait library is open in another window. Close it there and try again.",
      recoverable: true,
    }),
  });
  render(<App api={client} />);

  fireEvent.click(screen.getByRole("button", { name: "Open library" }));

  expect(await screen.findByRole("alert")).toHaveTextContent("Library already open");
  expect(screen.getByRole("alert")).toHaveTextContent("Close it there and try again");
  expect(screen.getByRole("button", { name: "Open library" })).toBeEnabled();
});

test.each([
  ["Create library", "create"],
  ["Open library", "open"],
] as const)("shows native dialog failures from %s", async (buttonName, action) => {
  const dialogError = new Error("The system directory dialog is unavailable.");
  const client = api(
    action === "create"
      ? { chooseCreateLocation: vi.fn().mockRejectedValue(dialogError) }
      : { chooseOpenLocation: vi.fn().mockRejectedValue(dialogError) },
  );
  render(<App api={client} />);

  fireEvent.click(screen.getByRole("button", { name: buttonName }));

  expect(await screen.findByRole("alert")).toHaveTextContent(
    "The system directory dialog is unavailable.",
  );
  expect(client.createLibrary).not.toHaveBeenCalled();
  expect(client.openLibrary).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: buttonName })).toBeEnabled();
});

test("starts a folder import with resize left opt-in and reports import warnings", async () => {
  const client = api({
    getLastLibraryPath: vi.fn().mockResolvedValue(library.path),
    openLibrary: vi.fn().mockResolvedValue(library),
    chooseImportFolder: vi.fn().mockResolvedValue("/tmp/portraits"),
    startImport: vi.fn().mockResolvedValue({
      id: "job-1",
      state: "running",
      completed: 0,
      total: null,
      message: "Starting import",
    }),
    getJob: vi.fn().mockResolvedValue({
      id: "job-1",
      state: "done",
      completed: 1,
      total: 1,
      message: "Imported 1 portrait",
    }),
    getImportReport: vi.fn().mockResolvedValue({
      sourceId: "source-1",
      imported: 1,
      skipped: 0,
      cancelled: false,
      issues: [{
        path: "odd",
        code: "NONSTANDARD_DIMENSIONS",
        message: "Small.png is 100 × 100",
        severity: "warning",
      }],
    }),
  } as Partial<LibraryApi>);
  render(<App api={client} />);
  fireEvent.click(await screen.findByRole("button", { name: "Reopen library" }));
  await screen.findByRole("heading", { name: library.name });

  fireEvent.click(screen.getAllByRole("button", { name: "Import portraits" })[0]);
  fireEvent.click(screen.getByRole("button", { name: "Choose folder" }));
  expect(await screen.findByDisplayValue("/tmp/portraits")).toBeInTheDocument();
  const sourceName = screen.getByRole("textbox", { name: "Source name" });
  expect(sourceName).toHaveValue("portraits");
  fireEvent.change(sourceName, { target: { value: "My curated pack" } });
  expect(screen.getByRole("checkbox", { name: "Fit and pad to game sizes" })).not.toBeChecked();

  fireEvent.click(screen.getByRole("button", { name: "Start import" }));

  await waitFor(() => expect(client.startImport).toHaveBeenCalledWith({
    path: "/tmp/portraits",
    sourceName: "My curated pack",
    kind: "folder",
    resize: false,
  }));
  expect(await screen.findByText("1 portrait imported")).toBeInTheDocument();
  expect(screen.getByText("odd")).toBeInTheDocument();
  expect(screen.getByText("Small.png is 100 × 100")).toBeInTheDocument();
});

test("closes destinations before an existing-game import exposes progress", async () => {
  const destination = { id: "game-destination", game: "kingmaker" as const, name: "Kingmaker Steam", path: "/tmp/game/Portraits", origin: "steam" as const, state: "existing" as const, evidence: ["fixture"] };
  const client = api({
    getLastLibraryPath: vi.fn().mockResolvedValue(library.path), openLibrary: vi.fn().mockResolvedValue(library),
    discoverDestinations: vi.fn().mockResolvedValue({ destinations: [destination], warnings: [] }), getSavedDestinations: vi.fn().mockResolvedValue([]),
    startImport: vi.fn().mockResolvedValue({ id: "game-job", state: "running", completed: 0, total: null, message: "Starting import" }),
    getJob: vi.fn().mockResolvedValue({ id: "game-job", state: "running", completed: 0, total: null, message: "Starting import" }),
  });
  render(<App api={client} />);
  fireEvent.click(await screen.findByRole("button", { name: "Reopen library" }));
  fireEvent.click(await screen.findByRole("button", { name: "Game destinations" }));
  fireEvent.click(await screen.findByRole("button", { name: "Import existing portraits from Kingmaker Steam" }));
  await waitFor(() => expect(client.startImport).toHaveBeenCalledWith({ path: destination.path, sourceName: "kingmaker game portraits", kind: "game", resize: false }));
  expect(screen.queryByRole("dialog", { name: "Game destinations" })).not.toBeInTheDocument();
  expect(screen.getByText("Starting import")).toBeInTheDocument();
});

test("clears an older discovery warning before a refresh failure", async () => {
  const discoverDestinations = vi.fn()
    .mockResolvedValueOnce({ destinations: [], warnings: ["Steam library list is malformed: /tmp/old.vdf"] })
    .mockRejectedValueOnce(new Error("Steam scan is unavailable"));
  const client = api({
    getLastLibraryPath: vi.fn().mockResolvedValue(library.path),
    openLibrary: vi.fn().mockResolvedValue(library),
    discoverDestinations,
    getSavedDestinations: vi.fn().mockResolvedValue([]),
  });
  render(<App api={client} />);
  fireEvent.click(await screen.findByRole("button", { name: "Reopen library" }));
  fireEvent.click(await screen.findByRole("button", { name: "Game destinations" }));
  expect(await screen.findByText("Steam library list is malformed: /tmp/old.vdf")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent("Steam scan is unavailable"));
  expect(screen.queryByText("Steam library list is malformed: /tmp/old.vdf")).not.toBeInTheDocument();
});

test("a reviewed export opens its job report and keeps cancellation available", async () => {
  const running = { id: "export-job", state: "running" as const, completed: 0, total: 1, message: "Preparing export" };
  const client = api({
    getLastLibraryPath: vi.fn().mockResolvedValue(library.path), openLibrary: vi.fn().mockResolvedValue(library),
    planExport: vi.fn().mockResolvedValue({ id: "stored-plan", target: "/tmp/export", portraitCount: 1, actions: [{kind:"add",path:"/tmp/export/a/Small.png",reason:"Missing image"}], requiresConfirmation: false, warnings: [] }),
    validateExportPlan: vi.fn().mockResolvedValue({id:"stored-plan"}), discardExportPlan: vi.fn().mockResolvedValue(undefined),
    applyExport: vi.fn().mockResolvedValue(running), getJob: vi.fn().mockResolvedValue(running), cancelJob: vi.fn().mockResolvedValue(undefined),
    getExportReport: vi.fn().mockResolvedValue({added:3,overwritten:0,removed:0,preserved:2,issues:[]}),
  });
  render(<App api={client}/>);
  fireEvent.click(await screen.findByRole("button", {name:"Reopen library"}));
  fireEvent.click(await screen.findByRole("button", {name:"Export portraits"}));
  fireEvent.change(screen.getByLabelText("Export target"), {target:{value:"/tmp/export"}});
  fireEvent.click(screen.getByRole("button", {name:"Preview export"}));
  await screen.findByText("1 frozen portraits");
  fireEvent.click(screen.getAllByRole("button", {name:"Export portraits"}).at(-1)!);
  expect(await screen.findByRole("heading", {name:"Export in progress"})).toBeInTheDocument();
  vi.mocked(client.getJob).mockResolvedValue({...running,completed:10,total:1395,message:"Preparing destination folders"});
  expect(await screen.findByText("Preparing destination folders")).toBeInTheDocument();
  expect(screen.getByText("10 of 1395")).toBeInTheDocument();
  expect(screen.queryByText(/portrait sets staged/)).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", {name:"Cancel export"}));
  await waitFor(()=>expect(client.cancelJob).toHaveBeenCalledWith("export-job"));
  vi.mocked(client.getJob).mockResolvedValue({...running,state:"done",completed:1,message:"Export complete"});
  expect(await screen.findByRole("heading", {name:"Export finished"})).toBeInTheDocument();
  expect(await screen.findByText("Files: 3 added · 0 overwritten · 0 removed · 2 preserved")).toBeInTheDocument();
});
