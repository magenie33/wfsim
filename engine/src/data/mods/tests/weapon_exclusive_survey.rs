/// **SCATTERED JUSTICE REACHES THREE HEK ENTRIES AND REFUSES THE FOURTH.**
///
/// The pellet count IS this weapon, so which copies may carry the card is
/// the whole of what it does. Three of the four are one statement and one
/// is the opposite:
///
///   * the **Hek** by id, and the wiki does the arithmetic — *"This mod
///     alone will provide the Hek a total pellet count of 21"*, which is
///     7 x (1 + 2.0) and therefore the SAME bucket Hell's Chamber writes
///     to, not a multiplier of its own;
///   * BOTH **Kuva Hek** entries off one name, because `kuva_hek` is their
///     transform group — the single barrel and the four-barrel volley are
///     one weapon holding one card, and listing only the default form
///     would arm the entry nobody scores;
///   * NOT the **Vaykor Hek**: *"Can not be equipped on Vaykor Hek"*, which
///     already carries a built-in Justice effect. A negative control that
///     is a real game rule rather than a fixture.
#[test]
fn scattered_justice_reaches_every_hek_that_may_carry_it() {
    let carries = |weapon: &str| {
        crate::data::mods::pool_for_weapon(weapon)
            .iter()
            .any(|m| m.id == "scattered_justice")
    };
    for w in ["hek", "kuva_hek", "kuva_hek_quad"] {
        assert!(carries(w), "{w} may equip Scattered Justice and its pool does not offer it");
    }
    assert!(!carries("vaykor_hek"), "the Vaykor Hek cannot equip Scattered Justice");

    // …AND IT LANDS IN THE MULTISHOT BUCKET, which is what makes the wiki's
    // 21 pellets reproduce. A grant of its own would read the same on a
    // bare weapon and diverge the moment Hell's Chamber goes in.
    let pool = crate::data::mods::pool_for_weapon("kuva_hek");
    let def = pool
        .iter()
        .find(|m| m.id == "scattered_justice")
        .expect("the Kuva Hek can equip it");
    let ms: f64 = def
        .effects
        .iter()
        .filter_map(|e| match *e {
            crate::model::ModEffect::Multishot(v) => Some(v),
            _ => None,
        })
        .sum();
    assert!((ms - 2.0).abs() < 1e-9, "+200% multishot at max rank, got {ms}");
}

/// **DREADFUL KILLSHOT PAYS IN WHOLE STEPS, AND STOPS AT THE CAP.**
///
/// The Basmu's augment: *"increases Damage and Status Chance for every 75
/// Current Warframe Health, up to 360% at all ranks"* — the first mod whose
/// value is a function of the PLAYER, so the arithmetic is asserted.
///
/// THE WIKI'S OWN CROSS-CHECK IS THE SHARP ONE: *"the equipped Warframe
/// must have at least 675 current health for the damage bonus to outdo
/// Serration"*. 675 is nine whole steps at 20% = 180% against Serration's
/// 165%, so asserting it is asserting that the damage half lands in
/// SERRATION'S BRACKET rather than a final multiplier.
#[test]
fn dreadful_killshot_pays_per_75_health_and_caps() {
    // FROM THE BASMU'S OWN POOL, which is also the assertion that the mod
    // is reachable: it is `exclusive_to: [basmu]`, so a transcription that
    // never joins a pool would fail here rather than pass unnoticed.
    let pool = crate::data::mods::pool_for_weapon("basmu");
    let def = pool
        .iter()
        .find(|m| m.id == "dreadful_killshot")
        .expect("the Basmu can equip its own augment");
    let terms: Vec<crate::model::TennoScaledTerm> = def
        .effects
        .iter()
        .filter_map(|e| match *e {
            crate::model::ModEffect::TennoScaled { stat, above, unit, per_unit, cap, grant } => {
                Some(crate::model::TennoScaledTerm { stat, above, unit, per_unit, cap, grant })
            }
            _ => None,
        })
        .collect();
    // TWO ENTRIES, ONE PERCENTAGE — "Both the Damage and Status chance
    // bonuses are additive", so they are separate grants with identical
    // parameters rather than one effect granting a pair.
    assert_eq!(terms.len(), 2, "{:?}", def.effects);
    assert!(terms.iter().any(|t| t.grant == crate::model::ArcGrant::BaseDamage));
    assert!(terms.iter().any(|t| t.grant == crate::model::ArcGrant::StatusChance));

    let at = |health: f64| {
        let mut t = crate::data::tenno::default_tenno().clone();
        t.health = health;
        let v: Vec<f64> = terms.iter().map(|x| x.value(&t)).collect();
        assert!((v[0] - v[1]).abs() < 1e-12, "the two halves must be one number: {v:?}");
        v[0]
    };
    let near = |a: f64, b: f64| assert!((a - b).abs() < 1e-9, "{a} vs {b}");

    // WHOLE STEPS ONLY — "rounded down to the nearest multiple of 20%".
    near(at(74.0), 0.0);
    near(at(75.0), 0.20);
    // 149 is one step and not 1.99 of one, which is the whole reason `unit`
    // is a field rather than the rate being pre-divided.
    near(at(149.0), 0.20);
    near(at(150.0), 0.40);
    // THE NEUTRAL PLAYER, which is what a build pays before anyone says
    // which frame is holding the gun: 250 health is three steps.
    near(at(250.0), 0.60);
    // THE WIKI'S OWN COMPARISON, and the assertion that pins the bucket.
    near(at(675.0), 1.80);
    // THE CAP, from the wiki's "minimum max health needed to reach cap"
    // column: 1350 at rank 5. One step under it is not capped.
    near(at(1275.0), 3.40);
    near(at(1350.0), 3.60);
    near(at(100_000.0), 3.60);
}

/// EVERY WEAPON-EXCLUSIVE GUN MOD OUR ROSTER CAN EQUIP, and how many of
/// them are still missing.
///
/// A mod that fits ONE weapon is invisible to every other check we have:
/// the pools are built from what `data/mods/` holds, so a mod nobody
/// transcribed is one the builder cannot offer and nothing notices.
///
/// `data/surveys/weapon_exclusive_mods.yaml` is the survey — generated by
/// `scripts/survey_weapon_mods.py` from DE's Public Export, joined on
/// `compatName` and read by this test and nothing else. An EXCLUSION has to
/// carry its reason, so refusing a mod costs the same sentence as
/// transcribing one.
///
/// A GENERATED FILE CANNOT ANSWER ABOUT WHAT WAS ADDED AFTER IT WAS
/// GENERATED: an unregenerated survey answered "0 still to transcribe"
/// about a question whose real answer had grown to 103 gaps. So it carries
/// the ROSTER SIZE it was joined against and this test compares it to the
/// live roster. The ceiling counts rows needing a CLASSIFICATION pass as
/// well as real gaps, because the honest number includes deciding.
#[test]
fn the_weapon_exclusive_mods_we_still_owe_only_goes_down() {
    // WHAT THE ROSTER STILL OWES, and the number only goes DOWN by
    // transcribing a card. A survey re-run can find MORE than it did last
    // time — the roster grows underneath it and a weapon added after the
    // last run can only ever be absent — so raising this line is a
    // deliberate edit whose reason goes in the commit, never the way to
    // make a red run green.
    const OWED: usize = 100;
    let text = crate::data::file("surveys/weapon_exclusive_mods.yaml")
        .expect("data/surveys/weapon_exclusive_mods.yaml — run scripts/survey_weapon_mods.py");
    let mut total = 0usize;
    let mut missing: Vec<&str> = Vec::new();
    let mut excluded: Vec<&str> = Vec::new();
    let mut unreasoned: Vec<&str> = Vec::new();
    let mut name = "";
    for line in text.lines() {
        let l = line.trim();
        if let Some(v) = l.strip_prefix("- name:") {
            name = v.trim();
            total += 1;
        } else if let Some(v) = l.strip_prefix("carried:") {
            match v.trim() {
                "~" => missing.push(name),
                "excluded" => {
                    excluded.push(name);
                    unreasoned.push(name);
                }
                _ => {}
            }
        } else if l.starts_with("reason:") {
            unreasoned.retain(|n| *n != name);
        }
    }
    assert!(total >= 20, "the survey looks empty: {total} rows");
    // …AND IT IS THE CURRENT ROSTER'S ANSWER. A generated file cannot know
    // about a weapon added after it was generated, so the file says which
    // roster it was joined against and this compares it to the live one.
    // Without it the ratchet above is a ratchet on a snapshot: it sat at
    // zero for thirteen days while the real gap grew to 103.
    let roster = text
        .lines()
        .find_map(|l| l.strip_prefix("roster:")?.trim().parse::<usize>().ok())
        .expect("the survey records the roster it was joined against — re-run the script");
    // DISTINCT NAMES, which is what the survey joins on: `basmu` and
    // `basmu_beam` are two entries and one weapon to `compatName`, so
    // counting entries would make the two numbers permanently unequal and
    // the guard permanently red — which is the same as no guard.
    let now = crate::data::weapons::all()
        .iter()
        .map(|w| w.name.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    assert_eq!(
        roster, now,
        "the survey was joined against {roster} weapon files and there are {now} now —              re-run scripts/survey_weapon_mods.py"
    );
    // EQUALITY, not a ceiling: at zero there is nowhere below to drift,
    // so the two directions collapse into one assertion — a mod appearing
    // fails it, and so does a mod being deleted without this line moving.
    assert_eq!(
        missing.len(),
        OWED,
        "{} weapon-exclusive mods missing, ceiling {OWED} — transcribe one or \
             raise this line deliberately:\n  {}",
        missing.len(),
        missing.join("\n  ")
    );
    // A REFUSAL IS NOT A SHORTCUT. `excluded` takes a mod out of the gap
    // count, so it must cost a written reason — otherwise the cheapest way
    // to close the ratchet is to declare everything out of scope.
    assert!(
        unreasoned.is_empty(),
        "excluded without a reason: {}",
        unreasoned.join(", ")
    );
    // Every mod that is neither carried nor excluded is missing, so these
    // three have to add up — a row the parser skipped would otherwise read
    // as one nobody owes.
    let carried = total - missing.len() - excluded.len();
    assert!(carried >= 13, "only {carried} of {total} carried");
}
