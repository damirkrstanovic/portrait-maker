import { useLayoutEffect, useRef, useState } from "react";
import type { Job } from "../../lib/contracts";
import { DeleteConfirmation } from "./DeleteConfirmation";

type Props = { total: number; emptyCount: number | null; markedIds: Set<string>; purgeJob?: Job | null; onRestore(ids: string[]): void; onPurge(ids: string[]): void; onEmpty(): void; onCancelPurge(): void };
const label = (count: number, word: string) => `${word} ${count.toLocaleString()} marked ${count === 1 ? "portrait" : "portraits"}`;

export function TrashView({ total, emptyCount, markedIds, purgeJob, onRestore, onPurge, onEmpty, onCancelPurge }: Props) {
  const [confirmation, setConfirmation] = useState<"marked" | "all" | null>(null);
  const emptyTrigger = useRef<HTMLButtonElement>(null);
  const markedPurgeTrigger = useRef<HTMLButtonElement>(null);
  const restoreFocus = useRef<HTMLButtonElement | null>(null);
  const marked = Array.from(markedIds);
  const count = confirmation === "all" ? (emptyCount ?? 0) : marked.length;
  const pending = purgeJob?.state === "running";
  useLayoutEffect(() => {
    if (!confirmation && restoreFocus.current) { restoreFocus.current.focus(); restoreFocus.current = null; }
  }, [confirmation]);
  const close = () => { restoreFocus.current = confirmation === "all" ? emptyTrigger.current : markedPurgeTrigger.current; setConfirmation(null); };
  return <><section className="trash-tools" aria-label="Trash tools"><span>{total.toLocaleString()} {total === 1 ? "portrait" : "portraits"} matching</span><button type="button" disabled={pending || !marked.length} onClick={() => onRestore(marked)}>{label(marked.length, "Restore")}</button><button ref={markedPurgeTrigger} type="button" disabled={pending || !marked.length} onClick={() => setConfirmation("marked")}>{label(marked.length, "Purge")}</button><button ref={emptyTrigger} type="button" disabled={pending || emptyCount === null || emptyCount === 0} onClick={() => setConfirmation("all")}>Empty trash</button>{pending ? <span aria-live="polite">{purgeJob.message} <button type="button" onClick={onCancelPurge}>Cancel deletion</button></span> : null}</section><p className="trash-note">Mark portraits in this view to restore or permanently delete them. Active selection is kept separate.</p>{confirmation ? <DeleteConfirmation count={count} onClose={close} onConfirm={() => { if (confirmation === "all") onEmpty(); else onPurge(marked); }} /> : null}</>;
}
