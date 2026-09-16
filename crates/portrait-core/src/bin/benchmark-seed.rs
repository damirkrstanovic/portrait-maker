//! Disposable scale fixture helper used by `scripts/seed-benchmark.mjs`.
//!
//! It imports normal folder sources instead of manufacturing SQLite rows, then
//! measures the public catalog and bulk-selection APIs in a fresh process.

use std::env;
use std::path::PathBuf;
use std::time::Instant;

use portrait_core::Library;
use portrait_core::catalog::{bump_catalog_revision, query_catalog, refresh_search_document};
use portrait_core::import::{JobContext, import_portraits};
use portrait_core::selection::{SelectionAction, SelectionTarget, change_selection};
use portrait_core::trash::trash_portraits;
use portrait_core::types::{ImportKind, ImportRequest, Label, Page, Query};
use rusqlite::params;
use serde_json::{Value, json};
use uuid::Uuid;

const PAGE_LIMIT: u32 = 16;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = env::args_os();
    let _program = args.next();
    let command = args
        .next()
        .ok_or("usage: benchmark-seed <seed|measure> <library> [source ...]")?;
    let library = PathBuf::from(
        args.next()
            .ok_or("usage: benchmark-seed <seed|measure> <library> [source ...]")?,
    );
    let command = command.to_string_lossy();
    let output = match command.as_ref() {
        "seed" => {
            let sources = args.map(PathBuf::from).collect::<Vec<_>>();
            seed(library, sources)?
        }
        "measure" => measure(library)?,
        _ => return Err("usage: benchmark-seed <seed|measure> <library> [source ...]".into()),
    };
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}

fn seed(library_root: PathBuf, sources: Vec<PathBuf>) -> Result<Value, Box<dyn std::error::Error>> {
    if sources.len() < 2 {
        return Err("seed requires at least two normal folder sources".into());
    }
    if library_root.exists() {
        return Err(format!(
            "refusing existing library destination: {}",
            library_root.display()
        )
        .into());
    }
    if sources.iter().any(|source| !source.is_dir()) {
        return Err("every benchmark source must be an existing directory".into());
    }

    let mut library = Library::create(&library_root)?;
    let mut imported = 0_u64;
    for (index, source) in sources.iter().enumerate() {
        let report = import_portraits(
            &mut library,
            ImportRequest {
                path: source.clone(),
                source_name: format!("Synthetic benchmark source {:02}", index + 1),
                kind: ImportKind::Folder,
                resize: false,
            },
            &JobContext::default(),
        )?;
        if report.cancelled || report.skipped != 0 {
            return Err(format!(
                "benchmark source {} was not fully imported: imported {}, skipped {}, cancelled {}",
                source.display(),
                report.imported,
                report.skipped,
                report.cancelled
            )
            .into());
        }
        imported += report.imported;
    }

    let ids = portrait_ids(&library)?;
    add_searchable_benchmark_metadata(&mut library, &ids)?;
    change_selection(
        &mut library,
        SelectionTarget::Matching(Query {
            text: "elf".into(),
            ..Query::default()
        }),
        SelectionAction::Add,
    )?;
    let trashed = ids
        .iter()
        .copied()
        .enumerate()
        .filter_map(|(index, id)| (index % 23 == 0).then_some(id))
        .collect::<Vec<_>>();
    trash_portraits(&mut library, &trashed)?;

    Ok(json!({
        "library": library_root,
        "imported": imported,
        "sources": sources.len(),
        "selected": selected_count(&library)?,
        "trashed": trashed.len(),
    }))
}

fn add_searchable_benchmark_metadata(
    library: &mut Library,
    ids: &[Uuid],
) -> Result<(), Box<dyn std::error::Error>> {
    let transaction = library.connection().unchecked_transaction()?;
    let mut mood_ids = Vec::new();
    for mood in ["calm", "grim", "resolute", "curious"] {
        transaction.execute(
            "INSERT OR IGNORE INTO labels (category, normalized_value, display_value) VALUES ('mood', ?1, ?1)",
            [mood],
        )?;
        mood_ids.push(transaction.query_row(
            "SELECT id FROM labels WHERE category = 'mood' AND normalized_value = ?1",
            [mood],
            |row| row.get::<_, i64>(0),
        )?);
    }
    for (index, id) in ids.iter().copied().enumerate() {
        transaction.execute(
            "UPDATE portraits SET description = ?1 WHERE id = ?2",
            params![
                format!(
                    "Synthetic benchmark portrait {index:05}; deterministic catalog description."
                ),
                id.to_string()
            ],
        )?;
        transaction.execute(
            "INSERT INTO portrait_labels (portrait_id, label_id, origin, producer, producer_version) VALUES (?1, ?2, 'user', NULL, NULL)",
            params![id.to_string(), mood_ids[index % mood_ids.len()]],
        )?;
        refresh_search_document(&transaction, id)?;
    }
    bump_catalog_revision(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn measure(library_root: PathBuf) -> Result<Value, Box<dyn std::error::Error>> {
    let opened_at = Instant::now();
    let mut library = Library::open(&library_root)?;
    let cold_open_ms = elapsed_ms(opened_at);

    let first_grid_at = Instant::now();
    let first_page = query_catalog(
        &library,
        &Query::default(),
        Page {
            offset: 0,
            limit: PAGE_LIMIT,
        },
    )?;
    let first_grid_ms = elapsed_ms(first_grid_at);

    let mut fts_samples = Vec::new();
    for text in ["elf", "female elf archer", "benchmark"] {
        for _ in 0..20 {
            let started = Instant::now();
            let page = query_catalog(
                &library,
                &Query {
                    text: text.into(),
                    ..Query::default()
                },
                Page {
                    offset: 0,
                    limit: PAGE_LIMIT,
                },
            )?;
            fts_samples.push(elapsed_ms(started));
            if page.items.len() > PAGE_LIMIT as usize {
                return Err("query exceeded the configured page bound".into());
            }
        }
    }

    let source_id = library.connection().query_row(
        "SELECT id FROM sources ORDER BY name LIMIT 1",
        [],
        |row| row.get::<_, String>(0),
    )?;
    let source_id = Uuid::parse_str(&source_id)?;
    let filter_started = Instant::now();
    let filtered = query_catalog(
        &library,
        &Query {
            source_ids: vec![source_id],
            labels: vec![Label {
                category: "mood".into(),
                value: "calm".into(),
            }],
            ..Query::default()
        },
        Page {
            offset: 0,
            limit: PAGE_LIMIT,
        },
    )?;
    if filtered.items.len() > PAGE_LIMIT as usize {
        return Err("filtered query exceeded the configured page bound".into());
    }
    let filter_change_ms = elapsed_ms(filter_started);

    let selection_query = Query {
        text: "dwarf".into(),
        ..Query::default()
    };
    let select_started = Instant::now();
    let selected_after_add = change_selection(
        &mut library,
        SelectionTarget::Matching(selection_query.clone()),
        SelectionAction::Add,
    )?;
    let bulk_select_ms = elapsed_ms(select_started);
    let remove_started = Instant::now();
    let selected_after_remove = change_selection(
        &mut library,
        SelectionTarget::Matching(selection_query),
        SelectionAction::Remove,
    )?;
    let bulk_remove_ms = elapsed_ms(remove_started);

    fts_samples.sort_by(f64::total_cmp);
    let fts_p95_ms = percentile(&fts_samples, 0.95);
    let peak_rss_kib = linux_peak_rss_kib();
    Ok(json!({
        "machine": {
            "os": env::consts::OS,
            "arch": env::consts::ARCH,
            "parallelism": std::thread::available_parallelism().map(|count| count.get()).ok(),
        },
        "dataset": {
            "totalRows": library.connection().query_row("SELECT count(*) FROM portraits", [], |row| row.get::<_, i64>(0))?,
            "portraits": first_page.total,
            "firstPageItems": first_page.items.len(),
            "pageLimit": PAGE_LIMIT,
            "sources": library.connection().query_row("SELECT count(*) FROM sources", [], |row| row.get::<_, i64>(0))?,
            "selectedAfterAdd": selected_after_add,
            "selectedAfterRemove": selected_after_remove,
            "trashed": library.connection().query_row("SELECT count(*) FROM portraits WHERE trashed_at IS NOT NULL", [], |row| row.get::<_, i64>(0))?,
        },
        "measurementsMs": {
            "freshProcessOpen": cold_open_ms,
            "firstVisibleGridQuery": first_grid_ms,
            "representativeFtsP95": fts_p95_ms,
            "filterChange": filter_change_ms,
            "bulkSelectAllMatching": bulk_select_ms,
            "bulkRemoveAllMatching": bulk_remove_ms,
        },
        "memory": {
            "processPeakRssKiB": peak_rss_kib,
            "scrollBound": "Catalog requests a 16-item page; browser virtual-grid evidence is recorded separately.",
            "fullImageDecode": "No thumbnail or image endpoint is called by this catalog-only measurement."
        },
        "targets": {
            "warmQueryP95Under200ms": fts_p95_ms < 200.0,
            "firstGridUnder2s": first_grid_ms < 2000.0,
        },
        "notes": [
            "Open is measured in a fresh benchmark process after seeding; filesystem cache state is not forcibly cleared.",
            "This tool keeps the generated library and input folders under the temporary directory for inspection."
        ]
    }))
}

fn portrait_ids(library: &Library) -> Result<Vec<Uuid>, Box<dyn std::error::Error>> {
    let mut statement = library
        .connection()
        .prepare("SELECT id FROM portraits ORDER BY id")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    rows.map(|row| Ok(Uuid::parse_str(&row?)?)).collect()
}

fn selected_count(library: &Library) -> Result<i64, Box<dyn std::error::Error>> {
    Ok(library
        .connection()
        .query_row("SELECT count(*) FROM selection", [], |row| row.get(0))?)
}

fn elapsed_ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1_000.0
}

fn percentile(samples: &[f64], percentile: f64) -> f64 {
    let index = ((samples.len() as f64 * percentile).ceil() as usize).saturating_sub(1);
    samples[index]
}

fn linux_peak_rss_kib() -> Option<u64> {
    let contents = std::fs::read_to_string("/proc/self/status").ok()?;
    contents.lines().find_map(|line| {
        line.strip_prefix("VmHWM:")?
            .split_whitespace()
            .next()?
            .parse::<u64>()
            .ok()
    })
}
