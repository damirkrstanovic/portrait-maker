import type { LibraryApi, LibraryInfo } from "../lib/api";
import type { CatalogPage, Destination, ExportPlan, Job, Page, Query } from "../lib/contracts";

const library: LibraryInfo = { id: "test-library", name: "Browser test library", path: "/tmp/browser-test-library" };
const portrait = (id: string, name: string) => ({ id, sourceId: "source-a", name, sourceName: "Source A", originalFolder: id, description: null, labels: [], selected: false, trashedAt: null });
const fixtureCount = Math.min(1395, Math.max(1, Number(new URLSearchParams(window.location.search).get("count")) || 1));
const destinationFixture = new URLSearchParams(window.location.search).get("testDestinations") === "long";
const longDestination: Destination = {
  id: "long-destination", game: "kingmaker", origin: "steam", state: "existing",
  name: "Kingmaker — /tmp/a-really-long-unbroken-destination-token-that-must-wrap-inside-the-destinations-modal-at-the-smallest-supported-viewport/Portraits",
  path: "/tmp/a-really-long-unbroken-destination-token-that-must-wrap-inside-the-destinations-modal-at-the-smallest-supported-viewport/Portraits",
  evidence: ["Steam library: /tmp/a-really-long-unbroken-steam-library-token-that-must-wrap-without-growing-the-modal-beyond-the-viewport"],
};
const trashed = new Set<string>();
const selected = new Set<string>();
const purged = new Set<string>();
const jobs = new Map<string, Job>();
const fixtureItems = (query: Query, page: Page) => {
  const all = fixtureCount === 1
    ? [portrait(query.sourceIds.length ? "filtered" : "old", query.sourceIds.length ? "Filtered ranger" : "Old ranger")]
    : Array.from({ length: fixtureCount }, (_, index) => portrait(`fixture-${index}`, `Fixture portrait ${index}`));
  return all.filter((item) => !purged.has(item.id) && trashed.has(item.id) === query.trash && (!query.selectedOnly || selected.has(item.id)))
    .slice(page.offset, page.offset + page.limit)
    .map((item) => ({ ...item, selected: selected.has(item.id), trashedAt: trashed.has(item.id) ? "2026-09-16T10:00:00Z" : null }));
};
const page = (query: Query, page: Page): Promise<CatalogPage> => new Promise((resolve) => window.setTimeout(() => {
  const all = fixtureItems(query, { offset: 0, limit: fixtureCount });
  resolve({ items: all.slice(page.offset, page.offset + page.limit), total: all.length, revision: 1 });
}, query.sourceIds.length ? 5_000 : 0));
const fixtureLabels = fixtureCount > 1 ? Array.from({ length: 48 }, (_, index) => ({ category: "role", value: `fixture-${index}`, displayValue: `Fixture ${index}`, count: 1 })) : [];

export const browserMockApi: LibraryApi = {
  chooseCreateLocation: async () => null, chooseOpenLocation: async () => library.path, createLibrary: async () => library, openLibrary: async () => library, closeLibrary: async () => {}, getLastLibraryPath: async () => null,
  chooseImportFolder: async () => null, chooseImportArchive: async () => null, startImport: async () => ({ id: "job", state: "done", completed: 0, total: 0, message: "" }), getJob: async (id) => jobs.get(id) ?? null, cancelJob: async (id) => { const job = jobs.get(id); if (job) jobs.set(id, { ...job, state: "cancelled", message: id==="export-job" ? "Export cancelled" : "Permanent deletion cancelled" }); }, getImportReport: async () => null,
  queryCatalog: page, getCatalogFacets: async () => ({ sources: [{ id: "source-a", name: "Source A", count: fixtureCount }], labels: fixtureLabels }), getDisplayRole: async () => "large", setDisplayRole: async () => {},
  discoverDestinations: async () => ({ destinations: destinationFixture ? [longDestination] : [], warnings: [] }), getSavedDestinations: async () => destinationFixture ? [longDestination] : [],
  changeSelection: async (target, action) => {
    const ids = "ids" in target ? target.ids : fixtureItems(target.matching, { offset: 0, limit: fixtureCount }).map((item) => item.id);
    if (action === "clear") selected.clear(); else for (const id of ids) action === "add" ? selected.add(id) : selected.delete(id);
    return selected.size;
  },
  trashPortraits: async (ids) => { for (const id of ids) { trashed.add(id); selected.delete(id); } },
  restorePortraits: async (ids) => { for (const id of ids) trashed.delete(id); },
  startPurge: async (ids) => {
    const running: Job = { id: `purge-${Date.now()}`, state: "running", completed: 0, total: ids.length, message: "Preparing permanent deletion" };
    for (const id of ids) { purged.add(id); trashed.delete(id); selected.delete(id); }
    jobs.set(running.id, { ...running, state: "done", completed: ids.length, message: `Permanently deleted ${ids.length} portraits` });
    return running;
  },
};

// Development-only export fixture; this module is eliminated from production builds.
let exportPlan: ExportPlan | null = null;
browserMockApi.designateExportTarget = async path => path;
browserMockApi.planExport = async request => {
  const count=fixtureItems({text:"",sourceIds:[],labels:[],trash:false,selectedOnly:request.scope==="selected"},{offset:0,limit:fixtureCount}).length;
  if(request.scope==="selected" && count===0) throw {message:"Select at least one active portrait before exporting the selection."};
  exportPlan={id:"browser-export-plan",target:request.target,portraitCount:count,requiresConfirmation:request.mode==="replace",warnings:request.mode==="replace" ? ["Saved-game references are not migrated."] : [],actions:Array.from({length:count},(_,i)=>["Small.png","Medium.png","Fulllength.png"].map(name=>({kind:"add" as const,path:`${request.target}/pm-library-portrait-${i}/${name}`,reason:"Missing portrait image"}))).flat()};
  return exportPlan;
};
browserMockApi.discardExportPlan = async id => { if(exportPlan?.id===id) exportPlan=null; };
browserMockApi.validateExportPlan = async id => { if(!exportPlan || exportPlan.id!==id) throw {message:"Preview again."}; window.sessionStorage.setItem("exportValidatedId",id);return exportPlan; };
browserMockApi.applyExport = async (id,confirmed) => {
  if(!exportPlan || exportPlan.id!==id || (exportPlan.requiresConfirmation && !confirmed) || window.sessionStorage.getItem("exportValidatedId")!==id) throw {message:"Exact confirmation required."};
  window.sessionStorage.setItem("exportApplyCount",String(Number(window.sessionStorage.getItem("exportApplyCount") ?? 0)+1)); exportPlan=null;
  const running: Job={id:"export-job",state:"running",completed:0,total:fixtureCount,message:"Preparing export"}; jobs.set(running.id,running);
  if(new URLSearchParams(window.location.search).get("exportPause")!=="1") window.setTimeout(()=>{if(jobs.get(running.id)?.state==="running") jobs.set(running.id,{...running,state:"done",completed:fixtureCount,message:"Export complete"});},600);
  return running;
};

browserMockApi.getExportReport = async () => ({added:fixtureCount*3,overwritten:0,removed:0,preserved:2,issues:[]});
