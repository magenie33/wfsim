use super::*;

/// WHICH PERKS LAND AFTER THE MODS, pinned — because the wording that
/// distinguishes them is one word long and it has been lost twice.
///
/// A card that reads "Increase BASE Critical Chance by +10%" is a base-stat
/// bonus and the crit mods multiply it. A card that reads "+10% Critical
/// Chance", with "Bonuses are added after mods as a flat value" under it, is
/// not — it lands on the final number and is worth several times less on a
/// modded build. Same stat, same size, same perk NAME sometimes, different
/// bracket.
///
/// It has been got wrong in both directions of authorship: the bulk intake
/// normalised the Felarx's two tier-4 cards into the "Increase Base…"
/// phrasing that DE does not use for them, and a HAND-WRITTEN file argued
/// the Phenmor's Survivor's Edge onto the base layer because the Boar's
/// perk of the same name lives there. So this is a list, and a new member
/// has to be argued for.
#[test]
fn the_after_mods_perks_are_the_ones_we_meant() {
    let mut found: Vec<String> = Vec::new();
    for def in pool() {
        for e in &def.effects {
            let what = match e {
                EvoEffect::PostModCritChance(v) => format!("crit {v:+}"),
                EvoEffect::PostModStatusChance(v) => format!("status {v:+}"),
                _ => continue,
            };
            found.push(format!("{} :: {what}", def.id));
        }
    }
    found.sort();
    let expected = [
        // Its status half is DIVIDED by the base multishot of 4 — verbatim
        // on the page, and right for a per-pellet model: 10% / 4.
        "felarx_brutal_edge :: crit +0.1",
        "felarx_brutal_edge :: status +0.025",
        "felarx_racking_wrath :: crit -0.1",
        "felarx_racking_wrath :: status +0.05",
        // The Laetum's, already on the right layer when it was written by
        // hand — the perk that gave the engine these two kinds.
        "laetum_elemental_excess :: crit -0.1",
        "laetum_elemental_excess :: status +0.2",
        // One projectile, so no division: the listed number is per pellet.
        "onos_elemental_excess :: crit -0.1",
        "onos_elemental_excess :: status +0.2",
        "phenmor_elemental_excess :: crit -0.1",
        "phenmor_elemental_excess :: status +0.2",
        "phenmor_survivors_edge :: crit +0.1",
        "phenmor_survivors_edge :: status +0.1",
    ];
    assert_eq!(found, expected, "the after-mods list moved");
}

/// A POST-MOD BONUS IS WORTH LESS THAN THE SAME NUMBER OF BASE POINTS, and
/// the gap is the whole reason the layer matters.
#[test]
fn the_two_layers_are_not_worth_the_same() {
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::StackPolicy;
    let after = ["felarx_brutal_edge".to_string()];
    let a: Vec<&str> = after.iter().map(|s| s.as_str()).collect();
    // A crit mod, so the base layer has something to be multiplied by.
    let pool = crate::data::mods::class_pool("shotgun");
    let crit = pool.iter().find(|m| m.id == "blunderbuss").or_else(
        || pool.iter().find(|m| m.id == "primed_ravage")).expect("a crit mod");
    let plain = resolve(&WeaponBase::from_data("felarx", true, &[]), &[crit],
                        StackPolicy::AssumedMax);
    let post = resolve(&WeaponBase::from_data("felarx", true, &a), &[crit],
                       StackPolicy::AssumedMax);
    // +10 points, flat, whatever the mod did.
    assert!(
        ((post.crit_chance - plain.crit_chance) - 0.10).abs() < 1e-9,
        "flat after mods: {} -> {}", plain.crit_chance, post.crit_chance
    );
}
/// A LIVE BASE-CRIT-DAMAGE BUFF REACHES THE DIRECT HIT AND NOTHING ELSE.
///
/// `cd_total` is the direct hit's crit multiplier; a radial explosion and a
/// lingering field each compute their OWN from their own base stats, and
/// neither reads this grant. That is not a decision — nobody has measured
/// whether Mauler's Magazine reaches an explosion, and no weapon carrying
/// the grant has one, so there is nothing to be right or wrong about yet.
///
/// This is the tripwire for the day that changes. A silently-absent factor
/// on an AoE part is worth a large fraction of the weapon's damage and would
/// read as "this build is weaker than it should be" rather than as a bug.
#[test]
fn no_weapon_with_a_base_crit_damage_buff_has_an_aoe_part() {
    let mut offenders: Vec<String> = Vec::new();
    for e in pool() {
        let grants_cd = e.effects.iter().any(|f| {
            matches!(f, EvoEffect::StackingGrant { grant, .. }
                if *grant == crate::model::BuffGrant::BaseCritDamage)
        });
        if !grants_cd {
            continue;
        }
        let base = crate::model::WeaponBase::from_data(&e.weapon, true, &[e.id.as_str()]);
        if base.radial.is_some() {
            offenders.push(format!("{} ({}): radial", e.id, e.weapon));
        }
    }
    assert!(
        offenders.is_empty(),
        "a base-crit-damage buff now rides a weapon with an AoE part, whose crit \
             multiplier is computed separately and does not read it — decide and \
             measure before shipping it:\n  {}",
        offenders.join("\n  ")
    );
}

/// A `# from:` COMMENT IS A CUT, AND A CUT AT THE WRONG PLACE PAYS TWICE:
/// the intake splits a card's sentence and files each piece as an effect,
/// so a split INSIDE a clause makes the head an unconditional grant the
/// card never had and the tail an inert remainder. Three such faults,
/// invisible to every other test:
///
///   - the Dera's High Ground, "+25% of current Status Chance" cut at the
///     plus sign into a flat +25% base crit chance;
///   - the Kunai's Deathtrap Trigger, "…by +1.4x for 4s" cut into a
///     PERMANENT +1.4x beside its window.
///
/// So a fragment must END where its clause ends: a sentence stop, a comma,
/// a semicolon, or the end of the description.
/// A GATED PERK ASKS THE FRAME HOLDING THE GUN — both sides of it.
///
/// Fortress Salvo is "With Armor Over 450: +4 Punch Through", and an arm
/// that reads `value` without looking at `condition:` pays it to everybody.
/// THE NEUTRAL FRAME IS THE FLOOR OF EVERY RELEASED ONE (105 armour), so it
/// is the case a reader meets by default and the one the gate must shut
/// on.
#[test]
fn a_gated_perk_asks_the_frame_holding_the_gun() {
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::StackPolicy;
    let pt = |armor: f64| {
        let mut t = crate::data::tenno::default_tenno().clone();
        t.armor = armor;
        let base = WeaponBase::from_data("boar_prime", true, &["boar_prime_fortress_salvo"]);
        crate::build::loadout::resolve_for(&base, &[], StackPolicy::Emergent, &t).punch_through_m
    };
    let bare = {
        let base = WeaponBase::from_data("boar_prime", true, &[]);
        resolve(&base, &[], StackPolicy::Emergent).punch_through_m
    };
    // THE FLOOR SHUTS IT. 105 armour is the least any released frame has,
    // so this is what the card shows until a scenario names a frame.
    assert_eq!(pt(105.0), bare, "the neutral frame is far under 450");
    // …and so does anything up to and including the threshold, which the
    // card states as OVER 450 rather than at least.
    assert_eq!(pt(450.0), bare, "\"Over 450\" is not 450");
    // A FRAME THAT CLEARS IT OPENS THE GATE, with no code change — which is
    // the whole reason the fight carries a Tenno.
    assert!(
        (pt(451.0) - bare - 4.0).abs() < 1e-9,
        "451 armour buys the 4 m: {} vs {bare}",
        pt(451.0)
    );
}

/// A `condition:` IS NEVER IGNORED — it gates, or the effect is INERT, and
/// it is never granted unconditionally.
///
/// This file already stated the rule in prose ("an unreadable `condition:`
/// falls to Inert rather than paying out unconditionally") and one arm had
/// never obeyed it: `punch_through_bonus` read `value` and nothing else, so
/// Fortress Salvo's "With Armor Over 450: +4 Punch Through" paid out to
/// every frame in the game. It was measured, and
/// nothing here could have caught it — so this is that thing.
///
/// IT WALKS THE DATA rather than a list of kinds. Every effect in every
/// evolution that declares a condition must load as one of the two honest
/// answers, so a kind added tomorrow is covered by nobody.
#[test]
fn no_conditional_effect_is_granted_unconditionally() {
    use serde_norway::Value;
    let mut bad: Vec<String> = Vec::new();
    let mut checked = 0;
    // THE RAW YAML, because `condition:` is consumed at load and the parsed
    // effect no longer carries it — which is exactly how an arm could drop
    // one without anything noticing.
    for (path, text) in crate::data::files_under("evolutions/") {
        if !path.ends_with(".yaml") {
            continue;
        }
        let doc: Value = serde_norway::from_str(text).expect("an evolution parses");
        let Some(effects) = doc.get("effects").and_then(Value::as_sequence) else {
            continue;
        };
        for raw in effects {
            let Some(cond) = raw.get("condition").and_then(Value::as_str) else {
                continue;
            };
            checked += 1;
            let kind = raw.get("kind").and_then(Value::as_str).unwrap_or("?");
            let Some(fx) = effect(raw) else {
                bad.push(format!("{path} / {kind} / `{cond}`: did not load at all"));
                continue;
            };
            // THE FOUR HONEST SHAPES. Gated on the Tenno; folded into a
            // FIELD of the effect (the sprint kinds, the headshot one);
            // folded into the effect's own TYPE, where the variant name
            // carries the condition and no field is needed; or Inert, which
            // says on the card that the perk does nothing.
            let honest = match &fx {
                EvoEffect::GatedByTenno { .. } | EvoEffect::Inert(_) => true,
                // The condition IS the variant: "on empty reload".
                EvoEffect::ReloadSpeedOnEmptyReload { .. } => true,
                EvoEffect::FireRateBonus { min_sprint, needs_melee_equipped, .. } => {
                    *min_sprint > 0.0 || *needs_melee_equipped
                }
                EvoEffect::ConditionOverload { min_sprint, .. } => *min_sprint > 0.0,
                EvoEffect::InstantReloadOnHeadshot { needs_kill, .. } => *needs_kill,
                _ => false,
            };
            if !honest {
                bad.push(format!(
                    "{path} / {kind} / `{cond}` loads as {fx:?} — a condition that pays out anyway"
                ));
            }
        }
    }
    assert!(checked >= 40, "only {checked} conditional effects found");
    assert!(bad.is_empty(), "{} unconditional grant(s):
  {}", bad.len(), bad.join("
  "));
}

#[test]
fn a_transcribed_fragment_ends_where_its_clause_ends() {
    let mut bad: Vec<String> = Vec::new();
    for (path, text) in crate::data::files_under("evolutions/") {
        let Some(desc) = text.lines().find_map(|l| {
            l.strip_prefix("description:")
                .map(str::trim)
                .and_then(|d| d.strip_prefix('"'))
                .and_then(|d| d.strip_suffix('"'))
        }) else {
            continue;
        };
        for line in text.lines() {
            let Some(frag) = line
                .trim()
                .strip_prefix("# from:")
                .map(str::trim)
                .and_then(|f| f.strip_prefix('"'))
                .and_then(|f| f.strip_suffix('"'))
            else {
                continue;
            };
            let Some(at) = desc.find(frag) else {
                bad.push(format!("{path}: `{frag}` is not in the description"));
                continue;
            };
            let tail = desc[at + frag.len()..].trim_start_matches(' ');
            if !tail.is_empty() && !tail.starts_with(['.', ',', ';']) {
                bad.push(format!("{path}: `{frag}` is cut mid-clause, before `{}`", &tail[..tail.len().min(48)]));
            }
        }
    }
    assert!(bad.is_empty(), "{} fragment(s) cut mid-clause:\n  {}", bad.len(), bad.join("\n  "));
}
