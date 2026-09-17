import { useEffect, useRef, useState } from "react";
import type { LibraryApi } from "../../lib/api";
import type { DuplicateScanReport, ImportDuplicateReport, ImportRequest, Job } from "../../lib/contracts";
import { assetUrl } from "../catalog/CatalogBrowser";

type Props = {
  api: LibraryApi;
  request?: ImportRequest;
  onImport?(request: ImportRequest): Promise<void>;
  onChanged(): void;
  onClose(): void;
};
const errorMessage = (error: unknown) => error && typeof error === "object" && "message" in error ? String(error.message) : "The duplicate operation could not finish.";

export function DuplicateDialog({ api, request, onImport, onChanged, onClose }: Props) {
  const [job, setJob] = useState<Job | null>(null);
  const [report, setReport] = useState<DuplicateScanReport | null>(null);
  const [importReport, setImportReport] = useState<ImportDuplicateReport | null>(null);
  const [phase, setPhase] = useState<"scanning" | "review" | "confirm" | "working" | "finished">("scanning");
  const [keepers, setKeepers] = useState<Record<string, string>>({});
  const [excluded, setExcluded] = useState<Set<string>>(() => new Set());
  const [page, setPage] = useState(0);
  const [memberPages, setMemberPages] = useState<Record<string, number>>({});
  const [error, setError] = useState<string | null>(null);
  const [summary, setSummary] = useState("");
  const started = useRef<Promise<Job> | null>(null);
  const panel = useRef<HTMLElement>(null);
  const callbacks = useRef({ onImport, onChanged });
  callbacks.current = { onImport, onChanged };

  useEffect(() => { panel.current?.focus(); }, []);
  useEffect(() => {
    let active = true;
    started.current ??= request
      ? api.startImportDuplicateScan!(request)
      : api.startDuplicateScan!();
    void started.current.then(next => { if (active) setJob(next); }, caught => {
      if (active) { setError(errorMessage(caught)); setPhase("review"); }
    });
    return () => { active = false; };
  }, [api, request]);

  useEffect(() => {
    if (!job) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const next = await api.getJob(job.id);
        if (!active) return;
        if (!next) throw new Error("The duplicate scan is no longer available. Close this window and scan again.");
        setJob(next);
        if (next.state === "running") { timer = setTimeout(() => void poll(), 300); return; }
        if (next.state !== "done") { setSummary(next.state === "cancelled" ? "Scan cancelled. No portraits were changed." : next.message); setPhase("finished"); return; }
        if (request) {
          const result = await api.getImportDuplicateReport!(next.id);
          if (!active) return;
          if (!result) throw new Error("The duplicate scan report is unavailable.");
          setImportReport(result);
          if (!result.matches.length && !result.issues.length) {
            setPhase("working");
            await callbacks.current.onImport?.({ ...request, duplicatePolicy: "skip" });
            return;
          }
        } else {
          const result = await api.getDuplicateScanReport!(next.id);
          if (!active) return;
          if (!result) throw new Error("The duplicate scan report is unavailable.");
          setReport(result);
          setKeepers(Object.fromEntries(result.groups.map(group => [group.fingerprint, group.members[0].portrait.id])));
        }
        setPhase("review");
      } catch (caught) { if (active) { setError(errorMessage(caught)); setPhase("review"); } }
    };
    void poll();
    return () => { active = false; clearTimeout(timer); };
  }, [api, job?.id, request]);

  const groups = report?.groups ?? [];
  const choices = groups.filter(group => !excluded.has(group.fingerprint)).map(group => ({
    keepId: keepers[group.fingerprint], removeIds: group.members.map(member => member.portrait.id).filter(id => id !== keepers[group.fingerprint]),
  }));
  const count = choices.reduce((sum, choice) => sum + choice.removeIds.length, 0);
  const issues = report?.issues ?? importReport?.issues ?? [];
  const busy = phase === "scanning" || phase === "working";
  const performImport = async (duplicatePolicy: "skip" | "keep") => {
    setError(null); setPhase("working");
    try { await onImport?.({ ...request!, duplicatePolicy }); }
    catch (caught) { setError(errorMessage(caught)); setPhase("review"); }
  };
  const consolidate = async () => {
    setError(null); setPhase("working");
    try {
      const result = await api.consolidateDuplicates!(choices);
      callbacks.current.onChanged();
      setSummary(`Moved ${result.trashed} duplicate portraits to Trash. You can restore them from the Trash view.`);
      setPhase("finished");
    } catch (caught) { setError(errorMessage(caught)); setPhase("review"); }
  };
  const containFocus = (event: React.KeyboardEvent) => {
    if (event.key === "Escape" && !busy) { event.preventDefault(); onClose(); }
    if (event.key !== "Tab") return;
    const items = Array.from(panel.current?.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled)") ?? []);
    if (!items.length) { event.preventDefault(); return; }
    if (event.shiftKey && (document.activeElement === items[0] || document.activeElement === panel.current)) { event.preventDefault(); items.at(-1)?.focus(); }
    else if (!event.shiftKey && document.activeElement === items.at(-1)) { event.preventDefault(); items[0].focus(); }
  };

  return <div className="import-backdrop"><section ref={panel} tabIndex={-1} className="import-dialog duplicate-dialog" role="dialog" aria-modal="true" aria-labelledby="duplicates-title" onKeyDown={containFocus}>
    <header><h2 id="duplicates-title">{request ? "Check import for duplicates" : "Find duplicate portraits"}</h2><button type="button" className="close-dialog" disabled={busy} onClick={onClose}>Close</button></header>
    <p>Exact matches only: all three images must have the same dimensions and pixels. Portraits in Trash are excluded.</p>
    {phase === "scanning" && <div aria-live="polite"><p>{job?.message || "Starting duplicate scan…"}</p>{job?.total != null && <p>{job.completed} of {job.total} processed</p>}<button type="button" disabled={!job} onClick={() => void api.cancelJob(job!.id).catch(caught => setError(errorMessage(caught)))}>Cancel scan</button></div>}
    {phase === "working" && <p role="status">{request ? "Starting import…" : "Consolidating duplicates…"}</p>}
    {error && <p className="import-error" role="alert">{error}</p>}
    {phase === "finished" && <p role="status">{summary}</p>}
    {(phase === "review" || phase === "confirm") && <>
      {issues.length > 0 && <details><summary>{issues.length} scan warnings</summary><ul className="import-issues">{issues.map((issue, index) => <li key={index}><strong>{issue.path}</strong> {issue.message}</li>)}</ul></details>}
      {importReport && <>
        <h3>{importReport.matches.length} incoming duplicates</h3>
        <p>Skipping duplicates keeps one copy and preserves the incoming source and inferred labels. Existing names and descriptions stay unchanged.</p>
        <ul className="duplicate-import-matches">{importReport.matches.slice(page * 20, page * 20 + 20).map((match, index) => <li key={`${match.folder}-${index}`}><strong>{match.name}</strong><span>{match.folder}</span><span>Matches {match.matchingName ?? match.duplicateOfInBatch ?? "another incoming portrait"}{match.matchingPortraitId ? " in your library" : " in this import"}</span></li>)}</ul>
        {importReport.matches.length > 20 && <nav className="duplicate-pages"><button disabled={!page} onClick={() => setPage(page - 1)}>Previous</button><span>Page {page + 1} of {Math.ceil(importReport.matches.length / 20)}</span><button disabled={(page + 1) * 20 >= importReport.matches.length} onClick={() => setPage(page + 1)}>Next</button></nav>}
        <footer><button className="secondary-action" onClick={() => void performImport("keep")}>Import anyway</button><button className="primary-action" onClick={() => void performImport("skip")}>Skip duplicates and import</button></footer>
      </>}
      {report && <>
        <h3>{groups.length ? `${groups.length} duplicate groups` : "No exact duplicates found"}</h3>
        {!!groups.length && <p>Choose one portrait to keep in each group. Sources and labels are combined; the keeper retains its name and description. Removed copies remain recoverable in Trash.</p>}
        {groups.slice(page * 10, page * 10 + 10).map((group, index) => <fieldset className="duplicate-group" key={group.fingerprint} disabled={phase === "confirm"}>
          <legend>Group {page * 10 + index + 1} · {group.members.length} copies</legend>
          <label><input type="checkbox" checked={!excluded.has(group.fingerprint)} onChange={event => setExcluded(current => { const next = new Set(current); if (event.target.checked) next.delete(group.fingerprint); else next.add(group.fingerprint); return next; })} /> Consolidate this group</label>
          {(group.nameConflict || group.descriptionConflict) && <p className="duplicate-conflict">Review differing {group.nameConflict && group.descriptionConflict ? "names and descriptions" : group.nameConflict ? "names" : "descriptions"} below before choosing the keeper.</p>}
          <p>Keeping: {group.members.find(member => member.portrait.id === keepers[group.fingerprint])?.portrait.name}</p>
          <div className="duplicate-members">{group.members.slice((memberPages[group.fingerprint] ?? 0) * 20, ((memberPages[group.fingerprint] ?? 0) + 1) * 20).map(({ portrait, sourceNames }) => <label key={portrait.id} className={keepers[group.fingerprint] === portrait.id ? "duplicate-member is-keeper" : "duplicate-member"}>
            <input type="radio" name={group.fingerprint} checked={keepers[group.fingerprint] === portrait.id} onChange={() => setKeepers(current => ({ ...current, [group.fingerprint]: portrait.id }))} />
            <img src={assetUrl(portrait.id, "small")} alt="" loading="lazy" />
            <span><strong>Keep {portrait.name}</strong><small>{sourceNames.join(", ")}</small><small>{portrait.originalFolder}</small><span>{portrait.description || "No description"}</span><small>{portrait.labels.map(label => `${label.category}: ${label.value}`).join(" · ") || "No labels"}</small></span>
          </label>)}</div>
          {group.members.length > 20 && <nav className="duplicate-pages" aria-label={`Copies in group ${page * 10 + index + 1}`}><button disabled={!memberPages[group.fingerprint]} onClick={() => setMemberPages(current => ({ ...current, [group.fingerprint]: (current[group.fingerprint] ?? 0) - 1 }))}>Previous copies</button><span>Page {(memberPages[group.fingerprint] ?? 0) + 1} of {Math.ceil(group.members.length / 20)}</span><button disabled={((memberPages[group.fingerprint] ?? 0) + 1) * 20 >= group.members.length} onClick={() => setMemberPages(current => ({ ...current, [group.fingerprint]: (current[group.fingerprint] ?? 0) + 1 }))}>Next copies</button></nav>}
        </fieldset>)}
        {groups.length > 10 && <nav className="duplicate-pages"><button disabled={!page} onClick={() => setPage(page - 1)}>Previous</button><span>Page {page + 1} of {Math.ceil(groups.length / 10)}</span><button disabled={(page + 1) * 10 >= groups.length} onClick={() => setPage(page + 1)}>Next</button></nav>}
        {!!groups.length && <footer>{phase === "confirm" ? <><p>Move {count} duplicates to Trash? The chosen keepers remain in your library.</p><button className="secondary-action" onClick={() => setPhase("review")}>Back to review</button><button className="primary-action" onClick={() => void consolidate()}>Confirm move to Trash</button></> : <button className="primary-action" disabled={!count} onClick={() => setPhase("confirm")}>Move {count} duplicates to Trash</button>}</footer>}
      </>}
    </>}
  </section></div>;
}
