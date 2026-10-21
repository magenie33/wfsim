// SPDX-License-Identifier: AGPL-3.0-or-later
//! TEXT A READER IS SHOWN, AND NO NUMBER IS COMPUTED FROM.
//!
//! A data key named in `data/prose.yaml` is read only as a [`Prose`], and the
//! engine id leaves it out (`scripts/build_site_app.py` `engine_files`), so
//! rewording a card moves no engine. What keeps that true is that the text is
//! reachable only through [`Prose::shown`], and `check_prose` allows a call to
//! it only in a `describe` module or the web api — what a reader is shown.

use serde::{Deserialize, Serialize};
use serde_norway::Value;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Prose(String);

impl Prose {
    /// The text, for a page, a card or a message.
    pub fn shown(&self) -> &str {
        &self.0
    }

    /// New prose made from this one — a template filled, a sentence framed.
    /// What comes out is still prose, so it may be made anywhere.
    pub fn map(&self, f: impl FnOnce(&str) -> String) -> Prose {
        Prose(f(&self.0))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// A prose key read off a raw map — the one way a loader that walks
    /// `Value`s names one.
    pub fn read(v: &Value, key: &str) -> Option<Prose> {
        v.get(key).and_then(Value::as_str).map(Prose::from)
    }
}

impl From<String> for Prose {
    fn from(s: String) -> Self {
        Prose(s)
    }
}

impl From<&str> for Prose {
    fn from(s: &str) -> Self {
        Prose(s.to_string())
    }
}
