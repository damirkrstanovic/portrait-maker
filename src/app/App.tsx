import { DuplicateDialog } from "../features/duplicates/DuplicateDialog";
import { BackupDialog } from "../features/backup/BackupDialog";
import { ExportProgress } from "../features/export/ExportProgress";
import type { ExportReport } from "../lib/contracts";
import { useEffect, useRef, useState } from "react";

import { desktopApi, type LibraryApi, type LibraryInfo } from "../lib/api";
import type { AppError, Destination, ImportReport, ImportRequest, Job } from "../lib/contracts";
import { ImportDialog } from "../features/import/ImportDialog";
import { ImportProgress } from "../features/import/ImportProgress";
import { LibraryChooser } from "./LibraryChooser";
import { CatalogBrowser } from "../features/catalog/CatalogBrowser";
import { AppLayout } from "./AppLayout";
import { ExportDialog } from "../features/export/ExportDialog";
import { DestinationEditor } from "../features/destinations/DestinationEditor";
import { DestinationList } from "../features/destinations/DestinationList";

type Props = { api?: LibraryApi };

function asAppError(error: unknown): AppError {
  if (
    typeof error === "object" &&
    error !== null &&
    "code" in error &&
    "message" in error &&
    "recoverable" in error
  ) {
    return error as AppError;
  }
  return {
    code: "UNEXPECTED_ERROR",
    message: error instanceof Error ? error.message : "An unexpected error occurred. Try again.",
    recoverable: true,
  };
}

export function App({ api = desktopApi }: Props) {
  const [library, setLibrary] = useState<LibraryInfo | null>(null);
  const [lastLibraryPath, setLastLibraryPath] = useState<string | null>(null);
  const [portableMode, setPortableMode] = useState<"backup" | "restore" | null>(null);
  const [showExport, setShowExport] = useState(false);
  const [duplicateReview, setDuplicateReview] = useState<{ request?: ImportRequest } | null>(null);
  const [showImport, setShowImport] = useState(false);
  const [job, setJob] = useState<Job | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [error, setError] = useState<AppError | null>(null);
  const [busy, setBusy] = useState(false);
  const [catalogRefresh, setCatalogRefresh] = useState(0);
  const [destinations, setDestinations] = useState<Destination[]>([]);
  const [showDestinationEditor, setShowDestinationEditor] = useState(false);
  const [jobKind, setJobKind] = useState<"import" | "export">("import");
  const [exportReport, setExportReport] = useState<ExportReport | null>(null);
  const [showDestinations, setShowDestinations] = useState(false);
  const [destinationError, setDestinationError] = useState<string | null>(null);
  const [destinationWarnings, setDestinationWarnings] = useState<string[]>([]);
  const destinationsTrigger = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    let active = true;
    api.getLastLibraryPath().then(
      (path) => active && setLastLibraryPath(path),
      () => active && setLastLibraryPath(null),
    );
    return () => {
      active = false;
    };
  }, [api]);

  useEffect(() => {
    if (!job || job.state !== "running") return;
    let active = true;
    const refresh = async () => {
      try {
        const next = await api.getJob(job.id);
        if (!active || !next) return;
        setJob((current) => current && current.id === next.id && current.state === next.state && current.completed === next.completed && current.total === next.total && current.message === next.message ? current : next);
        if (next.state !== "running") {
          if (jobKind === "export") setExportReport(await api.getExportReport?.(next.id) ?? null);
          else setReport(await api.getImportReport(next.id));
          if (next.completed > 0) setCatalogRefresh((current) => current + 1);
        }
      } catch (caught) {
        if (active) setError(asAppError(caught));
      }
    };
    void refresh();
    const timer = window.setInterval(() => void refresh(), 300);
    return () => { active = false; window.clearInterval(timer); };
  }, [api, job?.id, job?.state, jobKind]);

  const runOpen = async (operation: () => Promise<LibraryInfo | null>) => {
    setBusy(true);
    setError(null);
    try {
      const opened = await operation();
      if (opened) {
        setLibrary(opened);
        setLastLibraryPath(opened.path);
      }
    } catch (caught) {
      setError(asAppError(caught));
    } finally {
      setBusy(false);
    }
  };

  const createLibrary = async () => {
    await runOpen(async () => {
      const path = await api.chooseCreateLocation();
      return path ? api.createLibrary(path) : null;
    });
  };

  const openLibrary = async () => {
    await runOpen(async () => {
      const path = await api.chooseOpenLocation();
      return path ? api.openLibrary(path) : null;
    });
  };

  const closeLibrary = async () => {
    setBusy(true);
    setError(null);
    try {
      await api.closeLibrary();
      setLibrary(null);
    } catch (caught) {
      setError(asAppError(caught));
    } finally {
      setBusy(false);
    }
  };

  const executeImport = async (request: ImportRequest) => {
    const next = await api.startImport(request);
    setShowImport(false);
    setDuplicateReview(null);
    setJobKind("import");
    setReport(null);
    setJob(next);
  };

  const startImport = async (request: ImportRequest) => {
    if (api.startImportDuplicateScan && api.getImportDuplicateReport) {
      setShowImport(false);
      setDuplicateReview({ request });
    } else await executeImport(request);
  };

  const cancelImport = async () => {
    if (!job) return;
    try {
      await api.cancelJob(job.id);
    } catch (caught) {
      setError(asAppError(caught));
    }
  };

  const refreshDestinations = async () => {
    setDestinationError(null);
    setDestinationWarnings([]);
    try {
      const report = await api.discoverDestinations?.() ?? { destinations: [], warnings: [] };
      const found = report.destinations;
      setDestinationWarnings(report.warnings);
      const saved = await api.getSavedDestinations?.() ?? [];
      const seen = new Set<string>();
      setDestinations([...found, ...saved].filter((destination) => {
        const key = `${destination.game}:${destination.path}`;
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      }));
    } catch (caught) { setDestinationError(asAppError(caught).message); }
  };

  const importExisting = async (destination: Destination) => {
    try {
      setShowDestinations(false);
      setShowDestinationEditor(false);
      await startImport({ path: destination.path, sourceName: `${destination.game} game portraits`, kind: "game", resize: false });
    } catch (caught) { setError(asAppError(caught)); }
  };

  if (library) {
    return (
      <AppLayout onFindDuplicates={api.startDuplicateScan ? () => setDuplicateReview({}) : undefined} library={library} busy={busy} onImport={() => setShowImport(true)} onExport={() => setShowExport(true)} onDestinations={() => { setDestinationError(null); setShowDestinations(true); setShowDestinationEditor(false); void refreshDestinations(); }} destinationsTriggerRef={destinationsTrigger} onBackup={() => setPortableMode("backup")} onCloseLibrary={closeLibrary}>
        {error ? <div className="error-notice" role="alert"><span>{error.message}</span></div> : null}
        <CatalogBrowser api={api} onImport={() => setShowImport(true)} refreshKey={catalogRefresh} />
        {portableMode === "backup" && <BackupDialog api={api} mode="backup" onClose={() => { setPortableMode(null); requestAnimationFrame(() => document.querySelector<HTMLButtonElement>(".library-menu > button")?.focus()); }} />}
        {showExport ? <ExportDialog api={api} onClose={() => setShowExport(false)} onStarted={(next) => { setJobKind("export"); setExportReport(null); setJob(next); }} /> : null}
        {duplicateReview && <DuplicateDialog api={api} request={duplicateReview.request} onImport={executeImport} onChanged={() => setCatalogRefresh(current => current + 1)} onClose={() => { setDuplicateReview(null); requestAnimationFrame(() => document.querySelector<HTMLButtonElement>(".library-menu > button")?.focus()); }} />}
        {showImport ? <ImportDialog api={api} onClose={() => setShowImport(false)} onStart={startImport} /> : null}
        {job && jobKind === "export" ? <ExportProgress job={job} report={exportReport} onCancel={() => void cancelImport()} onClose={() => { setJob(null); setExportReport(null); requestAnimationFrame(() => document.querySelector<HTMLButtonElement>(".library-actions button:nth-child(2)")?.focus()); }} /> : null}
        {job && jobKind === "import" ? <ImportProgress job={job} report={report} onCancel={() => void cancelImport()} onClose={() => { setJob(null); setReport(null); }} /> : null}
        {showDestinations && !showDestinationEditor ? <div className="destination-backdrop" role="presentation"><DestinationList destinations={destinations} warnings={destinationWarnings} error={destinationError} busy={busy} onRefresh={() => void refreshDestinations()} onAdd={() => setShowDestinationEditor(true)} onClose={() => { setShowDestinations(false); requestAnimationFrame(() => destinationsTrigger.current?.focus()); }} onImportExisting={(destination) => void importExisting(destination)} /></div> : null}
        {showDestinationEditor ? <DestinationEditor api={api} onClose={() => setShowDestinationEditor(false)} onSave={(destination) => { setDestinations((items) => [...items, destination]); setShowDestinationEditor(false); }} /> : null}
      </AppLayout>
    );
  }

  return (
      <>
      {portableMode === "restore" && <BackupDialog api={api} mode="restore" onClose={() => setPortableMode(null)} onRestored={info => { setLibrary(info); setLastLibraryPath(info.path); }} />}
      <LibraryChooser
      busy={busy}
      error={error}
      lastLibraryPath={lastLibraryPath}
      onCreate={createLibrary}
      onOpen={openLibrary}
      onRestore={() => setPortableMode("restore")}
        onReopen={() => {
        if (lastLibraryPath) void runOpen(() => api.openLibrary(lastLibraryPath));
        }}
        desktopRequired={api === desktopApi && typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window)}
      />
      </>
  );
}
