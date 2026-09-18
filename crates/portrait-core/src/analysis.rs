//! Storage for portrait descriptions and labels generated from the large image.
//!
//! Model output is intentionally treated as a repeatable enrichment. It never overwrites the
//! user's description, labels, or a label they explicitly suppressed.

use rusqlite::{OptionalExtension, Transaction, params};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use crate::catalog::{bump_catalog_revision, refresh_search_document};
use crate::types::Label;
use crate::{CoreError, Library, Result};

/// Version of the structured prompt understood by this build.
pub const PROMPT_VERSION: &str = "2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortraitAnalysis {
    pub description: String,
    #[serde(default)]
    pub labels: Vec<Label>,
}

/// Return active portraits that should receive analysis, in a stable order.
pub fn analysis_candidates(
    library: &Library,
    selected_only: bool,
    overwrite: bool,
) -> Result<Vec<Uuid>> {
    let mut sql = String::from(
        "SELECT p.id FROM portraits p \
         LEFT JOIN portrait_analysis pa ON pa.portrait_id = p.id \
         WHERE p.trashed_at IS NULL",
    );
    if selected_only {
        sql.push_str(" AND EXISTS (SELECT 1 FROM selection s WHERE s.portrait_id = p.id)");
    }
    if !overwrite {
        sql.push_str(" AND pa.portrait_id IS NULL");
    }
    sql.push_str(" ORDER BY p.name COLLATE NOCASE, p.id");
    let mut statement = library.connection().prepare(&sql)?;
    statement
        .query_map([], |row| row.get::<_, String>(0))?
        .map(|id| Uuid::parse_str(&id?).map_err(|error| CoreError::Migration(error.to_string())))
        .collect()
}

/// Persist one model result and update catalog search atomically.
pub fn save_analysis(
    library: &mut Library,
    id: Uuid,
    analysis: &PortraitAnalysis,
    model: &str,
) -> Result<()> {
    let description = normalize_description(&analysis.description)?;
    let model = normalize_model(model)?;
    let labels = analysis
        .labels
        .iter()
        .filter_map(normalize_model_label)
        .collect::<Vec<_>>();

    let transaction = library.connection().unchecked_transaction()?;
    let active = transaction
        .query_row(
            "SELECT trashed_at IS NULL FROM portraits WHERE id = ?1",
            [id.to_string()],
            |row| row.get::<_, bool>(0),
        )
        .optional()?;
    if active != Some(true) {
        return Err(CoreError::PortraitNotFound);
    }

    transaction.execute(
        "INSERT INTO portrait_analysis (portrait_id, description, model, prompt_version, analyzed_at) \
         VALUES (?1, ?2, ?3, ?4, strftime('%Y-%m-%dT%H:%M:%fZ', 'now')) \
         ON CONFLICT(portrait_id) DO UPDATE SET \
           description = excluded.description, model = excluded.model, \
           prompt_version = excluded.prompt_version, analyzed_at = excluded.analyzed_at",
        params![id.to_string(), description, model, PROMPT_VERSION],
    )?;

    // A re-run only replaces labels owned by a model. User labels and their suppressions remain
    // authoritative, and filename labels are retained as independent metadata.
    transaction.execute(
        "DELETE FROM portrait_labels WHERE portrait_id = ?1 AND origin = 'model'",
        [id.to_string()],
    )?;
    for label in labels {
        let label_id = ensure_label(&transaction, &label)?;
        transaction.execute(
            "INSERT OR IGNORE INTO portrait_labels \
                 (portrait_id, label_id, origin, producer, producer_version) \
             SELECT ?1, ?2, 'model', ?3, ?4 \
             WHERE NOT EXISTS (SELECT 1 FROM user_label_suppressions \
                               WHERE portrait_id = ?1 AND category = ?5 AND normalized_value = ?6) \
               AND NOT EXISTS (SELECT 1 FROM portrait_labels user_pl \
                               JOIN labels user_l ON user_l.id = user_pl.label_id \
                               WHERE user_pl.portrait_id = ?1 AND user_pl.origin = 'user' \
                                 AND user_l.category = ?5)",
            params![
                id.to_string(),
                label_id,
                model,
                PROMPT_VERSION,
                label.category,
                label.value
            ],
        )?;
    }
    refresh_search_document(&transaction, id)?;
    bump_catalog_revision(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn normalize_description(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid("description must contain visible text"));
    }
    Ok(value.nfc().collect())
}

fn normalize_model(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(invalid("model name must not be empty"));
    }
    Ok(value.nfc().collect())
}

fn normalize_model_label(label: &Label) -> Option<Label> {
    let category = label
        .category
        .trim()
        .nfc()
        .collect::<String>()
        .to_lowercase();
    let value = label.value.trim().nfc().collect::<String>().to_lowercase();
    // Labels are freeform. Ignore blank entries without discarding the description.
    if category.is_empty() || value.is_empty() {
        return None;
    }
    Some(Label { category, value })
}

fn ensure_label(transaction: &Transaction<'_>, label: &Label) -> Result<i64> {
    transaction.execute(
        "INSERT OR IGNORE INTO labels (category, normalized_value, display_value) VALUES (?1, ?2, ?3)",
        params![label.category, label.value, label.value],
    )?;
    Ok(transaction.query_row(
        "SELECT id FROM labels WHERE category = ?1 AND normalized_value = ?2",
        params![label.category, label.value],
        |row| row.get(0),
    )?)
}

fn invalid(message: impl Into<String>) -> CoreError {
    CoreError::InvalidModelAnalysis(message.into())
}
