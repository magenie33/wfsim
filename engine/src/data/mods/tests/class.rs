use super::*;

/// Mod pools are DISCOVERED from `data/mods/<class>/`, so adding a class
/// is a data change. Today only `pistol` exists; the moment
/// `data/mods/rifle/` lands it must appear here with no code edit.
#[test]
fn classes_come_from_the_data_tree() {
    let cs = classes();
    assert!(cs.contains(&"pistol"), "expected the pistol class, got {cs:?}");
    for c in &cs {
        assert!(!class_pool(c).is_empty(), "class {c} has no mods");
    }
    // An unknown class is empty, never another class's pool.
    assert!(class_pool("no_such_class").is_empty());
}

/// A CONDITION DE PRINTS ON THE CARD MUST EXIST IN THE MODEL.
///
/// Primary Acuity read "+350% Weak Point Damage / +350% Weak Point
/// Critical Chance" and was modelled as plain `base_damage_bonus` +
/// `crit_chance_bonus` — every shot collected all of it, whether or not
/// anything was hit in the head. Its own pistol twin
/// had been right the whole time, which is what made one wrong file easy
/// to miss among a hundred right ones.
///
/// The check reads DE's own `description` beside the effects, so it works
/// for a mod nobody has thought about yet:
///
///   · "Weak Point" on the card ⇒ some effect is a `weakpoint_*` kind;
///   · "when/while Aiming" ⇒ a DAMAGE effect is wrapped in `aiming`
///     (a mod whose only payload is movement speed or accuracy is exempt —
///     the condition cannot change a number this calculator produces).
#[test]
fn a_condition_on_the_card_is_a_condition_in_the_model() {
    const DAMAGE_KINDS: [&str; 20] = [
        "base_damage_bonus", "crit_chance_bonus", "crit_damage_bonus",
        "crit_chance_per_combo", "status_chance_per_combo",
        "crit_chance_bonus_heavy_doubled", "crit_chance_on_slide",
        "slam_damage_bonus", "heavy_attack_damage_bonus", "combo_count_chance",
        "melee_combo_duration_bonus", "initial_combo", "heavy_attack_efficiency",
        "multishot_bonus", "status_chance_bonus", "fire_rate_bonus",
        "elemental_damage_bonus", "physical_damage_bonus",
        "faction_damage_bonus", "headshot_damage_bonus",
    ];
    let mut bad: Vec<String> = Vec::new();
    for (path, text) in crate::data::files_under("mods/").filter(|(p, _)| p.ends_with(".yaml")) {
        let id = text
            .lines()
            .find_map(|l| l.strip_prefix("id:"))
            .unwrap_or(path)
            .trim();
        let desc = text
            .lines()
            .find_map(|l| l.strip_prefix("description:"))
            .unwrap_or("")
            .to_lowercase();
        // Comments are stripped: a comment naming a trigger must not
        // satisfy a check about what the model does.
        let effects: String = match text.split_once("effects:") {
            Some((_, rest)) => rest
                .lines()
                .map(|l| l.split('#').next().unwrap_or(""))
                .collect::<Vec<_>>()
                .join("\n"),
            None => String::new(),
        };
        let has_damage = DAMAGE_KINDS.iter().any(|k| effects.contains(k));
        // "On Weak Point Hit:" is a TRIGGER and wants one; "Weak Point Damage"
        // is a STAT and wants its bucket. Update 44.0 put the first on cards
        // that never had the second.
        let stat = desc.replace("on weak point", "");
        if stat.contains("weak point") && !effects.contains("weakpoint_") {
            bad.push(format!("{id}: card says Weak Point, no weakpoint_* effect"));
        }
        if desc.contains("on weak point") && !effects.contains("trigger:") {
            bad.push(format!("{id}: card triggers on a weak point, no effect has a trigger"));
        }
        // ANY "while/when <state>" clause, not the two phrases that
        // happened to be known. Spectral Serration reads "+330% Damage
        // while Invisible" and was a flat bonus every build collected —
        // the check knew about aiming and weak points, so it walked past. A conditional is satisfied by a `condition:`,
        // by a `trigger:` the sim can evaluate, or by resolving to a
        // CondBuff — all three leave the word in the effects block.
        let conditional = effects.contains("condition:") || effects.contains("trigger:");
        if !conditional && has_damage {
            for clause in ["while ", "when "] {
                if let Some(at) = desc.find(clause) {
                    // "+X% Damage while Airborne" is a condition; "while
                    // Aiming" is too. A sentence that merely CONTAINS the
                    // word later (a note, not a gate) is why this looks
                    // only at what follows it.
                    let tail: String = desc[at..].chars().take(40).collect();
                    bad.push(format!(
                        "{id}: card gates on \"{}\" and no effect is conditional",
                        tail.trim_end()
                    ));
                    break;
                }
            }
        }
    }
    assert!(bad.is_empty(), "{} mod(s):\n  {}", bad.len(), bad.join("\n  "));
}
