import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import type { LibraryApi } from "../../lib/api";
import type { Destination, ExportPlan, ExportRequest, Job, Query } from "../../lib/contracts";
import { ExportPreview } from "./ExportPreview";

type Props = { api: LibraryApi; onClose(): void; onStarted?(job: Job): void };
const emptyQuery: Query = { text: "", sourceIds: [], labels: [], selectedOnly: false, trash: false };
function message(error: unknown) { return typeof error === "object" && error !== null && "message" in error ? String(error.message) : "The export preview could not be prepared. Try again."; }

export function ExportDialog({ api, onClose, onStarted }: Props) {
  const [request, setRequest] = useState<ExportRequest>({scope:"all",target:"",output:"directory",mode:"merge"});
  const [designated, setDesignated] = useState(false);
  const [counts, setCounts] = useState<{all:number;selected:number} | null>(null);
  const [destinations, setDestinations] = useState<Destination[]>([]);
  const [plan, setPlan] = useState<ExportPlan | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [applying, setApplying] = useState(false);
  const version = useRef(0);
  const currentPlan = useRef<ExportPlan | null>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const trigger = useRef<HTMLElement | null>(null);
  const retire = (old: ExportPlan | null) => { if (old) void api.discardExportPlan?.(old.id).catch(() => {}); };
  useEffect(() => {
    trigger.current = document.activeElement as HTMLElement;
    closeRef.current?.focus();
    let active = true;
    void Promise.all([api.queryCatalog(emptyQuery,{offset:0,limit:1}),api.queryCatalog({...emptyQuery,selectedOnly:true},{offset:0,limit:1})]).then(([all,selected]) => { if(active) setCounts({all:all.total,selected:selected.total}); }, caught => { if(active) setError(message(caught)); });
    void api.getSavedDestinations?.().then(saved => { if(active) setDestinations(saved); }, caught => { if(active) setError(message(caught)); });
    return () => { active=false; version.current++; retire(currentPlan.current); trigger.current?.focus(); };
  }, [api]);
  const invalidate = () => { version.current++; retire(currentPlan.current); currentPlan.current=null; setPlan(null); setError(""); setBusy(false); };
  const edit = (patch: Partial<ExportRequest>) => { invalidate(); setRequest(current=>({...current,...patch})); setDesignated(false); };
  const cancel = () => { if(applying) return; invalidate(); onClose(); };
  const preview = async () => {
    invalidate(); const started=version.current; setBusy(true);
    try {
      if(!api.planExport) throw new Error("Export planning is not available.");
      if(request.mode==="replace" && request.output==="directory") {
        if(!designated || !api.designateExportTarget) throw new Error("Designate this exact directory as a portrait collection first.");
        await api.designateExportTarget(request.target);
        if(started!==version.current) return;
      }
      const next=await api.planExport(request);
      if(started!==version.current) { retire(next); return; }
      currentPlan.current=next; setPlan(next);
    } catch(caught) { if(started===version.current) setError(message(caught)); }
    finally { if(started===version.current) setBusy(false); }
  };
  const confirm = async () => {
    if(!plan || !api.applyExport || !api.validateExportPlan) return;
    const exact=plan; setApplying(true); setError("");
    try {
      await api.validateExportPlan(exact.id);
      const job=await api.applyExport(exact.id,exact.requiresConfirmation);
      currentPlan.current=null; setPlan(null); onStarted?.(job); onClose();
    } catch(caught) { invalidate(); setError(message(caught)); }
    finally { setApplying(false); }
  };
  const browse = async () => { try { const selected=await api.chooseDestinationFolder?.(); if(selected) edit({target:selected}); } catch(caught) { setError(message(caught)); } };
  const keys = (event: KeyboardEvent<HTMLElement>) => {
    if(event.key==="Escape") { event.preventDefault(); cancel(); }
    if(event.key==="Tab") {
      const controls=Array.from(event.currentTarget.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled]), select:not([disabled]), summary"));
      const first=controls[0], last=controls.at(-1);
      if(event.shiftKey && document.activeElement===first) { event.preventDefault(); last?.focus(); }
      if(!event.shiftKey && document.activeElement===last) { event.preventDefault(); first?.focus(); }
    }
  };
  const needsDesignation=request.output==="directory" && request.mode==="replace";
  const confirmationLabel=request.mode==="replace" ? "Confirm replacement" : plan?.requiresConfirmation ? (plan.actions.some(action=>action.kind==="overwrite") ? "Confirm overwrite" : "Confirm export") : "Export portraits";
  return <div className="destination-backdrop" role="presentation"><section className="export-dialog" role="dialog" aria-modal="true" aria-labelledby="export-title" onKeyDown={keys}>
    <header><div><h2 id="export-title">Export game-ready portraits</h2><p>Choose a collection, then review every change.</p></div><button ref={closeRef} type="button" className="close-dialog" aria-label="Close export" disabled={applying} onClick={cancel}>×</button></header>
    <div className="export-options">
      <label>Export scope<select value={request.scope} disabled={applying} onChange={event=>edit({scope:event.target.value as ExportRequest["scope"]})}><option value="all">All active portraits{counts ? ` (${counts.all})` : ""}</option><option value="selected">Persisted selection{counts ? ` (${counts.selected})` : ""}</option></select></label>
      <label>Export output<select value={request.output} disabled={applying} onChange={event=>edit({output:event.target.value as ExportRequest["output"],mode:"merge"})}><option value="directory">Portrait directory</option><option value="zip">Game-ready ZIP</option></select></label>
      <label>Export mode<select value={request.mode} disabled={applying || request.output==="zip"} onChange={event=>edit({mode:event.target.value as ExportRequest["mode"]})}><option value="merge">Merge</option><option value="replace">Replace collection</option></select></label>
    </div>
    {request.output==="directory" && destinations.length>0 && <label className="export-target">Saved destination<select aria-label="Saved destination" value={destinations.find(d=>d.path===request.target)?.id ?? ""} disabled={applying} onChange={event=>{const found=destinations.find(d=>d.id===event.target.value); if(found) edit({target:found.path});}}><option value="">Choose a saved destination</option>{destinations.map(d=><option key={d.id} value={d.id}>{d.name} ({d.state === "existing" ? "Portraits ready" : d.state === "missingPortraits" ? "Portraits folder absent" : "Not initialized"})</option>)}</select></label>}
    <label className="export-target">Export target<input value={request.target} disabled={applying} placeholder={request.output==="zip" ? "/absolute/path/portraits.zip" : "/absolute/path/Portraits"} onChange={event=>edit({target:event.target.value})}/></label>
    {request.output==="directory" && <button type="button" className="secondary-action" disabled={applying} onClick={()=>void browse()}>Browse export folder</button>}
    {needsDesignation && <label className="export-designation"><input type="checkbox" checked={designated} disabled={applying} onChange={event=>{invalidate();setDesignated(event.target.checked);}}/>Use this exact directory as a portrait collection</label>}
    {request.mode==="replace" && <p className="export-save-warning">Removing existing portrait folders may affect saved characters. Saved-game references are not migrated.</p>}
    <p className="export-help">Trash is always excluded. Search filters do not limit the export scope.</p>
    {error && <p className="import-error" role="alert">{error}</p>}
    {plan && <ExportPreview plan={plan} zip={request.output==="zip"}/>}
    {plan && !api.applyExport && <p className="export-help" role="status">Export writing is not available in this build. You can review and cancel this preview.</p>}
    <footer><button type="button" className="secondary-action" disabled={applying} onClick={cancel}>Cancel</button><button type="button" className="secondary-action" disabled={busy || applying || !request.target.trim() || (needsDesignation && !designated)} onClick={()=>void preview()}>{busy ? "Preparing preview…" : "Preview export"}</button>{plan && <button type="button" className={request.mode==="replace" ? "danger-action" : "primary-action"} disabled={applying || !api.applyExport || !api.validateExportPlan} onClick={()=>void confirm()}>{applying ? "Validating…" : confirmationLabel}</button>}</footer>
  </section></div>;
}
