// SPDX-License-Identifier: AGPL-3.0-or-later
//! `/api/log`: the combat record of one engagement, on the wire.

use serde_json::{json, Value};
use wfsim_engine::model::ModDef;
use crate::fight::{Fight, parse_fight};
use crate::request::{get_f64, r1, r3};
use crate::rivens::mod_pool_with_rivens;
use crate::simulate::{AmmoEconomy, sim_params};

/// THE COMBAT RECORD of one engagement — see [`wfsim_engine::record`].
///
/// **A QUERY, NOT A PAYLOAD.** It is deliberately not a field on
/// `/api/simulate`: an ordinary fight deals 2,000–5,000 damage instances over
/// 180 s and the densest measured deals 408,817, so a log that rode along would
/// be free on most builds and megabytes on exactly the ones a player is most
/// likely to be arguing about. Asking separately costs ONE re-run of the
/// engagement — about a millisecond single-target — and lets a reader pan,
/// filter and page without re-simulating the thousand runs behind the report.
///
/// It also keeps the storage rule intact: a record never reaches the disk,
/// because it is never part of the thing that gets saved.
///
/// Request: the fight, exactly as `/api/simulate` was sent it, plus
/// `run: [hi, lo]` from that call's answer, and optionally `from`, `to`,
/// `body` and `limit`. Same fight + same run = the same numbers, bit for bit.
pub fn log_json(v: &Value) -> Value {
    let fight = match parse_fight(v) {
        Ok(f) => f,
        Err(e) => return e,
    };
    let Fight {
        info, policy, buff_cfg, denied_buff_triggers, arena, evos, cycle_from,
        single_form, tenno,
        infinite_ammo, ammo_drops, pickup_range_m, landscape,
        frenzy_single, frenzy_locks, cycle_frenzy_lock, ..
    } = fight;
    let ammo = AmmoEconomy { drops: ammo_drops, pickup_range_m, landscape };
    let evo_refs: Vec<&str> = evos.iter().map(String::as_str).collect();
    let mod_ids: Vec<String> = v
        .get("mods")
        .and_then(|x| x.as_array())
        .map(|a| a.iter().filter_map(|m| m.as_str().map(String::from)).collect())
        .unwrap_or_default();
    let pool = mod_pool_with_rivens(v, info, &evo_refs);
    let refs: Vec<&ModDef> = mod_ids
        .iter()
        .filter_map(|id| pool.iter().find(|m| m.id == *id))
        .collect();
    let (_, mut params) = sim_params(
        v, info, policy, &evo_refs, &refs, &tenno, &arena,
        cycle_from, single_form, infinite_ammo, ammo, frenzy_single, cycle_frenzy_lock,
        &frenzy_locks,
    );
    if let Some(cfg) = &buff_cfg {
        params.apply_buff_config(cfg);
    }
    // AFTER the cards: a card may open an on-kill buff at five stacks, and a
    // fight with no kills does not owe them.
    params.deny_buff_triggers(&denied_buff_triggers);

    // THE RUN, as two u32 halves — see the `run` key `/api/simulate` answers
    // with. A caller that sends none gets the run that state 0 produces, which
    // is a legal engagement and not the one the report is about; the answer
    // says which it used rather than letting a reader assume.
    let run = v.get("run").and_then(|x| x.as_array());
    let half = |i: usize| -> u64 {
        run.and_then(|a| a.get(i)).and_then(Value::as_u64).unwrap_or(0)
    };
    let state = (half(0) << 32) | (half(1) & 0xffff_ffff);

    let from = get_f64(v, "from", 0.0);
    let to = get_f64(v, "to", f64::INFINITY);
    // A CEILING THE READER SETS, and one this endpoint will not exceed. 60,000
    // is twelve times an ordinary fight's whole stream and a seventh of the
    // worst one measured, so the common case is never truncated and the
    // pathological one is answered rather than refused.
    let limit = get_f64(v, "limit", 60_000.0).clamp(1.0, 200_000.0) as usize;
    // WHERE THIS PAGE STARTS. A window bounds one read; the skip is what lets
    // several reads cover a stream no single one can hold, and the whole fight
    // is then a matter of asking again.
    let skip = get_f64(v, "skip", 0.0).max(0.0) as usize;
    let rec = wfsim_engine::fight::record(&params, state, from, to, limit, skip);

    // ONE BODY'S VIEW IS A FILTER, never a different query: a weapon event
    // belongs to nobody, so it belongs in every body's timeline.
    let only = v.get("body").and_then(Value::as_u64).map(|b| b as u16);
    // WHAT REPEATS IS SENT ONCE. The weapon's state is identical on every row
    // of a trigger pull and usually across several; the two stack lists change
    // only when something is applied or expires. A row that OMITS one means
    // "the same as the row before", and the page fills it forward — which is
    // the other half of the 17.2 MB above, and is exact rather than lossy
    // because the previous value is on the wire either way.
    let mut carry = Carry::default();
    let events: Vec<Value> = rec
        .events()
        .iter()
        .filter(|e| only.is_none_or(|b| e.subject.is_none_or(|s| s == b)))
        .map(|e| event_json(e, &mut carry))
        .collect();
    json!({
        "ok": true,
        "run": [(state >> 32) as u32, (state & 0xffff_ffff) as u32],
        "from": from,
        "to": if to.is_finite() { json!(to) } else { Value::Null },
        // WHAT DID NOT FIT, stated rather than swallowed — a cap nobody is told
        // about reads as "that is everyone".
        "dropped": rec.dropped(),
        "skip": rec.skipped(),
        // THE TWO ROSTERS the rows' stack lists index into: the shooter's, whose
        // ids are the buff cards' own, and the target's, which is a constant of
        // the engine.
        // THE FACTOR TABLE, once. A row names the factors it was built from by
        // their INDEX here, because a row carries the ones that did nothing by
        // name too — thirteen of them on an ordinary rifle hit, the same
        // thirteen strings on every row of the fight. Measured before this: 859
        // bytes an event, 17.2 MB for a 20,000-row window.
        "factors": wfsim_engine::record::Factor::ALL
            .iter().map(|f| f.name()).collect::<Vec<_>>(),
        // THE ATTACKER ROSTER, once, the way the factor table and the two stack
        // rosters are: a row names its dealer by index into this.
        "attackers": params.attacker_ids(),
        "buffs": rec.buffs(),
        "debuffs": wfsim_engine::fight::DEBUFF_ROSTER
            .iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        "events": events,
    })
}

/// One event, on the wire. Written by hand rather than derived: the enum's
/// shape is the ENGINE's business and the page joins on names it can translate.
/// WHAT THE LAST ROW ALREADY SAID, so this one can leave it out.
#[derive(Default)]
struct Carry {
    weapon: Option<Value>,
    buffs: Option<Value>,
    debuffs: Option<Value>,
}

impl Carry {
    /// Emit `v` under `key` only if it differs from the last one sent. The
    /// comparison is on the SERIALIZED value, so "unchanged" means what a
    /// reader filling forward would reconstruct, not what the engine thinks.
    fn once(&mut self, m: &mut serde_json::Map<String, Value>, key: &str, v: Value,
            slot: fn(&mut Carry) -> &mut Option<Value>) {
        if slot(self).as_ref() == Some(&v) {
            return;
        }
        *slot(self) = Some(v.clone());
        m.insert(key.into(), v);
    }
}

fn event_json(e: &wfsim_engine::record::Event, carry: &mut Carry) -> Value {
    use wfsim_engine::record::Kind;
    let weapon = json!({
            "form": if e.weapon.transmuted { "transmuted" } else { "base" },
            "magazine": e.weapon.magazine,
            "magazine_max": e.weapon.magazine_max,
            // THE FORM THAT IS NOT FIRING — drawn always, because a transmute
            // silently refills the base magazine and a column showing only the
            // active one hid it.
            "idle_magazine": e.weapon.idle_magazine.map(|(at, of)| json!([at, of])),
            // WHAT IS LEFT IN RESERVE — `null` where the fight grants infinite
            // ammo, which every ruler does.
            "reserve": e.weapon.reserve.map(r1),
            // THE INCARNON GAUGE, `null` on a weapon with no cycle. It starts
            // EMPTY, which is the model's own rule and was nowhere on screen.
            "gauge": e.weapon.gauge.map(|(at, of)| json!([at, of])),
    });
    let mut o = json!({
        "id": e.id,
        "t": (e.t * 1000.0).round() / 1000.0,
    });
    let m = o.as_object_mut().expect("object");
    carry.once(m, "weapon", weapon, |c| &mut c.weapon);
    if let Some(s) = e.subject {
        m.insert("body".into(), json!(s));
    }
    if let Some(c) = e.cause {
        m.insert("cause".into(), json!(c));
    }
    match &e.kind {
        Kind::Damage(d) => {
            m.insert("kind".into(), json!("damage"));
            // WHO DEALT IT, beside the `body` that took it. Sent on every row
            // and not only when a fight has a second attacker: the row is the
            // ledger, and a ledger that names the dealer only when it is
            // interesting is one a reader cannot audit.
            m.insert("attacker".into(), json!(d.attacker.0));
            m.insert("origin".into(), json!(d.origin.name()));
            // WHICH PELLET, and which half of its attack. A pellet with an
            // explosion is TWO rows and one pellet.
            if let Some(n) = d.pellet {
                m.insert("pellet".into(), json!(n));
            }
            if d.radial {
                m.insert("radial".into(), json!(true));
            }
            m.insert("pool".into(), json!(d.pool.name()));
            m.insert("type".into(), json!(d.dtype.name()));
            // WHAT THE GAME DRAWS THIS AS — crit, headcrit, status tick, the
            // radial half of a blast. It decides the floating number's colour
            // and size, and it is on the ROW because the row IS that number.
            m.insert("pop_kind".into(), json!(d.kind));
            if let Some(p) = &d.part {
                m.insert("part".into(), json!(p));
                m.insert("head".into(), json!(d.head));
            }
            if d.crit_tier > 0 {
                m.insert("crit".into(), json!(d.crit_tier));
            }
            m.insert("base".into(), json!(r1(d.base)));
            if d.crit_tier > 0 {
                m.insert("crit_damage".into(), json!(r3(d.crit_damage)));
            }
            // THE LEDGER, LAYER BY LAYER — see `record::Layer`. The shape of a
            // layer is the information: a bracket lists its terms, a snap shows
            // its grid, and only a `mul` is drawn with a multiplication sign.
            // Nothing here can express a quotient, which is the point.
            m.insert("layers".into(), json!(d.layers.iter().map(layer_json).collect::<Vec<_>>()));
            m.insert("raw".into(), json!(r1(d.raw)));
            m.insert("mitigation".into(),
                json!(steps_json(&d.mitigation.iter().collect::<Vec<_>>())));
            m.insert("effective".into(), json!(r1(d.effective)));
            m.insert("before".into(), json!({
                "overguard": r1(d.before.overguard),
                "shield": r1(d.before.shield),
                "health": r1(d.before.health),
                "armor": r1(d.before.armor),
                // THE WINDOW NOBODY TAKES, YET — see `record::TargetAt`. Drawn
                // rather than dropped because the day melee lands, this is how
                // the claim gets checked.
                "shield_gate_until": d.before.shield_gate_until.map(r1),
            }));
            // EACH SIDE AS `[stacks, expires at]`, and the expiry is ABSOLUTE
            // so it only moves when something is applied or refreshed — which
            // is what lets the carry drop the repeat. A countdown would change
            // on every row of the fight and never dedup.
            let pair = |v: &[(u16, f64)]| json!(v.iter()
                .map(|(n, e)| json!([n, if e.is_finite() { json!(r1(*e)) } else if e.is_nan() {
                    Value::Null
                } else {
                    json!("inf")
                }]))
                .collect::<Vec<_>>());
            carry.once(m, "debuffs", pair(&d.debuffs), |c| &mut c.debuffs);
            // …AND THE SHOOTER'S OWN SIDE, positional against `buffs` below.
            carry.once(m, "buffs", pair(&d.buffs), |c| &mut c.buffs);
            if !d.procs.is_empty() {
                m.insert(
                    "procs".into(),
                    json!(d.procs.iter().map(|p| p.name()).collect::<Vec<_>>()),
                );
            }
            // …AND WHAT IT SET OFF ON THE SHOOTER, the other half of `procs`.
            if !d.triggered.is_empty() {
                m.insert("triggered".into(), json!(d.triggered));
            }
            if d.killed {
                m.insert("killed".into(), json!(true));
            }
        }
        Kind::Shot { pellets } => {
            m.insert("kind".into(), json!("shot"));
            m.insert("pellets".into(), json!(pellets));
        }
        Kind::Miss { reason } => {
            m.insert("kind".into(), json!("miss"));
            m.insert("reason".into(), json!(reason));
        }
        Kind::ReloadStart { seconds } => {
            m.insert("kind".into(), json!("reload_start"));
            m.insert("seconds".into(), json!(r1(*seconds)));
        }
        Kind::ReloadEnd => {
            m.insert("kind".into(), json!("reload_end"));
        }
        Kind::TransformStart { seconds, into_transmuted } => {
            m.insert("kind".into(), json!("transform_start"));
            m.insert("seconds".into(), json!(r1(*seconds)));
            m.insert(
                "into".into(),
                json!(if *into_transmuted { "transmuted" } else { "base" }),
            );
        }
        Kind::TransformEnd { transmuted } => {
            m.insert("kind".into(), json!("transform_end"));
            m.insert(
                "into".into(),
                json!(if *transmuted { "transmuted" } else { "base" }),
            );
        }
        Kind::StatusExpired { dtype, remaining } => {
            m.insert("kind".into(), json!("status_expired"));
            m.insert("type".into(), json!(dtype.name()));
            m.insert("remaining".into(), json!(remaining));
        }
        Kind::Killed => {
            m.insert("kind".into(), json!("killed"));
        }
    }
    o
}

/// ONE LAYER OF THE OFFENSIVE LEDGER.
///
/// `k` is the shape — `b` bracket, `q` quantize, `m` mul — because a row is
/// read by its shape before it is read by its numbers, and the page draws three
/// different things.
fn layer_json(l: &wfsim_engine::record::Layer) -> Value {
    use wfsim_engine::record::Layer;
    match l {
        Layer::Bracket { factor, terms, sum, out } => json!({
            "k": "b",
            "f": factor.index(),
            "t": terms.iter().map(term_json).collect::<Vec<_>>(),
            "s": r3(*sum),
            "o": r1(*out),
        }),
        Layer::Quantize { scale, components, out } => json!({
            "k": "q",
            "scale": r3(*scale),
            "c": components.iter().map(|c| json!([
                c.dtype.name(), r1(c.before), r3(c.units), r1(c.after),
            ])).collect::<Vec<_>>(),
            "o": r1(*out),
        }),
        // `p` IS PARTS, AND EACH CARRIES A WHOLE NUMBER rather than a bonus —
        // the page must not draw them with a `+0.80`'s formatting.
        Layer::Sum { factor, parts, out } => json!({
            "k": "s",
            "f": factor.index(),
            "p": parts.iter()
                .map(|x| {
                    let mut o = json!({ "f": x.factor.index(), "a": r3(x.amount) });
                    // …AND WHAT THAT AMOUNT IS A PRODUCT OF, where the engine
                    // can still say. Absent on a consolidated tick, which is
                    // several stacks the arm cannot inspect one by one.
                    if !x.of.is_empty() {
                        let m = o.as_object_mut().expect("object");
                        m.insert("head".into(), json!(r3(x.head)));
                        m.insert("of".into(), json!(x.of.iter()
                            .map(|g| json!({ "f": g.factor.index(), "v": r3(g.value) }))
                            .collect::<Vec<_>>()));
                    }
                    o
                })
                .collect::<Vec<_>>(),
            "o": r3(*out),
        }),
        Layer::Mul { factor, value, of, head, out } => {
            let mut o = json!({ "k": "m", "f": factor.index(), "v": r3(*value), "o": r1(*out) });
            if !of.is_empty() {
                let m = o.as_object_mut().expect("object");
                // ITS OWN EXPANSION: a body part is `3.00 x (1 + 0.50)`, and
                // the head of that product is what the enemy card states.
                m.insert("head".into(), json!(r3(*head)));
                m.insert("t".into(), json!(of.iter().map(term_json).collect::<Vec<_>>()));
            }
            o
        }
    }
}

/// One term of an additive bracket. `o` is the pair it is a product of, where
/// it is one — Condition Overload is `rate x status types`.
fn term_json(t: &wfsim_engine::record::Term) -> Value {
    // NO `f` WHERE THERE IS NO EXACT NAME — see `record::Term::factor`. The
    // page draws the number alone rather than a category a reader would go
    // looking for a card to match.
    let mut o = match t.factor {
        Some(f) => json!({ "f": f.index(), "v": r3(t.value) }),
        None => json!({ "v": r3(t.value) }),
    };
    if let Some((a, b)) = t.of {
        o.as_object_mut().expect("object")
            .insert("o".into(), json!([r3(a), r3(b)]));
    }
    o
}

/// A ledger, as `[label, value]` pairs. An ARRAY rather than an object because
/// ORDER is the information: the factors are listed in the order the engine
/// applies them, and a JSON object does not promise one.
fn steps_json(steps: &[&wfsim_engine::record::Step]) -> Vec<Value> {
    steps.iter().map(|(k, v)| json!([k.index(), r3(*v)])).collect()
}
