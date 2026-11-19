// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use crate::rivens::mod_pool_with_rivens;

fn request(roll: f64) -> Value {
    json!({"weapon":"braton_prime", "duration":1, "runs":1, "seed":7,
    "mods":["riven:mine", "hellfire@4", "cryo_rounds"],
    "exilus":"vigilante_supplies@1",
    "rivens":[
        {"id":"mine", "spec":{"bonuses":[{"id":"heat","roll":roll},{"id":"damage","roll":roll}]}},
        {"name":"mine", "spec":{"positives":[{"id":"critical_damage","roll":1.1}]}}
    ]})
}

#[test]
fn the_entrant_view_agrees_with_the_owned_search_pool_in_request_order() {
    let terms = request(0.95);
    let entrant = Entrant::new(&terms, false).unwrap();
    let evos: Vec<_> = entrant.fight.evos.iter().map(String::as_str).collect();
    let owned = mod_pool_with_rivens(&terms, entrant.fight.info, &evos);
    let names = [
        "cryo_rounds",
        "riven:mine",
        "vigilante_supplies@1",
        "hellfire@4",
    ];
    let refs = entrant.seat(&names).unwrap();
    assert_eq!(refs.iter().map(|m| m.id).collect::<Vec<_>>(), names);
    for (name, from_view) in names.into_iter().zip(refs) {
        let old = owned.iter().find(|m| m.id == name).unwrap();
        assert_eq!(format!("{from_view:?}"), format!("{old:?}"));
    }
    let err = entrant.seat(&["hellfire@4", "hellfire"]).unwrap_err();
    assert!(err["error"]
        .as_str()
        .unwrap()
        .contains("both in the hellfire family"));

    let unranked = Entrant::new(
        &json!({"weapon":"braton_prime", "mods":["hellfire"]}),
        false,
    )
    .unwrap();
    assert_eq!(unranked.pool.get("hellfire").unwrap().family, None);
    assert!(unranked.pool.get("hellfire@4").is_none());
}

#[test]
fn same_id_riven_definitions_never_escape_their_request() {
    let first = request(0.95);
    let second = request(1.06);
    let a = simulate_json(&first);
    let b = simulate_json(&second);
    assert_eq!(a["ok"], true, "{a}");
    assert_eq!(b["ok"], true, "{b}");
    assert_ne!(a["panel"], b["panel"]);
    assert_eq!(simulate_json(&first), a);

    let mut reversed = first.clone();
    reversed["rivens"].as_array_mut().unwrap().reverse();
    let different_first = simulate_json(&reversed);
    assert_eq!(different_first["ok"], true, "{different_first}");
    assert_ne!(different_first["panel"], a["panel"]);
    reversed["rivens"].as_array_mut().unwrap().truncate(1);
    assert_eq!(simulate_json(&reversed), different_first);
}

#[test]
fn evolution_filtered_ranks_and_unknown_ids_keep_their_refusals() {
    let mut req = json!({"weapon":"dual_toxocyst", "mode":"base", "duration":1, "runs":1, "seed":7,
        "evolutions":[], "evolutions_as_given":true, "mods":["semi_pistol_cannonade@3"]});
    let bare = Entrant::new(&req, false).unwrap();
    assert!(bare.seat(&["semi_pistol_cannonade@3"]).is_ok());
    req["evolutions"] = json!(["dual_toxocyst_evo1_incarnon_form"]);
    let evolved = Entrant::new(&req, false).unwrap();
    let error = evolved.seat(&["semi_pistol_cannonade@3"]).unwrap_err();
    assert!(error["error"]
        .as_str()
        .unwrap()
        .contains("with these evolutions installed"));
    assert_eq!(
        evolved.seat(&["riven:missing"]).unwrap_err(),
        err_json("unknown mod id: riven:missing")
    );
    assert!(bare.seat(&["semi_pistol_cannonade@3"]).is_ok());
}
