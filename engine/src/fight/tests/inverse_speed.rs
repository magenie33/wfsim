use super::*;
use crate::model::RefireRule;

fn fixture() -> serde_json::Value {
    serde_json::from_str(include_str!("../../../../tests/fixtures/client/inverse-speed-2026-10-28.json")).unwrap()
}

fn source_bits(value: f64, row: &serde_json::Value) {
    let bits = (value as f32).to_le_bytes().map(|b| format!("{b:02x}")).join("");
    assert_eq!(bits, row["bits_le"].as_str().unwrap(), "{row}: {value}");
    assert_eq!(value, (value as f32) as f64, "native result must be widened after its last operation: {row}");
}

#[test]
fn inverse_speed_consumers_match_their_scoped_native_vectors() {
    let f = fixture();
    assert_eq!(f["source"]["outer_files_verified"], 4);
    assert_eq!(f["source"]["members_verified"], 206);
    let rows = f["verification"]["vectors"].as_array().unwrap();
    assert_eq!(rows.len(), 322);
    let (mut applied, mut retained) = (0, 0);
    for row in rows {
        let case = row["case"].as_str().unwrap();
        let x = row["stacking_bonus"].as_f64().unwrap() as f32;
        let scalar = 1.0_f32 + x;
        let result = match case {
            "state-rps" | "base-interval" | "semi-interval" => {
                let rpm = row["raw_rpm"].as_f64().unwrap() as f32;
                if rpm <= 0.0 {
                    retained += 1;
                    continue; // A zero configured rate needs state permissions, not an instant loop.
                }
                let configured = f64::from(rpm) / 60.0;
                let rate = RefireRule::NativeRate.weapon_rate(
                    RefireRule::NativeRate.modified_rate(configured, f64::from(x)), configured);
                match case {
                    "state-rps" => rate,
                    "base-interval" => RefireRule::NativeRate.interval(rate, false),
                    _ => RefireRule::SemiAutomatic { skip_wait_on_empty: false }.interval(rate, false),
                }
            }
            "reload-duration" => crate::rules::sim::reload_time(2.0, f64::from(x)),
            "weapon-charge-duration" => crate::rules::sim::charge_time(2.0, f64::from(x)),
            "projectile-initial-speed-clamp" => {
                let base = row["base_speed"].as_f64().unwrap() as f32;
                crate::rules::sim::projectile_initial_speed(f64::from(base), f64::from(base * scalar))
            }
            "burst-intershot-duration" => {
                let p = FightParams { fire_rate: f64::from(scalar), unmodded_fire_rate: 1.0,
                    refire: RefireRule::NativeRate,
                    burst: Some(crate::model::BurstSpec { count: 2, delay_seconds: f64::from(0.2_f32) }),
                    ..no_status() };
                let (_, mut me) = open(&p, Seat::WIELDER, &mut Rng::new(7), &crate::record::Record::off(), &None);
                me.ammo.rounds_this_mag = 1;
                seconds_to_next_shot(&p, 0.0, p.refire.weapon_rate(p.fire_rate, 1.0), p.fire_rate,
                    0.0, None, false, false, &me.ammo, &me.incarnon, &mut me.melee)
            }
            "weapon-rate-query" | "heavy-charge-duration-window" | "positive-weapon-override-bypasses-upgrade" => {
                retained += 1;
                continue;
            }
            _ => panic!("unreviewed native vector: {case}"),
        };
        source_bits(result, row);
        applied += 1;
    }
    assert_eq!((applied, retained), (255, 67));
}

#[test]
fn default_negative_rate_uses_the_inner_floor_without_changing_the_outer_floor() {
    let p = FightParams { fire_rate: -1.0, unmodded_fire_rate: 10.0,
        refire: RefireRule::NativeRate, no_magazine: true, ammo_cost: 0.0,
        duration_seconds: 45.0, ..no_status() };
    assert_eq!(run_once(&p, &mut Rng::new(7)).shots, 3);
    assert_eq!(p.refire.interval(p.refire.weapon_rate(-1.0, 10.0), false), f64::from(19.999998_f32));
    assert_eq!(p.refire.clock_rate(-1.0), f64::from(1.0_f32 / 60.0), "outer state getter remains distinct");
    assert_eq!(p.refire.weapon_rate(-1.0, 1.0 / 60.0), p.refire.clock_rate(-1.0));
}

#[test]
fn a_live_charge_buff_offsets_raw_negative_speed_before_the_charge_floor() {
    let base = crate::model::WeaponBase::from_data("lanka", false, &[]);
    let plain = crate::build::loadout::resolve(&base, &[], crate::model::StackPolicy::Emergent);
    let mut penalty = crate::data::mods::pool_for_weapon("lanka").into_iter()
        .find(|m| m.id == "speed_trigger").unwrap();
    penalty.effects = vec![crate::model::ModEffect::FireRate(-1.5),
        crate::model::ModEffect::ReloadSpeed(-1.5),
        crate::model::ModEffect::Indirect(crate::model::IndirectStat::ProjectileSpeed, -1.5)];
    let panel = crate::build::loadout::resolve(&base, &[&penalty], crate::model::StackPolicy::Emergent);
    let p = FightParams::from_panel(&panel, &crate::arena::Arena::training(45.0), &ArcaneFx::none());
    assert!(p.charge_speed.unwrap().multiplier < 0.0);
    let original = plain.charge_seconds.unwrap();
    assert!(crate::rules::upgrades::near_f32(p.charge_seconds.unwrap() * crate::rules::sim::charge_divisor(-0.5), original));
    assert!(crate::rules::upgrades::near_f32(p.reload_seconds * crate::rules::sim::reload_divisor(-0.5), plain.reload_seconds));
    assert!(crate::rules::upgrades::near_f32(reload_span(p.reload_seconds, p.reload_bonus, 1.0), 2.0 * plain.reload_seconds));
    assert_eq!(p.projectile_speed_mps, Some(f64::from(plain.projectile_speed_mps.unwrap() as f32 * 0.1_f32)));
    let raw = p.fire_rate + p.unmodded_fire_rate;
    let restored = charge_span_at_rate(&p, raw, p.refire.weapon_rate(raw, p.unmodded_fire_rate));
    assert!(crate::rules::upgrades::near_f32(restored, 2.0 * original), "raw -50% plus +100% leaves a 50% scalar query");
}
