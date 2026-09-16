import { useEffect, useMemo, useState } from "react";

import type { MetadataPatch, Portrait } from "../../lib/contracts";

type Props = {
  portraits: Portrait[];
  editMetadata(ids: string[], patch: MetadataPatch): Promise<void>;
  renameSource?(id: string, name: string): Promise<void>;
  onSaved?(): void;
};

export function MetadataEditor({ portraits, editMetadata, renameSource, onSaved }: Props) {
  const single = portraits.length === 1 ? portraits[0] : null;
  const [name, setName] = useState("");
  const [sourceName, setSourceName] = useState("");
  const [description, setDescription] = useState("");
  const [descriptionEdited, setDescriptionEdited] = useState(false);
  const [category, setCategory] = useState("");
  const [value, setValue] = useState("");
  const [added, setAdded] = useState<MetadataPatch["addLabels"]>([]);
  const [removed, setRemoved] = useState<MetadataPatch["removeLabels"]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setName(single?.name ?? "");
    setSourceName(single?.sourceName ?? "");
    setDescription(single?.description ?? "");
    setDescriptionEdited(false);
    setAdded([]);
    setRemoved([]);
    setError(null);
  }, [single?.id]);

  const visibleLabels = useMemo(() => Array.from(new Map(portraits.flatMap((portrait) => portrait.labels).map((label) => [`${label.category}\u0000${label.value}`, label])).values()).filter((label) => !removed.some((item) => item.category === label.category && item.value === label.value)), [portraits, removed]);
  if (!portraits.length) return null;
  const stopPreviewKeys = (event: React.KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>) => {
    if (event.key === "Escape") event.preventDefault();
    event.stopPropagation();
  };
  const addLabel = () => {
    if (!category.trim() || !value.trim()) return;
    setAdded((current) => [...current, { category: category.trim(), value: value.trim() }]);
    setCategory("");
    setValue("");
  };
  const save = async () => {
    setBusy(true);
    setError(null);
    try {
      const patch: MetadataPatch = {
        name: single && name !== single.name ? name : undefined,
        description: single
          ? description !== (single.description ?? "") ? description : undefined
          : descriptionEdited ? description : undefined,
        addLabels: added,
        removeLabels: removed,
      };
      if (patch.name !== undefined || patch.description !== undefined || patch.addLabels.length || patch.removeLabels.length) {
        await editMetadata(portraits.map((portrait) => portrait.id), patch);
      }
      if (single && renameSource && sourceName !== single.sourceName) await renameSource(single.sourceId, sourceName);
      onSaved?.();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "Metadata could not be saved.");
    } finally {
      setBusy(false);
    }
  };

  return <section className="metadata-editor" aria-label="Metadata editor">
    <h3>{single ? "Edit metadata" : `Edit ${portraits.length} portraits`}</h3>
    {single ? <label>Name<input aria-label="Name" value={name} onChange={(event) => setName(event.target.value)} onKeyDown={stopPreviewKeys} /></label> : <p>Names stay unchanged when editing several portraits.</p>}
    {single ? <label>Source<input aria-label="Source" value={sourceName} onChange={(event) => setSourceName(event.target.value)} onKeyDown={stopPreviewKeys} /></label> : null}
    <label>Description<textarea aria-label="Description" value={description} onChange={(event) => { setDescription(event.target.value); setDescriptionEdited(true); }} onKeyDown={stopPreviewKeys} /></label>
    <div className="metadata-label-list" aria-label="Current labels">{visibleLabels.map((label) => <button type="button" key={`${label.category}/${label.value}`} onClick={() => setRemoved((current) => [...current, label])}>Remove {label.category}: {label.value}</button>)}</div>
    <div className="metadata-label-fields"><label>Category<input aria-label="Label category" value={category} onChange={(event) => setCategory(event.target.value)} onKeyDown={stopPreviewKeys} /></label><label>Value<input aria-label="Label value" value={value} onChange={(event) => setValue(event.target.value)} onKeyDown={stopPreviewKeys} /></label><button type="button" onClick={addLabel}>Add label</button></div>
    {added.length ? <p className="metadata-pending-labels">Adding {added.map((label) => `${label.category}: ${label.value}`).join(", ")}</p> : null}
    {error ? <p role="alert">{error}</p> : null}
    <button type="button" className="secondary-action" disabled={busy} onClick={() => void save()}>Save metadata</button>
  </section>;
}
