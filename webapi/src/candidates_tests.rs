// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;

// Reference: substitute into ten slots, then check the resulting families.
fn replace_then_validate(b: &Build, idx: usize) -> Vec<Value> {
    let no = forbidden(&evo_forbids(b.info), &b.evo);
    let mut pool = wfsim_engine::data::mods::pool_for_weapon(&b.info.id);
    pool.extend(crate::rivens::rivens_from(b.req, b.info));
    let family = |id: &str| {
        let card = card_of(id);
        pool.iter().find(|m| m.id == card).and_then(|m| m.family).unwrap_or(card).to_owned()
    };
    let fixed = wfsim_engine::data::weapons::spec(&b.info.id).and_then(|s| s.fixed_stance.as_deref());
    let mut out = Vec::new();
    for m in pool.iter().filter(|m| !no.iter().any(|n| n == m.id)) {
        if m.stance.is_some() != (idx == STANCE) || idx == STANCE && fixed.is_some_and(|id| id != m.id) {
            continue;
        }
        let mut ids = vec![m.id.to_string()];
        if b.every_mods.iter().any(|id| id == m.id) {
            ids.extend((0..m.max_rank).map(|r| format!("{}@{r}", m.id)));
        }
        for id in ids {
            if b.slots.iter().any(|s| s.as_ref() == Some(&id)) || idx == EXILUS && !m.exilus {
                continue;
            }
            let mut next = b.slots.clone();
            next[idx] = Some(id.clone());
            let mods: Vec<_> = next.into_iter().flatten().collect();
            let mut families: Vec<_> = mods.iter().map(|id| family(id)).collect();
            families.sort();
            families.dedup();
            if families.len() == mods.len() {
                out.push(json!({"id":id, "payload":{"mods":mods}}));
            }
        }
    }
    if idx == STANCE && b.slots[STANCE].is_some() && fixed.is_none() {
        let mods: Vec<_> = b.slots[..STANCE].iter().flatten().collect();
        out.push(json!({"id":"none", "payload":{"mods":mods}}));
    }
    out
}

#[test]
fn mod_candidates_match_ordered_reference_across_all_slots() {
    let riven = |id, roll| json!({"id":id, "spec":{"rank":4, "bonuses":[{"id":"critical_chance", "roll":roll}]}});
    let cases = [
        json!({"weapon":"torid", "slots":["hellfire",null,null,"infected_clip@4","cryo_rounds","serration@4",null,null,"vigilante_supplies"],
            "rivens":[riven("gen:a@3", 0.9),riven("gen:b", 1.1)], "every_rank":{"mods":["serration","infected_clip","hellfire"]}}),
        json!({"weapon":"ocucor", "slots":["heated_charge","riven:gen:a@3",null,"convulsion"],
            "rivens":[riven("gen:a@3", 0.9),riven("gen:b", 1.1)], "every_rank":{"mods":["heated_charge","convulsion"]}}),
        json!({"weapon":"kuva_nukor", "slots":["hornet_strike@0","hornet_strike@1",null,"unknown@4","unknown"]}),
        json!({"weapon":"magistar", "slots":["primed_pressure_point","shocking_touch",null,null,null,null,null,null,null,"crushing_ruin"]}),
        json!({"weapon":"valkyr_talons", "slots":[null,null,null,null,null,null,null,null,null,"hysteria"]}),
    ];
    for mut req in cases {
        for idx in 0..10 {
            req["axis"] = json!({"kind":"mods","idx":idx});
            let b = parse(&req).unwrap();
            let expected = replace_then_validate(&b, idx);
            assert_eq!(candidates_json(&req)["candidates"], Value::Array(expected), "{} slot {idx}", b.info.id);
        }
    }
}
