use super::*;

#[test]
fn every_scoped_wiki_mod_is_catalogued_or_has_an_exclusion() {
    let text = crate::data::file("surveys/wiki_mods.yaml").expect("run scripts/survey_mod_coverage.py");
    let survey: Value = serde_norway::from_str(text).unwrap();
    let rows = survey["mods"].as_sequence().unwrap();
    assert!(rows.len() > 1200, "the Wiki survey is incomplete");
    let mut missing = Vec::new();
    for row in rows {
        let name = row["name"].as_str().unwrap();
        if let Some(reason) = row["reason"].as_str() {
            assert!(!reason.is_empty(), "{name}: an exclusion needs a reason");
        } else if let Some(path) = row["path"].as_str() {
            assert!(crate::data::file(path).is_some(), "{name}: {path} is missing");
        } else {
            missing.push(name);
        }
    }
    assert!(missing.is_empty(), "Wiki mods missing from the catalogs: {missing:?}");
}

#[test]
fn new_weapon_cards_obey_their_exclusive_and_aoe_restrictions() {
    let has = |w: &str, id: &str| pool_for_weapon(w).iter().any(|m| m.id == id);
    for w in ["venato", "venato_prime", "venato_prime_heavy"] {
        assert!(has(w, "sentient_incision"), "{w}");
    }
    assert!(!has("hate", "sentient_incision"));
    assert!(has("kunai", "concealed_explosives"));
    assert!(!has("castanas", "concealed_explosives"));
    assert!(has("braton", "mending_shot"));
    assert!(!has("ogris", "mending_shot"));
    assert!(has("lex", "energizing_shot"));
    assert!(!has("epitaph", "energizing_shot"));
}
