use std::sync::OnceLock;

use serde::Deserialize;

use crate::types::Label;

#[derive(Deserialize)]
struct Vocabulary {
    version: u32,
    labels: Vec<VocabularyEntry>,
}

#[derive(Deserialize)]
struct VocabularyEntry {
    category: String,
    value: String,
    aliases: Vec<Vec<String>>,
}

fn vocabulary() -> &'static Vocabulary {
    static VOCABULARY: OnceLock<Vocabulary> = OnceLock::new();
    VOCABULARY.get_or_init(|| {
        serde_json::from_str(include_str!("../../resources/labels.json"))
            .expect("built-in label vocabulary must be valid JSON")
    })
}

pub fn infer_labels(path: &str) -> Vec<Label> {
    let tokens = path
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(str::to_lowercase)
        .collect::<Vec<_>>();
    let vocabulary = vocabulary();
    debug_assert_eq!(vocabulary.version, 1);

    let mut matches = vocabulary
        .labels
        .iter()
        .enumerate()
        .filter_map(|(entry_index, entry)| {
            entry
                .aliases
                .iter()
                .filter_map(|alias| {
                    tokens.windows(alias.len()).position(|window| {
                        window
                            .iter()
                            .map(String::as_str)
                            .eq(alias.iter().map(String::as_str))
                    })
                })
                .min()
                .map(|position| (position, entry_index, entry))
        })
        .collect::<Vec<_>>();
    matches.sort_by_key(|(position, entry_index, _)| (*position, *entry_index));
    matches
        .into_iter()
        .map(|(_, _, entry)| Label {
            category: entry.category.clone(),
            value: entry.value.clone(),
        })
        .collect()
}
