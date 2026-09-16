import type { ExportPlan } from "../../lib/contracts";

export function ExportPreview({ plan, zip }: { plan: ExportPlan; zip: boolean }) {
  return <section className="export-preview" aria-label="Export preview">
    <h3>{plan.portraitCount} frozen portraits</h3>
    <p className="export-absolute-target">{plan.target}</p>
    <p className="export-help">{zip ? "Archive entries appear relative to the ZIP root." : "These exact file paths will be affected. Unrelated content is preserved."}</p>
    {plan.warnings.length > 0 && <ul className="export-warnings">{plan.warnings.map(warning => <li key={warning}>{warning}</li>)}</ul>}
    <div className="export-action-groups">{(["add", "overwrite", "remove", "preserve"] as const).map(kind => {
      const actions = plan.actions.filter(action => action.kind === kind);
      const folders = new Set(actions.map(action => action.path.replace(/[\\/][^\\/]+$/, ""))).size;
      return <details key={kind} open={actions.length > 0} className={`export-action-group export-${kind}`}>
        <summary>{({add:"Add",overwrite:"Overwrite",remove:"Remove",preserve:"Preserve"})[kind]}: {actions.length} paths · {folders} parent folders</summary>
        {actions.length === 0 ? <p>None</p> : <ul>{actions.map(action => <li key={action.path}><span>{action.path}</span><small>{action.reason}</small></li>)}</ul>}
      </details>;
    })}</div>
  </section>;
}
