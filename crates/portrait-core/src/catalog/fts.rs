use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;
use uuid::Uuid;

use crate::{Library, Result};

type SearchDocument = (i64, String, String, String, String, String);

#[must_use]
pub fn compile_fts(text: &str) -> Option<String> {
    let normalized = text.nfc().collect::<String>();
    let mut words = Vec::new();
    let mut word = String::new();
    for character in normalized.chars() {
        if character.is_alphanumeric() || (!word.is_empty() && is_combining_mark(character)) {
            word.push(character);
        } else if !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    let terms = words
        .into_iter()
        .map(|term| format!("\"{term}\""))
        .collect::<Vec<_>>();
    (!terms.is_empty()).then(|| terms.join(" AND "))
}

pub fn refresh_search_document(transaction: &Transaction<'_>, id: Uuid) -> Result<()> {
    let old = transaction
        .query_row(
            "SELECT id, name, original_folder, source_name, labels, description \
             FROM search_documents WHERE portrait_id = ?1",
            [id.to_string()],
            read_search_document,
        )
        .optional()?;
    if let Some((rowid, name, folder, source, labels, description)) = old {
        transaction.execute(
            "INSERT INTO search_index(search_index, rowid, name, original_folder, source_name, labels, description) \
             VALUES('delete', ?1, ?2, ?3, ?4, ?5, ?6)",
            params![rowid, name, folder, source, labels, description],
        )?;
        transaction.execute("DELETE FROM search_documents WHERE id = ?1", [rowid])?;
    }

    transaction.execute(
        "INSERT INTO search_documents \
            (portrait_id, name, original_folder, source_name, labels, description) \
         SELECT p.id, p.name, p.original_folder, COALESCE(( \
                    SELECT group_concat(source_name, ' ') FROM ( \
                        SELECT s2.name AS source_name FROM portrait_sources ps \
                        JOIN sources s2 ON s2.id = ps.source_id \
                        WHERE ps.portrait_id = p.id ORDER BY s2.name COLLATE NOCASE \
                    ) \
                ), s.name), \
                COALESCE(( \
                    SELECT group_concat(label_value, ' ') FROM ( \
                        SELECT l.display_value AS label_value \
                        FROM portrait_labels pl \
                        JOIN labels l ON l.id = pl.label_id \
                        WHERE pl.portrait_id = p.id \
                        ORDER BY l.category, l.normalized_value \
                    ) \
                ), ''), trim(COALESCE(p.description, '') || ' ' || COALESCE(pa.description, '')) \
         FROM portraits p JOIN sources s ON s.id = p.source_id \
         LEFT JOIN portrait_analysis pa ON pa.portrait_id = p.id \
         WHERE p.id = ?1",
        [id.to_string()],
    )?;

    if let Some((rowid, name, folder, source, labels, description)) = transaction
        .query_row(
            "SELECT id, name, original_folder, source_name, labels, description \
             FROM search_documents WHERE portrait_id = ?1",
            [id.to_string()],
            read_search_document,
        )
        .optional()?
    {
        transaction.execute(
            "INSERT INTO search_index(rowid, name, original_folder, source_name, labels, description) \
             VALUES(?1, ?2, ?3, ?4, ?5, ?6)",
            params![rowid, name, folder, source, labels, description],
        )?;
    }
    Ok(())
}

pub fn refresh_source_documents(transaction: &Transaction<'_>, source_id: Uuid) -> Result<u64> {
    let ids = {
        let mut statement = transaction.prepare(
            "SELECT id FROM portraits WHERE source_id = ?1 \
             UNION SELECT portrait_id FROM portrait_sources WHERE source_id = ?2 ORDER BY 1",
        )?;
        statement
            .query_map([source_id.to_string(), source_id.to_string()], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
    };
    for id in &ids {
        refresh_search_document(
            transaction,
            Uuid::parse_str(id).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    0,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?,
        )?;
    }
    Ok(ids.len() as u64)
}

pub fn rebuild_search_index(library: &Library) -> Result<()> {
    let transaction = library.connection().unchecked_transaction()?;
    transaction.execute(
        "INSERT INTO search_index(search_index) VALUES('delete-all')",
        [],
    )?;
    transaction.execute("DELETE FROM search_documents", [])?;
    let ids = {
        let mut statement = transaction.prepare("SELECT id FROM portraits ORDER BY id")?;
        statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?
    };
    for id in ids {
        let id = Uuid::parse_str(&id).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                0,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
        refresh_search_document(&transaction, id)?;
    }
    bump_catalog_revision(&transaction)?;
    transaction.commit()?;
    Ok(())
}

pub fn bump_catalog_revision(transaction: &Transaction<'_>) -> Result<u64> {
    let state: String = transaction.query_row(
        "SELECT state_json FROM operation_state WHERE operation_id = 'catalog_revision'",
        [],
        |row| row.get(0),
    )?;
    let revision = serde_json::from_str::<Value>(&state)?
        .get("revision")
        .and_then(Value::as_u64)
        .unwrap_or(0)
        .saturating_add(1);
    transaction.execute(
        "UPDATE operation_state SET state_json = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') \
         WHERE operation_id = 'catalog_revision'",
        [serde_json::to_string(&json!({ "revision": revision }))?],
    )?;
    Ok(revision)
}

fn read_search_document(row: &rusqlite::Row<'_>) -> rusqlite::Result<SearchDocument> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
    ))
}
