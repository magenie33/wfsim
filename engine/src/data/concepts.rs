// SPDX-License-Identifier: AGPL-3.0-or-later
//! HOW WFSIM JUDGES A BUILD — `data/concepts.yaml`, docs/AGENT.md §"The method".
//!
//! Text for a reader and nothing else: no number is computed from it, so it
//! rides in the `text` pack and `engine_id` leaves both this file and the data
//! out. `/api/meta` carries it to the page and the headless query; a locale's
//! words come through the `ui` overlay, keyed by the English.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Method {
    pub title: String,
    pub about: String,
    pub labels: Labels,
    pub concepts: Vec<Concept>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Labels {
    pub why: String,
    pub apply: String,
    pub evidence: String,
    pub game: String,
    pub engine: String,
    pub see: String,
    pub rulers: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Concept {
    pub id: String,
    pub title: String,
    pub rule: String,
    pub why: Vec<String>,
    pub apply: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    #[serde(default)]
    pub see: Vec<See>,
    /// The page lists every ruler's own `standard` beneath this concept.
    #[serde(default)]
    pub rulers: bool,
}

/// A READING, by the file in this repository that records it.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub source: String,
    pub kind: Kind,
    pub shows: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// Taken in the game.
    Game,
    /// A reading of the engine itself.
    Engine,
}

/// A DOCUMENT in this repository, and what it holds.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct See {
    pub source: String,
    pub shows: String,
}

pub fn method() -> &'static Method {
    static M: OnceLock<Method> = OnceLock::new();
    M.get_or_init(|| {
        let text = crate::data::file("concepts.yaml").expect("data/concepts.yaml");
        serde_norway::from_str(text).unwrap_or_else(|e| panic!("parse concepts.yaml: {e}"))
    })
}

impl Method {
    /// Every string a reader is shown, in reading order — what a locale has to
    /// translate whole.
    pub fn strings(&self) -> Vec<&str> {
        let l = &self.labels;
        let mut out = vec![self.title.as_str(), &self.about, &l.why, &l.apply, &l.evidence, &l.game, &l.engine, &l.see, &l.rulers];
        for c in &self.concepts {
            out.push(&c.title);
            out.push(&c.rule);
            out.extend(c.why.iter().chain(&c.apply).map(String::as_str));
            out.extend(c.evidence.iter().map(|e| e.shows.as_str()));
            out.extend(c.see.iter().map(|s| s.shows.as_str()));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A CONCEPT IS ADDRESSED BY ITS ID — the page's anchor and the query's
    /// argument — so one is unique, a lowercase word, and says something.
    #[test]
    fn every_concept_has_an_id_of_its_own_and_its_parts() {
        let m = method();
        let mut seen = std::collections::BTreeSet::new();
        for c in &m.concepts {
            assert!(c.id.chars().all(|ch| ch.is_ascii_lowercase() || ch == '_'), "{}: not a lowercase id", c.id);
            assert!(seen.insert(&c.id), "{}: two concepts", c.id);
            assert!(!c.why.is_empty() && !c.apply.is_empty(), "{}: a rule with no why or no way to apply it", c.id);
        }
        assert!(m.strings().iter().all(|s| !s.trim().is_empty()), "an empty string in concepts.yaml");
    }

    /// A SOURCE IS A FILE THAT EXISTS: the page links each one, and a renamed
    /// measurement would otherwise leave a link to nothing.
    #[test]
    fn every_source_is_a_file_in_the_repository() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        for c in &method().concepts {
            for src in c.evidence.iter().map(|e| &e.source).chain(c.see.iter().map(|s| &s.source)) {
                assert!(root.join(src).is_file(), "{}: {src} is not a file", c.id);
            }
        }
    }

    /// A LOCALE TRANSLATES THE METHOD WHOLE OR NOT AT ALL: one that holds any
    /// of it holds every string, or a page would read half in English.
    #[test]
    fn a_locale_that_translates_the_method_translates_all_of_it() {
        let m = method();
        for (code, l) in crate::data::i18n::locales() {
            if !l.ui.contains_key(&m.about) {
                continue;
            }
            let missing: Vec<_> = m.strings().into_iter().filter(|s| !l.ui.contains_key(*s)).collect();
            assert!(missing.is_empty(), "{code}: concepts.yaml strings with no translation: {missing:?}");
        }
    }
}
