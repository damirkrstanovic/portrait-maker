use rusqlite::params_from_iter;
use rusqlite::types::Value;

use crate::catalog::{bump_catalog_revision, matching_ids};
use crate::{Library, Result};

pub use crate::types::{SelectionAction, SelectionTarget};

/// Mutate persisted selection and return the total number of selected portraits.
pub fn change_selection(
    library: &mut Library,
    target: SelectionTarget,
    action: SelectionAction,
) -> Result<u64> {
    let transaction = library.connection().unchecked_transaction()?;
    match action {
        SelectionAction::Clear => {
            transaction.execute("DELETE FROM selection", [])?;
        }
        SelectionAction::Add | SelectionAction::Remove => {
            let (sql, parameters) = target_sql(target);
            let operation = match action {
                SelectionAction::Add => {
                    format!("INSERT OR IGNORE INTO selection (portrait_id) SELECT id FROM ({sql})")
                }
                SelectionAction::Remove => {
                    format!("DELETE FROM selection WHERE portrait_id IN (SELECT id FROM ({sql}))")
                }
                SelectionAction::Clear => unreachable!(),
            };
            transaction.execute(&operation, params_from_iter(parameters.iter()))?;
        }
    }
    let count: i64 =
        transaction.query_row("SELECT count(*) FROM selection", [], |row| row.get(0))?;
    bump_catalog_revision(&transaction)?;
    transaction.commit()?;
    Ok(u64::try_from(count).unwrap_or(0))
}

fn target_sql(target: SelectionTarget) -> (String, Vec<Value>) {
    match target {
        SelectionTarget::Ids(ids) => {
            if ids.is_empty() {
                ("SELECT id FROM portraits WHERE 0".into(), Vec::new())
            } else {
                let placeholders = std::iter::repeat_n("?", ids.len())
                    .collect::<Vec<_>>()
                    .join(", ");
                (
                    format!("SELECT id FROM portraits WHERE id IN ({placeholders})"),
                    ids.into_iter()
                        .map(|id| Value::Text(id.to_string()))
                        .collect(),
                )
            }
        }
        SelectionTarget::Matching(query) => {
            let matching = matching_ids(&query);
            (matching.sql, matching.parameters)
        }
    }
}
