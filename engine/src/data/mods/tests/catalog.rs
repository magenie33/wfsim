use super::*;

/// NO mod loads with an empty effect list.
///
/// A mod that parses to nothing equips, costs capacity, prints its card —
/// and does nothing, which the picker and the optimizer cannot see. 14 of
/// them shipped that way until 2026-08-01, every one a `kind: unmodeled`
/// the loader dropped on the floor: beam range, movement speed, ammo
/// conversion, self-stagger, noise, double jumps, kill explosions, status
/// spread. They carry no SINGLE-TARGET damage, which is what
/// [`ModEffect::Indirect`] is for — the value now survives into the panel
/// and the API, where the 2D multi-target model will read it instead of
/// re-deriving it from card text.
///
/// One of them, Shell Rush's "+50% Charge Rate", was not indirect at all:
/// a charged form's cadence IS its draw, so that was DPS being discarded.
#[test]
fn no_mod_loads_with_nothing() {
    // …UNLESS IT SAYS SO. `unmodeled` and `out_of_scope` are flags rather
    // than effects, so a card whose ENTIRE content is one of them loads
    // with an empty list — and that is the honest state rather than the
    // fault this test is about, which is an effect being silently dropped.
    //
    // MELEE IS WHERE THAT FIRST HAPPENED. Its exilus pool is
    // eleven cards and every one of them is either Tennokai (a window this
    // engine does not model) or blocking and movement (which this arena has
    // neither of), so eleven mods equip, pay nothing, and each says which of
    // the two it is on its own card.
    let mut empty: Vec<&str> = Vec::new();
    for class in classes() {
        for m in class_pool(class) {
            if m.effects.is_empty() && !m.unmodeled && !m.out_of_scope {
                empty.push(m.id);
            }
        }
    }
    empty.sort_unstable();
    empty.dedup();
    assert!(
        empty.is_empty(),
        "mods that equip and do nothing: {empty:?} — give the effect a \
             `kind` the loader knows, or an `IndirectStat` if it carries no \
             single-target damage"
    );
}

/// AN EXALTED MELEE SEATS THE COMBO, ACOLYTE AND AMALGAM CARDS: Techrot
/// Encore re-enabled every one of them (see notes: exalted_mods_reenabled),
/// on every form, and an ordinary melee keeps them too.
#[test]
fn an_exalted_melee_seats_the_cards_techrot_encore_reenabled() {
    let has = |weapon: &str, id: &str| pool_for_weapon(weapon).iter().any(|m| m.id == id);
    for id in ["blood_rush", "weeping_wounds", "body_count", "dispatch_overdrive", "gladiator_rush",
        "maiming_strike", "amalgam_organ_shatter", "condition_overload", "pressure_point"] {
        assert!(has("valkyr_talons", id), "{id}");
        assert!(has("valkyr_talons_heavy", id), "{id}: a form seats what its weapon does");
        assert!(has("magistar", id), "{id} still goes on an ordinary melee");
    }
    // …AND ITS FIXED STANCE IS IN ITS OWN POOL AND NO OTHER.
    assert!(has("valkyr_talons_slide", "hysteria"));
    assert!(!has("magistar", "hysteria"));
}

/// Every AMALGAM mod must declare that it cannot go on a sentinel weapon.
///
/// The wiki states it per mod — "This mod cannot be equipped on Sentinel
/// weapons", infobox tags `SENTINEL_WEAPON, POWER_WEAPON` — and the reason
/// is structural: an Amalgam mod's second half buffs the WARFRAME, which a
/// companion is not. DE's own taxonomy names that structure, so the check
/// can be mechanical: `/Lotus/Upgrades/Mods/DualSource/` is the directory
/// every Amalgam mod lives in.
///
/// The PATH is the check, not the rule — the wiki tag is the rule, and
/// each mod's yaml carries it with its citation. This exists so the next
/// Amalgam mod cannot be added without someone reading that infobox.
#[test]
fn every_amalgam_mod_declares_it_cannot_go_on_a_sentinel_weapon() {
    let mut missing: Vec<String> = Vec::new();
    for (p, text) in crate::data::files_under("mods/").filter(|(p, _)| p.ends_with(".yaml")) {
        let dual = text
            .lines()
            .any(|l| l.starts_with("internal_name:") && l.contains("/DualSource/"));
        if dual && !text.contains("sentinel_weapon") {
            missing.push(p.to_string());
        }
    }
    assert!(
        missing.is_empty(),
        "Amalgam (DualSource) mods with no `excludes_weapon: [sentinel_weapon, ...]`: \
             {missing:?} — check the wiki infobox's incompatibility tags"
    );
    // And the rule reaches the pool: plain Serration equips on a sentinel
    // weapon, the Amalgam one does not.
    let ids: Vec<&str> = pool_for_weapon("verglas_prime").iter().map(|m| m.id).collect();
    assert!(ids.contains(&"serration"), "plain Serration is fine on a sentinel weapon");
    assert!(!ids.contains(&"amalgam_serration"), "Amalgam Serration is not: {ids:?}");
    // Ammo Maximum is the wiki's other stated sentinel rule: "Mods that
    // affect Ammo Maximum have no effect on Robotic weapon because they
    // already have unlimited ammo reserves."
    assert!(!ids.contains(&"ammo_drum"), "an infinite reserve takes no ammo mod: {ids:?}");
    // The Torid keeps all three — it is neither a sentinel nor ammo-less.
    let torid: Vec<&str> = pool_for_weapon("torid").iter().map(|m| m.id).collect();
    for id in ["serration", "amalgam_serration", "ammo_drum"] {
        assert!(torid.contains(&id), "the torid keeps {id}");
    }
}

/// The Cannonade family states TWO rules on one card line, and all three
/// members must carry both. The shotgun one carried NEITHER until
/// 2026-08-03 — it had a bare zero-valued `fire_rate_bonus` where the lock
/// belongs, which is how "Fire Rate cannot be modified" ends up rendering
/// as "+0% Fire Rate" while a build stacks fire rate underneath it. Its
/// twins had been right since M23, which is exactly why a per-family
/// invariant is worth pinning: the outlier is invisible from either file.
#[test]
fn every_cannonade_states_both_of_its_rules() {
    let ids = ["semi_rifle_cannonade", "semi_pistol_cannonade", "semi_shotgun_cannonade"];
    for id in ids {
        let pools: Vec<String> = ["rifle", "pistol", "shotgun"].iter().map(|s| s.to_string()).collect();
        let m = pool_union(&pools)
            .into_iter()
            .find(|m| m.id == id)
            .unwrap_or_else(|| panic!("{id} is in the data"));
        assert_eq!(m.requires_weapon, Some("semi_auto"), "{id} states its EQUIP rule");
        assert_eq!(m.requires, Some("semi_auto"), "{id} states its CALC gate");
        assert!(m.disables.contains(&"fire_rate"), "{id} locks fire rate: {:?}", m.disables);
        // ...and states the lock as a lock, not as a zero-valued bonus.
        assert!(
            !m.effects.iter().any(|e| matches!(e, ModEffect::FireRate(_))),
            "{id} carries a fire-rate EFFECT under a fire-rate LOCK"
        );
    }
}

/// The lock BITES, on a real weapon with real mods: a fire-rate mod under
/// a Cannonade changes nothing, and neither does a fire-rate DRAWBACK —
/// "cannot be modified" is symmetric, which is why the mod is worth more
/// on a build carrying a negative, not less.
#[test]
fn a_cannonade_locks_fire_rate_both_ways() {
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::StackPolicy;
    let base = WeaponBase::from_data("torid", false, &[]);
    let pool = pool_for_weapon("torid");
    let pick = |id: &str| {
        pool.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id} in the torid pool"))
    };
    let cannon = pick("semi_rifle_cannonade");
    let speed = pick("speed_trigger");
    let slow = pick("critical_delay");          // -20% fire rate at max rank

    let fr = |mods: &[&ModDef]| resolve(&base, mods, StackPolicy::Emergent).fire_rate;
    let bare = fr(&[]);
    assert!(fr(&[speed]) > bare * 1.05, "speed trigger moves fire rate on its own");
    assert!(fr(&[slow]) < bare * 0.95, "critical delay moves it the other way");
    for (label, mods) in [
        ("a bonus", vec![cannon, speed]),
        ("a drawback", vec![cannon, slow]),
        ("both at once", vec![cannon, speed, slow]),
    ] {
        assert!(
            (fr(&mods) - bare).abs() < 1e-9,
            "under the lock the weapon keeps its BASE fire rate through {label}: {} vs {bare}",
            fr(&mods)
        );
    }
    // ...and the damage half still pays, so the lock is a lock and not a
    // whole-mod veto.
    let dmg = |mods: &[&ModDef]| resolve(&base, mods, StackPolicy::Emergent).damage.total();
    assert!(dmg(&[cannon]) > dmg(&[]) * 1.5, "the Cannonade still adds its damage");
}

/// "Only compatible with Semi-Auto Trigger" is an EQUIP rule, and the pool
/// is where an equip rule has to bite: the optimizer searches this list,
/// so a mod left in it is a mod a winning build can carry to a slot the
/// game refuses.
#[test]
fn the_cannonades_need_a_semi_auto_trigger() {
    let has = |w: &str, m: &str| pool_for_weapon(w).iter().any(|x| x.id == m);

    // Boar Prime is full-auto. This is the case that was wrong.
    assert!(!has("boar_prime", "semi_shotgun_cannonade"), "full-auto takes no Cannonade");
    // ...and so is its Incarnon form, a held beam — both firing modes fail.
    assert!(!has("boar_prime", "semi_rifle_cannonade"), "nor the rifle one");

    // The Torid IS semi-auto, and keeps it — the rule excludes, it does
    // not blanket-hide.
    assert!(has("torid", "semi_rifle_cannonade"), "a semi-auto rifle keeps it");
    for w in ["dual_toxocyst", "laetum"] {
        assert!(has(w, "semi_pistol_cannonade"), "{w} is semi-auto");
    }
    // Cernos Prime CHARGES; its uncharged form is semi-auto and does not
    // decide the pool.
    assert!(!has("cernos_prime", "semi_rifle_cannonade"), "a charge bow is not semi-auto");
}

/// A MOD WRITTEN FOR ONE WEAPON GOES NOWHERE ELSE.
///
/// "Can equip the Ocucor-exclusive Sentient Surge mod" (wiki, Ocucor), and
/// exclusivity is an EQUIP rule: the mod is never offered elsewhere rather
/// than equipping and sitting inert. Asserted in BOTH directions, because
/// only one of them is the interesting failure — a gate that hides the mod
/// everywhere passes any test that only checks it is absent from the
/// wrong weapons.
#[test]
fn an_exclusive_mod_reaches_its_weapon_and_no_other() {
    let has = |w: &str| pool_for_weapon(w).iter().any(|m| m.id == "sentient_surge");
    assert!(has("ocucor"), "the weapon it was written for must be offered it");
    for other in crate::data::weapons::roster().map(|s| s.id.clone()) {
        if other == "ocucor" {
            continue;
        }
        assert!(!has(&other), "{other} was offered an Ocucor-only mod");
    }
    // ...and it is a PISTOL mod, so it is in the pool it would otherwise
    // reach every pistol through. Without this the test above would pass
    // for a mod that simply failed to load.
    assert!(
        pool_union(&["pistol".to_string()]).iter().any(|m| m.id == "sentient_surge"),
        "it should be a pistol mod that exclusivity narrows, not a mod nobody has"
    );

    // GILDED TRUTH SPLITS A FAMILY, which is the harder case: the wiki says
    // it is "exclusive to the Burston Prime" AND "cannot be equipped on the
    // Burston", so one variant takes it and its twin does not — a
    // distinction a rule keyed on class, trigger or riven family could not
    // draw, since the two share all three.
    let gilded = |w: &str| pool_for_weapon(w).iter().any(|m| m.id == "gilded_truth");
    assert!(gilded("burston_prime"), "the Prime is what it was written for");
    assert!(!gilded("burston"), "and the wiki says the base variant cannot take it");
}

/// AN EQUIP RULE THE MOD DECLARES DECIDES EVERY POOL — derived, both ways.
///
/// A 136-row table, one line per weapon, guards against "a check that
/// recomputes the rule agrees with a wrong rule" — but the rule does not
/// live in this file: the mod's own yaml says `requires_weapon: semi_auto`
/// and `every_cannonade_states_both_of_its_rules` pins that against the
/// card. So this recomputes nothing; it asks the MOD what it requires, the
/// WEAPON what it is, and checks the pool agreed. The table cost one edit
/// per weapon and reproduced the rule 136 times out of 136, with no
/// exception in it — a snapshot with no surprises is a snapshot of a rule.
///
/// BOTH DIRECTIONS, because each alone passes on a different bug: "offered
/// ⇒ eligible" alone passes on a filter that offers nothing, and "eligible
/// ⇒ offered" alone passes on one that offers everything.
///
/// It is written over EVERY mod that declares a trigger requirement rather
/// than over the three Cannonades, so the next such mod is covered by
/// arriving.
#[test]
fn a_declared_trigger_rule_decides_every_pool() {
    // Every trigger a weapon in the roster actually lists — so a
    // requirement naming a trigger nothing has is caught as a typo rather
    // than passing vacuously.
    let triggers: std::collections::BTreeSet<&str> = crate::data::weapons::roster()
        .map(|s| s.attack.trigger.as_str())
        .collect();
    let gated: Vec<ModDef> = pool_union(&["rifle", "pistol", "shotgun", "archgun"]
        .iter().map(|s| s.to_string()).collect::<Vec<_>>())
        .into_iter()
        .filter(|m| m.requires_weapon.is_some_and(|r| triggers.contains(r)))
        .collect();
    assert!(gated.len() >= 3, "the three Cannonades at least: {}", gated.len());

    let mut wrong: Vec<String> = Vec::new();
    for m in &gated {
        let want = m.requires_weapon.expect("filtered on it");
        for w in crate::data::weapons::roster() {
            let offered = pool_for_weapon(&w.id).iter().any(|x| x.id == m.id);
            // ELIGIBLE means the weapon lists that trigger AND draws the
            // pool the mod lives in. The second half is what makes the
            // Fluctus interesting: it is semi-auto and takes none, because
            // an Arch-Gun draws neither the rifle nor the pistol pool.
            let draws = pool_for_weapon(&w.id).iter().any(|x| x.id == m.id)
                || pool_union(&w.mod_pools).iter().any(|x| x.id == m.id);
            let eligible = w.attack.trigger == want && draws;
            if offered != eligible {
                wrong.push(format!(
                    "{}: {} offered={offered} but trigger={} draws={draws}",
                    w.id, m.id, w.attack.trigger
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "a pool disagreed with a rule the mod DECLARES:\n  {}",
        wrong.join("\n  ")
    );
}

/// ...and the answers a reader would get wrong, written down. Not a
/// roster: three cases, each because something other than the trigger
/// decides it.
#[test]
fn the_cannonade_answers_worth_reading() {
    let has = |w: &str, m: &str| pool_for_weapon(w).iter().any(|x| x.id == m);
    // THE POOL BEATS THE TRIGGER. The Fluctus is semi-auto and takes none:
    // an Arch-Gun draws the archgun pool and the Cannonades are rifle,
    // pistol and shotgun mods.
    assert_eq!(crate::data::weapons::spec("fluctus").unwrap().attack.trigger, "semi_auto");
    assert!(!has("fluctus", "semi_rifle_cannonade"));
    // A SENTINEL WEAPON DOES TAKE ONE, which surprised the 2026-08-15
    // intake: Semi-Rifle Cannonade lives in the RIFLE pool rather than the
    // `primary` one, and a semi-auto companion weapon draws rifle mods.
    assert!(has("stinger", "semi_rifle_cannonade"));
    assert!(has("cryotra", "semi_rifle_cannonade"));
    // AND THE PISTOL ONE IS NOT THE RIFLE ONE. A weapon takes the Cannonade
    // of its own pool and no other.
    assert!(has("lex", "semi_pistol_cannonade"));
    assert!(!has("lex", "semi_rifle_cannonade"));
}

/// ...AND THE LOCK BITES, on every weapon that can equip one and in every
/// FORM of it.
///
/// The table above says who may wear a Cannonade; this says what wearing it
/// does. "Equipping this mod will set weapon's Fire Rate to its default
/// ignoring other bonuses, EVEN NEGATIVE EFFECTS" — the case that made the
/// question worth asking is the negative one, since a build pairs a
/// Cannonade with a fire-rate-for-crit trade precisely to be handed the
/// trade for free. Same sentence, same test, for the
/// Acuity twins' Multishot.
///
/// DERIVED: it finds the locking mods by their `disables`, the offending
/// mods by resolving each one alone and seeing which move that stat, and
/// the forms from the weapon. A fourth locking mod, or a fifth weapon that
/// can wear one, is covered without a line here.
#[test]
fn a_locking_mod_pins_its_stat_in_every_form_that_can_wear_it() {
    use crate::build::loadout::resolve;
    use crate::model::WeaponBase;
    use crate::model::StackPolicy;
    let read = |p: &crate::build::loadout::ResolvedPanel, key: &str| match key {
        "fire_rate" => p.fire_rate,
        "multishot" => p.multishot,
        other => panic!("no reader for the locked stat `{other}`"),
    };
    let mut proven = 0;
    for w in crate::data::weapons::roster() {
        let pool = pool_for_build(&w.id, &[]);
        let lockers: Vec<&ModDef> = pool.iter().filter(|m| !m.disables.is_empty()).collect();
        if lockers.is_empty() {
            continue;
        }
        for f in crate::data::weapons::forms_of(&w.id) {
            let base = WeaponBase::from_data(f.weapon_id, true, &[]);
            let bare = resolve(&base, &[], StackPolicy::Emergent);
            for lock in &lockers {
                for key in &lock.disables {
                    // Everything in this pool that MOVES the stat on its
                    // own — which is what the lock has to be tested
                    // against, in both directions of movement.
                    for other in pool.iter().filter(|m| m.id != lock.id) {
                        let alone = resolve(&base, &[other], StackPolicy::Emergent);
                        if (read(&alone, key) - read(&bare, key)).abs() < 1e-9 {
                            continue;
                        }
                        let both = resolve(&base, &[other, lock], StackPolicy::Emergent);
                        assert!(
                            (read(&both, key) - read(&bare, key)).abs() < 1e-9,
                            "{} ({}): {} moved {key} from {} to {} under {} — \
                                 a lock that lets a bonus through",
                            w.id,
                            f.weapon_id,
                            other.id,
                            read(&bare, key),
                            read(&both, key),
                            lock.id
                        );
                        proven += 1;
                    }
                }
            }
        }
    }
    // The three Cannonades and the two Acuities, across their weapons and
    // forms: a collapse here means the walk stopped finding the pairs.
    assert!(proven > 100, "the walk collapsed: only {proven} lock/bonus pairs");
}

/// A FORM IS NOT A WEAPON. `cernos_prime_uncharged` fires semi-auto, and
/// the bow it belongs to is listed "Charge" — asking the form entry its own
/// trigger would hand a Cannonade to a weapon that cannot hold one. The
/// weapon's trigger is its DEFAULT form's, which is what the arsenal and
/// the weapon-comparison table show.
#[test]
fn a_form_entry_answers_with_its_weapons_trigger() {
    let has = |w: &str, m: &str| pool_for_build(w, &[]).iter().any(|x| x.id == m);
    assert_eq!(
        crate::data::weapons::spec("cernos_prime_uncharged").unwrap().attack.trigger,
        "semi_auto",
        "the tapped shot really is semi-auto — that is the trap"
    );
    assert!(!has("cernos_prime_uncharged", "semi_rifle_cannonade"), "...but the bow is not");
    // AND THE POOL IS THE WEAPON'S TOO, which is the same sentence one
    // question along: a form declares no `mod_pools` and modding happens on
    // the weapon, so naming a form resolves the weapon's pool.
    assert_eq!(
        pool_for_build("dual_toxocyst_incarnon", &[]).len(),
        pool_for_build("dual_toxocyst", &[]).len(),
        "a form is modded as its weapon"
    );
}

/// …AND NOT ONE FORM IS AN EXCEPTION.
///
/// `mod_pools` is on the INHERITED list, so an `inherits:` line is enough
/// to give a form a pool of its own — which would make "can this form be
/// modded" a question about whether its file carries that line. THE WALK IS
/// THE ASSERTION: one witness cannot see a rule broken by a seventh of the
/// roster, and the two tests above hold one witness each.
#[test]
fn every_form_is_modded_as_the_weapon_it_is_a_form_of() {
    let mut off = Vec::new();
    for s in crate::data::weapons::all() {
        let Some(parent) = s.transforms_from.as_deref() else { continue };
        let (a, b) = (pool_for_weapon(&s.id).len(), pool_for_weapon(parent).len());
        if a != b {
            off.push(format!("{} has {a} against {parent}'s {b}", s.id));
        }
    }
    assert!(off.is_empty(), "a form's pool is not its weapon's:\n  {}", off.join("\n  "));
    // …and the walk has to have walked: a filter that matched nothing would
    // pass the line above without asserting anything at all.
    let forms = crate::data::weapons::all()
        .iter()
        .filter(|s| s.transforms_from.is_some())
        .count();
    assert!(forms > 60, "the roster has forms to check: {forms}");
}

/// INSTALLING THE GENESIS IS WHAT TAKES THE CANNONADE OFF. "Weapons with an Incarnon mode must have Semi-Auto trigger
/// type for both firing modes in order to equip this mod" (wiki,
/// Semi-Pistol_Cannonade), and the roster's three semi-auto Incarnon
/// weapons all transform into something that is not: Dual Toxocyst and
/// Laetum into full-auto, the Torid into a held beam.
///
/// So the pool is a question about the BUILD, not about the weapon: with
/// tier 1 unpicked the weapon has one firing mode and the mod fits, and the
/// moment tier 1 goes in it has two and the mod is gone.
#[test]
fn an_unlocked_incarnon_form_is_a_second_firing_mode() {
    let has = |w: &str, evos: &[&str], m: &str| {
        pool_for_build(w, evos).iter().any(|x| x.id == m)
    };
    for (w, evo, m) in [
        ("dual_toxocyst", "dual_toxocyst_evo1_incarnon_form", "semi_pistol_cannonade"),
        ("laetum", "laetum_evo1_incarnon_form", "semi_pistol_cannonade"),
        ("torid", "torid_evo1_incarnon_form", "semi_rifle_cannonade"),
    ] {
        assert!(has(w, &[], m), "{w} with nothing installed is pure semi-auto");
        assert!(!has(w, &[evo], m), "{w} with the Incarnon form installed is not");
        // The rest of the pool is untouched — this excludes one mod, it is
        // not a second pool for the transformed weapon.
        assert!(
            has(w, &[evo], "serration") || has(w, &[evo], "hornet_strike"),
            "{w} keeps its ordinary mods with the form unlocked"
        );
    }
    // An evolution that unlocks NOTHING changes nothing: only a form the
    // weapon gains can be a second trigger.
    assert!(
        has("dual_toxocyst", &["dual_toxocyst_carnage_reign"], "semi_pistol_cannonade"),
        "a stat evolution is not a firing mode"
    );
    // ...and a weapon whose Incarnon form is ALSO semi-auto would keep it.
    // The roster has none yet (the wiki names Bronco / Lato / Lex), so the
    // claim is pinned on the data instead: every entry here transforms into
    // a trigger that is not semi-auto, which is why all three drop it.
    for (w, evo) in [
        ("dual_toxocyst", "dual_toxocyst_evo1_incarnon_form"),
        ("laetum", "laetum_evo1_incarnon_form"),
        ("torid", "torid_evo1_incarnon_form"),
    ] {
        let form = crate::data::evolutions::get(evo).and_then(|e| e.unlocks_form()).unwrap();
        assert_ne!(
            crate::data::weapons::spec(form).unwrap().attack.trigger,
            "semi_auto",
            "{w}: the test above only proves the rule while this holds"
        );
    }
}

/// PvP-EXCLUSIVE mods must not ship in a PvE pool — they are a separate
/// balance pass, and offering them makes the picker and the optimizer
/// propose builds that cannot exist in the mission the sim models.
///
/// The trap is that `/Lotus/Upgrades/Mods/PvPMods/` in the `internal_name`
/// is an ORIGIN, not a restriction. Update 17.9 made a set of Conclave mods
/// equippable in PvE, so four of ours legitimately sit under that path.
/// Deleting on the path alone throws away real content; keeping everything
/// under it ships six mods that cannot be equipped.
///
/// The authority is the wiki's `Rifle_Mods` / `Pistol_Mods` /
/// `Shotgun_Mods` tables, which tag the genuinely restricted ones
/// "Exclusive to PvP". This pins the survivors as an explicit allowlist, so
/// a new PvP-path mod fails until someone checks that table — as the
/// SHOTGUN import did, where the generator brought 15 mods in under the
/// path and `Shotgun_Mods` tagged ten of them.
#[test]
fn only_pve_legal_conclave_mods_are_in_the_pools() {
    const PVE_LEGAL: [&str; 13] = [
        "agile_aim", "twitch", "eject_magazine", "reflex_draw",
        // Shotgun, from `Shotgun_Mods`.
        "broad_eye", "double_barrel_drift", "lock_and_load", "snap_shot", "soft_hands",
        // ASSAULT RIFLE, from `Rifle_Mods`. The RENDERED page
        // is what carries the tags — the raw wikitext is template
        // transclusions and names none of these mods, so a check against
        // `?action=raw` would have found nothing and concluded nothing.
        // Seven mods on that page are tagged "Exclusive to PvP" and two of
        // them are assault-rifle-only (Recover, Vanquished Prey); those
        // were NOT imported. The page's own "Assault rifle-only" list is
        // the positive statement, and it names these three.
        "gun_glide", "overview", "tactical_reload",
        // DOUBLE TAP is the fourth, and its own page states the direction
        // outright: "is a PvE and Conclave Latron, Latron Wraith, and
        // Latron Prime mod". The Bugs block goes further and says the
        // Conclave half is the broken one — "Despite originally being a
        // Conclave only mod, the buff does not actually function in
        // Conclave" — so a PvP path here is where it came FROM, not where
        // it works.
        "double_tap",
    ];
    let mut found: Vec<String> = crate::data::files_under("mods/")
        .filter(|(p, _)| p.ends_with(".yaml"))
        .filter(|(_, text)| {
            text.lines()
                .any(|l| l.starts_with("internal_name:") && l.contains("/PvPMods/"))
        })
        .map(|(p, _)| {
            p.rsplit('/').next().unwrap_or(p).trim_end_matches(".yaml").to_string()
        })
        .collect();
    found.sort();
    let mut want: Vec<String> = PVE_LEGAL.iter().map(|s| s.to_string()).collect();
    want.sort();
    assert_eq!(
        found, want,
        "every mod under /PvPMods/ must be one the wiki does NOT tag \
             \"Exclusive to PvP\" — check Rifle_Mods / Pistol_Mods before changing this"
    );
}

/// Every `X` in a description must be filled. A literal X on a mod card is
/// a rendering failure — "Stacks up to Xx." is what it looked like — and it
/// only became visible on the rifle pool once `desc_info` started covering
/// it, so the pool asserts it rather than waiting to be noticed again.
#[test]
fn every_mod_description_fills_all_its_x() {
    // KNOWN GAP, not a tolerance: these carry a parenthetical about BOWS
    // ("(xX for Bows)", and Internal Bleeding's fire-rate clause) whose
    // multiplier is real in-game data we do not hold. Because `fill_x`
    // substitutes positionally, that missing value does not merely leave an
    // X — it SHIFTS every later one, so Shred renders its punch-through
    // (1.2) as the bow multiplier "x2.2" and then has nothing left for
    // "+X Punch Through". Fixing it means adding the datum, not deleting
    // the clause: bows draw from the rifle pool, so the text is relevant.
    const MISSING_BOW_MULTIPLIER: [&str; 7] = [
        "critical_delay", "internal_bleeding", "primed_shred", "shred",
        "speed_trigger", "vile_acceleration", "vile_precision",
    ];
    let mut bad = Vec::new();
    for class in ["pistol", "rifle"] {
        for m in class_pool(class) {
            if MISSING_BOW_MULTIPLIER.contains(&m.id) {
                continue;
            }
            if let Some(info) = desc_info(m.id) {
                let s = info.at(info.max_rank);
                if s.contains('X') {
                    bad.push(format!("{}: {}", m.id, s.replace('\n', " / ")));
                }
            }
        }
    }
    assert!(bad.is_empty(), "unfilled X placeholders:\n{}", bad.join("\n"));
}

#[test]
fn loads_the_pistol_pool_from_yaml() {
    let mods = load_class("pistol");
    assert!(mods.len() >= 26, "expected >=26 mods, got {}", mods.len());

    let by = |id: &str| mods.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("missing {id}"));

    // Generic bonus.
    assert!(matches!(by("hornet_strike").effects[0], ModEffect::BaseDamage(v) if (v - 2.20).abs() < 1e-9));
    // Primary vs combined element dispatch.
    assert!(by("frostbite").effects.iter().any(|e| matches!(e, ModEffect::Element(DamageType::Cold, v) if (*v - 0.60).abs() < 1e-9)));
    assert!(by("magnetic_might").effects.iter().any(|e| matches!(e, ModEffect::CombinedElement(DamageType::Magnetic, v) if (*v - 0.60).abs() < 1e-9)));
    // Conditional families.
    // …AND WHAT EARNS IT, because the two cards sharing this variant differ
    // by nothing else: the Galvanized one waits for a kill and melee's own
    // waits for nothing, which is the whole of what a fight can deny.
    assert!(by("galvanized_shot").effects.iter().any(|e| matches!(e, ModEffect::ConditionOverload { per_stack, max_stacks: 3, earned_on: Some("kill"), .. } if (*per_stack - 0.40).abs() < 1e-9)));
    let melee = pool_for_weapon("magistar");
    let melee_co = melee.iter().find(|m| m.id == "condition_overload").expect("melee CO");
    assert!(melee_co.effects.iter().any(|e| matches!(e, ModEffect::ConditionOverload { earned_on: None, .. })));
    assert!(by("galvanized_diffusion").effects.iter().any(|e| matches!(e, ModEffect::OnKillMultishot { per_stack, max_stacks: 4, .. } if (*per_stack - 0.30).abs() < 1e-9)));
    // Galvanized Crosshairs is AIM-GATED, so its buffs arrive WRAPPED -
    // asserting the bare variant would pass on a build where the gate had
    // been silently dropped, which is the bug this wrapper exists to stop.
    assert!(by("galvanized_crosshairs").effects.iter().any(|e| matches!(e,
        ModEffect::WhileTenno(crate::model::TennoCondition::Aiming, inner)
            if matches!(**inner, ModEffect::OnHeadshotKillCritChance { max_stacks: 5, .. }))));
    assert!(by("galvanized_crosshairs").effects.iter().all(|e| matches!(e, ModEffect::WhileTenno(crate::model::TennoCondition::Aiming, _))),
        "every Galvanized Crosshairs effect is while-aiming");
    // ... and a mod with no condition is NOT wrapped.
    assert!(by("galvanized_diffusion").effects.iter().all(|e| !matches!(e, ModEffect::WhileTenno(crate::model::TennoCondition::Aiming, _))));
    // Faction-damage mod loads with the right faction + bonus (Expel Orokin
    // → Corrupted; +30% at max rank).
    assert!(by("expel_grineer").effects.iter().any(|e| matches!(e, ModEffect::FactionDamage(Faction::Grineer, v) if (*v - 0.30).abs() < 1e-9)));
    assert!(by("expel_orokin").effects.iter().any(|e| matches!(e, ModEffect::FactionDamage(Faction::Corrupted, _))));
    // The formerly-unmodeled kinds now map to real effects.
    assert!(by("pistol_acuity").effects.iter().any(|e| matches!(e, ModEffect::WeakpointDamage(v) if (*v - 3.50).abs() < 1e-9)));
    assert!(by("pistol_acuity").effects.iter().any(|e| matches!(e, ModEffect::WeakpointCritChance(v) if (*v - 3.50).abs() < 1e-9)));
    assert!(by("hemorrhage").effects.iter().any(|e| matches!(e,
        ModEffect::ProcConversion { from: DamageType::Impact, to: DamageType::Slash, chance, low_rate_threshold, low_rate_multiplier }
            if (*chance - 0.35).abs() < 1e-9 && (*low_rate_threshold - 2.5).abs() < 1e-9 && (*low_rate_multiplier - 2.0).abs() < 1e-9)));
    // Both of these are while-aiming too, so they arrive wrapped.
    assert!(by("sharpened_bullets").effects.iter().any(|e| matches!(e,
        ModEffect::WhileTenno(crate::model::TennoCondition::Aiming, inner)
            if matches!(**inner, ModEffect::OnKillCritDamage { bonus, duration }
                if (bonus - 0.75).abs() < 1e-9 && (duration - 9.0).abs() < 1e-9))));
    assert!(by("pressurized_magazine").effects.iter().any(|e| matches!(e,
        ModEffect::WhileTenno(crate::model::TennoCondition::Aiming, inner)
            if matches!(**inner, ModEffect::OnReloadFireRate { bonus, .. }
                if (bonus - 0.90).abs() < 1e-9))));
}

/// A description's numbers are of two kinds and they must not swap places:
/// some RAMP with rank, some are FIXED. Every case here was wrong when the
/// values were handed out by position (checked against WFCD `levelStats`,
/// 2026-07-31).
#[test]
fn fixed_and_rank_varying_values_land_in_the_right_slots() {
    // Literal duration and stack cap in the text, so the two X's are both
    // crit. By position the 12-second duration took the second one and
    // printed "+1200% Critical Chance".
    assert_eq!(
        desc_info("galvanized_crosshairs").unwrap().at(10),
        "On Weak Point Hit:
+120% Critical Chance when Aiming for 12s
On Weak Point Kill:
+40% Critical Chance when Aiming for 12s. Stacks up to 5x."
    );
    // Its rifle twin spells all five out. The first buff carries a
    // `max_stacks: 1` the text never mentions, so a per-kind queue would
    // hand THAT to "Stacks up to Xx" instead of the second buff's 5.
    assert_eq!(
        desc_info("galvanized_scope").unwrap().at(10),
        "On Weak Point Hit:
+120% Critical Chance when Aiming for 12s
On Weak Point Hit:
+40% Critical Chance when Aiming for 12s. Stacks up to 5x."
    );
    // A duration that RAMPS: 1.5s at rank 0, 9s at max. Stored as one number
    // it read "for 9s" at every rank.
    let argon = desc_info("argon_scope").unwrap();
    assert_eq!(argon.at(0), "On Weak Point Hit:
+22.5% Critical Chance when Aiming for 1.5s");
    assert_eq!(argon.at(5), "On Weak Point Hit:
+135% Critical Chance when Aiming for 9s");
    // A stack CAP that ramps, 1x -> 6x — rank-varying, not fixed.
    assert_eq!(
        desc_info("aerial_ace").unwrap().at(5),
        "On Kill:
Refresh Double Jump up to 6x while Airborne."
    );
    // The bows multiplier is fixed text; the fire rate is not.
    assert_eq!(
        desc_info("shred").unwrap().at(5),
        "+30% Fire Rate (x2 for Bows)
+1.2 Punch Through"
    );
}

#[test]
fn desc_info_fills_every_x_across_the_pool() {
    // EVERY class, not just pistol. The pool this walked was the only one
    // that existed when it was written, so a guard that NAMES a pool stops
    // guarding the moment a second appears — the rifle pool then shipped
    // descriptions whose X count exceeded their values, and Vile
    // Acceleration showed its damage downside as a bare placeholder. It reads the class registry now.
    //
    // (X count <= varying-effect count; hidden tail stats — Amalgam's
    // acrobatic speed — are legitimately unconsumed.)
    for c in classes() {
        for m in class_pool(c) {
            let info =
                desc_info(m.id).unwrap_or_else(|| panic!("{} has no description", m.id));
            for r in 0..=info.max_rank {
                let d = info.at(r);
                assert_eq!(
                    crate::model::count_x(&d),
                    0,
                    "{} rank {r}: unfilled X in {d:?}",
                    m.id
                );
            }
        }
    }
    // Spot checks: linear fill, the xX faction form, and a flat value.
    assert_eq!(desc_info("hornet_strike").unwrap().at(10), "+220% Damage");
    assert_eq!(desc_info("hornet_strike").unwrap().at(0), "+20% Damage");
    assert_eq!(desc_info("expel_grineer").unwrap().at(5), "x1.3 Damage to Grineer");
    assert_eq!(desc_info("seeker").unwrap().at(5), "+2.1 Punch Through");
    // Signed template + negative stored downside: magnitude only.
    assert_eq!(desc_info("anemic_agility").unwrap().at(5), "+90% Fire Rate\n-15% Damage");
    // Its rifle twin, plus the literal bows clause: the `2` is TEXT, not a
    // value — written as `X` it ate the damage stat and left the last
    // placeholder unfilled.
    assert_eq!(
        desc_info("vile_acceleration").unwrap().at(5),
        "+90% Fire Rate (x2 for Bows)\n-15% Damage"
    );
}
