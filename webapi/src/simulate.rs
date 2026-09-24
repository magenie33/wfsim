// SPDX-License-Identifier: AGPL-3.0-or-later
//! `/api/simulate` — whole, in shards, and merged — and the request a board
//! row makes.

use serde_json::{json, Value};
use wfsim_engine::fight::{BuffLock, FightParams};
use wfsim_engine::build::loadout::{resolve_for, ResolvedPanel};
use wfsim_engine::model::WeaponBase;
use wfsim_engine::model::{ModDef, StackPolicy};
use wfsim_engine::rules::capacity::PlannedMod;
use crate::buffs::{arcane_choices, arcane_fx_for};
use crate::fight::{Fight, parse_fight};
use crate::registry::{WeaponInfo, base_for, incarnon_id, innate_slots_for, mod_not_here, wspec};
use crate::request::{err_json, get_bool, r1, r3};
use crate::rivens::{mod_pool_with_rivens, riven_stat_ids_ok};

/// A rostered buff whose ceiling is a NUMBER, as the chart reads it: what one
/// stack is worth, where it stops, and the unit both are in. `null` for the
/// ordinary kind, where the stack count is the published fact and the chart is
/// a chart of stacks — see [`wfsim_engine::fight::StackValue`].
fn value_json(v: Option<wfsim_engine::fight::StackValue>) -> Value {
    match v {
        Some(v) => json!({ "per": v.per_stack, "max": v.max, "unit": v.unit }),
        None => Value::Null,
    }
}

// ---- THE REQUEST A BOARD ROW MAKES ----------------------------------------
//
// Here rather than in the scorer because there are two callers now: the one
// that MEASURES a build and the one that resolves a riven's numbers before the
// build is stored. A request built twice is two answers to what a fight IS, and
// the pair would drift a field at a time — the assembly went missing from one
// of them once already, and the number published was for a weapon nobody built.

/// The mod id a RIVEN takes in a simulate request. A record spells it `riven`
/// (the endpoint's ids are `[a-z0-9_]`); the request names an ITEM.
pub const RIVEN_ITEM: &str = "riven:board";

/// One riven, as the `rivens` array of a simulate request.
pub fn riven_request(spec: &wfsim_engine::build::rivens::RivenSpec) -> Value {
    let stat = |s: &wfsim_engine::build::rivens::RolledStat| json!({ "id": s.id, "roll": s.roll });
    json!([{
        "name": RIVEN_ITEM.trim_start_matches("riven:"),
        "spec": {
            "bonuses": spec.bonuses.iter().map(stat).collect::<Vec<_>>(),
            "malus": spec.malus.as_ref().map(stat),
            "rank": spec.rank,
            "polarity": "madurai",
        }
    }])
}

/// THE RULER'S TERMS PLUS THE ENTRANT, as the request the simulator answers.
///
/// **IT NAMES THE MODE, NEVER THE FORM THE MODE RESOLVES TO.** `form()` maps
/// every cycle onto the one policy word `gauge_cycle`, which does not say in
/// which half the gauge is filled — so a weapon with two cycles sent one
/// request for both and `parse_fight` fell back to the arsenal's own form.
///
/// Extracted so the assertion can be made on the REQUEST: a decision taken
/// inline in a scoring loop is one no test can reach.
pub fn simulate_request(
    scenario: &Value,
    v: &wfsim_engine::board::builds::ValidBuild,
    played: wfsim_engine::data::weapons::WeaponPlayMode,
) -> Value {
    let mut req = scenario.clone();
    let Some(o) = req.as_object_mut() else {
        return req;
    };
    o.insert("weapon".into(), json!(v.weapon));
    // THE RIVEN'S SLOT IS SPELLED DIFFERENTLY ON THE WIRE. A record carries the
    // bare `riven` because the endpoint's ids are `[a-z0-9_]`; a simulate
    // request names the riven ITEM, which is `riven:<name>`. The translation is
    // one line and lives here so neither protocol has to bend for the other.
    o.insert(
        "mods".into(),
        json!(v
            .mods
            .iter()
            .map(|m| if m == wfsim_engine::board::builds::RIVEN_SLOT {
                RIVEN_ITEM.to_string()
            } else {
                m.clone()
            })
            .chain(v.exilus.iter().cloned())
            .collect::<Vec<_>>()),
    );
    o.insert("evolutions".into(), json!(v.evolutions));
    o.insert("arcane".into(), json!(v.arcanes));
    // THE VALENCE, at the ruler's own terms: the element the entrant named, and
    // the roll's MAXIMUM whatever they said it was. Every player can fuse to
    // 60%, so ranking a lower roll would be ranking how many duplicates someone
    // farmed — the same reason every row here is scored at full Forma.
    if !v.valence.is_empty() {
        o.insert("valence_element".into(), json!(v.valence));
        let max = wfsim_engine::data::weapons::valence_of(&v.weapon).map_or(0.0, |s| s.max);
        o.insert("valence_bonus".into(), json!(max));
    }
    // THE PARTS. Without them the fight is fought with the chamber's DEFAULT
    // assembly, so a submitted grip is stored, validated and then silently not
    // used — the number published would be for a weapon nobody built.
    if let Some(a) = &v.assembly {
        o.insert("assembly".into(), json!({ "grip": a.grip, "loader": a.loader }));
    }
    o.insert("mode".into(), json!(played.id));
    // ONE SPELLING OF ONE FACT. `form` is what a request carries when it names
    // no mode, and a ruler that carried both would be two answers to one
    // question with the loser silent.
    o.remove("form");
    req
}

pub fn simulate_json(v: &Value) -> Value {
    simulate_json_reporting(v, &mut |_, _| {})
}

/// …TELLING A CALLER HOW FAR IT HAS GOT — `(done, total)` after every run.
///
/// A single-target fight is about a millisecond a run and nobody needs this. A
/// 361-body one is tens of milliseconds, so the rulers' 1000 runs is a minute
/// in the browser — and a button that says "Simulating…" for a minute reads as
/// a hang, which is what it was reported as.
///
/// THE ANSWER IS UNCHANGED. The callback observes and never steers.
pub fn simulate_json_reporting(v: &Value, on_run: &mut impl FnMut(u32, u32)) -> Value {
    simulate_from(v, Work::All, on_run)
}

/// ONE SHARD OF A SIMULATION — what a WORKER runs.
///
/// `from` is the index of the first run and `count` how many. Every run's dice
/// are a pure function of `(seed, index)`, so the shards of a range merge into
/// exactly what one call over the whole range produces
/// (`fight::tests::formation_and_spread::eight_shards_are_one_run`).
///
/// Returns the shard itself as JSON — small, because it carries sums rather
/// than runs — for `simulate_merged_json` to add up.
pub fn simulate_shard_json(
    v: &Value,
    from: u32,
    count: u32,
    on_run: &mut impl FnMut(u32, u32),
) -> Value {
    simulate_from(v, Work::Shard(from, count), on_run)
}

/// …AND THE MERGE, which produces the ordinary simulate response.
///
/// The page fans the shards out and collects them; every field of the answer is
/// computed HERE, so there is one implementation of the arithmetic rather than
/// a Rust one and a JavaScript one that drift.
pub fn simulate_merged_json(v: &Value, shards: &[Value]) -> Value {
    let mut merged = wfsim_engine::fight::Shard::default();
    for s in shards {
        match serde_json::from_value::<wfsim_engine::fight::Shard>(s.clone()) {
            Ok(part) => merged.merge(&part),
            Err(e) => return err_json(format!("shard: {e}")),
        }
    }
    simulate_from(v, Work::Merged(Box::new(merged)), &mut |_, _| {})
}

/// WHAT THIS CALL IS DOING — the three ways a simulation is run, sharing one
/// path so a shard and a whole run cannot build different params.
enum Work {
    /// The ordinary call: run every run here.
    All,
    /// A slice of the runs, for a worker. Returns the shard rather than a
    /// report.
    Shard(u32, u32),
    /// Already run by a fleet; this call only finishes it.
    Merged(Box<wfsim_engine::fight::Shard>),
}

/// THE FIGHT'S AMMO ECONOMY — the three settings that travel together because
/// none of them decides anything on its own: drops pay only into a finite
/// reserve, a reach only matters once something has fallen, and the squad's
/// place only moves the rate.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AmmoEconomy {
    pub(crate) drops: bool,
    pub(crate) pickup_range_m: f64,
    pub(crate) landscape: bool,
}

impl AmmoEconomy {
    fn apply(self, p: &mut FightParams) {
        p.ammo_drops = self.drops;
        p.pickup_range_m = self.pickup_range_m;
        p.landscape = self.landscape;
    }
}

/// THE PANEL AND THE ENGINE PARAMS a parsed fight resolves to.
///
/// Lifted out of `simulate_from` so `/api/log` runs the SAME fight rather than
/// a second spelling of it — which is the server's half of "THERE IS ONE
/// FIGHT" said about the thing `parse_fight` hands over rather than about the
/// request. Nothing is decided here that is not decided here for both.
// FOURTEEN ARGUMENTS, and they are the fight: grouping them into a struct
// would move the same list one line down and add a name nobody reads.
#[allow(clippy::too_many_arguments)]
pub(crate) fn sim_params(
    v: &Value,
    info: &'static WeaponInfo,
    policy: StackPolicy,
    evo_refs: &[&str],
    refs: &[&ModDef],
    tenno: &wfsim_engine::data::tenno::Tenno,
    arena: &wfsim_engine::arena::Arena,
    cycle_from: Option<&str>,
    single_form: &str,
    infinite_ammo: bool,
    ammo: AmmoEconomy,
    frenzy_single: bool,
    cycle_frenzy_lock: wfsim_engine::fight::LockMode,
    frenzy_locks: &[BuffLock],
) -> (ResolvedPanel, FightParams) {
    let arcane_fx = {
        let ab = WeaponBase::from_data(incarnon_id(info).unwrap_or(&info.id), true, evo_refs);
        arcane_fx_for(v, info, &ab, policy)
    };
    // Either ONE registered form, or the real two-form cycle (which needs the
    // gauge form and the form it transforms out of, so it resolves both).
    let panel_of = |id: &str| resolve_for(&base_for(v, id, evo_refs), refs, policy, tenno);
    if let Some(cycle_from) = cycle_from {
        let incarnon_panel = panel_of(incarnon_id(info).unwrap_or(&info.id));
        let base_panel = panel_of(cycle_from);
        // A TOME'S CYCLE IS NOT A TRANSFORMATION. Both are "fill a meter in one
        // form, spend it in the other", and only one of them puts the weapon in
        // the other form: a Tome shoots its primary fire the whole engagement
        // and THROWS the other form's orb, which is an entity rather than a
        // state you enter. So the params stay the base form's and the orb rides
        // along — see `FightParams::tome_cycle_from_panels`.
        //
        // Told apart by the METER rather than by the weapon, so the second Tome
        // costs nothing here.
        if incarnon_panel.meter.is_some() {
            let mut params = FightParams::tome_cycle_from_panels(
                &base_panel,
                &incarnon_panel,
                arena,
                &arcane_fx,
            );
            params.infinite_reserve = base_panel.reserve_is_infinite(infinite_ammo);
            ammo.apply(&mut params);
            params.frenzy = frenzy_single;
            params.locked_buffs = frenzy_locks.to_vec();
            // THE CYCLE REPORTS THE FORM IT FIRES, which for a Tome is the one
            // you are holding. An Incarnon cycle reports the form it transforms
            // INTO because that is where its damage is; here the primary fire
            // is a real part of the engagement and the panel is its own.
            return (base_panel, params);
        }
        let params = FightParams::incarnon_cycle_from_panels(
            &incarnon_panel,
            &base_panel,
            frenzy_single,
            cycle_frenzy_lock,
            arena,
            &arcane_fx,
        );
        // The cycle reports the form it transforms INTO, as it always has.
        let mut params = params;
        params.infinite_reserve = incarnon_panel.reserve_is_infinite(infinite_ammo);
        ammo.apply(&mut params);
        (incarnon_panel, params)
    } else {
        let panel = panel_of(single_form);
        // A MELEE INCARNON IS THE SAME WEAPON RESOLVED TWICE, and the second
        // resolve is this one without the tiers that turn it on. It is not a
        // form and unlocks no entry — `single_form` is unchanged — but the
        // numbers it grants are TIMED, so the fight needs both halves.
        //
        // The tiers are found by what they SAY (`states_incarnon_window`) and
        // not by id, so the next Genesis needs no edit here.
        let mut d = FightParams::for_panel(&panel, arena, &arcane_fx, || {
            let unarmed: Vec<&str> = evo_refs
                .iter()
                .copied()
                .filter(|id| !wfsim_engine::data::evolutions::states_incarnon_window(id))
                .collect();
            resolve_for(&base_for(v, single_form, &unarmed), refs, policy, tenno)
        });
        d.infinite_reserve = panel.reserve_is_infinite(infinite_ammo);
        ammo.apply(&mut d);
        // Frenzy is the WEAPON's passive: it persists across its forms, so it rides whichever one is fired.
        d.frenzy = frenzy_single;
        d.locked_buffs = frenzy_locks.to_vec();
        (panel, d)
    }
}

/// THE MODS A REQUEST NAMES, resolved against the weapon's own pool.
///
/// A FUNCTION BECAUSE EVERY SEAT DOES IT. The fight can hold more than one
/// build now, and a second one resolving its mods through a second copy of
/// this would be a second answer to "is this mod on this weapon" — which is
/// the class of defect this repo keeps closing, not opening.
///
/// `Err` is the answer already shaped for the wire, so a caller returns it.
fn seat_mods<'a>(
    v: &Value,
    info: &'static WeaponInfo,
    evo_refs: &[&str],
    pool: &'a [ModDef],
) -> Result<Vec<&'a ModDef>, Value> {
    // No count validation here: the sim runs whatever it is given — slot
    // legality (8 main + 1 exilus) is the UI's job, and the engine resolves any
    // mod list honestly.
    if let Err(e) = riven_stat_ids_ok(v, info) {
        return Err(err_json(e));
    }
    let mod_ids: Vec<String> = v
        .get("mods")
        .and_then(|x| x.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|m| m.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let mut refs: Vec<&ModDef> = Vec::with_capacity(mod_ids.len());
    for id in &mod_ids {
        match pool.iter().find(|m| m.id == id) {
            Some(m) => refs.push(m),
            None => return Err(err_json(mod_not_here(id, info, evo_refs))),
        }
    }
    // Reject family collisions (wiki Incompatible mods).
    for i in 0..refs.len() {
        for j in (i + 1)..refs.len() {
            if let (Some(fi), Some(fj)) = (refs[i].family, refs[j].family) {
                if fi == fj {
                    return Err(err_json(format!(
                        "{} and {} are incompatible (both in the {fi} family)",
                        refs[i].id, refs[j].id
                    )));
                }
            }
        }
    }
    Ok(refs)
}

/// THE REST OF THE ROSTER — every seat beside the one being reported on, and
/// the weapon each of them brought.
///
/// EVERY READER OF A FIGHT RESOLVES IT HERE. `simulate` reports a number,
/// `log` replays the same engagement event by event and `optimize` ranks
/// builds inside it; a roster wired into one of the three and not the others
/// is a second fight that reads as the first — every row of that record true,
/// every rank in that search honest, and neither about the engagement the
/// reader asked for.
///
/// The weapon ids come back in seat order so a report can name what fired
/// without the reader looking it up in a roster that has moved on since.
pub(crate) fn seats_beside(
    v: &Value,
    arena: &wfsim_engine::arena::Arena,
    info: &'static WeaponInfo,
) -> Result<(Vec<FightParams>, Vec<String>), Value> {
    let mut seats = Vec::new();
    let mut weapons = vec![info.id.to_string()];
    for extra in v.get("also_acting").and_then(|x| x.as_array()).into_iter().flatten() {
        seats.push(seat_from(extra, arena)?);
        weapons.push(extra.get("weapon").and_then(|x| x.as_str()).unwrap_or_default().to_string());
    }
    Ok((seats, weapons))
}

/// [`seats_beside`], put straight into a fight the caller is holding.
pub(crate) fn seat_the_rest(
    params: &mut FightParams,
    v: &Value,
    arena: &wfsim_engine::arena::Arena,
    info: &'static WeaponInfo,
) -> Result<Vec<String>, Value> {
    let (seats, weapons) = seats_beside(v, arena, info)?;
    params.also_acting.extend(seats);
    Ok(weapons)
}

/// ANOTHER THING ACTING IN THIS FIGHT, resolved through the SAME path as the
/// build the answer is about.
///
/// A seat is described by a whole request of its own — its weapon, its mods,
/// its evolutions, its arcanes — and what it does NOT get to describe is the
/// FIGHT: the arena is handed in, so a second seat cannot be fighting a
/// different enemy at a different level than the first. That is the one thing
/// a caller could otherwise get wrong and nothing would say so.
///
/// It resolves through `parse_fight`, `mod_pool_with_rivens`, `seat_mods` and
/// `sim_params` — every one of them the function the reported build uses. A
/// second resolution path would be a second answer, and the point of the fight
/// holding n builds is that they are the same kind of thing.
pub(crate) fn seat_from(v: &Value, arena: &wfsim_engine::arena::Arena) -> Result<FightParams, Value> {
    let fight = parse_fight(v)?;
    let Fight {
        info, policy, evos, cycle_from, single_form, tenno, infinite_ammo,
        ammo_drops, pickup_range_m, landscape,
        frenzy_single, frenzy_locks, cycle_frenzy_lock, ..
    } = fight;
    let ammo = AmmoEconomy { drops: ammo_drops, pickup_range_m, landscape };
    let evo_refs: Vec<&str> = evos.iter().map(String::as_str).collect();
    let pool = mod_pool_with_rivens(v, info, &evo_refs);
    let refs = seat_mods(v, info, &evo_refs, &pool)?;
    let (_panel, params) = sim_params(
        v, info, policy, &evo_refs, &refs, &tenno, arena,
        cycle_from, single_form, infinite_ammo, ammo, frenzy_single, cycle_frenzy_lock,
        &frenzy_locks,
    );
    Ok(params)
}

/// WHAT THE BUILD COSTS IN CAPACITY AND FORMA — a report about the build, and
/// nothing the fight reads. Separated from the resolution above so a seat the
/// answer is not about does not compute it.
fn forma_of(info: &'static WeaponInfo, refs: &[&ModDef]) -> Value {
    // ---- forma legality (order-independent; needs only the mod multiset) ----
    // THE STANCE IS NOT ONE OF THE NINE. It has a slot of its own — that is the
    // whole reason it hands capacity back rather than taking it — so counting
    // it here billed it twice: once for a slot it does not occupy and once as
    // the grant below. A full melee build is eight mains, an exilus and a
    // stance, which is ten against nine innate slots, and the planner refused
    // it. The refusal was an `assert!`, so in the browser it was a worker that
    // died mid-fight without a word.
    let planned: Vec<PlannedMod> = refs
        .iter()
        .filter(|m| m.stance.is_none())
        .map(|m| PlannedMod {
            base_drain: m.base_drain,
            polarity: m.polarity,
        })
        .collect();
    // CAPACITY IS NOT A CONSTANT. It follows the weapon's rank, and a rank-40
    // weapon reaches 80 — the literal 60 here was a rank-30 answer standing in
    // for the rule (docs/INVESTMENT.md). `fit` owns the whole question: the
    // rank the Forma buy, the capacity that gives, and the bill by item.
    let inv = wfsim_engine::rules::capacity::Investment::default();
    // …AND THE STANCE HANDS CAPACITY BACK rather than taking it, which is why
    // the panel's own bill has to ask for it too: a melee build reads five to
    // ten points of headroom the weapon's rank did not buy.
    let stance = refs.iter().find(|m| m.stance.is_some()).map(|m| {
        wfsim_engine::rules::capacity::StanceSlot {
            mod_polarity: m.polarity,
            slot_polarity: wfsim_engine::data::weapons::stance_polarity(&info.id),
        }
    });
    match wfsim_engine::rules::capacity::fit(
        wspec(&info.id).max_rank,
        &innate_slots_for(&info.id),
        &planned,
        inv,
        stance,
    ) {
        Ok(f) => json!({
            "legal": true,
            "used": f.cost.total(),
            "regular": f.cost.regular,
            "omni": f.cost.omni,
            "umbra": f.cost.umbra,
            "total_drain": f.drain,
            "rank": f.rank,
            "cap": f.capacity,
        }),
        Err(e) => json!({ "legal": false, "error": e }),
    }

}

fn simulate_from(v: &Value, work: Work, on_run: &mut impl FnMut(u32, u32)) -> Value {
    // THE FIGHT, parsed by the ONE function that parses it. The optimizer
    // calls the same one — see `parse_fight`.
    let fight = match parse_fight(v) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let Fight {
        info, policy, buff_cfg, denied_buff_triggers, arena, evos, cycle_from, single_form,
        enemy_name, metric, level, steel_path, eximus, tenno, infinite_ammo, runs, seed,
        ammo_drops, pickup_range_m, landscape,
        frenzy_single, frenzy_locks, cycle_frenzy_lock, ..
    } = fight;
    let ammo = AmmoEconomy { drops: ammo_drops, pickup_range_m, landscape };
    let evo_refs: Vec<&str> = evos.iter().map(String::as_str).collect();

    let p = mod_pool_with_rivens(v, info, &evo_refs);
    let refs = match seat_mods(v, info, &evo_refs, &p) {
        Ok(r) => r,
        Err(e) => return e,
    };

    // ---- enemy / target ----
    // The target's pools, for the report. Read off the arena rather than kept
    // beside it: one target, one place it lives.
    let (og, sh, hp, ar) = (
        arena.target.overguard(),
        arena.target.max_shield(),
        arena.target.max_health(),
        arena.target.armor(),
    );

    // WHAT THE BUILD COSTS — a REPORT, not a resolution. Nothing the fight does
    // depends on it, which is why it is a function of its own: a seat that is
    // not the one being reported on resolves without ever asking.
    let forma = forma_of(info, &refs);

    // ---- resolve panel(s) and build sim params, per weapon ----
    // Either ONE registered form, or the real two-form cycle (which needs the
    // gauge form and the form it transforms out of, so it resolves both).
    // THE ARCANE IS RESOLVED FIRST, because params are built with it rather
    // than assigned it afterwards: `from_panel` is where a build meets an
    // arcane (Primary Compression reads THIS build's blast radius, a stat lock
    // silences an arcane's buff), and an argument cannot be forgotten the way
    // a follow-up assignment can — the Incarnon cycle's inner base form never
    // got one.
    let (report_panel, mut params) = sim_params(
        v, info, policy, &evo_refs, &refs, &tenno, &arena,
        cycle_from, single_form, infinite_ammo, ammo, frenzy_single, cycle_frenzy_lock,
        &frenzy_locks,
    );
    params.sample_by = metric.run;
    // EVERYTHING ELSE ACTING IN THIS FIGHT. Each entry is a request of its own
    // and resolves through the same path; the ARENA is this fight's, so no
    // seat can quietly be fighting a different enemy.
    let seat_weapons = match seat_the_rest(&mut params, v, &arena, info) {
        Ok(w) => w,
        Err(e) => return e,
    };
    // An arcane the weapon cannot seat is an ERROR here, not a silent drop:
    // the sim is the one place a visitor is owed a reason.
    for (pool, aid, _) in arcane_choices(v, info) {
        if wfsim_engine::data::arcanes::for_slot(&pool, &aid).is_none() {
            return err_json(match wfsim_engine::data::arcanes::slot_of(&aid) {
                Some(s) => format!(
                    "{aid} is a {s} arcane — {} seats {}",
                    info.name,
                    info.arcane_pools.join(" + ")
                ),
                None => format!("unknown arcane id: {aid}"),
            });
        }
    }
    // ---- apply the per-buff configured policy onto the live specs ----
    // (weapon-scoped: recurses into the incarnon cycle's base form). Frenzy is
    // already applied above (cycle lock at construction / single-form vector).
    if let Some(cfg) = &buff_cfg {
        params.apply_buff_config(cfg);
    }
    // …then what the fight refuses to hand out: the cards say where a run
    // OPENS, this says what it can EARN.
    params.deny_buff_triggers(&denied_buff_triggers);
    let report_panel = &report_panel;

    // ---- run ----
    // THE PER-RUN SERIES, only when the caller says it will pair with it. Every
    // run of one build is drawn from the same luck as the same-numbered run of
    // another, so a comparison between two builds is PAIRED — and a paired
    // comparison cannot be assembled from two `mean ± σ/√n` summaries, which
    // describe each build alone. The quick calc is the caller; nobody else pays
    // the length.
    let want_series = get_bool(v, "run_series", false);
    // A SHARD STOPS HERE. Everything above is the fight; everything below is
    // the report, and a worker running an eighth of the runs has no business
    // producing one.
    if let Work::Shard(from, count) = work {
        let mut tick = |done: u32| on_run(done, count);
        let s =
            wfsim_engine::fight::shard(&params, from, count, seed, want_series, &mut tick);
        return serde_json::to_value(&s).unwrap_or_else(|e| err_json(format!("shard: {e}")));
    }
    let (s, series) = match work {
        // ALREADY RUN, by a fleet of workers — this call only finishes it.
        Work::Merged(m) => {
            let n = m.runs;
            m.finish(&params, n)
        }
        _ => {
            let mut tick = |done: u32| on_run(done, runs);
            if want_series {
                wfsim_engine::fight::monte_carlo_series_reporting(&params, runs, seed, &mut tick)
            } else {
                (
                    wfsim_engine::fight::monte_carlo_reporting(&params, runs, seed, &mut tick),
                    Default::default(),
                )
            }
        }
    };

    let damage: Vec<Value> = report_panel
        .damage
        .iter_nonzero()
        .map(|(t, val)| json!({ "type": format!("{t:?}"), "value": val }))
        .collect();

    // TWO KINDS OF NUMBER, kept apart. Every top-level figure is a MEAN over the
    // runs: it is what ranks and what the headline shows. `m` is the BENCHMARK
    // FIGHT (`Summary::median_run`), and the damage meter, the DPS curve, the
    // replay and `sample` are that one run — they agree with each other, not
    // with the means.
    let m = &s.median_run;
    // THE NAMES ARE DERIVED, not listed. This was a hand-written table of
    // fifteen that had to stay in `DamageType`'s declaration order, and the
    // enum has seventeen variants — so the day a weapon first dealt TAU
    // (the Haalvu) every per-type array in the engine went out of
    // range and this table would have named the wrong types if it had not.
    let type_name = |i: usize| -> String {
        let n = wfsim_engine::rules::damage::DamageType::ALL[i].name();
        let mut c = n.chars();
        match c.next() {
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            None => String::new(),
        }
    };
    let status_damage = &m.sources;
    // A WEAPON-damage row expands into the vector that dealt it — a status
    // row is already one type, which is what a proc is. Parts are ordered
    // biggest-first, the same rule the rows themselves follow.
    let by_type = |split: &[f64; wfsim_engine::rules::damage::DamageType::ALL.len()]| -> Option<Value> {
        let mut parts: Vec<(String, f64)> = split
            .iter()
            .enumerate()
            .filter(|(_, v)| **v > 0.0)
            .map(|(i, &v)| (type_name(i), v))
            .collect();
        if parts.is_empty() {
            return None;
        }
        parts.sort_by(|a, b| b.1.total_cmp(&a.1));
        Some(json!(parts
            .iter()
            .map(|(t, v)| json!({ "type": t, "dmg": v }))
            .collect::<Vec<Value>>()))
    };
    let mut sources: Vec<(String, f64, Option<Value>)> = vec![
        ("direct".to_string(), status_damage.direct, by_type(&status_damage.direct_by_type)),
        ("radial".to_string(), status_damage.radial, by_type(&status_damage.radial_by_type)),
        // The lingering FIELD is its own bucket — on the Torid it is most of the
        // output, and leaving it out silently lost it from the damage meter.
        ("field".to_string(), status_damage.field, by_type(&status_damage.field_by_type)),
        // Cascadia Empowered's instance matches the PROC's type, so this row
        // expands like the weapon-damage ones (user's rule for the direct row,
        // 2026-08-01: the damage has elements, so the meter should say which).
        ("arcane".to_string(), status_damage.arcane_on_status, by_type(&status_damage.arcane_by_type)),
        // A SYNDICATE RADIAL is its own row for the same reason the field is:
        // it is neither the weapon's hit nor a status tick, it lands on its own
        // clock, and folding it into "direct" would credit the build for damage
        // no mod on it scaled.
        ("syndicate".to_string(), status_damage.syndicate, by_type(&status_damage.syndicate_by_type)),
        // AN EXTRA HIT is its own row too, and it is the one row the build
        // cannot move directly: Xata's Whisper takes a percentage of everything
        // else on this list, so a player tuning mods watches it follow. Folding
        // it into "direct" would hide that a fifth of the output is an ability's
        // and vanishes when the buff does.
        ("extra hit".to_string(), status_damage.extra_hit, by_type(&status_damage.extra_hit_by_type)),
    ];
    sources.extend(
        status_damage.status
            .iter()
            .enumerate()
            .map(|(i, &v)| (type_name(i), v, None)),
    );
    sources.retain(|(_, v, _)| *v > 0.0);
    sources.sort_by(|a, b| b.1.total_cmp(&a.1));
    let damage_sources: Vec<Value> = sources
        .iter()
        .map(|(k, v, parts)| match parts {
            Some(p) => json!({ "source": k, "dmg": v, "by_type": p }),
            None => json!({ "source": k, "dmg": v }),
        })
        .collect();
    // One-second buckets, sliced to the engagement's actual duration.
    let nb = (s.duration_seconds.ceil() as usize).clamp(1, m.curve.buckets().len());

    // THE REPLAY: the median engagement, re-run from the RNG state it started
    // from and sampled into frames. Buff series ride the same frames as the
    // pools, because "what were my stacks when its overguard broke" is one
    // question.
    //
    // OPT-IN, and that is not a micro-optimisation: the marginal-gain scan
    // calls this endpoint once per CANDIDATE — seventy mods on an axis — and
    // shows none of it. Only the Simulator's own Run asks for it, and pays one
    // extra engagement plus the frames on the wire.
    let replay = if get_bool(v, "replay", false) {
        // WHO THE READER ASKED FOR, by `formation::FoeSpec::id`. Absent, the
        // replay picks the aimed body and the hardest-hit few, which is what a
        // first Run wants; present, it follows exactly these — the same fight,
        // re-run from the same `rng_state`, so a body followed on the second
        // asking gets the series it would have had on the first.
        //
        // AN UNKNOWN ID IS NOT AN ERROR: it drops out of the list, and a list
        // that empties falls back to the default. A reader clicking a body that
        // took nothing gets the ordinary report rather than a failure.
        let want: Vec<usize> = v
            .get("replay_follow")
            .and_then(|a| a.as_array())
            .into_iter()
            .flatten()
            .filter_map(|x| x.as_str())
            .filter_map(|id| {
                params.others.iter().position(|f| f.id == id).map(|i| i + 1)
            })
            .collect();
        // THE FIGHT TO REPLAY, and by default it is this call's own median run.
        // A caller that already HAS a run — the `run` key this endpoint answers
        // with — hands it back, and gets that fight's frames rather than a new
        // fight's: which is what makes following one more body a question about
        // the report on screen instead of a second report that merely agrees.
        // With `runs: 1` beside it the whole call is one engagement.
        let pinned = v.get("run").and_then(|x| x.as_array()).map(|a| {
            let half = |i: usize| a.get(i).and_then(Value::as_u64).unwrap_or(0);
            (half(0) << 32) | (half(1) & 0xffff_ffff)
        });
        let state = pinned.unwrap_or(m.rng_state);
        let rep = if want.is_empty() {
            wfsim_engine::fight::replay(&params, state, wfsim_engine::fight::REPLAY_FRAMES)
        } else {
            wfsim_engine::fight::replay_following(
                &params,
                state,
                wfsim_engine::fight::REPLAY_FRAMES,
                &want,
            )
        };
        // The panel's OWN shapes, one array per series instead of one number.
        // A frame is not a separate format: `kpi` mirrors the KPI row and
        // `sources` mirrors `damage_sources` key for key, so the client draws
        // an instant of the fight with the same code that draws the end of it.
        let pel = |f: &wfsim_engine::fight::Frame| f.pellets.max(1) as f64;
        let series = |g: fn(&wfsim_engine::fight::Frame) -> f64| {
            rep.frames.iter().map(&g).map(r1).collect::<Vec<_>>()
        };
        // Every (source, type) pair that carries damage BY THE END — the set
        // only ever grows, so the last frame names all of them and an earlier
        // frame simply reads zero there.
        let last = rep.frames.last().cloned().unwrap_or_default();
        let pick = |f: &wfsim_engine::fight::Frame, k: &str| -> (f64, [f64; wfsim_engine::rules::damage::DamageType::ALL.len()]) {
            match k {
                "direct" => (f.sources.direct, f.sources.direct_by_type),
                "radial" => (f.sources.radial, f.sources.radial_by_type),
                "field" => (f.sources.field, f.sources.field_by_type),
                "arcane" => (f.sources.arcane_on_status, f.sources.arcane_by_type),
                "syndicate" => (f.sources.syndicate, f.sources.syndicate_by_type),
                "extra hit" => (f.sources.extra_hit, f.sources.extra_hit_by_type),
                other => {
                    let i = (0..wfsim_engine::rules::damage::DamageType::ALL.len())
                        .position(|i| type_name(i) == other)
                        .unwrap_or(0);
                    (f.sources.status[i], [0.0; wfsim_engine::rules::damage::DamageType::ALL.len()])
                }
            }
        };
        let rp_sources: Vec<Value> = sources
            .iter()
            .map(|(name, _, by)| {
                let dmg: Vec<f64> =
                    rep.frames.iter().map(|f| pick(f, name).0.round()).collect();
                let types: Vec<Value> = if by.is_some() {
                    // SIZED FROM THE ENUM, never a literal. This read `0..15`
                    // while `DamageType::ALL` has seventeen, so the last two —
                    // Tau and Cinematic — could never appear in a replay's
                    // per-type breakdown however much of them a weapon dealt.
                    // Same fault class as the `[f64; 15]` arrays in `dummy`,
                    // found the same way: by counting.
                    (0..wfsim_engine::rules::damage::DamageType::ALL.len())
                        .filter(|&i| pick(&last, name).1[i] > 0.0)
                        .map(|i| {
                            json!({
                                "type": type_name(i),
                                "dmg": rep.frames.iter()
                                    .map(|f| pick(f, name).1[i].round())
                                    .collect::<Vec<_>>(),
                            })
                        })
                        .collect()
                } else {
                    Vec::new()
                };
                json!({ "source": name, "dmg": dmg, "by_type": types })
            })
            .collect();
        json!({
            "frame_seconds": rep.frame_seconds,
            // ONE ROSTER PER SEAT, in `combatants`' order — a buff is a seat's
            // and two seats are two builds, so the second one's rows are its
            // own rather than the wielder's names over its numbers.
            //
            // Ids are the buff cards' own — the client joins on them for names.
            // (id, stack ceiling, how the stacks read as a NUMBER where the
            // ceiling is one). `value` is null on every ordinary buff, which is
            // most of them — see `fight::StackValue`. It is a key on BOTH
            // rosters rather than on this one, because the debuff table is
            // drawn by the same component and `check_debuff_coverage` asserts
            // the two shapes match.
            "buffs": rep.buffs.iter()
                .map(|seat| seat.iter()
                    .map(|b| json!({ "id": b.id, "max": b.max_stacks, "value": value_json(b.value) }))
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            "t": rep.frames.iter().map(|f| (f.t * 100.0).round() / 100.0).collect::<Vec<_>>(),
            "og": series(|f| f.overguard),
            "sh": series(|f| f.shield),
            "hp": series(|f| f.health),
            "dmg": series(|f| f.damage),
            "kills": rep.frames.iter().map(|f| f.kills).collect::<Vec<_>>(),
            "kpi": {
                "dps": rep.frames.iter()
                    .map(|f| if f.t > 0.0 { (f.damage / f.t).round() } else { 0.0 })
                    .collect::<Vec<_>>(),
                "procs": rep.frames.iter().map(|f| f.procs).collect::<Vec<_>>(),
                "shots": rep.frames.iter().map(|f| f.shots).collect::<Vec<_>>(),
                "reloads": rep.frames.iter().map(|f| f.reloads).collect::<Vec<_>>(),
                "transforms": rep.frames.iter().map(|f| f.transforms).collect::<Vec<_>>(),
                "crit_tier": series(|f| f.crit_tier_sum as f64 / f.pellets.max(1) as f64),
                "crit_rate": rep.frames.iter()
                    .map(|f| r3(f.crits as f64 / pel(f))).collect::<Vec<_>>(),
                "big_crit_rate": rep.frames.iter()
                    .map(|f| r3(f.big_crits as f64 / pel(f))).collect::<Vec<_>>(),
                "headshot_rate": rep.frames.iter()
                    .map(|f| r3(f.headshots as f64 / pel(f))).collect::<Vec<_>>(),
            },
            "sources": rp_sources,
            // Per SEAT then per BUFF, the exact shape `dstacks` has on the
            // other side of the fight. A flat array per series is what a chart
            // wants, and it compresses far better than 600 tiny objects.
            "stacks": (0..rep.buffs.len())
                .map(|si| (0..rep.buffs[si].len())
                    .map(|i| rep.frames.iter()
                        .map(|f| f.stacks.get(si).and_then(|s| s.get(i)).copied().unwrap_or(0))
                        .collect::<Vec<_>>())
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            // THE SAME TWO FIELDS FOR THE TARGET. The roster is a constant of
            // the engine rather than a property of the build — a debuff is the
            // target's, and a status the run never applied draws a flat zero,
            // which is the answer to "was Corrosive ever up".
            "debuffs": wfsim_engine::fight::DEBUFF_ROSTER
                .iter()
                // ZERO IS "NO CEILING", which is the convention `buff_roster`
                // already uses in as many words — so the two rosters stay the
                // SAME SHAPE, which is the property `check_debuff_coverage`
                // asserts and the whole reason one component draws both.
                // Adding an `uncapped` field here instead broke that symmetry
                // on the first run.
                .map(|(id, cap)| json!({ "id": id, "max": cap.unwrap_or(0), "value": Value::Null }))
                .collect::<Vec<_>>(),
            // ONE TABLE PER BODY THE REPLAY FOLLOWED, and the first is the
            // aimed one. `dstacks` was a single body's until 2026-08-17, which
            // was the whole truth while a fight had one body.
            //
            // THE CAP IS ON SCREEN, never silent: `tracked` says who was
            // followed and `bodies` says who took damage, so a reader can see
            // that five more were hit and not followed. A cap nobody is told
            // about reads as "that is everyone".
            // WHICH FIGHT THESE FRAMES ARE FROM. Ordinarily this call's own
            // median run and the same as the top-level `run`; when a caller
            // PINNED one it is that, and the two differ — so the frames say
            // which they came from rather than leaving a reader to assume they
            // match the numbers beside them.
            "run": [(state >> 32) as u32, (state & 0xffff_ffff) as u32],
            // …AND WHO DEALT IT, frame by frame. The meter's series says what
            // KIND each instance was; this says WHOSE it was, and a replay
            // that carried only the first cannot follow one combatant's
            // contribution through a fight that has more than one.
            "dealt": params.combatant_ids().iter().enumerate()
                .map(|(i, _)| rep.frames.iter()
                    .map(|f| r1(f.dealt.0.get(i).copied().unwrap_or(0.0)))
                    .collect::<Vec<_>>())
                .collect::<Vec<_>>(),
            "combatants": params.combatant_ids(),
            "tracked": rep.tracked,
            "dstacks": (0..rep.tracked.len())
                .map(|b| {
                    (0..wfsim_engine::fight::DEBUFF_ROSTER.len())
                        .map(|i| {
                            rep.frames
                                .iter()
                                .map(|f| f.debuffs.get(b).and_then(|s| s.get(i)).copied().unwrap_or(0))
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
        })
    } else {
        Value::Null
    };

    // WHAT THE KILLS ACTUALLY COST — everything that landed, less the two
    // kinds of waste. It is the denominator the rate is read against, so a
    // rate of 1.0 means the fight spent as much on corpses and broken pools as
    // it did on killing.
    let needed = s.mean_effective_damage - s.mean_overkill - s.mean_spilled;
    let overkill_rate = if needed > 0.0 { s.mean_overkill / needed } else { 0.0 };

    json!({
        "ok": true,
        // **THE LIST THIS FIGHT RAN**, as SimC writes one, off the params the
        // loop was handed — not composed again here. The page prints what comes
        // back, so what it shows is what ran.
        "apl": params.apl().0.iter().map(wfsim_engine::data::apl::Rule::to_simc).collect::<Vec<_>>(),
        // WHICH ENGAGEMENT THE BENCHMARK FIGHT IS — its own RNG state, as TWO
        // u32 halves.
        //
        // It is the handle `/api/log` needs: the combat record re-runs the same
        // fight from this state and gets the same numbers bit for bit, which is
        // what makes the log the report's OWN fight rather than a similar one.
        // Two halves because a JSON number in JavaScript is a double and a
        // 64-bit state comes back ROUNDED — the same lesson `fight::RunKey`
        // records, learnt the same way.
        "run": [(m.rng_state >> 32) as u32, (m.rng_state & 0xffff_ffff) as u32],
        // THE BENCHMARK FIGHT'S OWN FIGURES, under the same field names as the
        // means, so a metric's `field` reads either and the page can say how
        // far this one run sits from the average beside it.
        "sample": {
            "score": m.kill_progress,
            "kills": m.kills,
            "dps": m.effective_damage() / s.duration_seconds.max(1e-9),
        },
        "score": s.mean_kill_progress,
        "kills": s.mean_kills,
        // WHAT THE FIGHT SPENT ON NOTHING, per run. `overkill` is damage that took a bar past zero —
        // a unit dies once however far past it goes — and `spilled` is what a
        // broken overguard or shield threw away rather than passing down. The
        // RATE's denominator is what the kills actually cost, so it reads as
        // "for every point that was needed, this many were wasted" and is not
        // capped at 1.
        "overkill": s.mean_overkill,
        "spilled": s.mean_spilled,
        "overkill_rate": overkill_rate,
        // THE AVERAGE VIRAL PILE THE DAMAGE WAS DEALT THROUGH — over every
        // body and every run. The debuff chart follows eight bodies of a formation that
        // may be nineteen, so it can show a pile rising and cannot say what
        // the fight as a whole was multiplied by.
        //
        // ABSENT RATHER THAN ZERO on a build that never applies Viral, the
        // same way `self_damage` is: a reader is not shown a Viral line for a
        // fight that had none.
        "virus_stacks": (s.mean_virus_stacks > 0.0).then_some(s.mean_virus_stacks),
        // …AND WHAT WAS LEFT OF ITS ARMOUR when that damage arrived. Absent on
        // an unarmoured target, where 1.0 would read as "nothing was stripped"
        // rather than as "there was nothing to strip".
        "armor_left": (ar > 0.0).then_some(s.mean_armor_left),
        "kills_std": s.std_kills,
        // HOW FAR THE MEAN CAN BE FROM THE TRUTH. The σ is reported because the caller cannot estimate it. Running the
        // reference a SECOND time at another seed and calling the gap its
        // resolution is one sample of a spread, and on identical inputs that
        // answer ranges 0.7%–11.2% — the same scan would suppress every chip or
        // none of them at random. The server has all N runs; it says the spread
        // it already computed.
        "score_se": s.std_kill_progress / f64::from(runs.max(1)).sqrt(),
        // …and the runs behind them, for a caller that will PAIR against
        // another build. Absent unless asked for: see `run_series` above.
        "score_runs": series.kill_progress,
        "dps_runs": series
            .effective
            .iter()
            .map(|e| e / s.duration_seconds.max(1e-9))
            .collect::<Vec<f64>>(),
        "dps_se": s.std_effective_damage
            / f64::from(runs.max(1)).sqrt()
            / s.duration_seconds.max(1e-9),
        "kills_min": s.min_kills,
        "kills_max": s.max_kills,
        "dps": s.mean_effective_damage / s.duration_seconds.max(1e-9),
        "shots": s.mean_shots,
        "pellets": s.mean_pellets,
        "crit_rate": s.mean_crit_rate,
        "big_crit_rate": s.mean_big_crit_rate,
        // WHAT THE BUILD CHARGED ITS OWNER, by type and never applied — see
        // `SelfDamage`. Absent rather than zero when nothing charged anything,
        // so a reader is not shown a cost line for a build that has none.
        "self_damage": (s.mean_self_damage.total() > 0.0).then(|| json!({
            "total": s.mean_self_damage.total(),
            "by_type": s.mean_self_damage.parts().into_iter()
                .map(|(t, v)| json!({ "type": t.name(), "amount": v }))
                .collect::<Vec<_>>(),
        })),
        // The tier, because the RATE stops saying anything past 100% crit
        // chance: every pellet crits, so it reads 1.0 whether the build is
        // at 110% or 410%. Uncapped — red is not the top.
        "crit_tier": s.mean_crit_tier,
        "headshot_rate": s.mean_headshot_rate,
        "procs": s.mean_procs,
        // THE SPEEDRUN SET. `dps` is the whole engagement; `burst_dps` is the
        // same damage over the time the weapon was actually firing, which is
        // what a room-clear is paced by. TTK carries its spread because a mean
        // alone reads as a promise.
        "burst_dps": s.burst_dps,
        "downtime": s.mean_downtime_seconds,
        "ttk": { "mean": s.ttk_mean, "median": s.ttk_median, "p90": s.ttk_p90, "runs": s.ttk_runs },
        "first_magazine": s.mean_first_magazine,
        "max_hit": s.max_hit,
        "mean_max_hit": s.mean_max_hit,
        "damage_per_shot": s.damage_per_shot,
        "damage_per_pellet": s.damage_per_pellet,
        // EVERY HIT SORTED BY WHAT IT WAS — [head][tier], tier capped at 2.
        "field_ticks": s.mean_field_ticks,
        "damage_sources": damage_sources,
        "timeline": m.curve.buckets()[..nb].to_vec(),
        "replay": replay,
        "transforms": s.mean_transforms,
        // COUNTED, NOT FOUGHT — `data::weapons::SpawnOnKillSpec`. Absent on
        // every weapon that leaves nothing, so the page draws no row for them.
        "ghosts": (s.mean_ghosts > 0.0).then_some(s.mean_ghosts),
        "ghosts_peak": (s.mean_ghosts > 0.0).then_some(s.ghosts_peak),
        "reloads": s.mean_reloads,
        // WHAT THE BODIES RESUPPLIED, in rounds. Absent where nothing was
        // picked up — an infinite reserve takes none — so the page draws the
        // row only for a fight the ammo economy decides something in.
        "picked_up_ammo": (s.mean_picked_up_ammo > 0.0).then_some(r3(s.mean_picked_up_ammo)),
        "duration": s.duration_seconds,
        "runs": s.runs,
        "panel": {
            "damage": damage,
            "total": report_panel.damage.total(),
            "crit_chance": report_panel.crit_chance,
            "crit_damage": report_panel.crit_damage,
            "status_chance": report_panel.status_chance,
            "fire_rate": report_panel.fire_rate,
            "multishot": report_panel.multishot,
            "modified_base": report_panel.modified_base,
            "co_per_type": report_panel.co_per_type,
        },
        "forma": forma,
        // WHO TOOK WHAT, by NAME. `damage_by_body` has been per body since the
        // formation landed and had no way to say WHOSE — the index into a list
        // is not a name, and a reader deleting the body in front would renumber
        // everything behind it.
        //
        // ONLY THE ONES THAT TOOK SOMETHING. A 19x19 ruler is 361 bodies and a
        // chaining beam reaches thirteen; listing 348 zeroes would bury the
        // thirteen, which is the same reason the debuff table drops the rows a
        // run never touched.
        //
        // MEAN OVER THE RUNS, like every other figure here.
        // WHO FIRED, AND WHAT EACH OF THEM DEALT — the mirror of `bodies`,
        // which cuts the same total by who TOOK it. Both are booked through
        // `ledger::settle`, so the two come to one number.
        //
        // A SEAT NOBODY FIRED FROM IS STILL LISTED, the opposite rule to
        // `bodies`: a roster that shrank to whoever dealt damage would read
        // "the companion fired nothing" and "there is no companion" the same
        // way, and those are different fights.
        "combatants": params.combatant_ids().iter().enumerate()
            .map(|(i, id)| {
                let c = s.by_seat.get(i).copied().unwrap_or_default();
                let runs = f64::from(s.runs.max(1));
                let pellets = f64::from(c.pellets.max(1));
                json!({
                    "id": id,
                    // WHAT IT BROUGHT. The engine knows no weapon names, so the
                    // seat's own id stays a slug and this is what a reader is
                    // shown beside it.
                    "weapon": seat_weapons.get(i).cloned().unwrap_or_default(),
                    "damage": s.mean_damage_by_combatant.0.get(i).copied().unwrap_or(0.0),
                    // WHAT THIS SEAT DID, and every rate here is ITS OWN — a
                    // crit rate is a seat's or it is nobody's. Means over the
                    // runs, like every other figure in this report.
                    "shots": f64::from(c.shots) / runs,
                    "pellets": f64::from(c.pellets) / runs,
                    "crit_rate": f64::from(c.crits) / pellets,
                    "big_crit_rate": f64::from(c.big_crits) / pellets,
                    "crit_tier": f64::from(c.crit_tier_sum) / f64::from(c.crits.max(1)),
                    "procs": f64::from(c.procs) / runs,
                    "reloads": f64::from(c.reloads) / runs,
                    "transforms": f64::from(c.transforms) / runs,
                    // THE FIGHT'S KILLS ARE THE FIGHT'S; this is what this seat
                    // finished. Without the split every seat claims them all.
                    "finishes": f64::from(c.finishes) / runs,
                })
            })
            .collect::<Vec<_>>(),
        "bodies": std::iter::once((
            arena.target_id.clone(),
            arena.target_at,
            true,
        ))
        .chain(
            arena
                .others
                .iter()
                .map(|f| (f.id.clone(), f.at, false)),
        )
        .enumerate()
        .filter_map(|(i, (id, at, aimed))| {
            let d = s.mean_damage_by_body.0[i];
            (d > 0.0).then(|| json!({
                "id": id,
                "aimed": aimed,
                "at": [at.x, at.y],
                "damage": d,
            }))
        })
        .collect::<Vec<_>>(),
        "target": {
            "name": enemy_name,
            "level": level,
            "steel_path": steel_path,
            // What was actually fought, not what was asked for — the default
            // is the unit's own answer, so a caller that said nothing still
            // needs telling which variant it got.
            "eximus": eximus,
            "overguard": og,
            "shield": sh,
            "health": hp,
            "armor": ar,
        },
    })
}

#[cfg(test)]
mod asset_tests {
    use super::*;
    use crate::panel::panel_json;
    use crate::registry::{assets, weapons};
    use crate::rivens::riven_class;

    /// Every weapon, mod and arcane in `data/` must have an image entry.
    ///
    /// A missing one does not fail anything at runtime — it renders as
    /// nothing, and the card just looks empty (Verglas Prime and ten mods
    /// shipped that way,). The map is filled by
    /// `scripts/gen_assets.py` from the committed WFCD export, so a failure
    /// here is one command away from fixed, and this is what makes anyone
    /// run it.
    /// Every weapon can be given a riven, so every weapon must reach a stat
    /// A FORMATION REACHES THE SIM, and each body in it is wholly its own. The request path for the multi-enemy arena, end to
    /// end: nine bodies at 3 m, the chain spreading into them, and a Primed
    /// Firestorm radius worth four times as much there as it is worth nothing
    /// against one.
    #[test]
    fn a_formation_in_the_request_reaches_the_fight() {
        let grid = |n: usize| -> Vec<serde_json::Value> {
            // Eight neighbours of a body standing at [0, 0.4] — the arena's
            // own opening position — laid out 3 m apart.
            (0..n)
                .map(|i| {
                    let (c, r) = ((i % 3) as f64 - 1.0, (i / 3) as f64);
                    serde_json::json!({ "at": [c * 3.0, 0.4 + r * 3.0] })
                })
                .collect()
        };
        let req = |bodies: Vec<serde_json::Value>| {
            serde_json::json!({
                "weapon": "torid", "mode": "transformed",
                "mods": [], "evolutions": ["torid_evo1_incarnon_form"],
                "enemy": "corrupted_heavy_gunner", "level": 100,
                "runs": 4, "seed": 7, "duration": 6,
                "formation": bodies,
            })
        };
        // DAMAGE, not kills: the claim is that a chain spreads, and a lone
        // unmodded beam does not finish a level-100 body inside six seconds.
        let kpm = |v: &serde_json::Value| -> f64 {
            simulate_json(v).get("dps").and_then(Value::as_f64).unwrap_or(-1.0)
        };
        let raw = simulate_json(&req(Vec::new()));
        assert!(raw.get("error").is_none(), "{raw}");
        let alone = kpm(&req(Vec::new()));
        let crowd = kpm(&req(grid(8)));
        assert!(alone > 0.0, "the lone fight must run: {alone}");
        assert!(
            crowd > alone,
            "a formation must out-kill one body: {crowd} against {alone}"
        );

        // …AND EACH BODY IS ITS OWN. A level-1 neighbour beside a level-9999
        // one is two different fights in one arena, which is the claim that
        // makes this a formation rather than a multiplier.
        let mixed = simulate_json(&serde_json::json!({
            "weapon": "torid", "mode": "transformed", "mods": [],
            "evolutions": ["torid_evo1_incarnon_form"],
            "enemy": "corrupted_heavy_gunner", "level": 100,
            "runs": 2, "seed": 7, "duration": 4,
            "formation": [
                { "at": [3.0, 0.4], "level": 1 },
                { "at": [-3.0, 0.4], "level": 9999, "enemy": "thrax_centurion" },
            ],
        }));
        assert!(mixed.get("error").is_none(), "{mixed}");
        assert!(mixed.get("dps").and_then(Value::as_f64).unwrap_or(-1.0) > 0.0);

        // …AND THE CAP IS REFUSED RATHER THAN TRUNCATED. Derived from the
        // constant, not written out: this test said "at most 50" and broke the
        // day the cap moved, which is the same two-declarations bug the error
        // message itself is now careful to avoid.
        let cap = wfsim_engine::formation::MAX_BODIES;
        let too_many = simulate_json(&req((0..=cap)
            .map(|i| serde_json::json!({ "at": [i as f64 * 3.0, 5.0] }))
            .collect()));
        assert!(
            too_many
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("")
                .contains(&format!("at most {cap}")),
            "{too_many}"
        );
    }

    /// EVERY ENEMY HAS A NAME, AND THE RESULT SAYS WHOSE.
    ///
    /// A formation body was identified only by its index in the request until
    /// 2026-08-17 — enough for the ENGINE, which reads bodies by index and
    /// always will, and not enough for anything that has to talk ABOUT one
    /// ("我们场景的敌人应该要每个都有id，才对"). Every debuff, pool and
    /// DoT was already per body; what was missing was a way to say WHOSE.
    ///
    /// Asserts the three halves that make it useful: a name given is a name
    /// kept, a name omitted is filled in BY POSITION so every scenario written
    /// before ids existed still reads, and the ones that took NOTHING are not
    /// listed — a 19x19 ruler is 361 bodies and a chaining beam reaches
    /// thirteen.
    #[test]
    fn every_enemy_is_named_and_the_result_says_who_took_what() {
        let req = |formation: Vec<serde_json::Value>| {
            serde_json::json!({
                "weapon": "torid", "mode": "transformed",
                "mods": [], "evolutions": ["torid_evo1_incarnon_form"],
                "enemy": "corrupted_heavy_gunner", "level": 40,
                "runs": 3, "seed": 7, "duration": 6,
                "formation": formation,
            })
        };
        // A NAMED ONE beside the target, and two unnamed further out.
        let out = simulate_json(&req(vec![
            serde_json::json!({ "at": [1.5, 0.4], "id": "the-one-i-care-about" }),
            serde_json::json!({ "at": [3.0, 0.4] }),
            serde_json::json!({ "at": [90.0, 90.0] }),
        ]));
        assert!(out.get("error").is_none(), "{out}");
        let bodies = out.get("bodies").and_then(Value::as_array).expect("a roll call");
        let named: Vec<&str> =
            bodies.iter().filter_map(|b| b.get("id").and_then(Value::as_str)).collect();

        // THE AIMED BODY IS `e1` and says so.
        assert_eq!(named.first(), Some(&"e1"), "{named:?}");
        assert_eq!(
            bodies[0].get("aimed").and_then(Value::as_bool),
            Some(true),
            "the first entry is the body the weapon is on"
        );
        // A NAME GIVEN IS A NAME KEPT.
        assert!(named.contains(&"the-one-i-care-about"), "{named:?}");
        // …AND ONE OMITTED IS FILLED IN BY POSITION, from `e2`.
        assert!(named.contains(&"e3"), "the second unnamed body is e3: {named:?}");
        // THE FAR ONE TOOK NOTHING and is therefore not on the roll call: 90 m
        // is outside every radius, chain and sphere this weapon has.
        assert!(!named.contains(&"e4"), "a body that took nothing is not listed: {named:?}");
        // …and every entry that IS listed took something.
        assert!(
            bodies.iter().all(|b| b.get("damage").and_then(Value::as_f64).unwrap_or(0.0) > 0.0),
            "{bodies:?}"
        );
    }

    /// EVERY TOP-LEVEL FIGURE IS A MEAN, and `sample` is the upper-middle run by
    /// the scenario's metric — asserted on the wire, which is what the page and
    /// the board read.
    #[test]
    fn the_response_is_means_and_the_sample_is_the_middle_run_by_the_metric() {
        for (metric, field, series) in [("kpm", "score", "score_runs"), ("dps", "dps", "dps_runs")] {
            let req = serde_json::json!({
                "weapon": "torid", "mode": "transformed",
                "mods": ["serration", "split_chamber"],
                "evolutions": ["torid_evo1_incarnon_form"],
                "enemy": "corrupted_heavy_gunner", "level": 40,
                "runs": 8, "seed": 7, "duration": 10,
                "metric": metric, "run_series": true,
            });
            let r = simulate_json(&req);
            assert!(r.get("error").is_none(), "{r}");
            let mut runs: Vec<f64> =
                r[series].as_array().into_iter().flatten().filter_map(Value::as_f64).collect();
            assert_eq!(runs.len(), 8, "{metric}: {series}");
            let mean = runs.iter().sum::<f64>() / 8.0;
            let top = r[field].as_f64().unwrap_or(f64::NAN);
            assert!((top - mean).abs() <= mean.abs() * 1e-12 + 1e-12, "{metric}: {field} {top} vs mean {mean}");
            runs.sort_by(f64::total_cmp);
            let sample = r["sample"][field].as_f64().unwrap_or(f64::NAN);
            assert!(
                (sample - runs[4]).abs() <= runs[4].abs() * 1e-9 + 1e-9,
                "{metric}: sample {sample} is not the upper middle of {runs:?}"
            );
        }
    }

    /// A FLEET PRODUCES THE SAME REPORT AS ONE WORKER.
    ///
    /// `eight_shards_are_one_run` asserts it of the SUMMARY; this asserts it of
    /// the whole response, which is what a reader sees — every KPI, the damage
    /// meter, the histogram, the roll call, the replay's own frames. A merge
    /// that lost one field would pass the engine's test and still put a
    /// different number on the page.
    ///
    /// It compares the JSON with the replay taken out, because a replay is one
    /// engagement sampled at 600 instants and is not what sharding is about —
    /// the benchmark fight it replays is asserted in the engine.
    ///
    /// NUMBERS ARE COMPARED TO A PART IN 10^12, not bit for bit. Adding a
    /// thousand runs in eight groups and then combining differs from adding
    /// them in one sequence in the last bit or two, because floating-point
    /// addition is not associative. That is arithmetic ORDER and not a
    /// different answer; anything the merge actually lost would be off by far
    /// more than a part in a trillion.
    #[test]
    fn a_fleet_of_shards_reports_what_one_worker_reports() {
        let req = serde_json::json!({
            "weapon": "torid", "mode": "transformed",
            "mods": ["serration", "split_chamber"],
            "evolutions": ["torid_evo1_incarnon_form"],
            "enemy": "corrupted_heavy_gunner", "level": 40,
            "runs": 42, "seed": 7, "duration": 10,
            "formation": [
                { "at": [1.5, 0.4] }, { "at": [-1.5, 0.4] }, { "at": [0.0, 2.0] },
            ],
        });
        let whole = simulate_json(&req);
        assert!(whole.get("error").is_none(), "{whole}");

        // EIGHT SLICES of 42, so the last ones are short — the fleet that
        // actually runs.
        let mut shards = Vec::new();
        let mut at = 0u32;
        for k in 0..8u32 {
            let count = 42 / 8 + u32::from(k < 42 % 8);
            shards.push(simulate_shard_json(&req, at, count, &mut |_, _| {}));
            at += count;
        }
        assert_eq!(at, 42);
        assert!(
            shards.iter().all(|s| s.get("error").is_none()),
            "a shard failed: {shards:?}"
        );
        let merged = simulate_merged_json(&req, &shards);
        assert!(merged.get("error").is_none(), "{merged}");

        let strip = |mut v: Value| -> Value {
            if let Some(o) = v.as_object_mut() {
                o.remove("replay");
            }
            v
        };
        fn same(a: &Value, b: &Value, path: &str) {
            match (a, b) {
                (Value::Number(x), Value::Number(y)) => {
                    let (x, y) = (x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0));
                    assert!(
                        (x - y).abs() <= y.abs() * 1e-12 + 1e-12,
                        "{path}: {x} vs {y}"
                    );
                }
                (Value::Array(x), Value::Array(y)) => {
                    assert_eq!(x.len(), y.len(), "{path}: length");
                    for (i, (u, v)) in x.iter().zip(y).enumerate() {
                        same(u, v, &format!("{path}[{i}]"));
                    }
                }
                (Value::Object(x), Value::Object(y)) => {
                    let (mut kx, mut ky): (Vec<_>, Vec<_>) =
                        (x.keys().collect(), y.keys().collect());
                    kx.sort();
                    ky.sort();
                    assert_eq!(kx, ky, "{path}: keys");
                    for k in kx {
                        same(&x[k], &y[k], &format!("{path}.{k}"));
                    }
                }
                _ => assert_eq!(a, b, "{path}"),
            }
        }
        same(&strip(merged), &strip(whole), "");
    }

    /// MELEE INFLUENCE IS ON THE BUFF BAR, and locking it open changes the
    /// answer.
    ///
    /// It is a WINDOW rather than a grant, so neither the arcane loop nor the
    /// perk case above saw it and the one arcane a melee build is built around
    /// had no control at all. A fight's average is not what the card is worth
    /// while the window is open, and the reader has to be able to ask for
    /// either.
    #[test]
    fn melee_influence_is_a_buff_a_reader_can_hold_open() {
        // THE RULER'S OWN CROWD, at a level things die at: the window is worth
        // what it spreads, so a fight too small or too short to spread in
        // cannot tell the two apart.
        let req = |buffs: serde_json::Value| {
            let b = wfsim_engine::board::benchmarks::get("standard_multi_target").expect("the ruler");
            let mut m = serde_json::to_value(&b.scenario).expect("a scenario is json");
            let o = m.as_object_mut().expect("a mapping");
            o.insert("weapon".into(), serde_json::json!("praedos"));
            o.insert("mods".into(), serde_json::json!([
                "primed_pressure_point", "sacrificial_steel", "organ_shatter",
                "voltaic_strike", "shocking_touch", "condition_overload"]));
            o.insert("arcane".into(), serde_json::json!(["melee_influence"]));
            o.insert("arcane_rank".into(), serde_json::json!([5]));
            o.insert("level".into(), serde_json::json!(60));
            o.insert("steel_path".into(), serde_json::json!(false));
            o.insert("runs".into(), serde_json::json!(12));
            if !buffs.is_null() { o.insert("buffs".into(), buffs); }
            m
        };
        let listed: Vec<String> = panel_json(&req(Value::Null))
            .get("buffs")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|b| b.get("id").and_then(Value::as_str))
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            listed.iter().any(|id| id == "arcane:melee_influence"),
            "the window has no card: {listed:?}"
        );

        let score = |v: &Value| v.get("score").and_then(Value::as_f64).unwrap_or(f64::NAN);
        let earned = simulate_json(&req(Value::Null));
        let held = simulate_json(&req(serde_json::json!({
            "arcane:melee_influence": { "stacks": 1, "locked": true }
        })));
        assert!(earned.get("error").is_none(), "{earned}");
        assert!(held.get("error").is_none(), "{held}");
        assert!(
            score(&held) > score(&earned),
            "holding the window open bought nothing: {} against {}",
            score(&held),
            score(&earned),
        );

        // …AND ITS COVERAGE IS READ FROM THE FIGHT, not left at zero. The card
        // is a CLOCK, so how much of the engagement it was up for is the whole
        // question a reader brings to it — and a series nothing samples reads
        // as an arcane that never fired.
        let coverage = |v: &Value| {
            // SEAT 0 IS THE WIELDER, on both keys — `buffs` and `stacks` are
            // per seat, and this fight has only the one.
            let rp = v.get("replay")?;
            let roster = rp.get("buffs")?.as_array()?.first()?.as_array()?;
            let i = roster.iter().position(|b| {
                b.get("id").and_then(Value::as_str) == Some("arcane:melee_influence")
            })?;
            let frames = rp.get("stacks")?.as_array()?.first()?.as_array()?.get(i)?.as_array()?;
            let live = frames.iter().filter(|f| f.as_u64().unwrap_or(0) > 0).count();
            Some(100.0 * live as f64 / frames.len().max(1) as f64)
        };
        let mut with_replay = req(Value::Null);
        with_replay
            .as_object_mut()
            .expect("map")
            .insert("replay".into(), serde_json::json!(true));
        let replayed = simulate_json(&with_replay);
        let up = coverage(&replayed).expect("the window has a series");
        assert!(up > 1.0, "the window's coverage read {up:.1}%");
    }

    /// A MELEE WITH EVERY SLOT FILLED SIMULATES — eight mains, an exilus and a
    /// stance, which is TEN mods against the nine innate slots a weapon has.
    ///
    /// The stance is not one of the nine; it has a slot of its own and hands
    /// capacity back rather than taking it. Counted among them, the Forma
    /// planner refused the build — and refused it with an `assert!`, so in the
    /// browser this was a worker that died mid-fight without a word and a quick
    /// calc that sat on "measuring the baseline" for ever. It needed the LAST
    /// main slot and an exilus to reach ten, which is why a build one mod short
    /// of full never found it.
    #[test]
    fn a_melee_with_every_slot_filled_simulates() {
        let out = simulate_json(&serde_json::json!({
            "weapon": "praedos",
            "mods": [
                "primed_pressure_point", "sacrificial_steel", "organ_shatter",
                "voltaic_strike", "shocking_touch", "condition_overload",
                "primed_fury", "molten_impact",   // eight mains
                "conditions_perfection",          // the exilus
                "sovereign_outcast",              // and the stance
            ],
            "enemy": "thrax_centurion",
            "level": 150,
            "duration": 8,
            "runs": 1,
        }));
        assert!(out.get("error").is_none(), "a legal melee build must simulate: {out}");
        assert!(
            out.get("shots").and_then(serde_json::Value::as_f64).unwrap_or(0.0) > 0.0,
            "…and swing: {out}"
        );
    }

    /// THE MULTI-TARGET RULER RUNS, AND IT MEASURES THE CROWD.
    ///
    /// A benchmark is a yaml the engine never type-checks — its `scenario` is a
    /// free-form map on purpose, so a field added to scenarios needs no second
    /// definition — which means the only thing that can say a ruler is WELL
    /// FORMED is running it. This does, through the same `simulate_json` every
    /// module goes through, and it asserts the two claims the file makes:
    ///
    ///   · the crowd is REAL — 361 bodies, expanded from three numbers;
    ///   · and it is what is being measured — a chaining weapon scores far
    ///     higher here than under the single-target ruler, on the same build.
    ///
    /// Cheap terms (a short fight, few runs), because the claim is about the
    /// SHAPE of the fight and not about the board's precision.
    #[test]
    fn the_standard_multi_target_ruler_runs_and_measures_the_crowd() {
        let bench = wfsim_engine::board::benchmarks::get("standard_multi_target").expect("the ruler exists");
        let single =
            wfsim_engine::board::benchmarks::get("standard_single_target").expect("its companion exists");
        // THE RULER'S OWN SCENARIO, with only the cost terms overridden.
        let req = |b: &wfsim_engine::board::benchmarks::Benchmark| {
            let mut m = serde_json::to_value(&b.scenario).expect("a scenario is json");
            let o = m.as_object_mut().expect("a mapping");
            o.insert("weapon".into(), serde_json::json!("torid"));
            o.insert("mode".into(), serde_json::json!("transformed"));
            o.insert("mods".into(), serde_json::json!([]));
            o.insert(
                "evolutions".into(),
                serde_json::json!(["torid_evo1_incarnon_form"]),
            );
            // A level the beam can actually move, so the comparison is not two
            // zeros; and a short cheap fight.
            o.insert("level".into(), serde_json::json!(60));
            o.insert("duration".into(), serde_json::json!(8));
            o.insert("runs".into(), serde_json::json!(4));
            m
        };
        let crowd = simulate_json(&req(bench));
        assert!(crowd.get("error").is_none(), "{crowd}");

        // 361 BODIES, FROM THREE NUMBERS — proved by making the SAME shorthand
        // overflow the cap. A grid that expanded into nothing could not, so
        // this is the crowd being real rather than declared, and it exercises
        // the shorthand through the one path that counts bodies.
        // 361 BODIES, AND THE YAML SAYS THREE NUMBERS. The expansion happens
        // once, where the yaml becomes a scenario (`board::benchmarks`), so what
        // this ruler HOLDS by the time anything reads it is bodies — which is
        // also what lets the canvas draw the crowd it is about to simulate.
        let n = bench
            .scenario
            .get("formation")
            .and_then(|f| f.as_sequence())
            .map_or(0, |a| a.len());
        assert_eq!(n, 19 * 19 - 1, "19x19 is 361 bodies, one of them the aimed one");
        assert!(
            bench.scenario.get("formation_grid").is_none(),
            "the shorthand must not survive into the scenario"
        );

        // …AND THE CROWD IS WHAT IT MEASURES.
        let lone = simulate_json(&req(single));
        assert!(lone.get("error").is_none(), "{lone}");
        let dps = |v: &Value| v.get("dps").and_then(Value::as_f64).unwrap_or(-1.0);
        assert!(
            dps(&crowd) > dps(&lone) * 1.5,
            "a chaining weapon must score far higher on the group ruler: {} against {}",
            dps(&crowd),
            dps(&lone)
        );
    }

    /// A BODY THE PAGE BUILT IS A BODY THE FIGHT ACCEPTS — the shape the canvas
    /// actually sends, blanks and all.
    ///
    /// It failed with `unknown enemy: ` and nothing after the colon: the page stores a body's unit as `""` for "same as the
    /// target", which is what EVERY body a formation is built from carries, and
    /// the parser read the empty string as a name. Absent and blank are one
    /// state, and it means the aimed body's.
    #[test]
    fn a_body_with_blank_fields_takes_the_aimed_bodys() {
        let run = |body: serde_json::Value| {
            simulate_json(&serde_json::json!({
                "weapon": "torid", "mode": "transformed", "mods": [],
                "evolutions": ["torid_evo1_incarnon_form"],
                "enemy": "corrupted_heavy_gunner", "level": 100,
                "runs": 2, "seed": 7, "duration": 4,
                "formation": [body],
            }))
        };
        // EXACTLY WHAT THE CANVAS SENDS.
        let page = run(serde_json::json!({ "at": [3.0, 0.4], "enemy": "", "level": null }));
        assert!(page.get("error").is_none(), "{page}");
        // …and it is the same fight as naming nothing at all.
        let bare = run(serde_json::json!({ "at": [3.0, 0.4] }));
        assert_eq!(page.get("dps"), bare.get("dps"), "blank must mean absent");
        // …while a REAL id is still honoured, and a wrong one still refused.
        let named = run(serde_json::json!({ "at": [3.0, 0.4], "enemy": "thrax_centurion" }));
        assert!(named.get("error").is_none(), "{named}");
        let wrong = run(serde_json::json!({ "at": [3.0, 0.4], "enemy": "no_such_unit" }));
        assert!(
            wrong.get("error").and_then(Value::as_str).unwrap_or("").contains("no_such_unit"),
            "{wrong}"
        );
    }

    /// AIM IS A DIRECTION, and the request may name a PLACE rather than a body.
    /// Aiming at the floor short of an enemy still hits it; aiming at floor
    /// that crosses nobody is refused, in words, rather than silently fought.
    #[test]
    fn an_aim_point_decides_which_body_the_beam_is_on() {
        let run = |aim: [f64; 2]| {
            simulate_json(&serde_json::json!({
                "weapon": "torid", "mode": "transformed", "mods": [],
                "evolutions": ["torid_evo1_incarnon_form"],
                "enemy": "corrupted_heavy_gunner", "level": 100,
                "runs": 2, "seed": 7, "duration": 4,
                "player_at": [0.0, 0.0], "target_at": [0.0, 20.0],
                "formation": [{ "at": [0.0, 10.0] }],
                "aim_at": aim,
            }))
        };
        // Straight down the line: the body at 10 m is in FRONT of the one at
        // 20, so it is the one the beam is on.
        let near = run([0.0, 30.0]);
        assert!(near.get("error").is_none(), "{near}");
        // …AND AIMED AT BARE FLOOR IT IS A LEGAL SHOT THAT MISSES. It was refused for a day, which was wrong twice over: a
        // miss is an answer, and aiming BESIDE a crowd so the splash catches
        // more of it is a tactic rather than a mistake.
        let miss = run([40.0, 0.1]);
        assert!(miss.get("error").is_none(), "a shot at bare floor is legal: {miss}");
        let (hit, floor) = (
            near.get("dps").and_then(Value::as_f64).unwrap_or(-1.0),
            miss.get("dps").and_then(Value::as_f64).unwrap_or(-1.0),
        );
        assert!(
            floor >= 0.0 && floor < hit,
            "pointing at nobody must deal less than pointing at somebody: {floor} vs {hit}"
        );
    }

    /// pool. `riven_class` walks outward from the narrowest mod pool and stops
    /// at the first one that has stats — with none, it returns "" and the
    /// editor renders a riven with NOTHING to roll, which is how the Larkspur
    /// Prime shipped until `data/rivens/archgun.yaml` existed. The next class added lands here instead of in the UI.
    ///
    /// EXCEPT A WEAPON THIS APP CANNOT MOD AT ALL. The Deconstructor pair
    /// declares no mod pool, and a weapon with no mod pool reaches no riven
    /// pool either — one absence rather than two bugs. The exemption is keyed
    /// on that empty pool rather than on the two ids, so the next unmoddable
    /// weapon needs no list here.
    #[test]
    fn every_weapon_reaches_a_riven_stat_pool() {
        let mut orphans: Vec<String> = Vec::new();
        for w in weapons() {
            // The ENTRY's own `mod_pools`, not `WeaponInfo`'s — that one is
            // derived and carries the SLOT ("sentinel"), so it is never empty
            // and this skip would never fire off it.
            let unmoddable = wfsim_engine::data::weapons::spec(&w.id)
                .is_some_and(|s| s.mod_pools.is_empty());
            if unmoddable {
                continue;
            }
            let class = riven_class(w);
            let n = wfsim_engine::build::rivens::pool(&class).len();
            // What is left after the weapon's own exclusions is what the
            // editor actually offers — a pool the weapon excludes down to
            // nothing is the same empty card by another route.
            let excluded = wfsim_engine::build::rivens::excluded_for(&w.id).len();
            if n == 0 || n <= excluded {
                orphans.push(format!("{} (pools {:?} -> {class:?}, {n} stats, {excluded} excluded)",
                    w.id, w.mod_pools));
            }
        }
        assert!(orphans.is_empty(), "weapons with no riven stats: {orphans:#?}");

        // …and the exemption stays SMALL and deliberate. An empty mod pool is
        // the strongest statement a weapon entry can make — nothing can be
        // built on it — so it is never the quiet default for a weapon whose
        // pool somebody forgot to fill in.
        let unmoddable: Vec<&str> = weapons()
            .iter()
            .filter(|w| wfsim_engine::data::weapons::spec(&w.id)
                .is_some_and(|s| s.mod_pools.is_empty()))
            .map(|w| w.id.as_str())
            .collect();
        assert_eq!(
            unmoddable,
            [
                // THE GRIMOIRE LEFT THIS LIST ON 2026-08-25, and the reason it
                // was on it was simply wrong: it read "a Tome's mods are their
                // own pool" and concluded that offering pistol cards would be
                // worse than offering none. The wiki says the opposite in one
                // sentence — *"Tomes can equip Pistol Mods but also have access
                // to unique Tome Mods"* (`Tome`) — so the weapon was
                // unbuildable for five days over a pool it does take. It is
                // `[pistol, tome]` now and `data/mods/tome/` holds the eight.
                //
                // The Deconstructor is a sentinel's GLAIVE — a melee weapon in
                // a companion's hands — and this roster loads no melee pool.
                "deconstructor",
                "deconstructor_prime",
            ],
            "a weapon with NO mod pool is a weapon this app cannot build at all —              if that is intended, add it here with its reason"
        );
    }

    #[test]
    fn every_data_entry_has_an_image() {
        let a = assets();
        let mut missing: Vec<String> = Vec::new();
        for w in weapons() {
            if !a.weapons.contains_key(&w.id) {
                missing.push(format!("weapon {}", w.id));
            }
        }
        for class in wfsim_engine::data::mods::classes() {
            for m in wfsim_engine::data::mods::class_pool(class) {
                if !a.mods.contains_key(m.id) {
                    missing.push(format!("mod {}", m.id));
                }
            }
        }
        for slot in wfsim_engine::data::arcanes::slots() {
            for arc in wfsim_engine::data::arcanes::slot_pool(slot) {
                if !a.arcanes.contains_key(arc.id.as_str()) {
                    missing.push(format!("arcane {}", arc.id));
                }
            }
        }
        assert!(
            missing.is_empty(),
            "no image in data/assets.yaml for {} entries: {}
             run `python scripts/gen_assets.py --write`",
            missing.len(),
            missing.join(", ")
        );
    }
}

#[cfg(test)]
mod form_tests {
    use super::*;
    use crate::panel::panel_json;
    use crate::registry::weapons;

    fn sim(weapon: &str, form: &str) -> Value {
        simulate_json(&json!({
            "weapon": weapon, "form": form, "mods": [], "arcane": "none",
            "enemy": "thrax_centurion", "duration": 30.0, "runs": 8,
            "headshot_pct": 100.0, "seed": 7,
        }))
    }

    /// A GAUGE FED BY KILLS IS WORTH WHAT THE FIGHT LETS YOU EARN.
    ///
    /// The Mausolon's alt-fire costs five kills with the primary (wiki), which
    /// is the first gauge in the roster that a HIT cannot fill. That makes it
    /// the one cycle whose availability is a property of the TARGET rather
    /// than of the build, and this is the assertion the machinery could not
    /// make before: same weapon, same mods, same seed, two enemy levels, and
    /// the cycle appears in one and is unreachable in the other.
    ///
    /// The negative half is the real one. A weakpoint- or hit-fed gauge fills
    /// against anything you can shoot, so every existing cycle test passes at
    /// any level; if the Mausolon's charged off hits too, the level-9999 case
    /// would transform just as happily and the run below would be identical to
    /// the base one. It is not.
    #[test]
    fn a_kill_fed_gauge_is_unreachable_against_a_target_that_does_not_die() {
        let run = |form: &str, level: u32| {
            simulate_json(&json!({
                "weapon": "mausolon", "form": form, "mods": [], "arcane": "none",
                "enemy": "thrax_centurion", "level": level, "duration": 30.0,
                "runs": 8, "headshot_pct": 0.0, "seed": 7,
            }))
        };
        let n = |v: &Value, k: &str| v.get(k).and_then(Value::as_f64).unwrap_or(0.0);

        // KILLABLE: the laser arrives, repeatedly.
        let easy = run("gauge_cycle", 1);
        assert!(n(&easy, "transforms") > 0.0, "no laser at level 1: {}", n(&easy, "transforms"));
        // ...and it is worth something — the cycle is not the base fight.
        let easy_base = run("base", 1);
        assert!(
            (n(&easy, "dps") - n(&easy_base, "dps")).abs() > 1e-6,
            "the cycle changed nothing: {} vs {}",
            n(&easy, "dps"),
            n(&easy_base, "dps")
        );

        // UNKILLABLE IN PRACTICE: nothing dies inside the engagement, so the
        // gauge never fills and the cycle degenerates to the base form. Not an
        // approximation — the same number, because it is the same fight.
        let hard = run("gauge_cycle", 9999);
        let hard_base = run("base", 9999);
        assert_eq!(n(&hard, "transforms"), 0.0, "a laser nobody paid for");

        // AND THE LASER LIFTS. An independent proc leaves no trace in the
        // damage — its whole payload is that Condition Overload counts it — so
        // the replay's debuff table is the one place it can be falsified: the
        // row must move in the run that fires a laser and stay flat in the run
        // that never earns one. Same weapon, same seed; the only difference is
        // whether the gauge filled.
        let lifted_row = |v: &Value| -> u64 {
            let rep = &v["replay"];
            let i = rep["debuffs"]
                .as_array()
                .expect("the roster is always sent")
                .iter()
                .position(|d| d["id"] == "lifted")
                .expect("lifted has a row of its own");
            // `dstacks` is per BODY since 2026-08-17, and `[0]` is the AIMED
            // one — which is the body this test is about.
            rep["dstacks"][0][i]
                .as_array()
                .expect("one series per row")
                .iter()
                .filter(|x| x.as_u64().unwrap_or(0) > 0)
                .count() as u64
        };
        // EIGHT FIGHTS, each replayed as its own benchmark. At level 1 the laser
        // kills what it lifts, so one fight's frames can fall between the stacks
        // entirely; the claim is about the attack, not about one run's luck.
        let replayed = |form: &str, duration: f64| -> u64 {
            (1..=8u64)
                .map(|seed| {
                    lifted_row(&simulate_json(&json!({
                        "weapon": "mausolon", "form": form, "mods": [], "arcane": "none",
                        "enemy": "thrax_centurion", "level": 1, "duration": duration,
                        "runs": 1, "headshot_pct": 0.0, "seed": seed, "replay": true,
                    })))
                })
                .sum()
        };
        // SIXTY SECONDS, and the reason is worth writing down: at thirty this
        // unmodded weapon earns its fifth kill so late that it transmutes and
        // the engagement ends before the 0.8 s charge completes — `transforms`
        // reads 1 and no laser was ever fired. A gauge bought with kills is
        // the first mechanic here that can be REACHED and still not PAY, which
        // is exactly what a player at low build strength experiences.
        assert!(replayed("gauge_cycle", 60.0) > 0, "the laser did not lift");
        // THE NEGATIVE CONTROL IS THE AUTO FIRE, not the level: only the
        // alt-fire's explosion declares `independent_procs: [lifted]`, so a
        // weapon firing its belt all engagement must never light this row —
        // which is what proves the proc is tied to the ATTACK that declares it
        // and not to the weapon.
        assert_eq!(replayed("base", 60.0), 0, "the auto fire lifted");
        assert!(
            (n(&hard, "dps") - n(&hard_base, "dps")).abs() < 1e-9,
            "an unfilled gauge is the base fight: {} vs {}",
            n(&hard, "dps"),
            n(&hard_base, "dps")
        );
    }

    /// ASKING FOR A FORM IS ENOUGH — the evolution that IS that form is
    /// implied, not demanded.
    ///
    /// This is the state the page STARTS in: no evolutions chosen. Falling back
    /// to the base form for every request gives all three options one number
    /// while the control says otherwise.
    ///
    /// Implying it is exact rather than generous: tier 1 is `selection: fixed`
    /// on every Incarnon ladder — not a choice but what installing the Genesis
    /// grants — and it applies no stat, so the numbers below are IDENTICAL to
    /// the same request with the evolution named explicitly. That equality is
    /// the real assertion; three different numbers alone would not prove the
    /// implication is free.
    #[test]
    fn a_form_request_implies_its_own_unlock() {
        let dps = |v: &Value| v.get("dps").and_then(Value::as_f64).unwrap_or(0.0);
        let with_evo = |form: &str| {
            simulate_json(&json!({
                "weapon": "boar_prime", "form": form, "mods": [], "arcane": "none",
                "evolutions": ["boar_prime_evo1_incarnon_form"],
                "enemy": "thrax_centurion", "duration": 30.0, "runs": 8,
                "headshot_pct": 100.0, "seed": 7,
            }))
        };
        let (base, inc, cyc) = (sim("boar_prime", "base"), sim("boar_prime", "incarnon"),
                                sim("boar_prime", "incarnon_cycle"));
        // Three options, three fights — which is BEST depends on the build, so
        // no ordering is asserted, only that the choice does something.
        for (a, b) in [(&base, &inc), (&base, &cyc), (&inc, &cyc)] {
            assert!((dps(a) - dps(b)).abs() > 1e-6, "{} vs {}", dps(a), dps(b));
        }
        // ...and naming the evolution changes nothing, because it carries none.
        for form in ["base", "incarnon", "incarnon_cycle"] {
            assert!(
                (dps(&sim("boar_prime", form)) - dps(&with_evo(form))).abs() < 1e-9,
                "{form}: implying the unlock is not the same as naming it"
            );
        }
    }

    /// A weapon is fired in ITS OWN forms, with ITS OWN evolutions. Both used
    /// to leak across weapons from the same place — a request that named
    /// neither got Dual Toxocyst's evolutions and, for anything but `base`,
    /// an Incarnon cycle built on a borrowed gauge. A bow has neither, so it
    /// is the case that shows both.
    #[test]
    fn a_weapon_never_inherits_another_weapons_form_or_evolutions() {
        // The gauge one first: asking a bow for the two-form cycle cannot
        // produce a transformation, because it has nothing to transform into.
        for form in ["charged", "base", "incarnon_cycle", "primary", ""] {
            let r = sim("cernos_prime", form);
            assert_eq!(r["ok"], json!(true), "form {form}");
            assert_eq!(r["transforms"].as_f64(), Some(0.0), "bow transformed on form {form}");
        }
        assert_eq!(sim("verglas_prime", "incarnon_cycle")["transforms"].as_f64(), Some(0.0));

        // ...and the POSITIVE case, because "0 transforms" is also what a
        // weapon that IS supposed to cycle looks like when its base entry was
        // never wired to its Incarnon entry. Boar Prime shipped that way for
        // an afternoon: the second weapon file existed, the evolutions
        // existed, and `transforms_to` did not — so the cycle silently ran the
        // base form and reported a number that looked fine on its own.
        let cycled = simulate_json(&json!({
            "weapon": "boar_prime", "form": "incarnon_cycle", "mods": [],
            "evolutions": ["boar_prime_evo1_incarnon_form"],
            "enemy": "thrax_centurion", "duration": 120.0, "runs": 4,
            "headshot_pct": 100.0, "seed": 7,
        }));
        assert_eq!(cycled["ok"], json!(true));
        assert!(
            cycled["transforms"].as_f64().unwrap_or(0.0) > 0.0,
            "Boar Prime's Incarnon cycle never transformed — is `transforms_to` wired? got {}",
            cycled["transforms"]
        );

        // The evolution one: an unstated `evolutions` key falls back to the
        // historical Dual Toxocyst build, which must reduce to nothing here.
        // Cernos Prime's unmodded base is 184 per arrow (3 x 184 = the 552 the
        // wiki quotes); Dual Toxocyst's Commodore's Fortune would add 50.
        let bow = sim("cernos_prime", "charged");
        let base_of = |r: &Value| r["panel"]["total"].as_f64().expect("a base damage");
        assert!((base_of(&bow) - 184.0).abs() < 1e-6, "{}", base_of(&bow));
        assert_eq!(bow["panel"]["multishot"], json!(3.0));
        assert_eq!(bow["panel"]["crit_chance"], json!(0.35));

        // Both bow forms are the same weapon: half the base damage per arrow,
        // and the tap fires more often for it (no draw, wiki Fire Rate's bow
        // formula) — the trade that makes tapping a real pattern.
        let tapped = sim("cernos_prime", "base");
        assert!((base_of(&tapped) - 92.0).abs() < 1e-6, "{}", base_of(&tapped));
        let (drawn_shots, tapped_shots) = (bow["shots"].as_f64(), tapped["shots"].as_f64());
        assert_eq!(drawn_shots, Some(27.0), "30 s / (0.5 + 0.65) + 1");
        assert_eq!(tapped_shots, Some(47.0), "30 s / 0.65 + 1");
    }

    /// Evolutions are a LADDER, not a menu: tier N needs tier N-1 installed.
    /// The UI locks the rows, but a preset saved before the rule existed can
    /// still name a tier-4 perk with nothing under it — that build is not
    /// weaker, it is unreachable, so its orphans are dropped rather than
    /// priced. Commodore's Fortune (tier 4, +20% base crit) shows it: alone it
    /// must change nothing, and only the full 1-2-3-4 chain may pay out.
    /// A MELEE BUILD IS TEN IDS, AND ALL TEN ARE LEGAL.
    ///
    /// Eight main slots, one exilus and a STANCE beside them — the stance is a
    /// slot of its own, so counting the flat list refused the full build
    /// outright. `board::builds::validate_with` has subtracted the stances before
    /// comparing since the slot landed, and the panel does the same
    /// subtraction now.
    ///
    /// …AND ONE STANCE, because there is one slot: two is a build nobody can
    /// hold, and it is refused rather than resolved with the second ignored.
    #[test]
    fn a_stance_is_not_one_of_the_nine_and_two_are_refused() {
        let ok = |mods: &[&str]| panel_json(&json!({ "weapon": "magistar", "mods": mods }))["ok"] == true;
        let eight = [
            "sacrificial_steel", "sacrificial_pressure", "killing_blow", "organ_shatter",
            "seismic_wave", "corrupt_charge", "gladiator_might", "primed_fever_strike",
        ];
        let with = |extra: &[&'static str]| -> Vec<&'static str> {
            [eight.as_slice(), extra].concat()
        };
        assert!(ok(&with(&[])), "eight is a build");
        assert!(ok(&with(&["motus_impact", "shattering_storm"])), "…and so is eight, exilus, stance");
        assert!(
            !ok(&with(&["motus_impact", "shattering_storm", "crushing_ruin"])),
            "two stances is one slot too many",
        );
    }

    /// A SET THAT ENHANCES ITS OWN MEMBERS SAYS SO ON THE CARD, and says it
    /// while the set is still SHORT.
    ///
    /// What a Sacrificial mod is worth depends on what is beside it, which is
    /// the one question this list exists to answer. Drawn dim at one card — the
    /// reader can see the 25% they are one card away from — and lit on both,
    /// on BOTH rows, because the set enhances every member and not the newcomer.
    #[test]
    fn a_self_scaling_set_states_itself_on_every_member() {
        let row = |mods: &[&str]| -> Vec<(String, bool)> {
            let p = panel_json(&json!({ "weapon": "magistar", "mods": mods }));
            p["conditionals"]
                .as_array()
                .expect("conditionals")
                .iter()
                .filter(|c| c["desc"].as_str().is_some_and(|d| d.contains("Sacrificial set")))
                .map(|c| {
                    (c["mod"].as_str().unwrap_or_default().to_string(), c["active"] == true)
                })
                .collect()
        };
        let alone = row(&["sacrificial_steel"]);
        assert_eq!(alone.len(), 1, "the one card still states the set: {alone:?}");
        assert!(!alone[0].1, "…and states that it is not paying yet");

        let pair = row(&["sacrificial_steel", "sacrificial_pressure"]);
        assert_eq!(pair.len(), 2, "both members carry the row: {pair:?}");
        assert!(pair.iter().all(|(_, on)| *on), "…and both are paying: {pair:?}");
    }

    #[test]
    fn an_evolution_tier_needs_the_one_below_it() {
        let with = |evos: Value| {
            let p = panel_json(&json!({ "weapon": "torid", "evolutions": evos }));
            // Crit chance rides the projectile, so it is a row of the "direct
            // hit" PART rather than of the weapon block.
            p["forms"][0]["parts"][0]["stats"]
                .as_array()
                .expect("stat rows")
                .iter()
                .find(|r| r["key"] == json!("crit_chance"))
                .expect("a crit chance row")["final"]
                .as_str()
                .expect("a formatted crit chance")
                .to_string()
        };
        let bare = with(json!([]));
        assert_eq!(bare, "15.0%", "the Torid's unmodded crit chance");
        assert_eq!(with(json!(["torid_commodores_fortune"])), bare, "tier 4 alone paid out");
        assert_eq!(
            with(json!(["torid_evo1_incarnon_form", "torid_commodores_fortune"])),
            bare,
            "tier 4 paid out over a gap at tiers 2-3"
        );
        // Order in the array is the client's, not the ladder's — the whole
        // chain counts however it arrives.
        let full = json!([
            "torid_commodores_fortune", "torid_extended_volley",
            "torid_final_fusillade", "torid_evo1_incarnon_form",
        ]);
        assert_eq!(with(full), "35.0%", "the full chain did not pay out");
    }

    /// The registry publishes what each weapon actually has: its own forms,
    /// and separately whether any of them is transformed into.
    #[test]
    fn the_registry_publishes_each_weapons_own_forms() {
        let ids = |w: &WeaponInfo| w.forms.iter().map(|(i, _, _)| *i).collect::<Vec<_>>();
        let get = |id: &str| weapons().iter().find(|w| w.id == id).expect(id);

        let bow = get("cernos_prime");
        assert_eq!(ids(bow), ["charged", "base"], "the arsenal's form first");
        assert!(!bow.has_cycle, "two forms, but nothing to transform into");

        let torid = get("torid");
        assert_eq!(ids(torid), ["base", "incarnon"]);
        assert!(torid.has_cycle);

        let verglas = get("verglas_prime");
        assert_eq!(ids(verglas), ["base"]);
        assert!(!verglas.has_cycle);
    }
}

#[cfg(test)]
mod valence_formation_blocks_attrition {
    
    use serde_json::json;

    fn dps(evolutions: &[&str], augment: bool) -> f64 {
        let req = json!({
            "weapon": "laetum",
            "mode": "base",
            "mods": [],
            "evolutions": evolutions,
            "arcane": ["none"],
            // NO HEADSHOTS: this is about the status half of the condition, and
            // a crit or a head would be a second reason for the perk not to arm.
            "headshot_pct": 0,
            "abilities": if augment {
                json!([{ "id": "valence_formation", "element": "heat" }])
            } else {
                json!([])
            },
            "ability_strength": 1.0,
            "runs": 120,
            "seed": 3,
        });
        let out = super::simulate_json(&req);
        assert!(out.get("ok").and_then(serde_json::Value::as_bool).unwrap_or(false),
            "simulate refused: {out}");
        out.get("dps").and_then(serde_json::Value::as_f64).expect("dps")
    }

    /// The ladder up to tier 5 — a tier-5 perk with no tier 4 is dropped by the
    /// evolution ladder and would make both arms of this test read "no effect".
    const LADDER: [&str; 4] = [
        "laetum_evo1_incarnon_form",
        "laetum_marksmans_hand",
        "laetum_feather_of_justice",
        "laetum_caput_mortuum",
    ];

    fn with_perk() -> Vec<&'static str> {
        let mut v = LADDER.to_vec();
        v.push("laetum_overwhelming_attrition");
        v
    }

    #[test]
    fn the_perk_is_worth_a_lot_without_the_augment() {
        let plain = dps(&LADDER, false);
        let perked = dps(&with_perk(), false);
        assert!(perked > plain * 5.0,
            "Overwhelming Attrition should dominate a bare Laetum: {plain} -> {perked}");
    }

    #[test]
    fn and_worth_exactly_nothing_with_it() {
        let plain = dps(&LADDER, true);
        let perked = dps(&with_perk(), true);
        // EXACTLY nothing, not approximately: the buff never arms, so the two
        // runs are the same fight with the same dice.
        assert_eq!(
            plain, perked,
            "a guaranteed status leaves no plain hit for Overwhelming Attrition to \
             arm on, so the perk must change nothing at all"
        );
    }
}

#[cfg(test)]
mod wide_beam {
    use super::*;

    /// A SECOND THING ACTING, THROUGH THE WIRE.
    ///
    /// `also_acting` carries whole requests, so a seat brings its own weapon,
    /// its own mods and its own evolutions — and NOT its own fight: the arena
    /// is the reported build's, which is the one thing a caller could get
    /// wrong in a way nothing would report.
    #[test]
    fn a_second_seat_arrives_through_the_request_and_is_reported_as_one() {
        let fight = |also: Value| {
            let mut req = json!({
                "weapon": "cernos_prime", "mods": [],
                "enemy": "corrupted_heavy_gunner", "level": 100,
                "runs": 3, "seed": 7, "duration": 8,
            });
            if !also.is_null() {
                req["also_acting"] = json!([also]);
            }
            simulate_json(&req)
        };

        let solo = fight(Value::Null);
        let pair = fight(json!({
            "weapon": "braton_prime", "mods": [],
            "enemy": "corrupted_heavy_gunner", "level": 100,
            "runs": 3, "seed": 7, "duration": 8,
        }));

        let seats = |r: &Value| r["combatants"].as_array().cloned().unwrap_or_default();
        assert_eq!(seats(&solo).len(), 1, "{solo}");
        assert_eq!(seats(&pair).len(), 2, "{pair}");

        // IT FIRED. A seat that arrived and never acted would leave this equal
        // to the solo fight, which is exactly what a roster listing an idle
        // seat is there to make visible.
        let dealt = |r: &Value| -> f64 {
            seats(r).iter().map(|c| c["damage"].as_f64().unwrap_or(0.0)).sum()
        };
        assert!(
            seats(&pair)[1]["damage"].as_f64().unwrap_or(0.0) > 0.0,
            "the second seat dealt nothing: {pair}"
        );
        assert!(
            dealt(&pair) > dealt(&solo),
            "two seats dealt {} where one dealt {}",
            dealt(&pair),
            dealt(&solo)
        );

        // …AND ITS READINGS ARE ITS OWN. A crit rate is a seat's or nobody's.
        for i in 0..2 {
            assert!(
                seats(&pair)[i]["shots"].as_f64().unwrap_or(0.0) > 0.0,
                "seat {i} reported no shots: {pair}"
            );
        }
    }

    /// …AND EVERY READER OF THE FIGHT SEES IT — the rule `seats_beside` states,
    /// asserted on the other two readers.
    ///
    /// The SEARCH is asserted on its PLAN rather than on a run: what has to
    /// hold is that the scenario handed to it carries the roster, and running
    /// one to find that out costs minutes to learn the same thing.
    #[test]
    fn the_roster_reaches_the_record_and_the_search_as_well_as_the_answer() {
        let req = json!({
            "weapon": "cernos_prime", "mods": [],
            "enemy": "corrupted_heavy_gunner", "level": 100,
            "runs": 2, "seed": 7, "duration": 6,
            "also_acting": [{ "weapon": "braton_prime", "mods": [] }],
        });

        // THE RECORD names the same seats, so its rows are about this fight.
        let mut log_req = req.clone();
        log_req["from"] = json!(0.0);
        log_req["to"] = json!(2.0);
        let rec = crate::log::log_json(&log_req);
        let seats: Vec<String> = rec["combatants"].as_array().cloned().unwrap_or_default()
            .iter().map(|c| c["weapon"].as_str().unwrap_or_default().to_string()).collect();
        assert_eq!(seats, ["cernos_prime", "braton_prime"], "{rec}");

        // THE SEARCH is handed the same roster before a candidate is scored.
        let mut plan_req = req.clone();
        plan_req["mods"] = json!({ "serration": "search" });
        match crate::optimize::parse_optimize(&plan_req) {
            Ok(plan) => assert_eq!(
                plan.scenario.also_acting.len(), 1,
                "the search was handed a fight with nobody else in it"
            ),
            Err(e) => panic!("the plan was refused: {e}"),
        }
    }

    /// THE FURIS INCARNON BEAM IS 2 M WIDE and pierces only on a modded punch
    /// through: a body a metre off the line, behind the target, is reached with
    /// Seeker and not without it.
    #[test]
    fn a_wide_beam_reaches_off_the_line_only_through_punch_through() {
        // The bodies a fight reports are the ones it damaged, by position.
        let hit = |mods: Value| -> Vec<f64> {
            let r = simulate_json(&json!({
                "weapon": "furis", "mode": "transformed",
                "evolutions": ["furis_evo1_incarnon_form"],
                "mods": mods, "enemy": "corrupted_heavy_gunner", "level": 100,
                "runs": 2, "seed": 7, "duration": 3,
                "player_at": [0, 0], "target_at": [0, 5],
                "formation": [{ "at": [1.0, 7.0] }, { "at": [2.0, 9.0] }],
            }));
            r["bodies"]
                .as_array()
                .unwrap_or_else(|| panic!("{r}"))
                .iter()
                .filter(|b| b["damage"].as_f64().unwrap_or(0.0) > 0.0)
                .filter_map(|b| b["at"][0].as_f64())
                .collect()
        };
        assert_eq!(hit(json!([])), vec![0.0]);
        assert_eq!(hit(json!(["seeker"])), vec![0.0, 1.0], "1 m in, 2 m out");
    }
}
