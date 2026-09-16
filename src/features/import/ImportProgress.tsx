import type { ImportReport, Job } from "../../lib/contracts";

type Props = {
  job: Job;
  report: ImportReport | null;
  onCancel: () => void;
  onClose: () => void;
};

export function ImportProgress({ job, report, onCancel, onClose }: Props) {
  const finished = job.state !== "running";
  const progress = job.total && job.total > 0 ? Math.min(100, (job.completed / job.total) * 100) : 0;
  return (
    <div className="import-backdrop" role="presentation">
      <section className="import-dialog import-progress" role="dialog" aria-modal="true" aria-labelledby="progress-title">
        <header><div><p className="section-kicker">Importing</p><h2 id="progress-title">{finished ? "Import finished" : "Import in progress"}</h2></div></header>
        <p className="import-copy">{job.message}</p>
        <div className="progress-track" aria-label="Import progress"><span style={{ width: `${progress}%` }} /></div>
        <p className="progress-count">{job.total === null ? "Preparing portrait sets" : `${job.completed} of ${job.total} sets processed`}</p>
        {report ? <>
          <p className="report-summary">{report.imported} {report.imported === 1 ? "portrait" : "portraits"} imported{report.skipped ? `; ${report.skipped} skipped` : ""}{report.cancelled ? "; cancelled" : ""}</p>
          {report.issues.length ? <ul className="import-issues">{report.issues.map((issue, index) => <li key={`${issue.path}-${issue.code}-${index}`} className={issue.severity}><strong>{issue.path || "Selected folder"}</strong><span>{issue.message}</span></li>)}</ul> : null}
        </> : null}
        <footer>
          {finished ? <button type="button" className="primary-action" onClick={onClose}>Close</button> : <button type="button" className="secondary-action" onClick={onCancel}>Cancel import</button>}
        </footer>
      </section>
    </div>
  );
}
