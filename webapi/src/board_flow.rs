//! Capture one bounded build expansion without executing any fight.
use std::collections::BTreeMap;
use serde_json::{json, Map, Value};
use wfsim_engine::board::{benchmarks, builds, groups};

fn decoded(value: Option<&Value>) -> Option<Value> {
    match value? {
        Value::String(text) => serde_json::from_str(text).ok(),
        value => Some(value.clone()),
    }.filter(|value| !value.is_null())
}
fn record(build: &builds::ValidBuild) -> Value {
    let record=crate::board_rows::canonical_record(build);
    if build.riven_rolls.is_empty() { record } else { crate::board_rows::with_rolls(record,&build.riven_rolls) }
}
fn intern(values: &mut Vec<Value>, value: Value) -> usize {
    if let Some(index)=values.iter().position(|v| v==&value) { return index; }
    let index=values.len();values.push(value);index
}
fn same_input(left: &Value, right: &Value) -> bool {
    match (left,right) {
        (Value::Number(a),Value::Number(b)) if a.is_f64() || b.is_f64()=>a.as_f64()==b.as_f64(),
        (Value::Array(a),Value::Array(b))=>a.len()==b.len() && a.iter().zip(b).all(|(a,b)|same_input(a,b)),
        (Value::Object(a),Value::Object(b))=>a.len()==b.len() && a.iter().all(|(k,v)|b.get(k).is_some_and(|other|same_input(v,other))),
        _=>left==right,
    }
}
struct Expansion {
    builds: Vec<Value>, nodes: Vec<Value>, templates: Vec<Value>, patches: Vec<Value>,
    bases: BTreeMap<(String,String),usize>,
}
impl Expansion {
    fn add(&mut self, build: &builds::ValidBuild, scope: &Value) {
        let identity=builds::build_id(build);
        if self.builds.iter().any(|b| b["id"]==identity) { return; }
        let record=record(build);
        self.builds.push(json!({"id":identity,"record":record}));
        let Some(boards)=scope["boards"].as_array() else { return };
        for board in boards {
            let Some(ruler)=board["id"].as_str() else { continue };
            if benchmarks::all().iter().any(|b| b.id==ruler)
                && crate::board_rows::admitted(&record,ruler,build).is_err() { continue; }
            for (played, request) in crate::board_rows::row_requests(build,&board["scenario"]) {
                let mode=if played.id.is_empty() {"base"}else{played.id};
                if !scope["modes"][&build.weapon].as_array().is_some_and(|m| m.iter().any(|v| v==mode)) { continue; }
                let Ok((request,_))=crate::computation::normalize("simulation",&request) else { continue };
                let key=(identity.clone(),mode.to_owned());
                let template=if let Some(index)=self.bases.get(&key) { *index } else {
                    let index=self.templates.len();self.templates.push(request.clone());self.bases.insert(key,index);index
                };
                let base=self.templates[template].as_object().expect("fight object");
                let current=request.as_object().expect("fight object");
                let set: Map<String,Value>=current.iter().filter(|(k,v)| base.get(*k)!=Some(*v)).map(|(k,v)|(k.clone(),v.clone())).collect();
                let remove: Vec<_>=base.keys().filter(|k| !current.contains_key(*k)).collect();
                let patch=intern(&mut self.patches,json!({"set":set,"remove":remove}));
                self.nodes.push(json!({"kind":"measure","identity":identity,"ruler":ruler,"mode":mode,
                    "template":template,"patch":patch}));
            }
        }
    }
}

/// Scope is captured at acceptance; derived builds reuse their parent's scope.
pub fn plan(input: &Value, ruler: &str, mode: &str, family: bool, captured: Option<&Value>) -> Value {
    let Ok(build)=crate::board_rows::library_build(input) else { return Value::Null };
    let mut jobs=if family {crate::board_family::transfers(input,ruler,mode)} else {Vec::new()};
    jobs.sort_by(|a,b| a["weapon"].as_str().cmp(&b["weapon"].as_str()));
    let scope=decoded(captured).unwrap_or_else(|| {
        let mut boards: Vec<_>=benchmarks::all().iter().map(|b| json!({"id":b.id,"scenario":b.scenario})).collect();
        boards.sort_by(|a,b| a["id"].as_str().cmp(&b["id"].as_str()));
        let mut modes=BTreeMap::new();
        for weapon in std::iter::once(build.weapon.as_str()).chain(jobs.iter().filter_map(|j| j["weapon"].as_str())) {
            modes.insert(weapon.to_owned(),groups::board_modes(weapon));
        }
        json!({"boards":boards,"modes":modes})
    });
    let mut out=Expansion{builds:Vec::new(),nodes:Vec::new(),templates:Vec::new(),patches:Vec::new(),bases:BTreeMap::new()};
    out.add(&build,&scope);
    for job in jobs {
        let mut copy=input.clone();copy["weapon"]=job["weapon"].clone();
        for axis in ["grip","loader","link"] {
            copy.as_object_mut().expect("record object").remove(axis);
            if let Some(value)=job["context"].get(axis) { copy[axis]=value.clone(); }
        }
        let valid=crate::board_rows::library_build(&copy).ok().filter(|target| {
            let mut source_mods=build.mods.clone();source_mods.sort();
            let mut target_mods=target.mods.clone();target_mods.sort();
            source_mods==target_mods && build.evolutions==target.evolutions && build.arcanes==target.arcanes
        });
        if let Some(target)=valid { out.add(&target,&scope); }
        else { out.nodes.push(json!({"kind":"optimize","request":job["request"],"context":job["context"],"weapon":job["weapon"]})); }
    }
    json!({"schema":1,"scope":scope,"builds":out.builds,"nodes":out.nodes,"templates":out.templates,"patches":out.patches})
}

/// A local observation must describe the exact official fight before correction.
pub fn observation(record: &Value, raw: Option<&Value>) -> Value {
    let Some(mut value)=decoded(raw) else { return Value::Null };
    value["valid"]=json!(false);
    let ruler=value["ruler"].as_str().unwrap_or("");
    let mode=value["mode"].as_str().unwrap_or("");
    let Some(bench)=benchmarks::all().iter().find(|b| b.id==ruler) else {return value};
    let Ok(build)=crate::board_rows::scored_build(record,ruler) else {return value};
    let physical=|request:&Value| {
        let mut request=request.clone();
        if let Some(object)=request.as_object_mut() {
            // Search controls and capacity layouts do not change the final fight.
            for field in ["replay","replay_follow","run_series","run","mod_slots","starts","limits","final",
                "strategy","search_strategy","candidate_runs","finalists","final_runs","max_evals","max_jobs",
                "build_min","build_size","every_rank","screen_ratio","keep_ratio","shard","shards","threads",
                "quick_fleet","arcanes","modes","valence","exilus","stance","assemblies"] {object.remove(field);}
            for field in ["custom_enemies","also_acting"] {
                if object.get(field).is_some_and(|v|v.as_array().is_some_and(Vec::is_empty)) {object.remove(field);}
            }
        }
        let fight=crate::fight::parse_fight(&request)?;
        let ranks:Vec<_>=request["arcane"].as_array().into_iter().flatten().enumerate().map(|(i,id)| {
            request["arcane_rank"][i].as_u64().or_else(|| fight.info.arcane_pools.get(i)
                .and_then(|pool|wfsim_engine::data::arcanes::for_slot(pool,id.as_str().unwrap_or("")))
                .map(|card|u64::from(card.max_rank)))
        }).collect();
        request["arcane_rank"]=json!(ranks);
        // Scenarios retain every class and buff; this comparison covers only
        // the submitted build. Keep unknown fight terms conservative.
        let panel=crate::panel::panel_json(&request);
        if panel["ok"]!=true {return Err(panel);}
        if let Some(buffs)=request.get_mut("buffs").and_then(Value::as_object_mut) {
            buffs.retain(|id,setting| panel["buffs"].as_array().is_some_and(|cards| cards.iter().any(|card| {
                card["id"]==*id && !setting.as_object().is_some_and(|value| {
                    value.keys().all(|key| ["stacks","locked"].contains(&key.as_str()))
                        && setting["stacks"]==card["default_stacks"]
                        && setting["locked"].as_bool().unwrap_or(false)==card["default_locked"].as_bool().unwrap_or(false)
                })
            })));
        }
        if let Some(rules)=request.get_mut("class_rules").and_then(Value::as_object_mut) {
            let class=wfsim_engine::build::scenario::class_of(&fight.info.id);
            rules.retain(|key,_| class==Some(key.as_str())
                || !wfsim_engine::build::scenario::WEAPON_CLASSES.contains(&key.as_str()));
            if rules.is_empty() {request.as_object_mut().unwrap().remove("class_rules");}
        }
        crate::computation::normalize("simulation",&request)
    };
    let Ok((local,_))=physical(&value["request"]) else {return value};
    let scenario=serde_json::to_value(&bench.scenario).expect("scenario");
    let matched=crate::board_rows::row_requests(&build,&scenario).into_iter()
        .filter(|(p,_)| (if p.id.is_empty(){"base"}else{p.id})==mode)
        .any(|(_,request)| physical(&request).is_ok_and(|(official,_)| same_input(&official,&local)));
    value["valid"]=json!(matched);value
}

#[cfg(test)]
mod tests {
    use super::*;
    fn example() -> Value { json!({"weapon":"burston_prime","mods":["serration","split_chamber","point_strike","vital_sense","hellfire","infected_clip","stormbringer","gilded_truth"],"arcanes":["primary_merciless"]}) }
    #[test]
    fn captures_measurements_and_one_incompatible_sibling_search() {
        let record=example();let p=plan(&record,"standard_single_target","cycle",true,None);
        assert_eq!(p["schema"],1);
        let nodes=p["nodes"].as_array().unwrap();
        assert_eq!(nodes.iter().filter(|n| n["kind"]=="optimize").count(),1);
        assert!(nodes.iter().any(|n| n["kind"]=="measure"));
        assert!(serde_json::to_vec(&p).unwrap().len()<1024*1024);
        for node in nodes.iter().filter(|n| n["kind"]=="measure") {
            let mut request=p["templates"][node["template"].as_u64().unwrap() as usize].clone();
            let patch=&p["patches"][node["patch"].as_u64().unwrap() as usize];
            request.as_object_mut().unwrap().extend(patch["set"].as_object().unwrap().clone());
            for key in patch["remove"].as_array().unwrap() {request.as_object_mut().unwrap().remove(key.as_str().unwrap());}
            assert!(crate::simulate::PreparedSimulation::new(&request).is_ok());
        }
    }
    #[test]
    fn actual_rank_and_riven_roll_pairing_survive_intake() {
        let mut a=example();a["mods"][0]=json!("riven");a["mods"][1]=json!("split_chamber@3");
        a["riven_pos"]=json!(["multishot","critical_damage"]);a["riven_rolls"]=json!([0.97,1.03]);
        let first=crate::board_rows::library_build(&a).unwrap();
        assert!(first.mods.contains(&"split_chamber@3".to_owned()));
        a["riven_pos"]=json!(["critical_damage","multishot"]);a["riven_rolls"]=json!([1.03,0.97]);
        let second=crate::board_rows::library_build(&a).unwrap();
        assert_eq!(builds::build_id(&first),builds::build_id(&second));
        assert_eq!(first.riven_rolls,vec![1.03,0.97]);
    }
    #[test]
    fn correction_requires_the_exact_official_environment() {
        let record=example();let p=plan(&record,"standard_single_target","cycle",false,None);
        let node=p["nodes"].as_array().unwrap().iter().find(|n|n["ruler"]=="standard_single_target").unwrap();
        let mut request=p["templates"][node["template"].as_u64().unwrap() as usize].clone();
        let patch=&p["patches"][node["patch"].as_u64().unwrap() as usize];
        request.as_object_mut().unwrap().extend(patch["set"].as_object().unwrap().clone());
        for key in patch["remove"].as_array().unwrap() {request.as_object_mut().unwrap().remove(key.as_str().unwrap());}
        let raw=json!({"ruler":node["ruler"],"mode":node["mode"],"request":request});
        assert_eq!(observation(&record,Some(&raw))["valid"],true);
        let mut report=raw.clone();report["request"]["replay"]=json!(true);report["request"]["custom_enemies"]=json!([]);report["request"]["also_acting"]=json!([]);
        report["request"]["player_at"]=json!([0,0]);report["request"]["target_at"]=json!([0,0.5]);
        assert_eq!(observation(&record,Some(&report))["valid"],true);
        let mut finalist=raw.clone();
        finalist["request"]["strategy"]=json!("quick");
        finalist["request"]["starts"]=json!([]);
        finalist["request"]["candidate_runs"]=json!(10);
        finalist["request"]["finalists"]=json!(3);
        finalist["request"]["limits"]=json!({});
        finalist["request"]["mod_slots"]=json!([]);
        finalist["request"]["arcane_rank"]=json!([5]);
        assert_eq!(observation(&record,Some(&finalist))["valid"],true,"a final replay has the same official fight");
        let ordered=crate::board_rows::board_order_json(&json!({"record":record,"ruler":node["ruler"],
            "mode":node["mode"],"observation":finalist["request"]}));
        assert_eq!(ordered["matches_observation"],true);
        let mut unrelated=raw.clone();
        unrelated["request"]["class_rules"]=json!({"archgun":{"infinite_ammo":true}});
        unrelated["request"]["buffs"]=json!({"unrelated_weapon_buff":{"stacks":10,"locked":true}});
        assert_eq!(observation(&record,Some(&unrelated))["valid"],true);
        let panel=crate::panel::panel_json(&raw["request"]);
        let buff=&panel["buffs"][0];
        assert!(buff["id"].is_string());
        let mut changed=raw.clone();
        changed["request"]["buffs"]=json!({buff["id"].as_str().unwrap():{
            "stacks":if buff["default_stacks"]==0 {1}else{0},
            "locked":!buff["default_locked"].as_bool().unwrap()}});
        assert_eq!(observation(&record,Some(&changed))["valid"],false);
        for (field,value) in [("unknown_fight_term",json!(1)),
            ("class_rules",json!({"primary":{"headshot_pct":0}})),
            ("arcane_rank",json!([0])),("also_acting",json!([{"weapon":"furis"}]))] {
            let mut changed=raw.clone();changed["request"][field]=value;
            assert_eq!(observation(&record,Some(&changed))["valid"],false,"{field}");
        }
        for field in ["runs","seed","duration"] {
            let mut changed=raw.clone();changed["request"][field]=if field=="duration" {
                json!(changed["request"][field].as_f64().unwrap()+1.0)
            } else {json!(changed["request"][field].as_u64().unwrap()+1)};
            assert_eq!(observation(&record,Some(&changed))["valid"],false,"{field}");
        }
    }
    #[test]
    fn correction_rejects_a_consumed_class_override() {
        let record=json!({"weapon":"larkspur","mods":["rubedo_lined_barrel","dual_rounds","parallax_scope",
            "hollowed_bullets","combustion_rounds","electrified_barrel","venomous_clip","magma_chamber"],
            "arcanes":["primary_merciless","secondary_merciless"]});
        let p=plan(&record,"standard_single_target","base",false,None);
        let node=p["nodes"].as_array().unwrap().iter().find(|n|n["ruler"]=="standard_single_target").unwrap();
        let mut request=p["templates"][node["template"].as_u64().unwrap() as usize].clone();
        let patch=&p["patches"][node["patch"].as_u64().unwrap() as usize];
        request.as_object_mut().unwrap().extend(patch["set"].as_object().unwrap().clone());
        for key in patch["remove"].as_array().unwrap() {request.as_object_mut().unwrap().remove(key.as_str().unwrap());}
        let mut raw=json!({"ruler":node["ruler"],"mode":node["mode"],"request":request});
        assert_eq!(observation(&record,Some(&raw))["valid"],true);
        assert!(!crate::fight::parse_fight(&raw["request"]).unwrap().infinite_ammo);
        raw["request"]["class_rules"]=json!({"archgun":{"infinite_ammo":true}});
        assert!(crate::fight::parse_fight(&raw["request"]).unwrap().infinite_ammo);
        assert_eq!(observation(&record,Some(&raw))["valid"],false);
    }
    #[test]
    fn storage_legality_does_not_require_a_visible_board_row() {
        let record=json!({"weapon":"braton","mods":["serration"],"scope":"library"});
        assert_eq!(crate::board_check_json(&record)["accepted"],true);
        let bad=json!({"weapon":"missing","mods":[],"scope":"library"});
        assert_eq!(crate::board_check_json(&bad)["accepted"],false);
    }
}
