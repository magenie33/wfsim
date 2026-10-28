//! Package durations, input boundaries and damage event counts are independent evidence.
#[test]
fn a_package_stance_runs_for_each_recorded_animation_duration() {
    let raw: serde_norway::Value = serde_norway::from_str(include_str!(
        "../../../../../tests/fixtures/melee/package-2026-10-08.json")).unwrap();
    let mut checked = 0;
    for (_, text) in crate::data::files_under("mods/") {
        let source: serde_norway::Value = serde_norway::from_str(text).unwrap();
        let Some(combos) = source.get("combos").and_then(|v| v.as_mapping()) else { continue };
        let id = source["id"].as_str().unwrap();
        let m = crate::data::mods::classes().into_iter()
            .flat_map(crate::data::mods::class_pool).find(|m| m.id == id).unwrap();
        for (form, binding) in combos {
            let Some(tree) = binding.get("tree").and_then(|v| v.as_str()) else { continue };
            assert_eq!(tree, source["internal_name"].as_str().unwrap(), "{id}: wrong package tree");
            let context = binding["context"].as_str().unwrap();
            let expected_context = match form.as_str().unwrap() {
                "neutral" => "CC_GROUND", "forward" => "CC_GROUND_BRANCH_A",
                "block_forward" => "CC_GROUND_BRANCH_B", "block" => "CC_GROUND_BRANCH_C",
                "heavy" => "CC_GROUND_HEAVY", "slide" => "CC_SLIDING", _ => panic!("unknown stance input"),
            };
            assert_eq!(context, expected_context, "{id}: wrong input context");
            let paths = raw["trees"][tree]["contexts"][context].as_sequence().unwrap();
            let script = m.stance.unwrap().iter().find(|(f, _)| *f == form.as_str().unwrap()).unwrap().1;
            let mut duration = 0.0;
            let mut count = 0;
            let mut damage = 0.0;
            let mut contacts = Vec::new();
            for path in paths {
                let a = &raw["attacks"][path.as_str().unwrap()];
                let seconds = a["duration"].as_f64().unwrap();
                let hits = a["hits"].as_sequence().unwrap();
                count += hits.len();
                let mut times = hits.iter().map(|e| duration + seconds * e["t"].as_f64().unwrap()).collect::<Vec<_>>();
                for (i, _) in hits.iter().enumerate() {
                    let p = a["swings"].as_sequence().unwrap().get(i).filter(|v| !v.is_null()).unwrap_or(a);
                    damage += p["atten"].as_f64().unwrap();
                }
                count += a["events"].as_sequence().unwrap().iter().filter(|e| {
                    matches!(e["event"].as_str(), Some("MeleeSlam" | "AxeSlam"))
                }).count();
                times.extend(a["events"].as_sequence().unwrap().iter().filter(|e| {
                    matches!(e["event"].as_str(), Some("MeleeSlam" | "AxeSlam"))
                }).map(|e| duration + seconds * e["t"].as_f64().unwrap()));
                times.sort_by(f64::total_cmp);
                contacts.extend(times);
                duration += seconds;
            }
            let clock: f64 = script.iter().map(|h| h.delay_seconds).sum();
            let dealt: f64 = script.iter().map(|h| h.multiplier * f64::from(h.hits)).sum();
            assert!((clock - duration).abs() < 1e-9, "{id}/{form:?}: {clock} != {duration}");
            assert!((dealt - damage).abs() < 1e-9, "{id}/{form:?}: {dealt} != {damage}");
            assert_eq!(script.len(), count, "{id}/{form:?}: dropped attack event");
            assert_eq!(script.iter().filter(|h| h.ends_input()).count(), paths.len(), "{id}/{form:?}: wrong input boundaries");
            let mut elapsed = 0.0;
            for (h, at) in script.iter().zip(contacts) {
                let got = elapsed + h.hit_delay_seconds.unwrap();
                assert!((got - at).abs() < 1e-9, "{id}/{form:?}: event {got} != {at}");
                elapsed += h.delay_seconds;
            }
            checked += 1;
        }
    }
    assert!(checked >= 296, "only {checked} package stance modes were checked");
}
