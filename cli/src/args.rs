// SPDX-License-Identifier: AGPL-3.0-or-later
//! Command-line flags shared by the pipeline binaries.

/// A flag's value, `--name value` anywhere after the positionals.
pub fn flag(name: &str) -> Option<String> {
    let a: Vec<String> = std::env::args().collect();
    a.iter()
        .position(|x| x == name)
        .and_then(|i| a.get(i + 1).cloned())
}

pub fn has_flag(name: &str) -> bool {
    std::env::args().any(|x| x == name)
}
