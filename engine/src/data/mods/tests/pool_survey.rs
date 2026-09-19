/// EVERY CLASS-TAGGED GUN MOD THE ROSTER'S POOLS CAN HOLD, and how many
/// are still missing.
///
/// The sibling of `the_weapon_exclusive_mods_we_still_owe_only_goes_down`:
/// that survey joins `compatName` against WEAPON NAMES, this one against
/// the POOL TAGS — Rifle, Bow, Sniper, Shotgun, Pistol, Assault Rifle,
/// PRIMARY, Archgun — which is where the other five hundred live.
///
/// The failure mode it covers is invisible: a pool a weapon DECLARES and no
/// directory holds resolves to an empty list with no error anywhere. Nine
/// bows claimed `bow` while `data/mods/bow/` did not exist, so Split
/// Flights was unreachable; fifteen snipers claimed no `sniper` pool at
/// all, so both Chambers were. `scripts/survey_pool_mods.py` refuses to run
/// when a weapon claims a pool no export tag maps to.
///
/// The ceiling is a RATCHET starting where the pools stood the day the
/// survey was written. It is not zero and is not meant to be yet: the rest
/// are a work list rather than a defect.
#[test]
fn the_pool_mods_we_still_owe_only_goes_down() {
    // LOWERED 28 -> 21 ON 2026-08-29, and not by transcribing seven cards.
    // MELEE OWES NOTHING — all 89 of its cards and both hammer stances are
    // carried — and the survey learned two exclusions on the way in: DE's
    // own `/Beginner/` and `/Intermediate/` tiers, which carry a released
    // card's display name and different numbers, and one unreleased
    // Pressure Point variant that carries neither marker. That is the
    // repo's `internal_name` rule made executable: joining by NAME is what
    // put a phantom +200% Pressure Point in front of the melee intake.
    const OWED: usize = 21;
    let text = crate::data::file("surveys/pool_mods.yaml")
        .expect("data/surveys/pool_mods.yaml — run scripts/survey_pool_mods.py");
    let mut total = 0usize;
    let mut missing: Vec<String> = Vec::new();
    let mut unreasoned: Vec<String> = Vec::new();
    let (mut name, mut pool) = ("", "");
    for line in text.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("- name:") {
            name = v.trim();
            total += 1;
        } else if let Some(v) = l.strip_prefix("pool:") {
            pool = v.trim();
        } else if let Some(v) = l.strip_prefix("carried:") {
            match v.trim() {
                "~" => missing.push(format!("{pool}: {name}")),
                "excluded" => unreasoned.push(format!("{pool}: {name}")),
                _ => {}
            }
        } else if l.starts_with("reason:") {
            let key = format!("{pool}: {name}");
            unreasoned.retain(|n| *n != key);
        }
    }
    assert!(total >= 400, "the survey looks empty: {total} rows");
    assert_eq!(
        missing.len(),
        OWED,
        "{} class-tagged mods missing, ceiling {OWED} — transcribe one and lower \
             this line, or raise it deliberately:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
    // A REFUSAL IS NOT A SHORTCUT — the same rule the weapon-exclusive
    // survey holds. `excluded` takes a mod out of the gap count, so it
    // costs a written reason; otherwise the cheapest way to close a ratchet
    // is to declare everything out of scope.
    assert!(
        unreasoned.is_empty(),
        "excluded without a reason: {}",
        unreasoned.join(", ")
    );

    // AND EVERY POOL A WEAPON CLAIMS HOLDS SOMETHING. This is the assertion
    // that bites on the actual bug: `bow` and `sniper` were both legal
    // names carried by real weapons and both resolved to nothing.
    for w in crate::data::weapons::all() {
        for p in &w.mod_pools {
            assert!(
                !crate::data::mods::class_pool(p).is_empty(),
                "{}: mod pool `{p}` is empty — every mod tagged for it is unreachable",
                w.id
            );
        }
    }
}

/// NOTHING THE SURVEY EXCLUDED IS IN THE POOLS.
///
/// The survey's `excluded` rows are the export entries a player cannot
/// equip — riven placeholders, DE's internal tiers, Conclave-only cards,
/// and mods the export marks unreleased. The generator refuses to write a
/// row that is both excluded and carried, but the generator reads
/// `vendor/`, which is gitignored and therefore absent from CI: a mod file
/// added without re-running it would be excluded in the committed survey
/// and in the pool at once, and nothing would say so.
///
/// The bug this was written for is `Primed Electrified Barrel`: DE built
/// the card, never shipped it, WFCD's export carries it with `introduced:
/// TBA`, the survey listed it as a gap, and it was transcribed. The wiki
/// has no page for it. It sat in the archgun pool and on three boards.
#[test]
fn no_mod_the_survey_excluded_is_carried() {
    let text = crate::data::file("surveys/pool_mods.yaml")
        .expect("data/surveys/pool_mods.yaml — run scripts/survey_pool_mods.py");
    let mut excluded: std::collections::BTreeMap<&str, &str> = Default::default();
    let (mut name, mut uniq) = ("", "");
    for line in text.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("- name:") {
            name = v.trim();
        } else if let Some(v) = l.strip_prefix("internal_name:") {
            uniq = v.trim();
        } else if l.strip_prefix("carried:").map(str::trim) == Some("excluded") {
            excluded.insert(uniq, name);
        }
    }
    assert!(!excluded.is_empty(), "the survey excludes nothing: it looks unparsed");
    let mut carried: Vec<String> = Vec::new();
    for (path, body) in crate::data::files_under("mods/").filter(|(p, _)| p.ends_with(".yaml")) {
        for l in body.lines() {
            if let Some(v) = l.strip_prefix("internal_name:") {
                if let Some(why) = excluded.get(v.trim()) {
                    carried.push(format!("{path} carries {why}"));
                }
            }
        }
    }
    assert!(
        carried.is_empty(),
        "the survey excludes these and the pool holds them — delete the file, \
             or change the rule in scripts/survey_pool_mods.py and say why:\n  {}",
        carried.join("\n  ")
    );
}
