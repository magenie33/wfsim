// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ordinary proc generation and scene application projections (MECHANICS §6).
//! N07A binds chance/CDF arithmetic and counters; the scene supplies handlers.

mod packet;
pub mod ordinary;
pub mod radial;
mod proc_draw;
pub use packet::{ProcCounters, ProcType};
pub(crate) use packet::ORDINARY_DAMAGE_TYPES;
pub(crate) use proc_draw::SceneProcDraw;

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


pub(crate) const SCENE_CDF_CAP: usize = 15;

#[cfg(test)]
#[wfsim_gas_macro::metered]
pub(crate) fn scene_weights(vector: &DamageVector, handlers: SceneProcHandlers) -> [f32; 32] {
    let mut weights = [0.0; 32];
    for (index, weight) in weights.iter_mut().enumerate().take(13) {
        if handlers.ordinary_mask & (1 << index) == 0 { continue; }
        let kind = ORDINARY_DAMAGE_TYPES[index];
        let component = vector.get(kind) as f32;
        if component > 0.0 { *weight = component; }
    }
    if handlers.void { weights[30] = (vector.get(DamageType::Void) as f32).max(0.0); }
    if handlers.tau { weights[31] = (vector.get(DamageType::Tau) as f32).max(0.0); }
    weights
}

#[inline]
#[wfsim_gas_macro::metered]
pub(crate) fn build_scene_cdf(
    vector: &DamageVector,
    handlers: SceneProcHandlers,
    entries: &mut [(DamageType, f32); SCENE_CDF_CAP],
) -> (usize, f32) {
    let mut len = 0usize;
    let mut total = 0.0_f32;
    let mut mask = handlers.ordinary_mask & ProcCounters::ORDINARY_MASK;
    while mask != 0 {
        let index = mask.trailing_zeros() as usize;
        let dtype = ORDINARY_DAMAGE_TYPES[index];
        let component = vector.get(dtype) as f32;
        if component > 0.0 {
            total += component;
            entries[len] = (dtype, total);
            len += 1;
        }
        mask &= mask - 1;
    }
    if handlers.void {
        let component = vector.get(DamageType::Void) as f32;
        if component > 0.0 {
            total += component;
            entries[len] = (DamageType::Void, total);
            len += 1;
        }
    }
    if handlers.tau {
        let component = vector.get(DamageType::Tau) as f32;
        if component > 0.0 {
            total += component;
            entries[len] = (DamageType::Tau, total);
            len += 1;
        }
    }
    (len, total)
}

/// Raw generated statuses. Immunity is checked at application, after counters exist.
/// Forced entries retain scene order; accepted ordinary draws saturate per type.
#[wfsim_gas_macro::metered]
pub fn procs_for_hit(forced: &[DamageType], status_chance: f64,
    vector: &DamageVector, handlers: SceneProcHandlers, rng: &mut Rng) -> HitProcs
{
    let mut procs = HitProcs::from_slice(forced);
    let chance = status_chance as f32;
    if chance <= 0.0 {
        return procs;
    }
    if cannot_saturate(forced, chance) {
        let mut entries = [(DamageType::Impact, 0.0_f32); SCENE_CDF_CAP];
        let mut len = 0usize;
        let mut total = 0.0_f32;
        let mut built = false;
        let mut remaining = chance;
        while remaining > 0.0 {
            if rng.unit() > remaining { break; }
            if !built {
                (len, total) = build_scene_cdf(vector, handlers, &mut entries);
                built = true;
            }
            let threshold = rng.unit() * total;
            for &(dtype, cumulative) in &entries[..len] {
                if cumulative >= threshold {
                    procs.push(dtype);
                    break;
                }
            }
            remaining -= 1.0_f32;
        }
    } else {
        sample_hit_procs_saturated(&mut procs, forced, chance, vector, handlers, rng);
    }
    procs
}

#[cold]
#[wfsim_gas_macro::metered]
pub(crate) fn sample_hit_procs_saturated(
    procs: &mut impl ProcSink,
    forced: &[DamageType],
    chance: f32,
    vector: &DamageVector,
    handlers: SceneProcHandlers,
    rng: &mut Rng,
) {
    let mut counts = [0u8; DamageType::ALL.len()];
    for &ty in forced {
        let slot = &mut counts[ty as usize];
        *slot = slot.saturating_add(1);
    }
    let mut entries = [(DamageType::Impact, 0.0_f32); SCENE_CDF_CAP];
    let mut len = 0usize;
    let mut total = 0.0_f32;
    let mut built = false;
    let mut remaining = chance;
    while remaining > 0.0 {
        if rng.unit() > remaining { break; }
        if !built {
            (len, total) = build_scene_cdf(vector, handlers, &mut entries);
            built = true;
        }
        let threshold = rng.unit() * total;
        for &(dtype, cumulative) in &entries[..len] {
            if cumulative >= threshold {
                let slot = &mut counts[dtype as usize];
                if *slot < 255 {
                    *slot += 1;
                    procs.push_proc(dtype);
                }
                break;
            }
        }
        remaining -= 1.0_f32;
    }
}

#[inline]
#[wfsim_gas_macro::metered]
pub(crate) fn cannot_saturate(forced: &[DamageType], chance: f32) -> bool {
    forced.len() <= 255 && chance <= (255 - forced.len()) as f32
}

/// Scene consumers that react to landed statuses share the final-immunity filter.
#[wfsim_gas_macro::metered]
pub fn applied_procs<'a>(generated: &'a [DamageType], immune: &'a [DamageType])
    -> impl Iterator<Item = DamageType> + 'a
{
    generated.iter().copied().filter(|kind| !immune.contains(kind))
}

#[inline]
#[wfsim_gas_macro::metered]
pub fn applied_procs_count(generated: &[DamageType], immune: &[DamageType]) -> usize {
    if immune.is_empty() {
        return generated.len();
    }
    let mut n = 0usize;
    for kind in generated {
        if !immune.contains(kind) {
            n += 1;
        }
    }
    n
}

#[inline]
#[wfsim_gas_macro::metered]
pub fn has_applied_proc(generated: &[DamageType], immune: &[DamageType]) -> bool {
    if immune.is_empty() {
        return !generated.is_empty();
    }
    for kind in generated {
        if !immune.contains(kind) {
            return true;
        }
    }
    false
}

const INLINE_PROCS_CAP: usize = 16;

#[derive(Debug, Clone)]
pub enum HitProcs {
    Inline { buf: [DamageType; INLINE_PROCS_CAP], len: u8 },
    Heap(Vec<DamageType>),
}

#[wfsim_gas_macro::metered]
impl Default for HitProcs {
    #[inline]
    fn default() -> Self {
        Self::Inline { buf: [DamageType::Impact; INLINE_PROCS_CAP], len: 0 }
    }
}

#[wfsim_gas_macro::metered]
impl HitProcs {
    #[inline]
    pub fn from_slice(forced: &[DamageType]) -> Self {
        if forced.len() <= INLINE_PROCS_CAP {
            let mut buf = [DamageType::Impact; INLINE_PROCS_CAP];
            buf[..forced.len()].copy_from_slice(forced);
            Self::Inline { buf, len: forced.len() as u8 }
        } else {
            Self::Heap(forced.to_vec())
        }
    }

    #[inline]
    pub fn push(&mut self, ty: DamageType) {
        if let Self::Inline { buf, len } = self {
            let l = *len as usize;
            if l < INLINE_PROCS_CAP {
                buf[l] = ty;
                *len += 1;
                return;
            }
        }
        self.push_slow(ty);
    }

    #[cold]
    fn push_slow(&mut self, ty: DamageType) {
        match self {
            Self::Inline { buf, .. } => {
                let mut v = Vec::with_capacity(INLINE_PROCS_CAP + 8);
                v.extend_from_slice(buf);
                v.push(ty);
                *self = Self::Heap(v);
            }
            Self::Heap(v) => v.push(ty),
        }
    }
}

#[wfsim_gas_macro::metered]
impl std::ops::Deref for HitProcs {
    type Target = [DamageType];
    #[inline]
    fn deref(&self) -> &[DamageType] {
        match self {
            Self::Inline { buf, len } => &buf[..*len as usize],
            Self::Heap(v) => v.as_slice(),
        }
    }
}

#[wfsim_gas_macro::metered]
impl PartialEq for HitProcs {
    fn eq(&self, other: &Self) -> bool {
        (**self) == (**other)
    }
}

#[wfsim_gas_macro::metered]
impl PartialEq<[DamageType]> for HitProcs {
    fn eq(&self, other: &[DamageType]) -> bool {
        (**self) == *other
    }
}

#[wfsim_gas_macro::metered]
impl PartialEq<Vec<DamageType>> for HitProcs {
    fn eq(&self, other: &Vec<DamageType>) -> bool {
        (**self) == *other.as_slice()
    }
}

#[wfsim_gas_macro::metered]
impl<const N: usize> PartialEq<[DamageType; N]> for HitProcs {
    fn eq(&self, other: &[DamageType; N]) -> bool {
        (**self) == *other.as_slice()
    }
}

#[wfsim_gas_macro::metered]
impl PartialEq<HitProcs> for &[DamageType] {
    fn eq(&self, other: &HitProcs) -> bool {
        **self == **other
    }
}

pub enum HitProcsIter {
    Inline { buf: [DamageType; INLINE_PROCS_CAP], pos: u8, len: u8 },
    Heap(std::vec::IntoIter<DamageType>),
}

#[wfsim_gas_macro::metered]
impl Iterator for HitProcsIter {
    type Item = DamageType;
    fn next(&mut self) -> Option<DamageType> {
        match self {
            Self::Inline { buf, pos, len } => {
                if *pos < *len {
                    let item = buf[*pos as usize];
                    *pos += 1;
                    Some(item)
                } else {
                    None
                }
            }
            Self::Heap(it) => it.next(),
        }
    }
}

#[wfsim_gas_macro::metered]
impl IntoIterator for HitProcs {
    type Item = DamageType;
    type IntoIter = HitProcsIter;
    fn into_iter(self) -> HitProcsIter {
        match self {
            Self::Inline { buf, len } => HitProcsIter::Inline { buf, pos: 0, len },
            Self::Heap(v) => HitProcsIter::Heap(v.into_iter()),
        }
    }
}

#[wfsim_gas_macro::metered]
pub trait ProcSink: std::ops::Deref<Target = [DamageType]> {
    fn push_proc(&mut self, ty: DamageType);
}

#[wfsim_gas_macro::metered]
impl ProcSink for Vec<DamageType> {
    #[inline]
    fn push_proc(&mut self, ty: DamageType) {
        self.push(ty);
    }
}

#[wfsim_gas_macro::metered]
impl ProcSink for HitProcs {
    #[inline]
    fn push_proc(&mut self, ty: DamageType) {
        self.push(ty);
    }
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
