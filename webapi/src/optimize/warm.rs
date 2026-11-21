// SPDX-License-Identifier: AGPL-3.0-or-later
use serde_json::{json, Value};
use wfsim_engine::build::rivens::RolledStat;
use wfsim_engine::fight::Shard;
use wfsim_optimizer::Candidate;

use super::{carry::Carry, Replay};

const VERSION: u8 = 1;

pub(super) struct OrderIdentity {
    pub(super) key: String,
    witness: String,
}

impl OrderIdentity {
    fn binding(&self) -> (&str, &str) {
        (&self.key, &self.witness)
    }
}

impl Replay {
    // The namespace owns scenario-wide inputs. Prepared scoring seats mod_ids,
    // including the ordered contents of fixed layouts, but never sees c.slots.
    pub(super) fn warm_identity(&self, c: &Candidate, ai: usize, weapon: &str, valence_bonus: f64, seed: u64) -> OrderIdentity {
        let mut axes = self.build_axes(c, ai);
        let stat = |s: &RolledStat| json!([s.id, s.roll]);
        if !self.riven_specs.is_empty() {
            for card in axes["mods"].as_array_mut().expect("ordered cards") {
                // NamedPool seats the first entry for a request-local id.
                if let Some((_, spec)) = self.riven_specs.iter().find(|(id, _)| Some(*id) == card.as_str()) {
                    let bonuses: Vec<_> = spec.bonuses.iter().map(stat).collect();
                    *card = json!(["riven", spec.class, spec.rank, bonuses, spec.malus.as_ref().map(stat)]);
                }
            }
        }
        let valence = wfsim_engine::data::weapons::valence_of(weapon)
            .map(|spec| valence_bonus.clamp(spec.min, spec.max));
        let witness = json!([VERSION, "quick-paired", weapon, axes, valence, seed]).to_string();
        let digest = wfsim_engine::board::builds::identity_id(&witness);
        OrderIdentity { key: format!("q{VERSION}:{digest}"), witness }
    }
}

pub(super) fn pack(id: &OrderIdentity, shard: Shard) -> String {
    shard.pack_search_with_identity(Some(id.binding()))
}

pub(super) fn prefix(warm: Option<&Value>, carried: Option<&Value>, id: &OrderIdentity, seed: u64, target: u32) -> Option<Shard> {
    let warm = warm.and_then(Value::as_str).and_then(|s| Shard::unpack_search_with_identity(s, Some(id.binding())))
        .filter(|s| s.is_search_prefix(seed, target));
    if warm.as_ref().is_some_and(|s| s.runs == target) { return warm; }
    let carried = carried.and_then(Carry::read).and_then(Carry::open)
        .filter(|s| s.is_search_prefix(seed, target));
    match (warm, carried) {
        (Some(warm), Some(carried)) if carried.runs > warm.runs => Some(carried),
        (warm, carried) => warm.or(carried),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warm_cache_witness_rejects_a_collision_and_key_only_receipts() {
        let seed = 0xDEAD_BEEF;
        let mut data = serde_json::to_value(Shard::default()).unwrap();
        data["runs"] = json!(1);
        data["min"] = json!(0.0);
        data["max"] = json!(0.0);
        data["index"] = json!([{"v": 0.0, "hi": 0, "lo": seed}]);
        let shard: Shard = serde_json::from_value(data).unwrap();
        let expected = OrderIdentity { key: "q1:same-bucket".into(), witness: "ordered build A".into() };
        let collision = OrderIdentity { key: expected.key.clone(), witness: "ordered build B".into() };
        let receipt = json!(pack(&expected, shard.clone()));
        let collided = json!(pack(&collision, shard.clone()));
        assert!(prefix(Some(&receipt), None, &expected, seed, 1).is_some());
        assert!(prefix(Some(&collided), None, &collision, seed, 1).is_some());
        assert!(prefix(Some(&collided), None, &expected, seed, 1).is_none());
        assert!(prefix(Some(&receipt), None, &expected, seed + 1, 1).is_none());

        let mut key_only: Value = serde_json::from_str(receipt.as_str().unwrap()).unwrap();
        key_only[2].as_object_mut().unwrap().remove("warm_identity");
        let key_only = json!(key_only.to_string());
        assert!(prefix(Some(&key_only), None, &expected, seed, 1).is_none());
        let legacy = json!(shard.pack_search());
        assert!(prefix(Some(&collided), Some(&legacy), &expected, seed, 1).is_some());
    }
}
