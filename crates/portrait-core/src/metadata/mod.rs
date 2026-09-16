mod infer;

use rusqlite::{Transaction, params};
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use crate::catalog::{bump_catalog_revision, refresh_search_document, refresh_source_documents};
use crate::types::Label;
use crate::{CoreError, Library, Result};

pub use infer::infer_labels;

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataPatch {
    pub name: Option<String>,
    pub description: Option<Option<String>>,
    pub add_labels: Vec<Label>,
    pub remove_labels: Vec<Label>,
}

pub fn edit_metadata(library: &mut Library, ids: &[Uuid], patch: MetadataPatch) -> Result<()> {
    if patch.name.is_some() && ids.len() != 1 {
        return Err(CoreError::MetadataNameRequiresSinglePortrait);
    }
    let name = patch.name.map(trimmed_name).transpose()?;
    let description = patch.description.map(|value| {
        value
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty())
    });
    let added = patch
        .add_labels
        .iter()
        .map(normalize_label)
        .collect::<Result<Vec<_>>>()?;
    let removed = patch
        .remove_labels
        .iter()
        .map(normalize_label)
        .collect::<Result<Vec<_>>>()?;
    let transaction = library.connection().unchecked_transaction()?;
    for id in ids {
        if !portrait_exists(&transaction, *id)? {
            return Err(CoreError::PortraitNotFound);
        }
        if let Some(name) = &name {
            transaction.execute(
                "UPDATE portraits SET name = ?1 WHERE id = ?2",
                params![name, id.to_string()],
            )?;
        }
        if let Some(description) = &description {
            transaction.execute(
                "UPDATE portraits SET description = ?1 WHERE id = ?2",
                params![description, id.to_string()],
            )?;
        }
        for label in &added {
            let label_id = ensure_label(&transaction, label)?;
            transaction.execute(
                "INSERT INTO portrait_labels (portrait_id, label_id, origin, producer, producer_version) VALUES (?1, ?2, 'user', NULL, NULL) ON CONFLICT(portrait_id, label_id) DO UPDATE SET origin = 'user', producer = NULL, producer_version = NULL",
                params![id.to_string(), label_id],
            )?;
        }
        for label in &removed {
            suppress_label(&transaction, *id, label)?;
            transaction.execute(
                "DELETE FROM portrait_labels WHERE portrait_id = ?1 AND label_id = (SELECT id FROM labels WHERE category = ?2 AND normalized_value = ?3)",
                params![id.to_string(), label.category, label.value],
            )?;
        }
        refresh_search_document(&transaction, *id)?;
    }
    if !ids.is_empty() {
        bump_catalog_revision(&transaction)?;
    }
    transaction.commit()?;
    Ok(())
}

pub fn rename_source(library: &mut Library, id: Uuid, name: &str) -> Result<()> {
    let name = trimmed_name(name.to_owned())?;
    let transaction = library.connection().unchecked_transaction()?;
    if transaction.execute(
        "UPDATE sources SET name = ?1 WHERE id = ?2",
        params![name, id.to_string()],
    )? == 0
    {
        return Err(CoreError::SourceNotFound);
    }
    refresh_source_documents(&transaction, id)?;
    bump_catalog_revision(&transaction)?;
    transaction.commit()?;
    Ok(())
}

/// Apply a repeatable inference producer while honoring user ownership and suppression.
pub fn apply_inferred_labels(
    library: &mut Library,
    id: Uuid,
    labels: &[Label],
    producer: &str,
    producer_version: &str,
) -> Result<()> {
    let labels = labels
        .iter()
        .map(normalize_label)
        .collect::<Result<Vec<_>>>()?;
    let transaction = library.connection().unchecked_transaction()?;
    if !portrait_exists(&transaction, id)? {
        return Err(CoreError::PortraitNotFound);
    }
    for label in labels {
        let label_id = ensure_label(&transaction, &label)?;
        transaction.execute(
            "INSERT OR IGNORE INTO portrait_labels (portrait_id, label_id, origin, producer, producer_version) SELECT ?1, ?2, 'filename', ?3, ?4 WHERE NOT EXISTS (SELECT 1 FROM user_label_suppressions WHERE portrait_id = ?1 AND category = ?5 AND normalized_value = ?6)",
            params![id.to_string(), label_id, producer, producer_version, label.category, label.value],
        )?;
    }
    refresh_search_document(&transaction, id)?;
    bump_catalog_revision(&transaction)?;
    transaction.commit()?;
    Ok(())
}

fn portrait_exists(transaction: &Transaction<'_>, id: Uuid) -> Result<bool> {
    Ok(transaction.query_row(
        "SELECT EXISTS(SELECT 1 FROM portraits WHERE id = ?1)",
        [id.to_string()],
        |row| row.get(0),
    )?)
}

fn trimmed_name(value: String) -> Result<String> {
    let value = value.trim();
    if value.is_empty() {
        Err(CoreError::InvalidMetadataName)
    } else {
        Ok(value.to_owned())
    }
}

fn normalize_label(label: &Label) -> Result<Label> {
    let category = label.category.trim();
    let value = label.value.trim();
    if category.is_empty() || value.is_empty() {
        return Err(CoreError::InvalidMetadataLabel);
    }
    Ok(Label {
        category: category.nfc().collect::<String>().to_lowercase(),
        value: value.nfc().collect::<String>().to_lowercase(),
    })
}

fn ensure_label(transaction: &Transaction<'_>, label: &Label) -> Result<i64> {
    transaction.execute("INSERT OR IGNORE INTO labels (category, normalized_value, display_value) VALUES (?1, ?2, ?3)", params![label.category, label.value, label.value])?;
    Ok(transaction.query_row(
        "SELECT id FROM labels WHERE category = ?1 AND normalized_value = ?2",
        params![label.category, label.value],
        |row| row.get(0),
    )?)
}

fn suppress_label(transaction: &Transaction<'_>, id: Uuid, label: &Label) -> Result<()> {
    transaction.execute(
        "INSERT OR IGNORE INTO user_label_suppressions (portrait_id, category, normalized_value) VALUES (?1, ?2, ?3)",
        params![id.to_string(), label.category, label.value],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO suppressed_inferred_labels (portrait_id, category, normalized_value, producer, producer_version) VALUES (?1, ?2, ?3, 'path-vocabulary', '1')",
        params![id.to_string(), label.category, label.value],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO suppressed_inferred_labels (portrait_id, category, normalized_value, producer, producer_version) SELECT ?1, ?2, ?3, producer, producer_version FROM portrait_labels pl JOIN labels l ON l.id = pl.label_id WHERE pl.portrait_id = ?1 AND l.category = ?2 AND l.normalized_value = ?3 AND pl.origin != 'user' AND pl.producer IS NOT NULL AND pl.producer_version IS NOT NULL",
        params![id.to_string(), label.category, label.value],
    )?;
    Ok(())
}
