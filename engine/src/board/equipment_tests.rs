use super::equipment::{canonical, build_id};
use crate::data::{shards, warframes as wf, companions};
use serde_json::{json, Value};

fn frame() -> Value {
    let shard = &shards::all()[0];
    let mods = ["vitality", "redirection", "steel_fiber", "intensify", "continuity", "stretch", "streamline", "flow"]
        .map(|id| json!({"id":id}));
    json!({"kind":"warframe", "build":{
        "frame":"volt", "mods":mods,
        "aura":{"id":"energy_siphon"}, "exilus":{"id":"power_drift"},
        "arcanes":wf::arcanes().iter().take(2).map(|a| json!({"id":a.id})).collect::<Vec<_>>(),
        "shards":(0..5).map(|_| json!({"shard":shard.id, "effect":shard.options[0].id, "tauforged":false})).collect::<Vec<_>>()
    }})
}

#[test]
fn tau_and_permutations_have_one_identity() {
    let mut source = frame();
    let normal = canonical(&source).unwrap();
    assert!(normal["build"]["shards"].as_array().unwrap().iter().all(|s| s["tauforged"] == true));
    source["build"]["mods"].as_array_mut().unwrap().reverse();
    source["build"]["arcanes"].as_array_mut().unwrap().reverse();
    for shard in source["build"]["shards"].as_array_mut().unwrap() { shard["tauforged"] = json!(true); }
    source["build"]["operator"] = json!({"school":"vazarin"});
    assert_eq!(canonical(&source).unwrap(), normal);
    assert_eq!(canonical(&normal).unwrap(), normal, "canonicalization is idempotent");
    assert_eq!(build_id(&normal).len(), 32);
}

#[test]
fn every_required_position_is_checked() {
    for field in ["mods", "arcanes", "shards"] {
        let mut source = frame(); source["build"][field].as_array_mut().unwrap().pop();
        assert!(canonical(&source).is_err(), "{field}");
    }
    for field in ["aura", "exilus"] {
        let mut source = frame(); source["build"][field] = Value::Null;
        assert!(canonical(&source).is_err(), "{field}");
    }
    assert!(canonical(&frame()).is_ok(), "no derived weapon or infusion is required");
}

#[test]
fn ranks_choices_and_infusion_keep_their_meaning() {
    let source = frame(); let normal = canonical(&source).unwrap();
    let mut lower = source.clone(); lower["build"]["mods"][0]["rank"] = json!(0);
    assert_eq!(canonical(&lower).unwrap(), normal, "submitted ranks normalize to max like weapon builds");
    let mut maximum = source.clone(); maximum["build"]["mods"][0]["rank"] = json!(wf::mod_by_id("vitality").unwrap().max_rank);
    assert_eq!(canonical(&maximum).unwrap(), normal);
    let mut invalid = source.clone(); invalid["build"]["mods"][0]["rank"] = json!(999);
    assert!(canonical(&invalid).is_err());
    invalid = source.clone(); invalid["build"]["mods"][1] = invalid["build"]["mods"][0].clone();
    assert!(canonical(&invalid).is_err());
    invalid = source.clone(); invalid["build"]["arcanes"][1] = invalid["build"]["arcanes"][0].clone();
    assert!(canonical(&invalid).is_err());
    invalid = source.clone(); invalid["build"]["shards"][0]["effect"] = json!("not_an_effect");
    assert!(canonical(&invalid).is_err());
    let mut infused = source; infused["build"]["helminth"] = json!({"slot":1, "ability":"roar"});
    assert_ne!(build_id(&canonical(&infused).unwrap()), build_id(&normal));
}

#[test]
fn companion_cards_are_validated_and_canonical() {
    let mut source = json!({"kind":"companion", "build":{"companion":"sentinel", "mods":
        companions::seatable_by("sentinel").take(10).map(|m| json!({"id":m.id})).collect::<Vec<_>>()}});
    let normal = canonical(&source).unwrap();
    source["build"]["mods"].as_array_mut().unwrap().reverse();
    assert_eq!(canonical(&source).unwrap(), normal);
    source["build"]["mods"][0] = source["build"]["mods"][1].clone();
    assert!(canonical(&source).is_err());
    source = normal; source["build"]["mods"].as_array_mut().unwrap().pop();
    assert!(canonical(&source).is_err());
}
