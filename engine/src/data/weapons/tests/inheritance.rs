use super::*;

/// A FORM NEVER RESTATES ITS WEAPON. This is the guard the Larkspur bug
/// needed and did not have.
///
/// Its alt-fire carried its BASE form's accuracy while its Prime's alt-fire
/// carried the alt-fire's — one weapon, two entries, two answers, and no
/// way to notice because nothing knew the two were the same gun. An audit
/// found it among 313 identical values written twice.
///
/// The rule that closes it is not "inherit everything", which would be
/// wrong — a form legitimately overrides its magazine (the Scourge's throw
/// holds one round against forty) and its accuracy (a scoped alt-fire is
/// not a hip-fired beam). The rule is that a form may not state a value
/// IDENTICAL to its weapon's: a restatement carries no information and is
/// the only way the two can drift apart.
#[test]
fn a_form_states_only_what_differs_from_its_weapon() {
    use serde_norway::Value;
    // Read the FILES rather than the merged specs — after the merge the
    // inherited value and a restated one are the same thing, which is
    // exactly the distinction this asserts.
    let raw: Vec<(&str, Value)> = crate::data::files_under("weapons/")
        .filter(|(p, _)| p.ends_with(".yaml"))
        .map(|(p, text)| (p, serde_norway::from_str::<Value>(text).expect(p)))
        .collect();
    let by_id: std::collections::HashMap<&str, &Value> = raw
        .iter()
        .filter_map(|(_, v)| v.get("id").and_then(Value::as_str).map(|i| (i, v)))
        .collect();

    let mut echoed: Vec<String> = Vec::new();
    for (p, v) in &raw {
        let Some(parent) = v.get("inherits").and_then(Value::as_str) else { continue };
        let up = by_id[parent];
        for k in INHERITED.iter().chain(INHERITED_BLOCKS.iter()) {
            if let (Some(mine), Some(theirs)) = (v.get(*k), up.get(*k)) {
                if mine == theirs {
                    echoed.push(format!("{p}: `{k}` is its weapon's own value"));
                }
            }
        }
    }
    assert!(
        echoed.is_empty(),
        "a form restated a value it already inherits — drop the line, and if it              is meant to DIFFER, the value is what is wrong:
  {}",
        echoed.join("
  ")
    );
}

/// ...and every form sibling that CAN inherit, does. A weapon whose form
/// carries a full copy of its metadata is the state this was written to
/// leave, so a new one is a regression rather than a style choice.
#[test]
fn a_form_that_copies_its_weapon_declares_the_inheritance() {
    use serde_norway::Value;
    let raw: Vec<(&str, Value)> = crate::data::files_under("weapons/")
        .filter(|(p, _)| p.ends_with(".yaml"))
        .map(|(p, text)| (p, serde_norway::from_str::<Value>(text).expect(p)))
        .collect();
    // group -> the entry that is the arsenal's form
    let mut head: std::collections::HashMap<&str, &Value> = std::collections::HashMap::new();
    for (_, v) in &raw {
        if v.get("default_form").and_then(Value::as_bool) == Some(true) {
            if let Some(g) = v.get("transform_group").and_then(Value::as_str) {
                head.insert(g, v);
            }
        }
    }
    let mut copiers: Vec<String> = Vec::new();
    for (p, v) in &raw {
        if v.get("inherits").is_some()
            || v.get("default_form").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        let Some(g) = v.get("transform_group").and_then(Value::as_str) else { continue };
        let Some(up) = head.get(g) else { continue };
        let same = INHERITED
            .iter()
            .filter(|k| v.get(**k).is_some() && v.get(**k) == up.get(**k))
            .count();
        // A handful of shared fields is a form describing itself; a dozen
        // is a copy of the weapon.
        if same >= 6 {
            copiers.push(format!("{p}: {same} fields identical to `{g}`"));
        }
    }
    assert!(
        copiers.is_empty(),
        "these forms copy their weapon instead of inheriting it — add              `inherits:` and delete the copies:
  {}",
        copiers.join("
  ")
    );
}
