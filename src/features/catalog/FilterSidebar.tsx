import type { CatalogFacets, Label, Query } from "../../lib/contracts";

type Props = { facets: CatalogFacets; query: Query; onQuery(query: Query): void };
const sameLabel = (left: Label, right: Label) => left.category === right.category && left.value === right.value;

export function FilterSidebar({ facets, query, onQuery }: Props) {
  const updateSources = (id: string, checked: boolean) => onQuery({ ...query, sourceIds: checked ? [...query.sourceIds, id] : query.sourceIds.filter((current) => current !== id) });
  const updateLabel = (label: Label, checked: boolean) => onQuery({ ...query, labels: checked ? [...query.labels, label] : query.labels.filter((current) => !sameLabel(current, label)) });
  return <aside className="filter-sidebar" aria-label="Catalog filters">
    <div className="filter-heading"><h2>Sources & labels</h2>{query.sourceIds.length || query.labels.length || query.text ? <button type="button" onClick={() => onQuery({ text: "", sourceIds: [], labels: [], selectedOnly: false, trash: false })}>Clear filters</button> : null}</div>
    <fieldset><legend>View</legend><label><input type="radio" name="catalog-view" checked={!query.trash} onChange={() => onQuery({ ...query, trash: false })} />Active portraits</label><label><input type="radio" name="catalog-view" checked={query.trash} onChange={() => onQuery({ ...query, trash: true, selectedOnly: false })} />Trash</label>{!query.trash ? <label><input type="checkbox" checked={query.selectedOnly} onChange={(event) => onQuery({ ...query, selectedOnly: event.target.checked })} />Selected only</label> : null}</fieldset>
    <fieldset><legend>Sources</legend>{facets.sources.map((source) => <label key={source.id}><input type="checkbox" checked={query.sourceIds.includes(source.id)} onChange={(event) => updateSources(source.id, event.target.checked)} />{source.name}<small>{source.count}</small></label>)}</fieldset>
    {facets.labels.map((label) => <label className="label-filter" key={`${label.category}/${label.value}`}><input type="checkbox" checked={query.labels.some((current) => sameLabel(current, label))} onChange={(event) => updateLabel({ category: label.category, value: label.value }, event.target.checked)} /><span>{label.category}: {label.displayValue}</span><small>{label.count}</small></label>)}
  </aside>;
}
