import { useEffect, useRef, useState, type KeyboardEvent } from "react";

import type { Destination, Game } from "../../lib/contracts";
import type { LibraryApi } from "../../lib/api";

type Props = { api: LibraryApi; onSave(destination: Destination): void; onClose(): void };

export function DestinationEditor({ api, onSave, onClose }: Props) {
  const [game, setGame] = useState<Game>("kingmaker");
  const [mode, setMode] = useState<"folder" | "prefix">("folder");
  const [path, setPath] = useState("");
  const [candidates, setCandidates] = useState<Destination[]>([]);
  const [error, setError] = useState("");
  const close = useRef<HTMLButtonElement>(null);
  useEffect(() => { close.current?.focus(); }, []);
  const browse = async () => {
    try {
      const selected = mode === "folder" ? await api.chooseDestinationFolder?.() : await api.chooseCompatibilityPrefix?.();
      if (selected) setPath(selected);
    } catch (caught) { setError(caught instanceof Error ? caught.message : "The folder chooser could not open."); }
  };
  const save = async () => {
    try {
      if (mode === "prefix") {
        const resolved = await api.resolvePrefix?.(path, game) ?? [];
        if (!resolved.length) throw new Error("No game users were found in this compatibility prefix.");
        setCandidates(resolved);
        return;
      }
      const destination = await api.validateDestination?.(path, game);
      if (!destination) throw new Error("Choose a valid folder or compatibility prefix.");
      await api.saveDestination?.(destination);
      onSave(destination);
    } catch (caught) { setError(caught instanceof Error ? caught.message : "The destination could not be added."); }
  };
  const chooseCandidate = async (destination: Destination) => { try { await api.saveDestination?.(destination); onSave(destination); } catch (caught) { setError(caught instanceof Error ? caught.message : "The destination could not be saved."); } };
  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape") { event.preventDefault(); onClose(); }
    if (event.key === "Tab") {
      const controls = Array.from(event.currentTarget.querySelectorAll<HTMLElement>("button:not([disabled]), input:not([disabled]), select:not([disabled])"));
      const first = controls[0]; const last = controls.at(-1);
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
      if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
    }
  };
  return <div className="destination-backdrop" role="presentation"><section className="destination-editor" role="dialog" aria-modal="true" aria-labelledby="destination-title" onKeyDown={onKeyDown}>
    <header><div><p className="section-kicker">Manual destination</p><h2 id="destination-title">Add game portraits</h2></div><button ref={close} type="button" className="close-dialog" aria-label="Close destination editor" onClick={onClose}>×</button></header>
    <p>Choose the Portraits folder directly, or choose a launcher-neutral Wine or Proton prefix.</p>
    <label>Game<select value={game} onChange={(event) => setGame(event.target.value as Game)}><option value="kingmaker">Pathfinder: Kingmaker</option><option value="wotr">Pathfinder: Wrath of the Righteous</option></select></label>
    <fieldset><legend>Location type</legend><label><input type="radio" checked={mode === "folder"} onChange={() => setMode("folder")} /> Portraits folder</label><label><input type="radio" checked={mode === "prefix"} onChange={() => setMode("prefix")} /> Wine or Proton prefix</label></fieldset>
    <label className="path-field">Folder path<input value={path} onChange={(event) => setPath(event.target.value)} /></label><button type="button" className="secondary-action" onClick={() => void browse()}>Browse</button>
    {candidates.length ? <section className="destination-candidates" aria-label="Compatibility prefix candidates"><h3>Choose a game user</h3>{candidates.map((candidate) => <button type="button" key={candidate.id} className="secondary-action" onClick={() => void chooseCandidate(candidate)}>{candidate.name} — {candidate.state === "existing" ? "Portraits ready" : candidate.state === "missingPortraits" ? "Portraits folder absent" : "Not initialized"}</button>)}</section> : null}
    {error ? <p role="alert" className="import-error">{error}</p> : null}<footer><button type="button" className="secondary-action" onClick={onClose}>Cancel</button><button type="button" className="primary-action" disabled={!path} onClick={() => void save()}>Save destination</button></footer>
  </section></div>;
}
