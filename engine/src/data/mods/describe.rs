use super::*;

/// Display info for a mod's DESCRIPTION at any rank: the X-templated game
/// text plus the (rank0, rankMax) pair of every rank-VARYING effect, in
/// yaml order. The description's `X`s map to these in order (extra varying
/// effects beyond the X count are hidden stats — Amalgam Barrel Diffusion's
/// acrobatic speed — and are correctly left unconsumed at the tail).
#[derive(Debug, Clone)]
pub struct ModDescInfo {
    pub description: String,
    pub xvals: Vec<(f64, f64)>,
    pub max_rank: u32,
}

impl ModDescInfo {
    /// The description with each `X` filled at `rank` (linear rank0→rankMax
    /// — the schema stores real endpoints; regular mods scale linearly).
    pub fn at(&self, rank: u32) -> String {
        let r = rank.min(self.max_rank) as f64;
        let m = self.max_rank.max(1) as f64;
        let vals: Vec<f64> = self.xvals.iter().map(|(a, b)| a + (b - a) * r / m).collect();
        crate::model::fill_x(&self.description, &vals)
    }
}

/// Description info by mod id — the VERBATIM in-game text with each `X`
/// filled, which is what the picker and a configured slot display.
///
/// Covers EVERY class. Scanning one directory silently falls every other pool
/// back to the engine's modeled effect lines, which state only what the ENGINE
/// models — so anything unmodeled on a mod vanishes from the UI and the card
/// reads as doing less than it does. None means the file genuinely has no
/// `description`, and the caller falls back to the effect lines.
/// Where in `hay` this effect is SPOKEN ABOUT, if it is at all.
///
/// A kind reads `<what>_<qualifiers>` and a card names the `<what>`:
/// `life_steal_on_own_damage` is written "Life Steal", `status_chance_bonus` is
/// written "Status Chance". So the longest form is tried first and trailing
/// words are dropped until one is found — never below two words, because a lone
/// word matches too easily to be evidence of anything.
///
/// A syndicate radial is named by its SYNDICATE (Purity, Truth); "syndicate
/// radial" appears on no card.
pub(crate) fn effect_spoken_at(e: &Value, hay: &str) -> Option<usize> {
    let kind = e.get("kind").and_then(Value::as_str)?;
    if kind == "syndicate_radial" {
        let sy = e.get("syndicate").and_then(Value::as_str)?.to_lowercase();
        return hay.find(&sy);
    }
    let words: Vec<&str> = kind
        .trim_end_matches("_bonus")
        .trim_end_matches("_reduction")
        .split('_')
        .collect();
    let floor = if words.len() <= 1 { 1 } else { 2 };
    (floor..=words.len())
        .rev()
        .find_map(|take| hay.find(&words[..take].join(" ")))
}

/// The effect kinds on this mod that the loader DROPPED — what the card must
/// admit it does not do.
///
/// `effect()` is a `filter_map`, so an effect it cannot build simply vanishes
/// and the mod loads as one that silently does less than its card says. Two
/// kinds say so on purpose (`unmodeled`, `out_of_scope`) and the ModDef carries
/// a flag for each; this covers the third case, a mod that is PARTLY modelled.
///
/// Winds of Purity is the one today: its Purity radial lands 1,000 damage a
/// blast and its life steal heals a Tenno this arena does not have. Flagging
/// the whole mod `unmodeled` would say the card does nothing, which is worse
/// than saying nothing — so the disclosure has to be per effect.
///
/// DERIVED, never listed: it re-asks `effect()` the same question the loader
/// asked, so a mod that starts dropping an effect discloses it without anyone
/// noticing they should come back here (memory: derive triggers, don't list
/// them).
pub fn unmodeled_effects(id: &str) -> &'static [String] {
    static MAP: OnceLock<std::collections::HashMap<String, Vec<String>>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = std::collections::HashMap::new();
        for (_, text) in crate::data::files_under("mods/") {
            let Ok(mf) = serde_norway::from_str::<ModFile>(text) else { continue };
            let dropped: Vec<String> = mf
                .effects
                .iter()
                .filter(|e| effect(&mf.id, e).is_none())
                .filter_map(|e| e.get("kind").and_then(Value::as_str))
                // The two that already have their own flag and their own line
                // on the card.
                .filter(|k| *k != "unmodelled" && *k != "out_of_scope")
                // `life_steal_on_own_damage` -> "life steal on own damage": the
                // kind IS the description, in the vocabulary the yaml chose.
                .map(|k| k.replace('_', " "))
                .collect();
            if !dropped.is_empty() {
                map.insert(mf.id.clone(), dropped);
            }
        }
        map
    })
    .get(id)
    .map_or(&[], |v| v.as_slice())
}

pub fn desc_info(id: &str) -> Option<&'static ModDescInfo> {
    static INFO: OnceLock<std::collections::HashMap<String, ModDescInfo>> = OnceLock::new();
    INFO.get_or_init(|| {
        let mut map = std::collections::HashMap::new();
        for (_, text) in crate::data::files_under("mods/") {
            let Ok(mf) = serde_norway::from_str::<ModFile>(text) else { continue };
            let Some(desc) = mf.description else { continue };
            // Values are matched to placeholders by KIND and by SENTENCE,
            // never by position in a flat queue.
            //
            // A `X%`-style placeholder opens the next effect that has a
            // rank-varying value; "for Xs" and "up to Xx" then describe THAT
            // effect. Position alone put Galvanized Crosshairs' 12-second
            // duration into its crit slot — "+1200% Critical Chance" — because
            // that description spells its duration out and offers no slot for
            // it, so everything after shifted up one. A flat per-kind queue
            // gets Galvanized Scope wrong the same way: its first buff carries
            // `max_stacks: 1` that the text never mentions, and the one "Xx"
            // in the sentence belongs to the second buff.
            //
            // Constants ride as (v, v) so `at(rank)` interpolates them to
            // themselves. A placeholder with nothing to fill it STOPS the fill,
            // so it stays visible and
            // `desc_info_fills_every_x_across_the_pool` fails, rather than a
            // wrong-kind value quietly taking the slot.
            let varying = |e: &Value| match (f(e, "rank0"), f(e, "rankMax")) {
                (Some(a), Some(b)) if (a - b).abs() > 1e-12 => Some((a, b)),
                _ => None,
            };
            // `duration` (buff) and `duration_seconds` (on_equip_buff) are the
            // same slot in the sentence; a mod carries one or neither. A
            // duration that RAMPS with rank (Argon Scope: 2s -> 9s) also states
            // `duration_rank0` — without it the card read "for 9s" at every
            // rank, a rank-varying value shown as a constant.
            let dur = |e: &Value| {
                let d = n(e, "duration").or_else(|| n(e, "duration_seconds"))?;
                Some((n(e, "duration_rank0").unwrap_or(d), d))
            };
            // The card's own lines, lowercased once: a `Value` placeholder asks
            // about the effect its LINE names, and only falls back to position
            // when the line names nothing.
            let lines: Vec<String> = desc.lines().map(str::to_lowercase).collect();
            let x_line = crate::model::x_lines(&desc);
            let mut xvals: Vec<(f64, f64)> = Vec::new();
            let mut ei: Option<usize> = None; // the effect the sentence is on
            let mut used: Vec<usize> = Vec::new();
            for (xi, kind) in crate::model::x_kinds(&desc).into_iter().enumerate() {
                use crate::model::XKind;
                // Seek forward to an effect that can answer this placeholder;
                // a `Value` always moves on, the others stay put once the
                // sentence has an effect to describe.
                let seek = |from: usize, pick: &dyn Fn(&Value) -> bool| {
                    (from..mf.effects.len()).find(|&i| pick(&mf.effects[i]))
                };
                let next = match kind {
                    XKind::Value => {
                        // BY NAME FIRST. Position alone made the yaml's effect
                        // ORDER an unwritten part of the card's meaning, and
                        // Winds of Purity broke it the day it was written: its
                        // radial was listed first while the card says "+X% Life
                        // Steal" first, so the two ladders landed in each
                        // other's slots and it printed "+100% Life Steal /
                        // +0.2 Purity" for the wiki's "+20% / +1". Both wrong,
                        // both the kind of number a mod could have.
                        let named = x_line.get(xi).and_then(|&l| lines.get(l)).and_then(|line| {
                            (0..mf.effects.len()).find(|i| {
                                !used.contains(i)
                                    && varying(&mf.effects[*i]).is_some()
                                    && effect_spoken_at(&mf.effects[*i], line).is_some()
                            })
                        });
                        ei = named.or_else(|| {
                            seek(ei.map_or(0, |i| i + 1), &|e| varying(e).is_some())
                        });
                        if let Some(i) = ei {
                            used.push(i);
                        }
                        ei.and_then(|i| varying(&mf.effects[i]))
                    }
                    XKind::Duration => {
                        if ei.is_none() {
                            ei = seek(0, &|e| dur(e).is_some());
                        }
                        ei.and_then(|i| dur(&mf.effects[i]))
                    }
                    XKind::Stacks => {
                        if ei.is_none() {
                            ei = seek(0, &|e| n(e, "max_stacks").is_some());
                        }
                        // A stack CAP that scales with rank (Aerial Ace's
                        // 1x -> 6x) is a rank-varying value, not a constant.
                        ei.and_then(|i| n(&mf.effects[i], "max_stacks"))
                            .map(|s| (s, s))
                            .or_else(|| {
                                ei = seek(ei.map_or(0, |i| i + 1), &|e| varying(e).is_some());
                                ei.and_then(|i| varying(&mf.effects[i]))
                            })
                    }
                };
                match next {
                    Some(v) => xvals.push(v),
                    None => break,
                }
            }
            map.insert(
                mf.id,
                ModDescInfo { description: desc, xvals, max_rank: mf.max_rank },
            );
        }
        map
    })
    .get(id)
}
