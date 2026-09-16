import { useEffect, useRef, type KeyboardEvent } from "react";
import type { Destination } from "../../lib/contracts";

type Props = {
  destinations: Destination[];
  error?: string | null;
  warnings?: string[];
  busy: boolean;
  onRefresh(): void;
  onAdd(): void;
  onImportExisting(destination: Destination): void;
  onClose?(): void;
};

const stateText: Record<Destination["state"], string> = {
  existing: "Portraits folder ready",
  missingPortraits: "Portraits folder is absent",
  uninitialized: "Game data has not been initialized",
};

export function DestinationList({ destinations, error, warnings = [], busy, onRefresh, onAdd, onImportExisting, onClose }: Props) {
  const close = useRef<HTMLButtonElement>(null);
  useEffect(() => { close.current?.focus(); }, []);
  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key === "Escape" && onClose) { event.preventDefault(); onClose(); }
    if (event.key === "Tab") {
      const buttons = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not([disabled])"));
      const first = buttons[0]; const last = buttons.at(-1);
      if (event.shiftKey && document.activeElement === first) { event.preventDefault(); last?.focus(); }
      if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first?.focus(); }
    }
  };
  return <section className="destination-panel" role="dialog" aria-modal="true" aria-label="Game destinations" onKeyDown={onKeyDown}>
    <header className="destination-panel-heading">
      <div><p className="section-kicker">Game portraits</p><h2>Destinations</h2></div>
      <div><button type="button" className="secondary-action" disabled={busy} onClick={onRefresh}>Refresh</button><button type="button" className="primary-action" disabled={busy} onClick={onAdd}>Add destination</button>{onClose ? <button ref={close} type="button" className="close-dialog" aria-label="Close game destinations" onClick={onClose}>×</button> : null}</div>
    </header>
    {error ? <p role="alert" className="import-error">{error}</p> : null}
    {warnings.length ? <ul className="destination-warnings" aria-label="Discovery warnings">{warnings.map((warning) => <li key={warning}>{warning}</li>)}</ul> : null}
    {destinations.length ? <ul className="destination-list">{destinations.map((destination) => <li key={destination.id}>
      <div className="destination-summary"><strong>{destination.name}</strong><span>{stateText[destination.state]}</span><code title={destination.path}>{destination.path}</code></div>
      <p>{destination.evidence.join(" · ")}</p>
      {destination.state === "existing" ? <button type="button" className="secondary-action" disabled={busy} onClick={() => onImportExisting(destination)}>Import existing portraits from {destination.name}</button> : null}
    </li>)}</ul> : <p className="destination-empty">Refresh to find Steam installations, or add a portrait folder or compatibility prefix.</p>}
  </section>;
}
