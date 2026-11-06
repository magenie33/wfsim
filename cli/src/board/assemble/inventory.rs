// SPDX-License-Identifier: AGPL-3.0-or-later
//! Engine-owned inventory membership over coherent, immutable input cuts.

use super::super::entry::{park_under_entry_line, Who};
use super::super::facts::load_cross_facts;
use super::super::queue::Queue;
use super::*;
use serde::Deserialize;
use serde_json::json;

const CHUNK: usize = 256;
const MAX_CHANGES: usize = 512;
const MAX_FACT_IDS: usize = 1024;

#[derive(Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Scope {
    weapon: String,
    #[serde(default)]
    riven: Option<bool>,
}

impl Scope {
    fn contains(&self, b: &Built) -> bool {
        self.weapon == b.weapon && self.riven.is_none_or(|r| r == b.riven.is_some())
    }
}

#[derive(Clone, Deserialize, PartialEq)]
struct Meta {
    engine: String,
    contract: Value,
    snapshot: String,
    generation: Value,
    epoch: String,
}

#[derive(Default, Deserialize)]
struct Counts {
    builds: Option<usize>,
    facts: Option<usize>,
    queue: Option<usize>,
}

#[derive(Deserialize)]
struct Request {
    v: u8,
    op: String,
    #[serde(flatten)]
    meta: Meta,
    #[serde(default)]
    base_epoch: Option<String>,
    #[serde(default)]
    builds: Option<String>,
    #[serde(default)]
    facts: Option<String>,
    #[serde(default)]
    queue: Option<String>,
    #[serde(default)]
    fact_ids: Vec<String>,
    #[serde(default)]
    counts: Counts,
    #[serde(default)]
    scope: Option<Value>,
    #[serde(default)]
    after_owner: Option<String>,
    #[serde(default)]
    limit: Option<usize>,
}

#[derive(Deserialize)]
struct Owner {
    id: String,
    record: Option<Box<RawValue>>,
}

struct Source {
    slot: usize,
    key: (u64, u64),
}

struct Cohort {
    scope: Scope,
    facts: String,
    queue: String,
    fact_rows: usize,
    queue_rows: usize,
    owners: Vec<(Arc<str>, usize)>,
    earning: HashMap<String, u64>,
    queued: HashMap<String, HashMap<String, u64>>,
}

#[derive(Default)]
pub(super) struct State {
    engaged: bool,
    meta: Option<Meta>,
    owners: BTreeMap<Arc<str>, Source>,
    slots: Vec<Option<Arc<str>>>,
    cohort: Option<Cohort>,
    last_apply: Option<Vec<u8>>,
}

impl State {
    pub(super) fn invalidate(&mut self) {
        self.meta = None;
        self.cohort = None;
        self.last_apply = None;
    }

    pub(super) fn permits_publish(&self) -> bool {
        !self.engaged || self.meta.is_some()
    }

    pub(super) fn source_count(&self) -> Option<usize> {
        self.meta.as_ref().map(|_| self.owners.len())
    }

    pub(super) fn execute(
        &mut self,
        board: &mut Board,
        request: &str,
        output: &str,
    ) -> Result<(String, BTreeSet<(usize, String)>), String> {
        if std::fs::metadata(request).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
            return Err("inventory request too large".into());
        }
        let bytes = std::fs::read(request).map_err(|e| e.to_string())?;
        let req: Request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if req.v != 1
            || req.meta.engine.len() != 12
            || !req.meta.engine.bytes().all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || req.meta.contract.get("algorithm").and_then(Value::as_str) != Some("board-inventory-v1")
            || req.meta.snapshot.is_empty()
            || req.meta.generation.is_null()
            || req.meta.epoch.is_empty()
        {
            return Err("invalid inventory contract".into());
        }
        if !Path::new(output).is_absolute() {
            return Err("absolute inventory output required".into());
        }
        let partial = format!("{output}.partial-{}", std::process::id());
        let result = (|| {
            let file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
            let mut out = std::io::BufWriter::new(file);
            let (owners, rows, complete, next, moved) = match req.op.as_str() {
                "bootstrap" => {
                    let (n, moved) = self.bootstrap(board, &req, &mut out)?;
                    (n, 0, true, None, moved)
                }
                "apply" => {
                    let (n, moved) = if self.meta.as_ref() == Some(&req.meta)
                        && self.last_apply.as_deref() == Some(bytes.as_slice())
                    {
                        (self.replay(board, &req, &mut out)?, BTreeSet::new())
                    } else {
                        self.apply(board, &req, &mut out)?
                    };
                    (n, 0, true, None, moved)
                }
                "cohort" => {
                    let (n, rows, complete, next) = self.members(board, &req, &mut out)?;
                    (n, rows, complete, next, BTreeSet::new())
                }
                _ => return Err("unknown inventory operation".into()),
            };
            write_json(
                &mut out,
                &json!({ "type":"end", "v":1, "op":req.op,
                "engine":req.meta.engine, "contract":req.meta.contract, "snapshot":req.meta.snapshot,
                "generation":req.meta.generation, "epoch":req.meta.epoch, "scope":req.scope,
                "owners":owners, "rows":rows, "complete":complete, "next_owner":next }),
            )?;
            out.flush().map_err(|e| e.to_string())?;
            std::fs::rename(&partial, output).map_err(|e| e.to_string())?;
            Ok((format!("ok inventory {} {owners} {rows}", req.op), moved))
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&partial);
            if req.op != "cohort" {
                self.invalidate();
            }
        } else if req.op == "apply" {
            self.last_apply = Some(bytes);
        }
        result
    }

    fn header(&self, board: &Board, owner: &str) -> Value {
        let source = self.owners.get(owner);
        let built = source.and_then(|s| board.lib.records[s.slot].as_ref().as_ref());
        let fallback = built.and_then(|b| board.lib.by_identity.get(&b.identity).map(|i| i.first));
        let canonical = fallback.and_then(|i| board.lib.records[i].as_ref().as_ref()).or(built);
        let record = canonical.map(|b| {
            let rec = wfsim_webapi::board_rows::canonical_record(&b.v);
            if b.v.riven_rolls.is_empty() {
                rec
            } else {
                wfsim_webapi::board_rows::with_rolls(rec, &b.v.riven_rolls)
            }
        });
        json!({ "type":"owner", "owner":owner, "build_id":built.map(|b| &b.identity),
            "weapon":built.map(|b| &b.weapon), "riven":built.map(|b| b.riven.is_some()),
            "record":record, "source":source.map(|s| s.slot),
            "binding_owner":fallback.and_then(|i| self.slots[i].as_deref()).or(Some(owner)) })
    }

    fn bootstrap(
        &mut self,
        board: &mut Board,
        req: &Request,
        out: &mut impl Write,
    ) -> Result<(usize, BTreeSet<(usize, String)>), String> {
        self.engaged = true;
        self.invalidate();
        self.owners.clear();
        self.slots.clear();
        let mut moved = board.all_pairs();
        let mut reusable = std::mem::take(&mut board.lib.cache);
        let mut spare: Vec<_> = reusable.keys().copied().collect();
        board.lib = Library::default();
        board.facts = Facts::new(&board.rulers);
        let started = Instant::now();
        progress(
            json!({"phase":"inventory-bootstrap","event":"start","retained_unused_cache":reusable.len(),"active_cache":0}),
        );
        let (mut hits, mut misses, mut n) = (0, 0, 0);
        let mut batch = Vec::with_capacity(CHUNK);
        {
            let mut accept = |batch: &mut Vec<Owner>| -> Result<(), String> {
                for owner in batch.iter() {
                    let raw = owner.record.as_ref().ok_or("bootstrap owner has no record")?;
                    let key = content_key(raw);
                    if board.lib.cache.contains_key(&key) {
                        hits += 1;
                        continue;
                    }
                    if let Some(d) = reusable.remove(&key) {
                        board.lib.cache.insert(key, d);
                        hits += 1;
                    } else {
                        misses += 1;
                        // A miss replaces one unused old derivation before the new
                        // chunk is derived; the old and new libraries never coexist.
                        while let Some(key) = spare.pop() {
                            if reusable.remove(&key).is_some() {
                                break;
                            }
                        }
                    }
                }
                let refs: Vec<_> = batch.iter().map(|o| o.record.as_deref().unwrap()).collect();
                let derived = board.lib.derived(&refs, &board.rulers);
                for (owner, derived) in batch.drain(..).zip(derived) {
                    if self.owners.contains_key(owner.id.as_str()) {
                        return Err("duplicate inventory source".into());
                    }
                    let key = content_key(owner.record.as_deref().unwrap());
                    let slot = board.lib.records.len();
                    board.lib.index(slot, &derived);
                    board.lib.records.push(derived);
                    let id: Arc<str> = Arc::from(owner.id);
                    self.slots.push(Some(id.clone()));
                    self.owners.insert(id.clone(), Source { slot, key });
                    write_json(out, &self.header(board, &id))?;
                    n += 1;
                }
                if n % (CHUNK * 16) == 0 {
                    progress(
                        json!({"phase":"inventory-bootstrap","event":"progress","rows":n,"cache_hits":hits,"cache_misses":misses,"active_cache":board.lib.cache.len(),"retained_unused_cache":reusable.len(),"elapsed_ms":started.elapsed().as_millis() as u64}),
                    );
                }
                Ok(())
            };
            read_lines(required(&req.builds)?, |line| {
                let owner = parse_owner(line)?;
                if owner.record.is_none() {
                    return Err("bootstrap owner has no record".into());
                }
                batch.push(owner);
                if batch.len() == CHUNK {
                    accept(&mut batch)?;
                }
                Ok(())
            })?;
            if !batch.is_empty() {
                accept(&mut batch)?;
            }
        }
        count(n, req.counts.builds, "builds")?;
        drop(reusable);
        read_facts(&mut board.facts, required(&req.facts)?, req.counts.facts, None)?;
        moved.extend(board.all_pairs());
        self.meta = Some(req.meta.clone());
        progress(
            json!({"phase":"inventory-bootstrap","event":"complete","rows":n,"cache_hits":hits,"cache_misses":misses,"active_cache":board.lib.cache.len(),"retained_unused_cache":0,"elapsed_ms":started.elapsed().as_millis() as u64}),
        );
        Ok((n, moved))
    }

    fn apply(
        &mut self,
        board: &mut Board,
        req: &Request,
        out: &mut impl Write,
    ) -> Result<(usize, BTreeSet<(usize, String)>), String> {
        let old = self.meta.as_ref().ok_or("inventory inputs require bootstrap")?;
        if req.base_epoch.as_deref() != Some(old.epoch.as_str())
            || req.meta.engine != old.engine
            || req.meta.contract != old.contract
            || req.meta.epoch == old.epoch && req.meta != *old
        {
            return Err("inventory base epoch mismatch".into());
        }
        if req.fact_ids.len() > MAX_FACT_IDS {
            return Err("too many inventory fact identities".into());
        }
        let fact_ids: HashSet<_> = req.fact_ids.iter().cloned().collect();
        if fact_ids.len() != req.fact_ids.len() {
            return Err("duplicate inventory fact identity".into());
        }
        let mut owners = Vec::new();
        let mut unique = HashSet::new();
        read_lines(required(&req.builds)?, |line| {
            let owner = parse_owner(line)?;
            if owners.len() == MAX_CHANGES || !unique.insert(owner.id.clone()) {
                return Err("invalid inventory source batch".into());
            }
            owners.push(owner);
            Ok(())
        })?;
        count(owners.len(), req.counts.builds, "builds")?;
        self.invalidate();
        let mut moved = BTreeSet::new();
        let mut affected = BTreeSet::new();
        let mut changed_slots = HashSet::new();
        let mut old_keys = Vec::new();
        let mut identities = HashSet::new();
        for owner in &owners {
            if let Some(s) = self.owners.get(owner.id.as_str()) {
                changed_slots.insert(s.slot);
                old_keys.push(s.key);
                if let Some(b) = board.lib.records[s.slot].as_ref() {
                    affected.insert(b.weapon.clone());
                    identities.insert(b.identity.clone());
                }
            }
        }
        for w in &affected {
            moved.extend((0..board.rulers.len()).map(|r| (r, w.clone())));
        }
        for w in &affected {
            if let Some(indices) = board.lib.by_weapon.get_mut(w) {
                indices.retain(|i| !changed_slots.contains(i));
            }
        }
        let recs: Vec<_> = owners.iter().filter_map(|o| o.record.as_deref()).collect();
        let derived = board.lib.derived(&recs, &board.rulers);
        let mut derived = derived.into_iter();
        for owner in &owners {
            let old = self.owners.remove(owner.id.as_str());
            let slot = old.as_ref().map_or(board.lib.records.len(), |s| s.slot);
            if let Some(record) = &owner.record {
                let d = derived.next().unwrap();
                if let Some(b) = d.as_ref() {
                    affected.insert(b.weapon.clone());
                }
                if slot == board.lib.records.len() {
                    board.lib.records.push(d);
                    self.slots.push(None);
                } else {
                    board.lib.records[slot] = d;
                }
                let id: Arc<str> = Arc::from(owner.id.as_str());
                self.slots[slot] = Some(id.clone());
                self.owners.insert(id, Source { slot, key: content_key(record) });
                if let Some(b) = board.lib.records[slot].as_ref() {
                    board.lib.by_weapon.entry(b.weapon.clone()).or_default().push(slot);
                }
            } else if old.is_some() {
                board.lib.records[slot] = Arc::new(None);
                self.slots[slot] = None;
            }
        }
        for w in &affected {
            if let Some(indices) = board.lib.by_weapon.get_mut(w) {
                indices.sort_unstable();
            }
        }
        board.lib.by_weapon.retain(|_, rows| !rows.is_empty());
        for w in &affected {
            for &i in board.lib.by_weapon.get(w).into_iter().flatten() {
                if let Some(b) = board.lib.records[i].as_ref() {
                    identities.insert(b.identity.clone());
                }
            }
        }
        for id in identities {
            board.lib.by_identity.remove(&id);
        }
        for w in &affected {
            for &i in board.lib.by_weapon.get(w).into_iter().flatten() {
                if let Some(b) = board.lib.records[i].as_ref() {
                    let source = board
                        .lib
                        .by_identity
                        .entry(b.identity.clone())
                        .or_insert_with(|| IdentitySources { weapons: BTreeSet::new(), first: i });
                    source.first = source.first.min(i);
                    source.weapons.insert(w.clone());
                }
            }
            moved.extend((0..board.rulers.len()).map(|r| (r, w.clone())));
        }
        for key in old_keys {
            if board.lib.cache.get(&key).is_some_and(|d| Arc::strong_count(d) == 1) {
                board.lib.cache.remove(&key);
            }
        }
        for id in &fact_ids {
            if let Some(&key) = board.facts.ids.at.get(id) {
                for rows in &mut board.facts.rows {
                    for mode in 0..board.facts.modes.all.len() {
                        rows.remove(&(key, mode as u32));
                    }
                }
            }
            for w in board.lib.by_identity.get(id).map(|i| &i.weapons).into_iter().flatten() {
                moved.extend((0..board.rulers.len()).map(|r| (r, w.clone())));
            }
        }
        read_facts(&mut board.facts, required(&req.facts)?, req.counts.facts, Some(&fact_ids))?;
        self.meta = Some(req.meta.clone());
        for owner in &owners {
            write_json(out, &self.header(board, &owner.id))?;
        }
        Ok((owners.len(), moved))
    }

    fn replay(&self, board: &Board, req: &Request, out: &mut impl Write) -> Result<usize, String> {
        let mut n = 0;
        read_lines(required(&req.builds)?, |line| {
            let owner = parse_owner(line)?;
            write_json(out, &self.header(board, &owner.id))?;
            n += 1;
            Ok(())
        })?;
        count(n, req.counts.builds, "builds")?;
        Ok(n)
    }

    fn members(
        &mut self,
        board: &Board,
        req: &Request,
        out: &mut impl Write,
    ) -> Result<(usize, usize, bool, Option<String>), String> {
        if self.meta.as_ref() != Some(&req.meta) {
            return Err("inventory input epoch mismatch".into());
        }
        let scope: Scope =
            serde_json::from_value(req.scope.clone().ok_or("inventory cohort required")?).map_err(|e| e.to_string())?;
        let limit = req.limit.unwrap_or(64);
        if limit == 0 || limit > 64 {
            return Err("inventory owner limit must be 1..64".into());
        }
        let facts = required(&req.facts)?;
        let queue = required(&req.queue)?;
        if self.cohort.as_ref().is_none_or(|c| c.scope != scope || c.facts != facts || c.queue != queue) {
            self.cohort = Some(self.prepare(board, req, scope)?);
        }
        let cache = self.cohort.as_ref().unwrap();
        count(cache.fact_rows, req.counts.facts, "facts")?;
        count(cache.queue_rows, req.counts.queue, "queue")?;
        let start = cache
            .owners
            .partition_point(|(id, _)| req.after_owner.as_ref().is_some_and(|after| id.as_ref() <= after.as_str()));
        let end = (start + limit).min(cache.owners.len());
        let mut rows = 0;
        for (owner, slot) in &cache.owners[start..end] {
            let b = board.lib.records[*slot].as_ref().as_ref().unwrap();
            write_json(out, &self.header(board, owner))?;
            for (r, ruler) in board.rulers.iter().enumerate() {
                let bit = 1u64 << r;
                if b.admit & bit == 0 {
                    continue;
                }
                for &mode in &b.modes {
                    let mode = if mode.is_empty() { "base" } else { mode };
                    let queued = cache.queued.get(&b.identity).and_then(|m| m.get(mode)).is_some_and(|m| m & bit != 0);
                    if !queued
                        && (board.facts.get(r, &b.identity, mode).is_some() || cache.earning[&b.identity] & bit == 0)
                    {
                        continue;
                    }
                    write_json(
                        out,
                        &json!({"type":"row","owner":owner.as_ref(),"build_id":b.identity,"ruler":ruler.id,"mode":mode,"queued":queued}),
                    )?;
                    rows += 1;
                }
            }
        }
        let complete = end == cache.owners.len();
        let next = (!complete).then(|| cache.owners[end - 1].0.to_string());
        Ok((end - start, rows, complete, next))
    }

    fn prepare(&self, board: &Board, req: &Request, scope: Scope) -> Result<Cohort, String> {
        let started = Instant::now();
        let facts = required(&req.facts)?;
        let n = read_lines(facts, |line| {
            serde_json::from_str::<serde::de::IgnoredAny>(line).map_err(|e| e.to_string())?;
            Ok(())
        })?;
        count(n, req.counts.facts, "facts")?;
        let cross = load_cross_facts(Some(facts.to_string()));
        let queue_path = required(&req.queue)?;
        let rulers: Vec<_> = board.rulers.iter().map(|r| r.id.as_str()).collect();
        let queue = Queue::load_inventory(queue_path.to_string(), &rulers);
        count(queue.rows, req.counts.queue, "queue")?;
        let queue_rows = queue.rows;
        let (queue, owed) = queue.into_parts()?;
        let by_ruler: HashMap<_, _> = board.rulers.iter().enumerate().map(|(i, r)| (r.id.as_str(), i)).collect();
        let mut queued: HashMap<String, HashMap<String, u64>> = HashMap::new();
        for (ruler, rows) in queue {
            let bit = 1u64 << by_ruler[ruler.as_str()];
            for (id, mode) in rows {
                *queued.entry(id).or_default().entry(mode).or_default() |= bit;
            }
        }
        let mut owners = Vec::new();
        let mut earning = HashMap::new();
        let mut corner = HashMap::new();
        for &slot in board.lib.by_weapon.get(&scope.weapon).into_iter().flatten() {
            let Some(b) = board.lib.records[slot].as_ref() else { continue };
            if !scope.contains(b) {
                continue;
            }
            let Some(owner) = self.slots.get(slot).and_then(Option::as_ref) else {
                return Err("inventory source metadata unavailable".into());
            };
            owners.push((owner.clone(), slot));
            if let std::collections::hash_map::Entry::Vacant(e) = earning.entry(b.identity.clone()) {
                e.insert(0u64);
                if let Some(def) = wfsim_engine::board::builds::default_corner(&b.v) {
                    corner.insert(b.identity.clone(), def);
                }
            }
        }
        owners.sort_by(|a, b| a.0.cmp(&b.0));
        for (r, ruler) in board.rulers.iter().enumerate() {
            let mut who = HashMap::new();
            for (_, slot) in &owners {
                let b = board.lib.records[*slot].as_ref().as_ref().unwrap();
                if b.admit & (1 << r) != 0 {
                    who.entry(b.identity.clone())
                        .or_insert_with(|| Who { weapon: b.weapon.clone(), riven: b.riven.is_some() });
                }
            }
            let mut pending: Vec<_> = who.keys().cloned().map(|id| (id, String::new())).collect();
            let scene = wfsim_engine::board::benchmarks::get(&ruler.id).and_then(|b| b.cell_dir());
            park_under_entry_line(&mut pending, scene.as_deref(), &cross, &who, &owed, &corner);
            for (id, _) in pending {
                *earning.get_mut(&id).unwrap() |= 1 << r;
            }
        }
        progress(
            json!({"phase":"inventory-cohort","event":"complete","weapon":scope.weapon,"riven":scope.riven,"owners":owners.len(),"facts":cross.len(),"elapsed_ms":started.elapsed().as_millis() as u64}),
        );
        Ok(Cohort {
            scope,
            facts: facts.to_string(),
            queue: queue_path.to_string(),
            fact_rows: n,
            queue_rows,
            owners,
            earning,
            queued,
        })
    }
}

fn parse_owner(line: &str) -> Result<Owner, String> {
    let owner: Owner = serde_json::from_str(line).map_err(|e| e.to_string())?;
    if owner.id.is_empty() || owner.id.len() > 128 || owner.id.chars().any(char::is_control) {
        return Err("invalid inventory source identity".into());
    }
    if owner.record.as_ref().is_some_and(|r| !r.get().trim_start().starts_with('{')) {
        return Err("inventory source record must be an object".into());
    }
    Ok(owner)
}

fn required(path: &Option<String>) -> Result<&str, String> {
    path.as_deref().filter(|p| Path::new(p).is_absolute()).ok_or_else(|| "absolute inventory input required".into())
}

fn read_lines(path: &str, mut visit: impl FnMut(&str) -> Result<(), String>) -> Result<usize, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut n = 0;
    for line in std::io::BufReader::new(file).lines() {
        let line = line.map_err(|e| e.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        visit(&line)?;
        n += 1;
    }
    Ok(n)
}

fn count(actual: usize, expected: Option<usize>, name: &str) -> Result<(), String> {
    if Some(actual) != expected {
        return Err(format!("incomplete inventory {name}"));
    }
    Ok(())
}

fn read_facts(
    facts: &mut Facts,
    path: &str,
    expected: Option<usize>,
    ids: Option<&HashSet<String>>,
) -> Result<(), String> {
    let n = read_lines(path, |line| {
        let f: FactLine = serde_json::from_str(line).map_err(|e| e.to_string())?;
        if ids.is_some_and(|ids| text(f.identity).is_none_or(|id| !ids.contains(id.as_ref()))) {
            return Err("inventory fact outside authoritative identities".into());
        }
        facts.apply_fact(f);
        Ok(())
    })?;
    count(n, expected, "facts")
}

fn write_json(out: &mut impl Write, value: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *out, value).map_err(|e| e.to_string())?;
    writeln!(out).map_err(|e| e.to_string())
}

fn progress(mut event: Value) {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        for (key, field) in [("VmRSS:", "rss_bytes"), ("VmHWM:", "peak_rss_bytes"), ("VmSwap:", "swap_bytes")] {
            let value = status
                .lines()
                .find_map(|line| line.strip_prefix(key))
                .and_then(|v| v.split_whitespace().next())
                .and_then(|v| v.parse::<u64>().ok())
                .and_then(|v| v.checked_mul(1024));
            if let Some(value) = value {
                event[field] = json!(value);
            }
        }
    }
    super::super::print_log(format!("inventory-phase {event}\n").as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    type Answer = (Vec<Value>, BTreeSet<(usize, String)>);

    struct Fixture {
        dir: PathBuf,
        board: Board,
        state: State,
        serial: usize,
    }

    impl Fixture {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("wfsim-inventory-adapter-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self {
                board: Board::new(dir.join("board"), dir.join("state.yml")),
                state: State::default(),
                dir,
                serial: 0,
            }
        }

        fn file(&self, name: &str, rows: &[Value]) -> String {
            let path = self.dir.join(name);
            let text: String = rows.iter().map(|r| format!("{r}\n")).collect();
            std::fs::write(&path, text).unwrap();
            path.to_string_lossy().into_owned()
        }

        fn request(&self, op: &str, epoch: &str) -> Value {
            json!({"v":1,"op":op,"engine":"0ae1bf8edb0f","contract":{"v":1,"covers":["new_build"],"algorithm":"board-inventory-v1"},
                "snapshot":format!("snapshot-{epoch}"),"generation":"opaque-pg-fence","epoch":epoch})
        }

        fn execute(&mut self, req: &Value) -> Result<Answer, String> {
            self.serial += 1;
            let input = self.dir.join(format!("req-{}.json", self.serial));
            let output = self.dir.join(format!("out-{}.ndjson", self.serial));
            std::fs::write(&input, req.to_string()).unwrap();
            match self.state.execute(&mut self.board, input.to_str().unwrap(), output.to_str().unwrap()) {
                Ok((_, moved)) => Ok((
                    std::fs::read_to_string(output)
                        .unwrap()
                        .lines()
                        .map(|l| serde_json::from_str(l).unwrap())
                        .collect(),
                    moved,
                )),
                Err(e) => {
                    assert!(!output.exists(), "failed input left a final inventory");
                    Err(e)
                }
            }
        }

        fn bootstrap(&mut self, owners: &[Value], facts: &[Value]) {
            let mut req = self.request("bootstrap", "one");
            req["builds"] = json!(self.file("builds.ndjson", owners));
            req["facts"] = json!(self.file("facts.ndjson", facts));
            req["counts"] = json!({"builds":owners.len(),"facts":facts.len()});
            self.execute(&req).unwrap();
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn braton(last: &str) -> Value {
        json!({"weapon":"braton_prime","mods":["vital_sense","hellfire","speed_trigger","primary_acuity",
            "galvanized_aptitude","hammer_shot","vigilante_fervor",last],
            "evolutions":["braton_prime_evo1_incarnon_form","braton_prime_daring_reverie","braton_prime_voids_guidance","braton_prime_prelude_of_might"],
            "arcanes":["primary_deadhead"]})
    }

    fn id(record: &Value) -> String {
        wfsim_engine::board::builds::build_id(&wfsim_webapi::board_rows::board_build(record).unwrap())
    }

    fn fact(id: &str, ruler: &str, score: f64, at: &str) -> Value {
        json!({"identity":id,"ruler":ruler,"mode":"base","score":score,"finished_at":at,"measured_by":"verified:test"})
    }

    fn cohort(f: &Fixture, epoch: &str, facts: &str, queue: &str, nf: usize, nq: usize) -> Value {
        let mut req = f.request("cohort", epoch);
        req["scope"] = json!({"weapon":"braton_prime","riven":false});
        req["facts"] = json!(facts);
        req["queue"] = json!(queue);
        req["counts"] = json!({"facts":nf,"queue":nq});
        req["limit"] = json!(1);
        req
    }

    fn full_members(
        f: &Fixture,
        owners: &[Value],
        queue: &[Value],
        facts_path: String,
        queue_path: String,
    ) -> BTreeMap<(String, String, String), bool> {
        let lib: Vec<_> = owners
            .iter()
            .map(|o| {
                let rec = o["record"].clone();
                let built = wfsim_webapi::board_rows::board_build(&rec);
                (rec, built)
            })
            .collect();
        let ids: Vec<_> = f.board.rulers.iter().map(|r| r.id.as_str()).collect();
        let mut by = crate::board::facts::load_facts_of(Some(facts_path.clone()), &ids);
        let cross = load_cross_facts(Some(facts_path));
        let shared = Queue::load(Some(queue_path), None, true);
        let mut expected = BTreeMap::new();
        for ruler in &f.board.rulers {
            let missing = f.dir.join(format!("{}.missing", ruler.id)).to_string_lossy().into_owned();
            let orphan = f.dir.join(format!("{}.orphan", ruler.id)).to_string_lossy().into_owned();
            let flag = |n: &str| match n {
                "--queue-missing" => Some(missing.clone()),
                "--queue-orphans" => Some(orphan.clone()),
                _ => None,
            };
            crate::board::captured(|| {
                crate::board::run::ruler(
                    &ruler.id,
                    &lib,
                    by.remove(&ruler.id).unwrap(),
                    &cross,
                    shared.builds_owed(),
                    &shared,
                    &flag,
                    &|n| n == "--dry-run" || n == "--gate",
                )
            });
            let read = |p: &str| {
                std::fs::read_to_string(p)
                    .unwrap()
                    .lines()
                    .map(|l| serde_json::from_str::<Value>(l).unwrap())
                    .collect::<Vec<_>>()
            };
            let excluded: BTreeSet<_> = read(&orphan)
                .iter()
                .map(|r| (r["build_id"].as_str().unwrap().to_string(), r["mode"].as_str().unwrap().to_string()))
                .collect();
            for q in queue {
                let key = (q["build_id"].as_str().unwrap().to_string(), q["mode"].as_str().unwrap().to_string());
                if q["ruler"] == ruler.id && !excluded.contains(&key) {
                    expected.insert((key.0, ruler.id.clone(), key.1), true);
                }
            }
            for m in read(&missing) {
                expected
                    .entry((
                        m["build_id"].as_str().unwrap().to_string(),
                        ruler.id.clone(),
                        m["mode"].as_str().unwrap().to_string(),
                    ))
                    .or_insert(false);
            }
        }
        expected
    }

    #[test]
    fn paged_membership_is_the_complete_ruler_walk_including_aliases_and_old_rulers() {
        let mut f = Fixture::new("parity");
        let lead = braton("magnetic_capacity");
        let low = braton("primed_magazine_warp");
        let mut alias = lead.clone();
        alias["evolutions"].as_array_mut().unwrap().reverse();
        assert_eq!(id(&lead), id(&alias));
        let owners = vec![
            json!({"id":"z-first","record":lead}),
            json!({"id":"a-alias","record":alias}),
            json!({"id":"middle","record":low}),
        ];
        let lead_id = id(&owners[0]["record"]);
        let low_id = id(&owners[2]["record"]);
        let facts = vec![
            fact(&lead_id, "standard_single_target_v19", 100.0, "2026-11-06T00:00:00.000Z"),
            fact(&low_id, "standard_single_target", 5.0, "2026-11-06T00:00:00.000Z"),
        ];
        let queue = vec![
            json!({"build_id":lead_id,"ruler":"demolisher","mode":"base"}),
            json!({"build_id":lead_id,"ruler":"demolisher","mode":"base"}),
            json!({"build_id":low_id,"ruler":"standard_single_target","mode":"base"}),
            json!({"build_id":"orphan","ruler":"standard_single_target","mode":"base"}),
            json!({"build_id":lead_id,"ruler":"retired","mode":"base"}),
        ];
        f.bootstrap(&owners, &facts);
        let facts_path = f.file("cohort-facts.ndjson", &facts);
        let queue_path = f.file("queue.ndjson", &queue);
        let mut req = cohort(&f, "one", &facts_path, &queue_path, facts.len(), queue.len());
        let mut actual = BTreeMap::new();
        let mut owner_ids = BTreeSet::new();
        let mut canonical = None;
        loop {
            let (rows, _) = f.execute(&req).unwrap();
            for row in &rows {
                match row["type"].as_str().unwrap() {
                    "owner" => {
                        owner_ids.insert(row["owner"].as_str().unwrap().to_string());
                        if row["build_id"] == lead_id {
                            if let Some(record) = &canonical {
                                assert_eq!(&row["record"], record, "pagination changed the shared fallback");
                            } else {
                                canonical = Some(row["record"].clone());
                            }
                            assert_eq!(row["binding_owner"], "z-first");
                        }
                    }
                    "row" => {
                        let k = (
                            row["build_id"].as_str().unwrap().to_string(),
                            row["ruler"].as_str().unwrap().to_string(),
                            row["mode"].as_str().unwrap().to_string(),
                        );
                        actual
                            .entry(k)
                            .and_modify(|v| *v |= row["queued"].as_bool().unwrap())
                            .or_insert(row["queued"].as_bool().unwrap());
                    }
                    _ => {}
                }
            }
            let end = rows.last().unwrap();
            if end["complete"] == true {
                break;
            }
            req["after_owner"] = end["next_owner"].clone();
        }
        assert_eq!(
            owner_ids,
            ["a-alias".to_string(), "middle".to_string(), "z-first".to_string()].into_iter().collect()
        );
        let expected = full_members(&f, &owners, &queue, facts_path, queue_path);
        assert!(!actual.is_empty());
        assert_eq!(actual, expected);
    }

    #[test]
    fn source_deletion_and_fact_replacement_are_authoritative_and_replay_safe() {
        let mut f = Fixture::new("replace");
        let record = braton("magnetic_capacity");
        let build = id(&record);
        f.bootstrap(
            &[json!({"id":"first","record":record}), json!({"id":"alias","record":record})],
            &[fact(&build, "standard_single_target", 10.0, "2026-11-06T00:00:00.000Z")],
        );
        let mut req = f.request("apply", "two");
        req["base_epoch"] = json!("one");
        req["builds"] = json!(f.file("changed.ndjson", &[json!({"id":"first","record":null})]));
        req["facts"] = json!(f.file("changed-facts.ndjson", &[]));
        req["fact_ids"] = json!([build]);
        req["counts"] = json!({"builds":1,"facts":0});
        let (rows, moved) = f.execute(&req).unwrap();
        assert_eq!(rows[0]["owner"], "first");
        assert!(rows[0]["build_id"].is_null());
        assert!(!f.state.owners.contains_key("first"));
        assert!(f.state.owners.contains_key("alias"));
        assert_eq!(moved.len(), f.board.rulers.len());
        assert!(f.board.facts.rows.iter().all(HashMap::is_empty));
        assert_eq!(f.execute(&req).unwrap().0, rows, "lost acknowledgement changed the replay");
        let header = f.state.header(&f.board, "alias");
        assert_eq!(header["binding_owner"], "alias");
        let mut same = req.clone();
        same["base_epoch"] = json!("two");
        same["builds"] = json!(f.file("second-chunk.ndjson", &[json!({"id":"new-alias","record":record})]));
        same["fact_ids"] = json!([]);
        f.execute(&same).unwrap();
        assert!(f.state.owners.contains_key("new-alias"));
        let mut stale = same.clone();
        stale["snapshot"] = json!("changed-under-same-epoch");
        assert!(f.execute(&stale).is_err());
    }

    #[test]
    fn riven_corners_and_scene_fallback_match_the_complete_walk_before_and_after_retirement() {
        let record = |rolls| {
            json!({"weapon":"furis","mods":["primed_convulsion","pathogen_rounds","galvanized_diffusion","primed_target_cracker",
            "galvanized_shot","gunslinger","magnetic_might","riven"],
            "evolutions":["furis_evo1_incarnon_form","furis_stormburst","furis_extended_volley","furis_headcracker"],
            "arcanes":["secondary_enervate"],"exilus":"eject_magazine","riven_pos":["damage","multishot"],"riven_neg":"weapon_recoil","riven_rolls":rolls})
        };
        let default = record([1.1, 1.1, 0.9]);
        let corner = record([0.9, 1.1, 0.9]);
        let default_id = id(&default);
        let corner_id = id(&corner);
        let owners = vec![json!({"id":"default","record":default}), json!({"id":"corner","record":corner})];
        let mut totals = Vec::new();
        for retired in [false, true] {
            let mut f = Fixture::new(if retired { "corner-retired" } else { "corner-live" });
            let scene = f.board.rulers.iter().find(|r| !r.dir.is_empty()).unwrap().id.clone();
            let mut facts = vec![
                fact(&default_id, "standard_single_target", 100.0, "2026-11-06T00:00:00.000Z"),
                fact(&corner_id, &scene, 100.0, "2026-11-06T00:00:00.000Z"),
            ];
            if retired {
                let mut removal = facts[0].clone();
                removal["retired_at"] = json!("2026-11-06T00:01:00.000Z");
                facts.push(removal);
            }
            let queue = vec![
                json!({"build_id":default_id,"ruler":"retired","mode":"base"}),
                json!({"build_id":corner_id,"ruler":"demolisher","mode":"base"}),
            ];
            f.bootstrap(&owners, &facts);
            let fp = f.file("scope-facts.ndjson", &facts);
            let qp = f.file("scope-queue.ndjson", &queue);
            let mut req = cohort(&f, "one", &fp, &qp, facts.len(), queue.len());
            req["scope"] = json!({"weapon":"furis","riven":true});
            req["limit"] = json!(64);
            let (rows, _) = f.execute(&req).unwrap();
            let actual: BTreeMap<_, _> = rows
                .iter()
                .filter(|r| r["type"] == "row")
                .map(|r| {
                    (
                        (
                            r["build_id"].as_str().unwrap().to_string(),
                            r["ruler"].as_str().unwrap().to_string(),
                            r["mode"].as_str().unwrap().to_string(),
                        ),
                        r["queued"].as_bool().unwrap(),
                    )
                })
                .collect();
            assert_eq!(actual, full_members(&f, &owners, &queue, fp, qp));
            totals.push(actual.len());
        }
        assert!(totals[0] > totals[1], "retiring the default must change which corners earn work");
    }

    #[test]
    fn incomplete_or_legacy_inputs_cannot_acknowledge_an_inventory() {
        let mut f = Fixture::new("closed");
        let record = braton("magnetic_capacity");
        f.bootstrap(&[json!({"id":"source","record":record})], &[]);
        f.state.invalidate();
        assert!(!f.state.permits_publish());
        let empty = f.file("empty.ndjson", &[]);
        let req = cohort(&f, "one", &empty, &empty, 0, 0);
        assert!(f.execute(&req).is_err());
        let mut boot = f.request("bootstrap", "two");
        boot["builds"] = json!(f.file("broken-builds.ndjson", &[json!({"id":"source","record":record})]));
        boot["facts"] = json!(empty);
        boot["counts"] = json!({"builds":2,"facts":0});
        assert!(f.execute(&boot).is_err());
        assert!(!f.state.permits_publish());
    }

    #[test]
    fn invalid_queue_input_cannot_become_a_complete_inventory() {
        let mut f = Fixture::new("invalid-queue");
        let record = braton("magnetic_capacity");
        let build = id(&record);
        f.bootstrap(&[json!({"id":"source","record":record})], &[]);
        let facts = f.file("empty.facts", &[]);
        let queue = f.file("invalid.queue", &[json!({"build_id":build,"ruler":"standard_single_target","mode":null})]);
        let req = cohort(&f, "one", &facts, &queue, 0, 1);
        assert!(f.execute(&req).is_err());
    }

    #[test]
    fn each_targets_admitted_builds_determine_its_gate_leaders() {
        let mut f = Fixture::new("contexts");
        let lead = braton("magnetic_capacity");
        let low = braton("primed_magazine_warp");
        let lead_id = id(&lead);
        let low_id = id(&low);
        let facts = vec![
            fact(&lead_id, "standard_single_target", 100.0, "2026-11-06T00:00:00.000Z"),
            fact(&low_id, "standard_single_target", 10.0, "2026-11-06T00:00:00.000Z"),
        ];
        f.bootstrap(&[json!({"id":"leader","record":lead}), json!({"id":"low","record":low})], &facts);
        f.board.lib.cache.clear();
        Arc::get_mut(&mut f.board.lib.records[0]).unwrap().as_mut().unwrap().admit = 1;
        Arc::get_mut(&mut f.board.lib.records[1]).unwrap().as_mut().unwrap().admit = 3;
        let fp = f.file("context.facts", &facts);
        let qp = f.file("context.queue", &[]);
        let req: Request = serde_json::from_value(cohort(&f, "one", &fp, &qp, facts.len(), 0)).unwrap();
        let cache =
            f.state.prepare(&f.board, &req, Scope { weapon: "braton_prime".into(), riven: Some(false) }).unwrap();
        assert_eq!(cache.earning[&low_id] & 1, 0, "the admitted leader parks the low build");
        assert_ne!(cache.earning[&low_id] & 2, 0, "a refused leader cannot set this target's line");
    }

    #[test]
    fn a_retirement_wins_a_late_fact_at_the_same_clock() {
        let mut f = Fixture::new("retired");
        let record = braton("magnetic_capacity");
        let build = id(&record);
        let old = fact(&build, "standard_single_target", 10.0, "2026-11-06T00:00:00.000Z");
        let mut retired = old.clone();
        retired["retired_at"] = json!("2026-11-06T00:00:00.000Z");
        f.bootstrap(&[json!({"id":"source","record":record})], &[old.clone(), retired, old]);
        let r = f.board.facts.rulers["standard_single_target"];
        assert!(f.board.facts.get(r, &build, "base").is_none());
    }

    #[test]
    fn bootstrap_reuses_derivations_and_drops_unreferenced_old_records() {
        let mut f = Fixture::new("cache");
        let old = braton("magnetic_capacity");
        let fresh = braton("primed_magazine_warp");
        let raw = serde_json::value::to_raw_value(&old).unwrap();
        f.board.add_builds(&[raw.as_ref()]);
        let weak = Arc::downgrade(&f.board.lib.records[0]);
        f.bootstrap(&[json!({"id":"kept","record":old})], &[]);
        assert!(Arc::ptr_eq(&weak.upgrade().unwrap(), &f.board.lib.records[0]));
        f.bootstrap(&[json!({"id":"replacement","record":fresh})], &[]);
        assert!(weak.upgrade().is_none());
        assert_eq!(f.board.lib.cache.len(), 1);
        assert_eq!(f.state.owners.len(), 1);
    }
}
