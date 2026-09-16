import { useEffect, useRef, type KeyboardEvent } from "react";
import type { ExportReport, Job } from "../../lib/contracts";

type Props = { job: Job; report: ExportReport | null; onCancel(): void; onClose(): void };
export function ExportProgress({job,report,onCancel,onClose}:Props) {
  const action=useRef<HTMLButtonElement>(null);
  const finished=job.state!=="running";
  useEffect(()=>{action.current?.focus();},[finished]);
  const keys=(event:KeyboardEvent<HTMLElement>)=>{
    if(event.key==="Tab"){event.preventDefault();action.current?.focus();}
    if(event.key==="Escape" && finished){event.preventDefault();onClose();}
  };
  const progress=job.total ? Math.min(100,job.completed/job.total*100):0;
  return <div className="import-backdrop" role="presentation"><section className="import-dialog import-progress export-progress" role="dialog" aria-modal="true" aria-labelledby="export-progress-title" onKeyDown={keys}>
    <header><h2 id="export-progress-title">{finished?"Export finished":"Export in progress"}</h2></header>
    <p className="import-copy" role={job.state==="failed"?"alert":"status"}>{job.message}</p>
    {!finished && <><div className="progress-track" aria-label="Export progress"><span style={{width:`${progress}%`}}/></div><p className="progress-count">{job.total===null?"Working…":`${job.completed} of ${job.total}`}</p><p className="export-help">Cancellation before completion restores the previous files. Committed exports finish cleanup.</p></>}
    {report && <><p className="report-summary">Files: {report.added} added · {report.overwritten} overwritten · {report.removed} removed · {report.preserved} preserved</p>{report.issues.length>0 && <ul className="import-issues">{report.issues.map((issue,index)=><li key={index} className={issue.severity}><strong>{issue.path}</strong><span>{issue.message}</span></li>)}</ul>}</>}
    <footer><button ref={action} type="button" className={finished?"primary-action":"secondary-action"} onClick={finished?onClose:onCancel}>{finished?"Close":"Cancel export"}</button></footer>
  </section></div>;
}
