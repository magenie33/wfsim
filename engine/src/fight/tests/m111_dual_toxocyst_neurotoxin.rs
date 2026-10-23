//! MEASUREMENTS M111 — Neurotoxin joins Frenzy's Toxin, Frenzy's Toxin is up
//! only while Frenzy is, and a ricochet's head arms it.
use super::*;

const FEVERED: &str = "dual_toxocyst_fevered_frenzy";
const NEUROTOXIN: &str = "dual_toxocyst_neurotoxin";

fn panel(form: &str, evos: &[&str]) -> crate::build::loadout::ResolvedPanel {
    let base = crate::model::WeaponBase::from_data(form, true, evos);
    crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent)
}

/// The reading: unmodded, Fevered Frenzy, both up, a body shot — 336, which
/// is 335.94 quantized. Frenzy alone is 250 and the resting hit is the panel.
#[test]
fn neurotoxin_adds_seventy_percent_toxin_beside_frenzy() {
    for form in ["dual_toxocyst", "dual_toxocyst_incarnon"] {
        let both = panel(form, &[FEVERED, NEUROTOXIN]);
        let frenzy = panel(form, &[FEVERED]);
        let q = |p: &crate::build::loadout::ResolvedPanel, v: &DamageVector| {
            v.quantized_against(p.modified_base).total()
        };
        assert!((q(&both, &both.damage) - 335.9375).abs() < 1e-9, "{form}: {}", q(&both, &both.damage));
        assert!((both.damage.get(DamageType::Toxin) - 212.5).abs() < 1e-9);
        assert!((q(&frenzy, &frenzy.damage) - 250.0).abs() < 1e-9, "{form}");
        let resting = both.resting.as_ref().expect("a Frenzy panel has a resting hit");
        assert_eq!(resting.damage.get(DamageType::Toxin), 0.0);
        assert!((q(&both, &resting.damage) - 125.0).abs() < 1e-9, "{form}");
    }
}

/// …AND IT ADDS, rather than multiplying the Toxin: with a maxed Pathogen
/// Rounds the body shot reads 449 — `125 x (0.9 + 1.0 + 0.7)` = 325 Toxin,
/// quantized to 324.22, beside the 125. Multiplying would read about 529.
#[test]
fn neurotoxin_adds_beside_a_toxin_mod() {
    let pathogen = crate::data::mods::pistol_pool()
        .into_iter()
        .find(|m| m.id == "pathogen_rounds")
        .expect("Pathogen Rounds");
    for form in ["dual_toxocyst", "dual_toxocyst_incarnon"] {
        let base = crate::model::WeaponBase::from_data(form, true, &[FEVERED, NEUROTOXIN]);
        let p = crate::build::loadout::resolve(&base, &[&pathogen], crate::model::StackPolicy::Emergent);
        assert!((p.damage.get(DamageType::Toxin) - 325.0).abs() < 1e-9, "{form}");
        let hit = p.damage.quantized_against(p.modified_base).total();
        assert!((hit - 449.21875).abs() < 1e-9, "{form}: {hit} against a measured 449");
    }
}

/// A Dual Toxocyst Incarnon on the never-dying dummy with nothing random left
/// in the hit: no status, a crit worth 1x, one body part. Fevered Frenzy's
/// twenty stacks are earned before the fight, so every shot is two pellets.
const PELLETS: f64 = 2.0;
fn dt_on_the_dummy(head: bool) -> FightParams {
    let mut p = FightParams::from_panel(
        &panel("dual_toxocyst_incarnon", &[FEVERED, NEUROTOXIN]),
        &crate::arena::Arena::training(10.0),
        &crate::data::arcanes::ArcaneFx::none(),
    );
    p.frenzy = true;
    p.status_chance = 0.0;
    p.base_status_chance = 0.0;
    p.crit_multiplier = 1.0;
    p.body_parts = if head { all_head() } else { mono_body(1.0) };
    p
}

/// FRENZY'S TOXIN IS A BUFF'S. With the body only, Frenzy never comes up and
/// every shot is the resting 125; locked up, every shot is the 336.
#[test]
fn frenzys_toxin_is_on_the_hit_only_while_frenzy_is_up() {
    let ms = PELLETS;
    let down = monte_carlo(&dt_on_the_dummy(false), 4, 1);
    assert!(
        (down.mean_damage / down.mean_shots - ms * 125.0).abs() < 1e-6,
        "{} a shot with Frenzy down",
        down.mean_damage / down.mean_shots
    );
    let mut up = dt_on_the_dummy(false);
    up.locked_buffs = vec![BuffLock::permanent(LockedBuff::Frenzy)];
    let up = monte_carlo(&up, 4, 1);
    assert!(
        (up.mean_damage / up.mean_shots - ms * 335.9375).abs() < 1e-6,
        "{} a shot with Frenzy locked up",
        up.mean_damage / up.mean_shots
    );
}

/// THE FIRST SHOT ARMS IT, it does not ride it: all heads, every shot but the
/// first carries the Toxin.
#[test]
fn the_shot_that_arms_frenzy_does_not_carry_its_toxin() {
    let p = dt_on_the_dummy(true);
    let head = p.body_parts[0].multiplier;
    let s = monte_carlo(&p, 4, 1);
    let want = PELLETS * head * (125.0 + (s.mean_shots - 1.0) * 335.9375);
    assert!((s.mean_damage - want).abs() < 1e-6, "{} against {want}", s.mean_damage);
}

/// "Ricochets can headshot and trigger Frenzy": the aimed body is only ever a
/// body shot, so a bounce is the one head in the fight, and when every bounce
/// lands on one Frenzy's x2.5 cadence shows in the shot count.
#[test]
fn a_ricochets_head_arms_frenzy() {
    let crowd = |chance: f64| {
        let mut p = dt_on_the_dummy(false);
        p.others = (1..=4)
            .map(|i| crate::formation::FoeSpec {
                id: String::new(),
                params: Foe::training_dummy(),
                body_parts: BodyPart::humanoid(),
                at: crate::rules::space::Vec2::new(i as f64 * 0.6, p.target_at.y),
            })
            .collect();
        if let Some(r) = p.ricochet.as_mut() {
            r.headshot_chance = chance;
        }
        monte_carlo(&p, 4, 1).mean_shots
    };
    let (never, always) = (crowd(0.0), crowd(1.0));
    assert!(always > never * 2.0, "Frenzy from a ricochet's head: {never} -> {always} shots");
}
