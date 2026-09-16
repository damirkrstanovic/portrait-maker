import { useState } from "react";
import type { Query, SelectionAction, SelectionTarget } from "../../lib/contracts";

type Props = {
  query: Query;
  matchingCount: number;
  selectedCount: number | null;
  changeSelection(target: SelectionTarget, action: SelectionAction): Promise<number>;
  onChanged(count: number): void;
};

export function SelectionToolbar({ query, matchingCount, selectedCount, changeSelection, onChanged }: Props) {
  const [error, setError] = useState<string | null>(null);
  const change = async (target: SelectionTarget, action: SelectionAction) => {
    try {
      setError(null);
      onChanged(await changeSelection(target, action));
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Selection could not be changed.");
    }
  };
  return <section className="selection-toolbar" aria-label="Selection tools">
    <span>{selectedCount === null ? "Selection count unavailable" : `${selectedCount.toLocaleString()} selected`}</span>
    <button type="button" disabled={!matchingCount} onClick={() => void change({ matching: { ...query, sourceIds: [...query.sourceIds], labels: [...query.labels] } }, "add")}>Select all {matchingCount.toLocaleString()} matching</button>
    <button type="button" disabled={!matchingCount} onClick={() => void change({ matching: { ...query, sourceIds: [...query.sourceIds], labels: [...query.labels] } }, "remove")}>Remove {matchingCount.toLocaleString()} matching</button>
    <button type="button" onClick={() => void change({ ids: [] }, "clear")}>Clear selection</button>
    {error ? <span role="alert">{error}</span> : null}
  </section>;
}
