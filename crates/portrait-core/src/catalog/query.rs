use std::collections::BTreeMap;

use rusqlite::params_from_iter;
use rusqlite::types::{Type, Value};
use uuid::Uuid;

use super::compile_fts;
use crate::types::{
    CatalogFacets, CatalogPage, Label, LabelFacet, Page, Portrait, Query, SourceFacet,
};
use crate::{CoreError, Library, Result};

#[derive(Debug, Clone)]
pub struct MatchingIds {
    pub sql: String,
    pub parameters: Vec<Value>,
    pub ranked: bool,
}

#[must_use]
pub fn matching_ids(query: &Query) -> MatchingIds {
    let fts = compile_fts(&query.text);
    let mut sql = String::from("SELECT p.id");
    if fts.is_some() {
        sql.push_str(
            ", bm25(search_index) AS rank FROM portraits p \
                      JOIN search_documents sd ON sd.portrait_id = p.id \
                      JOIN search_index ON search_index.rowid = sd.id",
        );
    } else {
        sql.push_str(", 0.0 AS rank FROM portraits p");
    }
    let mut clauses = vec![if query.trash {
        "p.trashed_at IS NOT NULL".to_owned()
    } else {
        "p.trashed_at IS NULL".to_owned()
    }];
    let mut parameters = Vec::new();
    if let Some(expression) = fts {
        clauses.push("search_index MATCH ?".into());
        parameters.push(Value::Text(expression));
    }
    if !query.source_ids.is_empty() {
        clauses.push(format!(
            "(p.source_id IN ({0}) OR EXISTS (SELECT 1 FROM portrait_sources source_match WHERE source_match.portrait_id = p.id AND source_match.source_id IN ({0})))",
            placeholders(query.source_ids.len())
        ));
        parameters.extend(
            query
                .source_ids
                .iter()
                .map(|id| Value::Text(id.to_string())),
        );
        parameters.extend(
            query
                .source_ids
                .iter()
                .map(|id| Value::Text(id.to_string())),
        );
    }
    if query.selected_only {
        clauses
            .push("EXISTS (SELECT 1 FROM selection chosen WHERE chosen.portrait_id = p.id)".into());
    }
    let mut categories: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for label in &query.labels {
        categories
            .entry(label.category.as_str())
            .or_default()
            .push(label.value.as_str());
    }
    for (category, values) in categories {
        clauses.push(format!(
            "EXISTS (SELECT 1 FROM portrait_labels filtered_pl \
             JOIN labels filtered_l ON filtered_l.id = filtered_pl.label_id \
             WHERE filtered_pl.portrait_id = p.id \
               AND filtered_l.category = ? \
               AND filtered_l.normalized_value IN ({}))",
            placeholders(values.len())
        ));
        parameters.push(Value::Text(category.to_owned()));
        parameters.extend(
            values
                .into_iter()
                .map(|value| Value::Text(value.to_owned())),
        );
    }
    sql.push_str(" WHERE ");
    sql.push_str(&clauses.join(" AND "));
    MatchingIds {
        sql,
        parameters,
        ranked: compile_fts(&query.text).is_some(),
    }
}

pub fn query_catalog(library: &Library, query: &Query, page: Page) -> Result<CatalogPage> {
    if !(1..=200).contains(&page.limit) {
        return Err(CoreError::InvalidPagination);
    }
    let matching = matching_ids(query);
    let transaction = library.connection().unchecked_transaction()?;
    let total = transaction.query_row(
        &format!(
            "WITH matching_ids AS ({}) SELECT count(*) FROM matching_ids",
            matching.sql
        ),
        params_from_iter(matching.parameters.iter()),
        |row| row.get::<_, i64>(0),
    )?;
    let total = u64::try_from(total)
        .map_err(|error| CoreError::Migration(format!("invalid catalog count: {error}")))?;
    let revision = catalog_revision(&transaction)?;
    let order = if matching.ranked {
        "m.rank ASC, p.name ASC, p.id ASC"
    } else {
        "p.name ASC, p.id ASC"
    };
    let mut page_parameters = matching.parameters.clone();
    page_parameters.push(Value::Integer(i64::from(page.limit)));
    page_parameters.push(Value::Integer(i64::from(page.offset)));
    let sql = format!(
        "WITH matching_ids AS ({}) \
         SELECT p.id, p.source_id, p.name, s.name, p.original_folder, p.description, \
                EXISTS(SELECT 1 FROM selection chosen WHERE chosen.portrait_id = p.id), \
                p.trashed_at \
         FROM matching_ids m \
         JOIN portraits p ON p.id = m.id \
         JOIN sources s ON s.id = p.source_id \
         ORDER BY {order} LIMIT ? OFFSET ?",
        matching.sql
    );
    let mut statement = transaction.prepare(&sql)?;
    let rows = statement.query_map(params_from_iter(page_parameters.iter()), |row| {
        Ok(Portrait {
            id: parse_uuid(row.get::<_, String>(0)?, 0)?,
            source_id: parse_uuid(row.get::<_, String>(1)?, 1)?,
            name: row.get(2)?,
            source_name: row.get(3)?,
            original_folder: row.get(4)?,
            description: row.get(5)?,
            labels: Vec::new(),
            selected: row.get(6)?,
            trashed_at: row.get(7)?,
        })
    })?;
    let mut items = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    drop(statement);
    let mut label_statement = transaction.prepare(
        "SELECT l.category, l.normalized_value FROM portrait_labels pl \
         JOIN labels l ON l.id = pl.label_id WHERE pl.portrait_id = ?1 \
         ORDER BY l.category, l.normalized_value",
    )?;
    for portrait in &mut items {
        portrait.labels = label_statement
            .query_map([portrait.id.to_string()], |row| {
                Ok(Label {
                    category: row.get(0)?,
                    value: row.get(1)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
    }
    drop(label_statement);
    transaction.commit()?;
    Ok(CatalogPage {
        items,
        total,
        revision,
    })
}

pub fn catalog_facets(library: &Library) -> Result<CatalogFacets> {
    let mut sources = library.connection().prepare("SELECT s.id, s.name, count(*) FROM sources s JOIN (SELECT id AS portrait_id, source_id FROM portraits UNION SELECT portrait_id, source_id FROM portrait_sources) refs ON refs.source_id = s.id JOIN portraits p ON p.id = refs.portrait_id WHERE p.trashed_at IS NULL GROUP BY s.id, s.name ORDER BY s.name COLLATE NOCASE, s.id")?;
    let sources = sources
        .query_map([], |row| {
            Ok(SourceFacet {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(error))
                })?,
                name: row.get(1)?,
                count: u64::try_from(row.get::<_, i64>(2)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(2, Type::Integer, Box::new(error))
                })?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut labels = library.connection().prepare("SELECT l.category, l.normalized_value, l.display_value, count(*) FROM labels l JOIN portrait_labels pl ON pl.label_id = l.id JOIN portraits p ON p.id = pl.portrait_id WHERE p.trashed_at IS NULL GROUP BY l.id ORDER BY l.category COLLATE NOCASE, l.display_value COLLATE NOCASE")?;
    let labels = labels
        .query_map([], |row| {
            Ok(LabelFacet {
                category: row.get(0)?,
                value: row.get(1)?,
                display_value: row.get(2)?,
                count: u64::try_from(row.get::<_, i64>(3)?).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(3, Type::Integer, Box::new(error))
                })?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(CatalogFacets { sources, labels })
}

fn catalog_revision(transaction: &rusqlite::Transaction<'_>) -> Result<u64> {
    let json: String = transaction.query_row(
        "SELECT state_json FROM operation_state WHERE operation_id = 'catalog_revision'",
        [],
        |row| row.get(0),
    )?;
    Ok(serde_json::from_str::<serde_json::Value>(&json)?
        .get("revision")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0))
}

fn placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}

fn parse_uuid(value: String, column: usize) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(&value).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(column, Type::Text, Box::new(error))
    })
}
