//! A CARD'S SENTENCES AND ITS EFFECTS ARE ONE ORDER.
//!
//! Filling the X placeholders by walking the effects forward makes the yaml's
//! effect ORDER an unwritten part of the card's meaning — which a card like
//! Winds of Purity breaks. The filler asks the LINE first
//! (`effect_spoken_at`), so what this guards is the two things left: the
//! FALLBACK, still positional and what an unnamed effect gets, and a reader,
//! for whom a yaml ordered differently from the card is a puzzle.
//!
//! The check is derived: it does not know what any mod does. For each effect
//! whose KIND names something the description actually says ("status chance",
//! "fire rate", "life steal", or a syndicate's own word), it takes where that
//! phrase sits in the sentence — and those positions must climb with the
//! effects. An effect whose kind is not spoken in the description is skipped,
//! so this only ever fires on a mismatch it can prove.
use super::*;

/// Where in the description this effect is spoken about, if it is.
///
/// A kind reads `<what>_<qualifiers>`, and a card names the `<what>`:
/// `life_steal_on_own_damage` is written "Life Steal", `status_chance_bonus`
/// is written "Status Chance". So the longest form is tried first and
/// trailing words are dropped until one is found — never below two words,
/// because a lone word matches too easily to be evidence of anything.
fn spoken_at(e: &Value, hay: &str) -> Option<(usize, String)> {
    let kind = e.get("kind").and_then(Value::as_str)?;
    // A syndicate radial is named by its SYNDICATE (Purity, Truth), never
    // by its kind — "syndicate radial" appears on no card.
    if kind == "syndicate_radial" {
        let s = e.get("syndicate").and_then(Value::as_str)?.to_lowercase();
        return hay.find(&s).map(|at| (at, s));
    }
    let words: Vec<&str> = kind
        .trim_end_matches("_bonus")
        .trim_end_matches("_reduction")
        .split('_')
        .collect();
    let floor = if words.len() <= 1 { 1 } else { 2 };
    for take in (floor..=words.len()).rev() {
        let p = words[..take].join(" ");
        if let Some(at) = hay.find(&p) {
            return Some((at, p));
        }
    }
    None
}

#[test]
fn effects_are_listed_in_the_order_the_card_says_them() {
    for (path, text) in crate::data::files_under("mods/") {
        let Ok(mf) = serde_norway::from_str::<ModFile>(text) else { continue };
        let Some(desc) = mf.description.as_ref() else { continue };
        let hay = desc.to_lowercase();
        let mut seen: Vec<(usize, String, usize)> = Vec::new(); // (position, phrase, effect index)
        for (i, e) in mf.effects.iter().enumerate() {
            if let Some((at, p)) = spoken_at(e, &hay) {
                seen.push((at, p, i));
            }
        }
        for w in seen.windows(2) {
            let (a, b) = (&w[0], &w[1]);
            assert!(
                a.0 <= b.0,
                "{path}: the card says `{}` before `{}`, but the effects are listed \
                     the other way round — `desc_info` fills the X's in effect order, so \
                     the two ladders land in each other's slots",
                b.1, a.1
            );
        }
    }
}
