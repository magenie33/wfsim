// SPDX-License-Identifier: AGPL-3.0-or-later
//! THE CARD TEMPLATES: `x` placeholders in a card's text and the numbers that
//! fill them.


/// Is the char at `i` a rank-varying `X` placeholder in a description
/// template? Matches the data convention (docs: description-X): a bare `X`
/// (`+X%`, `+X Punch Through`), the multiplier form `xX`, and the UNIT forms
/// `Xm` / `Xs` / `Xx` — but never a letter inside a word.
///
/// The unit suffixes are the subtle ones. Without them "…for Xs" and "Stacks
/// up to Xx." were not placeholders at all, so no value could ever be
/// substituted and the card showed a literal X — which is exactly how
/// Galvanized Chamber came to read "Stacks up to Xx."
fn is_x_at(b: &[char], i: usize) -> bool {
    if b[i] != 'X' {
        return false;
    }
    let prev_ok = i == 0 || !b[i - 1].is_ascii_alphabetic() || b[i - 1] == 'x';
    // A unit letter counts only when the word ENDS there: "Xm"/"Xs"/"Xx" are
    // placeholders, "Xmod" or "Xstack" would be a word starting with X.
    let unit_ends = |j: usize| b.get(j + 1).is_none_or(|c| !c.is_ascii_alphabetic());
    let next_ok = match b.get(i + 1) {
        None => true,
        Some('%') => true,
        Some('m' | 's' | 'x') => unit_ends(i + 1),
        Some(c) => !c.is_ascii_alphabetic(),
    };
    prev_ok && next_ok
}

/// What a description placeholder expects, read off the character right after
/// it. The `X` in "for Xs" wants a DURATION and the one in "up to Xx" a stack
/// cap; every other `X` wants the effect's rank-varying value.
///
/// Filling by POSITION alone put Galvanized Crosshairs' 12-second duration in
/// its crit slot and printed "+1200% Critical Chance" — a description that
/// writes its duration as a literal supplies no slot for it, so the values
/// after it all shifted up one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XKind {
    /// A rank-varying stat.
    Value,
    /// Seconds — "for Xs".
    Duration,
    /// A stack cap — "up to Xx".
    Stacks,
}

/// The kind of every `X` in a template, in order.
pub fn x_kinds(template: &str) -> Vec<XKind> {
    let b: Vec<char> = template.chars().collect();
    (0..b.len())
        .filter(|&i| is_x_at(&b, i))
        .map(|i| match b.get(i + 1) {
            Some('s') => XKind::Duration,
            Some('x') => XKind::Stacks,
            _ => XKind::Value,
        })
        .collect()
}

/// The LINE each `X` sits on, in the same order as [`x_kinds`].
///
/// A card breaks its lines where DE breaks them, and a line is one sentence
/// about one effect — "+X% Life Steal" then "+X Purity". That makes the line
/// the unit that says WHICH effect a placeholder is asking about, which is the
/// only thing position cannot say.
pub fn x_lines(template: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let b: Vec<char> = template.chars().collect();
    let mut line = 0usize;
    for i in 0..b.len() {
        if b[i] == '\n' {
            line += 1;
        } else if is_x_at(&b, i) {
            out.push(line);
        }
    }
    out
}

/// Number of rank-varying `X` placeholders in a description template.
pub fn count_x(template: &str) -> usize {
    let b: Vec<char> = template.chars().collect();
    (0..b.len()).filter(|&i| is_x_at(&b, i)).count()
}

/// Fill a description template's `X` placeholders with concrete values, in
/// order. Values are stored as BONUSES (schema): before `%` they render
/// ×100; in the multiplier form `xX` they render +1 (a stored 0.3 shows as
/// `x1.3`); any other position renders the raw number. Extra `X`s beyond
/// `vals` stay as-is (the caller's honest fallback).
pub fn fill_x(template: &str, vals: &[f64]) -> String {
    let b: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut vi = 0;
    for i in 0..b.len() {
        if is_x_at(&b, i) && vi < vals.len() {
            let mut v = if b.get(i + 1) == Some(&'%') {
                vals[vi] * 100.0
            } else if i > 0 && b[i - 1] == 'x' {
                vals[vi] + 1.0
            } else {
                vals[vi]
            };
            // The template carries the sign ("+X%" / "-X%"); the stored
            // value may carry it too (corrupted downsides are negative
            // bonuses) — render the magnitude to avoid "--15%".
            if i > 0 && matches!(b[i - 1], '+' | '-' | '−') {
                v = v.abs();
            }
            // THREE DECIMALS, trailing zeros trimmed. Two was enough until a
            // card printed a THIRD: Split Flights opens at 0.333 s, which
            // rendered as "0.33s" against DE's own "0.333" and failed
            // `localized_card_numbers_are_numbers_we_also_state` — a real
            // disagreement, since the check's whole job is that a number we
            // state is a number DE states. Nothing else moves: a value whose
            // third decimal is zero trims back to exactly what it printed
            // before.
            let s = format!("{v:.3}");
            out.push_str(s.trim_end_matches('0').trim_end_matches('.'));
            vi += 1;
        } else {
            out.push(b[i]);
        }
    }
    out
}

/// "+60%" / "−15%" / "+109.5%" from a fraction (true minus sign).
pub fn pct(x: f64) -> String {
    let p = x.abs() * 100.0;
    let s = if (p - p.round()).abs() < 1e-6 {
        format!("{}", p.round() as i64)
    } else {
        format!("{p:.2}").trim_end_matches('0').trim_end_matches('.').to_string()
    };
    if x >= 0.0 { format!("+{s}%") } else { format!("−{s}%") }
}
