import { useEffect, useRef, useState } from "react";
import type { LibraryApi } from "../../lib/api";
import type { AnalysisSettings, Job } from "../../lib/contracts";

type Props = { api: LibraryApi; visible: boolean; onChanged(): void; onClose(): void };
const message = (error: unknown) => error && typeof error === "object" && "message" in error ? String(error.message) : "Portrait analysis could not finish.";

export function AnalysisDialog({ api, visible, onChanged, onClose }: Props) {
  const [settings, setSettings] = useState<AnalysisSettings | null>(null);
  const [selectedOnly, setSelectedOnly] = useState(true);
  const [overwrite, setOverwrite] = useState(false);
  const [job, setJob] = useState<Job | null>(null);
  const [starting, setStarting] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const panel = useRef<HTMLElement>(null);
  const callbacks = useRef({ onChanged });
  callbacks.current = { onChanged };
  const active = starting || job?.state === "running";

  useEffect(() => {
    let mounted = true;
    void api.getAnalysisSettings?.().then(value => { if (mounted) setSettings(value); }, caught => { if (mounted) setError(message(caught)); });
    return () => { mounted = false; };
  }, [api]);
  useEffect(() => { if (visible) panel.current?.focus(); }, [visible]);
  useEffect(() => {
    if (!job || job.state !== "running") return;
    let mounted = true;
    let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const next = await api.getJob(job.id);
        if (!mounted) return;
        if (!next) throw new Error("The analysis job is no longer available.");
        setJob(next);
        if (next.state === "running") timer = setTimeout(() => void poll(), 750);
        else { setCancelling(false); callbacks.current.onChanged(); }
      } catch (caught) {
        if (mounted) { setError(message(caught)); timer = setTimeout(() => void poll(), 2000); }
      }
    };
    void poll();
    return () => { mounted = false; clearTimeout(timer); };
  }, [api, job?.id, job?.state]);

  const start = async () => {
    if (!settings || !api.startAnalysis) return;
    setStarting(true); setError(null); setCancelling(false);
    try {
      const next = await api.startAnalysis({ ...settings, selectedOnly, overwrite });
      setJob(next);
      if (next.state !== "running") callbacks.current.onChanged();
    } catch (caught) { setError(message(caught)); }
    finally { setStarting(false); }
  };
  const cancel = async () => {
    if (!job) return;
    setCancelling(true);
    try { await api.cancelJob(job.id); }
    catch (caught) { setCancelling(false); setError(message(caught)); }
  };
  const containFocus = (event: React.KeyboardEvent) => {
    if (event.key === "Escape") { event.preventDefault(); onClose(); }
    if (event.key !== "Tab") return;
    const elements = Array.from(panel.current?.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled), select:not(:disabled)") ?? []);
    if (!elements.length) { event.preventDefault(); return; }
    const first = elements[0], last = elements[elements.length - 1];
    if (event.shiftKey && (document.activeElement === first || document.activeElement === panel.current)) { event.preventDefault(); last.focus(); }
    else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
  };
  if (!visible) return null;
  return <div className="import-backdrop"><section className="import-dialog analysis-dialog" ref={panel} tabIndex={-1} role="dialog" aria-modal="true" aria-labelledby="analysis-heading" onKeyDown={containFocus}>
    <header><h2 id="analysis-heading">Describe portraits</h2><button type="button" onClick={onClose}>{active ? "Run in background" : "Close"}</button></header>
    <p>Describe the large portrait and suggest RPG labels: gender presentation, fantasy ancestry, class, combat style, weapons, armor, and visible magic. Descriptions become searchable; your notes and label edits are preserved.</p>
    {settings ? <fieldset disabled={active} className="analysis-settings"><legend>Model server</legend>
      <label>Chat completions endpoint<input type="url" value={settings.endpoint} onChange={event => setSettings({ ...settings, endpoint: event.target.value })} /></label>
      <label>Model<input value={settings.model} onChange={event => setSettings({ ...settings, model: event.target.value })} /></label>
      <label>API token file<input value={settings.apiKeyPath} onChange={event => setSettings({ ...settings, apiKeyPath: event.target.value })} /></label>
      <p>Choose the path to your .apikey file. The token stays on this computer and is sent only to the configured server.</p>
    </fieldset> : <p>Loading model settings…</p>}
    <fieldset disabled={active} className="analysis-scope"><legend>Portraits to analyze</legend>
      <label><input type="radio" name="analysis-scope" checked={selectedOnly} onChange={() => setSelectedOnly(true)} />Selected active portraits</label>
      <label><input type="radio" name="analysis-scope" checked={!selectedOnly} onChange={() => setSelectedOnly(false)} />All active portraits</label>
      <label><input type="checkbox" checked={overwrite} onChange={event => setOverwrite(event.target.checked)} />Reanalyze portraits that already have a model description</label>
    </fieldset>
    <p>One image is sent at a time. Completed results are saved as the batch runs. By default, restarting skips portraits already analyzed.</p>
    {job && <div className="analysis-progress" role="status" aria-live="polite"><p>{job.message}</p>{job.total !== null && <><progress value={job.completed} max={Math.max(1, job.total)} /><p>{job.completed} / {job.total} portraits processed</p></>}</div>}
    {cancelling && <p role="status">Stopping after the current request finishes. Completed results will be kept.</p>}
    {error && <p role="alert">{error}</p>}
    <footer>{job?.state === "running" ? <button type="button" disabled={cancelling} onClick={() => void cancel()}>Stop analysis</button> : <button type="button" className="primary-action" disabled={starting || !settings || !settings.endpoint.trim() || !settings.model.trim() || !settings.apiKeyPath.trim()} onClick={() => void start()}>{starting ? "Starting…" : "Start analysis"}</button>}</footer>
  </section></div>;
}
