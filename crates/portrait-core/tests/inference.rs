use portrait_core::metadata::infer_labels;
use portrait_core::types::Label;

fn labels(path: &str) -> Vec<Label> {
    infer_labels(path)
}

#[test]
fn inference_uses_delimited_tokens_and_known_aliases() {
    assert_eq!(
        labels("packs/Female Elf Arcane Archer"),
        vec![
            Label {
                category: "gender".into(),
                value: "woman".into()
            },
            Label {
                category: "race".into(),
                value: "elf".into()
            },
            Label {
                category: "class".into(),
                value: "mage".into()
            },
            Label {
                category: "class".into(),
                value: "archer".into()
            },
        ]
    );
}

#[test]
fn inference_does_not_treat_male_as_a_match_inside_female_or_words_as_substrings() {
    assert_eq!(
        labels("female_elf_malediction"),
        vec![
            Label {
                category: "gender".into(),
                value: "woman".into()
            },
            Label {
                category: "race".into(),
                value: "elf".into()
            },
        ]
    );
}

#[test]
fn inference_keeps_conflicting_tokens_visible() {
    assert_eq!(
        labels("man woman dwarf fighter"),
        vec![
            Label {
                category: "gender".into(),
                value: "man".into()
            },
            Label {
                category: "gender".into(),
                value: "woman".into()
            },
            Label {
                category: "race".into(),
                value: "dwarf".into()
            },
            Label {
                category: "class".into(),
                value: "martial".into()
            },
        ]
    );
}
