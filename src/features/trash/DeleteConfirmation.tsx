import { useEffect, useRef } from "react";

type Props = { count: number; onConfirm(): void; onClose(): void };
const noun = (count: number) => count === 1 ? "portrait" : "portraits";

export function DeleteConfirmation({ count, onConfirm, onClose }: Props) {
  const dialog = useRef<HTMLDivElement>(null);
  const cancel = useRef<HTMLButtonElement>(null);
  useEffect(() => { cancel.current?.focus(); }, []);
  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); onClose(); return; }
    if (event.key !== "Tab") return;
    const focusable = Array.from(dialog.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? []);
    const first = focusable[0]; const last = focusable[focusable.length - 1];
    if (first && ((event.shiftKey && document.activeElement === first) || (!event.shiftKey && document.activeElement === last))) {
      event.preventDefault(); (event.shiftKey ? last : first).focus();
    }
  };
  const action = `Permanently delete ${count.toLocaleString()} ${noun(count)}`;
  return <div className="delete-backdrop" role="presentation"><div ref={dialog} className="delete-dialog" role="dialog" aria-modal="true" aria-label="Permanently delete portraits" onKeyDownCapture={onKeyDown}>
    <p className="section-kicker">Irreversible action</p><h2>{action}</h2><p>Managed portrait files and their catalog entries will be removed. Imported source folders are never changed.</p>
    <footer><button ref={cancel} type="button" onClick={onClose}>Cancel</button><button type="button" className="danger-action" onClick={() => { onConfirm(); onClose(); }}>{action}</button></footer>
  </div></div>;
}
