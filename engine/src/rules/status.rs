// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ordinary proc generation and scene application projections (MECHANICS §6).
//! N07A binds chance/CDF arithmetic and counters; the scene supplies handlers.

mod packet;
pub mod ordinary;
pub mod radial;
pub use packet::{ProcCounters, ProcType};

use crate::rules::damage::{DamageType, DamageVector};
use crate::rules::rng::Rng;

/// Scene-selected handlers; Void/Tau alternatives do not carry guessed native ids.
#[derive(Debug, Clone, Copy)]
pub struct SceneProcHandlers {
    pub ordinary_mask: u32,
    pub void: bool,
    pub tau: bool,
}

#[wfsim_gas_macro::metered]
impl SceneProcHandlers {
    pub fn all() -> Self { Self { ordinary_mask: ProcCounters::ORDINARY_MASK, void: true, tau: true } }

    /// Wiki scene eligibility fallback, pending native per-enemy graph bindings.
    pub fn from_scene_immunity(immune: &[DamageType]) -> Self {
        let mut handlers = Self::all();
        for &kind in immune {
            if let Some(kind) = ProcType::from_damage_type(kind) { handlers.ordinary_mask &= !(1 << kind.id()); }
        }
        handlers.void = !immune.contains(&DamageType::Void);
        handlers.tau = !immune.contains(&DamageType::Tau);
        handlers
    }
}

/// MagneticDamageProc's effective child coefficient is float32(0.005).
/// M8 native pool fixtures: multiply pool then count in f32, truncate once.
#[wfsim_gas_macro::metered]
pub fn magnetic_break_source(maximum_pool: f64, stacks: usize) -> f64 {
    ((maximum_pool as f32 * 0.005_f32) * stacks.min(10) as f32).trunc() as f64
}

/// Declared proxy values used before Magnetic's body-part classification (M8).
#[derive(Debug, Clone, Copy, serde::Deserialize)]
pub struct HitProxy {
    pub weakness: f64,
    #[serde(default)]
    pub damage_multiplier: Option<f64>,
}

/// The first strictly greatest positive weakness wins the native selection.
#[wfsim_gas_macro::metered]
pub fn magnetic_break_proxy(proxies: &[HitProxy]) -> Option<HitProxy> {
    let mut selected = None;
    let mut greatest = 0.0_f32;
    for proxy in proxies {
        let weakness = proxy.weakness as f32;
        if weakness > greatest {
            greatest = weakness;
            selected = Some(*proxy);
        }
    }
    selected
}

/// M8: normalize and round the packet, then apply the selected generic modifier.
#[wfsim_gas_macro::metered]
pub fn magnetic_break_proxy_source(source: f64, weapon_weakness_bonus: f64, proxy: HitProxy) -> f64 {
    let Some(damage_multiplier) = proxy.damage_multiplier else { return source; };
    (magnetic_break_normalized_source(source, weapon_weakness_bonus, proxy) * damage_multiplier as f32) as f64
}

/// The parent query consumes the rounded source before the generic proxy modifier.
#[wfsim_gas_macro::metered]
pub fn magnetic_break_normalized_source(source: f64, weapon_weakness_bonus: f64, proxy: HitProxy) -> f32 {
    if proxy.damage_multiplier.is_none() { return source as f32; }
    let divisor = 1.0_f32 + (weapon_weakness_bonus as f32).max(0.0) * proxy.weakness as f32;
    ((source as f32 / divisor) + 0.5_f32).floor()
}


/// Scene projection: selected default13 graph plus separate Void and Tau statuses.
/// Native handler installation and special proc ids remain unbound (N07A).
#[wfsim_gas_macro::metered]
fn scene_weights(vector: &DamageVector, handlers: SceneProcHandlers) -> [f32; 32] {
    let mut weights = [0.0; 32];
    for (index, weight) in weights.iter_mut().enumerate().take(13) {
        if handlers.ordinary_mask & (1 << index) == 0 { continue; }
        // One explicit component and unit WeightMultiplier reduce to its positive value.
        let kind = ProcType::new(index as u8).expect("ordinary index").damage_type().expect("ordinary type");
        let component = vector.get(kind) as f32;
        if component > 0.0 { *weight = component; }
    }
    // Scene alternatives stay outside ProcCounters; their native proc ids are unbound.
    if handlers.void { weights[30] = (vector.get(DamageType::Void) as f32).max(0.0); }
    if handlers.tau { weights[31] = (vector.get(DamageType::Tau) as f32).max(0.0); }
    weights
}

/// Raw generated statuses. Immunity is checked at application, after counters exist.
/// Forced entries retain scene order; accepted ordinary draws saturate per type.
#[wfsim_gas_macro::metered]
pub fn procs_for_hit(forced: &[DamageType], status_chance: f64,
    vector: &DamageVector, handlers: SceneProcHandlers, rng: &mut Rng) -> Vec<DamageType>
{
    let mut procs = forced.to_vec();
    let mut counters = ProcCounters::from_types(forced.iter().copied());
    let special = [DamageType::Void, DamageType::Tau];
    let mut special_counts = special.map(|kind| forced.iter().filter(|&&ty| ty == kind).count().min(255));
    ordinary::sample(status_chance as f32, 0, || scene_weights(vector, handlers), || rng.unit(), |index| {
        let Some(index) = index else { return; };
        if index >= 30 {
            let count = &mut special_counts[index - 30];
            if *count < 255 { procs.push(special[index - 30]); *count += 1; }
        } else {
            let kind = ProcType::new(index as u8).expect("proc index");
            if counters.count(kind) < 255 {
                counters.add(kind, 1);
                if let Some(dtype) = kind.damage_type() { procs.push(dtype); }
            }
        }
    });
    procs
}

/// Scene consumers that react to landed statuses share the final-immunity filter.
#[wfsim_gas_macro::metered]
pub fn applied_procs<'a>(generated: &'a [DamageType], immune: &'a [DamageType])
    -> impl Iterator<Item = DamageType> + 'a
{
    generated.iter().copied().filter(|kind| !immune.contains(kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_counts_have_the_native_mean_and_replacement() {
        let vector = DamageVector::new().with(DamageType::Heat, 1.0);
        let mut rng = Rng::new(7);
        let mut count = 0;
        for _ in 0..100_000 {
            let procs = procs_for_hit(&[], 1.4, &vector, SceneProcHandlers::all(), &mut rng);
            assert!(procs.len() == 1 || procs.len() == 2);
            assert!(procs.iter().all(|&kind| kind == DamageType::Heat));
            count += procs.len();
        }
        assert!((count as f64 / 100_000.0 - 1.4).abs() < 0.01);
    }

    #[test]
    fn final_immunity_does_not_renormalize_generated_weights() {
        let vector = DamageVector::new().with(DamageType::Radiation, 75.0).with(DamageType::Slash, 25.0);
        let mut rng = Rng::new(0xd3e0);
        let (mut accepted, mut radiation) = (0, 0);
        for _ in 0..40_000 {
            let raw = procs_for_hit(&[], 1.0, &vector, SceneProcHandlers::all(), &mut rng);
            radiation += usize::from(raw.contains(&DamageType::Radiation));
            accepted += applied_procs(&raw, &[DamageType::Radiation]).count();
        }
        assert!((29_500..30_500).contains(&radiation));
        assert!((9_500..10_500).contains(&accepted));
    }

    #[test]
    fn forced_raw_counters_survive_immunity_without_consuming_rng() {
        let mut rng = Rng::new(1);
        let raw = procs_for_hit(&[DamageType::Radiation], 0.0, &DamageVector::new(), SceneProcHandlers::all(), &mut rng);
        assert_eq!(raw, [DamageType::Radiation]);
        assert_eq!(rng.state(), 1);
        assert_eq!(applied_procs(&raw, &[DamageType::Radiation]).count(), 0);
    }

    #[test]
    fn zero_weights_still_consume_the_successful_type_draw() {
        let mut actual = Rng::new(1);
        assert!(procs_for_hit(&[], 2.0, &DamageVector::new(), SceneProcHandlers::all(), &mut actual).is_empty());
        let mut expected = Rng::new(1);
        for _ in 0..4 { expected.unit(); }
        assert_eq!(actual.state(), expected.state());
    }

    #[test]
    fn void_uses_a_separate_scene_alternative() {
        let mut rng = Rng::new(3);
        let raw = procs_for_hit(&[], 1.0, &DamageVector::new().with(DamageType::Void, 5.0), SceneProcHandlers::all(), &mut rng);
        assert_eq!(raw, [DamageType::Void]);
        assert_eq!(ProcCounters::from_types(raw).mask(), 0);
    }
}

#[cfg(test)]
mod magnetic_native {
    #[test]
    fn magnetic_proxy_source_matches_native_entry_and_block_replays() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/client/magnetic-proxy-inputs.json"
        )).unwrap();
        for case in fixture["source_cases"].as_array().unwrap() {
            let multiplier = if case["proxy_modifier_enabled"].as_bool().unwrap() {
                case["proxy_damage_attenuation"].as_f64().unwrap()
            } else { 1.0 };
            let proxy = super::HitProxy {
                weakness: case["proxy_weakness"].as_f64().unwrap(),
                damage_multiplier: Some(multiplier),
            };
            assert_eq!(super::magnetic_break_proxy_source(
                case["integer_source"].as_f64().unwrap(),
                case["weapon_weakness_bonus"].as_f64().unwrap(), proxy),
                case["ordinary_child_input"].as_f64().unwrap(), "{case}");
        }
    }

    #[test]
    fn magnetic_proxy_selection_keeps_the_first_positive_maximum() {
        let proxy = |weakness, multiplier| super::HitProxy { weakness, damage_multiplier: Some(multiplier) };
        assert!(super::magnetic_break_proxy(&[proxy(0.0, 3.0), proxy(-1.0, 4.0)]).is_none());
        let selected = super::magnetic_break_proxy(&[proxy(0.05, 1.0), proxy(1.0, 2.0), proxy(1.0, 4.0)]).unwrap();
        assert_eq!(selected.damage_multiplier, Some(2.0));
        assert_eq!(super::magnetic_break_proxy_source(5181.0, -1.0, selected), 10362.0);
        assert_eq!(super::magnetic_break_proxy_source(5181.0, 0.2, selected), 8636.0);
    }

    #[test]
    fn magnetic_pool_source_matches_original_byte_replay_vectors() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/client/magnetic-break-source.json"
        )).unwrap();
        for case in fixture["cases"].as_array().unwrap() {
            assert_eq!(
                super::magnetic_break_source(case["pool"].as_f64().unwrap(),
                    case["stacks"].as_u64().unwrap() as usize),
                case["expected"].as_f64().unwrap(), "{case}",
            );
        }
    }
}
