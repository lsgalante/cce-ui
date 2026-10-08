//! The toolkit's own words, translatable (`docs/rfc-accessibility-locale.md`, phase 5).
//!
//! Every string cce-ui shows of its own — a menu row, a placeholder, a dock's action — is
//! a message in `locale/en-US/cce-ui.ftl`, looked up by id through [`tr`] / [`tr_args`] in
//! the user's language (`cce_core::l10n`: a translation is `<tag>/cce-ui.ftl` in a
//! translation directory). An app's own words are its own domain: a
//! `static CATALOG: Catalog = Catalog::new("<app>", include_str!(…))` of its own.
//! `every_message_the_toolkit_names_is_in_its_english` holds the ids to the file.

pub use cce_core::l10n::*;

static CATALOG: Catalog = Catalog::new("cce-ui", include_str!("../locale/en-US/cce-ui.ftl"));

/// The toolkit's catalogue (to add a translation to, as a page or a test does).
pub fn catalog() -> &'static Catalog {
    &CATALOG
}

/// The toolkit's message `id` in the user's language.
pub fn tr(id: &str) -> String {
    CATALOG.get(id)
}

/// The toolkit's message `id`, its `{ $name }` placeables filled from `args`.
pub fn tr_args(id: &str, args: &[(&str, &str)]) -> String {
    CATALOG.format(id, args)
}

#[cfg(test)]
mod tests {
    /// Every id the toolkit's source looks up is in its English, and every message there
    /// is looked up somewhere: a missing one shows as its id, a stray one is a string no
    /// translator should be asked for.
    #[test]
    fn every_message_the_toolkit_names_is_in_its_english() {
        let english = include_str!("../locale/en-US/cce-ui.ftl");
        let defined: std::collections::BTreeSet<&str> = english
            .lines()
            .filter(|l| !l.starts_with('#') && !l.starts_with(' ') && l.contains(" = "))
            .map(|l| l.split(" = ").next().unwrap().trim())
            .collect();
        let mut used = std::collections::BTreeSet::new();
        let src = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
        let mut stack = vec![std::path::PathBuf::from(src)];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") && !path.ends_with("l10n.rs") {
                    let text = std::fs::read_to_string(&path).unwrap();
                    for call in ["tr(\"", "tr_args(\""] {
                        for (i, _) in text.match_indices(call) {
                            // A call of ours, not the end of another name (`attr("`, `str("`).
                            if text[..i].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                                continue;
                            }
                            let rest = &text[i + call.len()..];
                            if let Some(end) = rest.find('"') {
                                used.insert(rest[..end].to_string());
                            }
                        }
                    }
                }
            }
        }
        let missing: Vec<&String> = used.iter().filter(|u| !defined.contains(u.as_str())).collect();
        assert!(missing.is_empty(), "looked up but not in locale/en-US/cce-ui.ftl: {missing:?}");
        let unused: Vec<&&str> = defined.iter().filter(|d| !used.contains(**d)).collect();
        assert!(unused.is_empty(), "in locale/en-US/cce-ui.ftl but looked up nowhere: {unused:?}");
    }

    /// A translation changes what a message says and falls back to English for what it
    /// lacks.
    #[test]
    fn a_translation_is_used_where_it_has_the_message() {
        let c = super::Catalog::new("cce-ui", include_str!("../locale/en-US/cce-ui.ftl"));
        c.add_translation("de", "menu-copy = Kopieren\nmenu-config-key = Schlüssel: { $key }\n");
        assert_eq!(c.get("menu-copy"), "Kopieren");
        assert_eq!(c.format("menu-config-key", &[("key", "x.y")]), "Schlüssel: x.y");
        assert_eq!(c.get("menu-paste"), "Paste");
    }
}
