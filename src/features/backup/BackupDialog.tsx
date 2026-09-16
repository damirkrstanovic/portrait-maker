import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { LibraryApi, LibraryInfo } from "../../lib/api";
import type { AppError, Job } from "../../lib/contracts";

type Props = { api: LibraryApi; mode: "backup" | "restore"; onClose(): void; onRestored?(library: LibraryInfo): void };
function message(error: unknown) { return error && typeof error === "object" && "message" in error ? String(error.message) : "The operation could not complete. Try again."; }
export function BackupDialog({ api, mode, onClose, onRestored }: Props) {
  const [path, setPath] = useState("");
  const [archive, setArchive] = useState("");
  const [starting, setStarting] = useState(false);
  const [job, setJob] = useState<Job | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [confirmationPath, setConfirmationPath] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [restored, setRestored] = useState<LibraryInfo | null>(null);
  const dialog = useRef<HTMLElement>(null);
  const restore = mode === "restore";
  const running = starting || job?.state === "running";
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLInputElement>("input")?.focus();
    return () => { previous?.focus(); };
  }, []);
  useEffect(() => {
    if (!job || job.state !== "running") return;
    let active = true;
    const poll = async () => {
      try {
        const next = await api.getJob(job.id);
        if (!active || !next) return;
        if (next.state === "done" && restore) {
          const info = await api.getRestoreResult?.(job.id);
          if (active && info) setRestored(info);
        }
        if (active) setJob(next);
      } catch (caught) { if (active) setError(message(caught)); }
    };
    void poll(); const timer = window.setInterval(() => void poll(), 300);
    return () => { active = false; window.clearInterval(timer); };
  }, [api, job?.id, job?.state, restore]);
  useEffect(() => { if (job) dialog.current?.querySelector<HTMLButtonElement>("footer button")?.focus(); }, [job?.state]);
  const close = () => { if (running) return; if (restored) onRestored?.(restored); onClose(); };
  const keys = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape" && !running) { event.preventDefault(); close(); }
    if (event.key === "Tab") {
      const nodes = Array.from(dialog.current?.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled)") ?? []);
      const first = nodes[0], last = nodes.at(-1);
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
      else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
    }
  };
  const start = async () => {
    setStarting(true); setError(null);
    try {
      if (restore) {
        if (!api.startRestore) throw new Error("Restore is unavailable in this app.");
        setJob(await api.startRestore(archive.trim(), path.trim()));
      } else {
        if (!api.startBackup) throw new Error("Backup is unavailable in this app.");
        setJob(await api.startBackup(path.trim(), confirmed && confirmationPath === path.trim() ? confirmationPath : null));
      }
    } catch (caught) {
      if ((caught as AppError)?.code === "BACKUP_CONFIRMATION_REQUIRED") { setConfirmationPath(path.trim()); setConfirmed(false); }
      else setError(message(caught));
    } finally { setStarting(false); }
  };
  const chooseArchive = async () => { try { const selected = await api.chooseBackupArchive?.(); if (selected) setArchive(selected); } catch (caught) { setError(message(caught)); } };
  const chooseFolder = async () => { try { const selected = await (restore ? api.chooseCreateLocation() : api.chooseBackupFolder?.() ?? api.chooseCreateLocation()); if (selected) { setPath(restore ? selected : `${selected.replace(/[\\/]$/, "")}/portrait-library-backup.zip`); setConfirmationPath(null); setConfirmed(false); } } catch (caught) { setError(message(caught)); } };
  return <div className="import-backdrop" role="presentation"><section ref={dialog} className="import-dialog backup-dialog" role="dialog" aria-modal="true" aria-labelledby="backup-title" onKeyDown={keys}>
    <header><h2 id="backup-title">{restore ? "Restore library backup" : "Back up library"}</h2></header>
    <p className="import-copy">{restore ? "Restore all portraits, metadata, selection, and trash into a new or empty folder. The restored library opens when you finish." : "Save a portable ZIP containing every portrait, metadata, selection, and trash. Game destinations and display preferences stay on this computer."}</p>
    {!job && <>
      {restore && <><label className="path-field"><span>Backup archive</span><input aria-label="Backup archive" value={archive} disabled={running} onChange={event => setArchive(event.target.value)} /></label><button className="secondary-action" disabled={running} onClick={() => void chooseArchive()}>Choose backup archive</button></>}
      <label className="path-field"><span>{restore ? "Restore folder" : "Backup ZIP path"}</span><input aria-label={restore ? "Restore folder" : "Backup ZIP path"} value={path} disabled={running} onChange={event => { setPath(event.target.value); setConfirmationPath(null); setConfirmed(false); }} placeholder={restore ? "/path/to/empty-library" : "/path/to/library-backup.zip"} /></label>
      <button className="secondary-action" disabled={running} onClick={() => void chooseFolder()}>{restore ? "Choose empty folder" : "Choose backup folder"}</button>
      {confirmationPath && <div className="backup-confirmation"><p>This file already exists: <strong>{confirmationPath}</strong></p><label><input type="checkbox" aria-label="Confirm backup overwrite" checked={confirmed} disabled={running} onChange={event => setConfirmed(event.target.checked)} /> Replace this exact backup file</label></div>}
    </>}
    {error && <p className="error-notice" role="alert">{error}</p>}
    {job && <><p role={job.state === "failed" ? "alert" : "status"}>{job.message}</p>{job.state === "running" && <p>{job.total === null ? "Working…" : `${job.completed} of ${job.total}`}</p>}</>}
    <footer>{running && job ? <button className="secondary-action" onClick={() => { void api.cancelJob(job.id).catch(caught => setError(message(caught))); }}>{restore ? "Cancel restore" : "Cancel backup"}</button> : <button className="secondary-action" disabled={starting} onClick={close}>{restored ? "Open restored library" : "Close"}</button>}
      {!job && <button className="primary-action" disabled={running || !path.trim() || (restore && !archive.trim()) || (!!confirmationPath && !confirmed)} onClick={() => void start()}>{restore ? "Restore backup" : "Save backup"}</button>}
    </footer>
  </section></div>;
}
