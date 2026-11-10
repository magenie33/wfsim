// SPDX-License-Identifier: AGPL-3.0-or-later
use crate::{
    budget::{self, Budget, Pressure},
    compute::{self, Progress},
    engine::Engine,
    net::Net,
    state::{self, Config, Pending, Result},
};
use serde_json::{json, Value};
use std::io::IsTerminal;
use std::{
    collections::VecDeque,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

struct Active {
    lease: String,
    task: Value,
    search: bool,
    budget: Arc<Budget>,
    answer: mpsc::Receiver<Result<Value>>,
    progress: mpsc::Receiver<Progress>,
    began: Instant,
}

#[derive(Debug)]
pub struct EngineChanged;
impl std::fmt::Display for EngineChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "engine changed")
    }
}
impl std::error::Error for EngineChanged {}

fn millis(v: &Value, key: &str, fallback: u64) -> u64 {
    v[key].as_u64().unwrap_or(fallback)
}

fn pending(dir: &Path) -> Result<Vec<Pending>> {
    if dir.join("pending.json").exists() {
        state::read(&dir.join("pending.json"))
    } else {
        Ok(Vec::new())
    }
}

fn save(dir: &Path, out: &[Pending]) -> Result<()> {
    state::write(&dir.join("pending.json"), &out)
}

pub fn run(dir: &Path, id: &str, quit: Arc<AtomicBool>) -> Result<()> {
    let config = Config::load(dir)?;
    let net = Net::new(&config.server);
    let nonce = match std::env::var("WFSIM_CLI_RUN") {
        Ok(nonce) if nonce.len() == 24 && nonce.bytes().all(|b| b.is_ascii_hexdigit()) => nonce,
        _ => state::random_hex(12)?,
    };
    state::write(
        &dir.join("status.json"),
        &json!({"run":nonce,"pid":std::process::id(),"at":state::timestamp(),"state":"starting"}),
    )?;
    let cores = std::thread::available_parallelism()?.get().min(256);
    let shared_budget = Arc::new(Budget::new(cores));
    let mut pressure = Pressure::new()?;
    let mut out = pending(dir)?;
    let mut engine: Option<Arc<Engine>> = None;
    let mut engine_id = String::new();
    let mut next_engine = Instant::now();
    let mut next_signal = Instant::now();
    let mut next_sync = Instant::now();
    let mut next_retry = Instant::now();
    let mut next_report = Instant::now();
    let mut next_renew = Instant::now() + Duration::from_secs(60);
    let mut blocked_until = Instant::now();
    let mut next_sample = Instant::now();
    let mut next_status = Instant::now();
    let mut next_flush = Instant::now();
    let mut result_gate = Instant::now();
    let mut draining = !out.is_empty();
    let main_thread = std::thread::current();
    let mut observed = (0.0, 0.0, None);
    let mut last_range = config.range.clone();
    let mut last_held = false;
    let mut queue: VecDeque<(String, Value)> = VecDeque::new();
    let mut active: Option<Active> = None;
    let mut dials = json!({});
    let mut protocol = 8;
    let mut signal_open = true;
    let mut previous = 0.0;
    let mut backoff = 2u64;
    let mut pace_ms: f64 = 0.0;
    let mut completed = 0usize;
    let mut releasing = false;
    let mut give_back: Vec<(String, Option<String>)> = Vec::new();
    let mut refused: Vec<Value> = Vec::new();
    let mut progress = String::new();
    let mut previous_state = String::new();
    let mut last_error: Option<String> = None;
    let mut logged_at = Instant::now();
    lower_priority();
    state::write(
        &dir.join("host-ready.json"),
        &json!({"run":nonce,"pid":std::process::id(),"version":crate::BUILD}),
    )?;
    loop {
        let now = Instant::now();
        // Completed answers reach durable storage before any release or shutdown.
        if let Some(job) = &active {
            while let Ok(p) = job.progress.try_recv() {
                progress = match p {
                    Progress::Row { done, total } => format!("{done}/{total} runs"),
                    Progress::Search { step, builds } => {
                        format!("round {}: {builds} builds", step + 1)
                    }
                };
            }
            if let Ok(result) = job.answer.try_recv() {
                match result {
                    Ok(mut answer) => {
                        if !job.search {
                            let elapsed = job.began.elapsed().as_secs_f64() * 1000.0;
                            pace_ms = if pace_ms == 0.0 {
                                elapsed
                            } else {
                                pace_ms * 0.8 + elapsed * 0.2
                            };
                        }
                        let path = if job.search {
                            answer["lease"] = json!(job.lease);
                            answer["verifier"] = json!(id);
                            answer["engine"] = json!(engine_id);
                            format!(
                                "/api/appraise/{}/result",
                                job.task["code"].as_str().ok_or("missing appraisal code")?
                            )
                        } else {
                            answer["lease"] = json!(job.lease);
                            "/api/board/sync".into()
                        };
                        out.push(Pending {
                            engine: engine_id.clone(),
                            path,
                            body: answer,
                        });
                        save(dir, &out)?;
                        completed += 1;
                    }
                    Err(e) if !job.budget.cancelled.load(Ordering::Relaxed) => {
                        if job.search {
                            give_back.push((
                                job.lease.clone(),
                                job.task["code"].as_str().map(String::from),
                            ));
                            releasing = true;
                        } else if protocol == 7 {
                            give_back.push((job.lease.clone(), None));
                            give_back.extend(queue.iter().map(|(lease, _)| (lease.clone(), None)));
                            queue.clear();
                            releasing = true;
                        } else {
                            releasing = true;
                        }
                        if !job.search && e.to_string().starts_with("engine /api/board/order:") {
                            refused.push(json!({"lease":job.lease,"question":job.task["question"],"step":"order"}));
                        }
                        eprintln!("Task returned: {e}");
                    }
                    Err(_) => {}
                }
                active = None;
                progress.clear();
            }
        }
        let config = Config::load(dir)?;
        if config.server != net.base {
            return Err("server changed while running; stop and restart the client".into());
        }
        let stop_requested = state::read::<Value>(&dir.join("stop.json"))
            .ok()
            .is_some_and(|s| s["run"] == nonce);
        let updating = state::read::<Value>(&dir.join("host-update.json"))
            .ok()
            .is_some_and(|s| s["run"] == nonce);
        let stopping = quit.load(Ordering::Relaxed) || stop_requested || updating;
        let sampled = now >= next_sample;
        if sampled {
            observed = pressure.sample();
            next_sample = now + Duration::from_secs(1);
        }
        let (cpu, own, reason) = observed;
        let consented = config
            .consent
            .as_ref()
            .is_some_and(|c| c.v == state::CONSENT_VERSION);
        let cooling = now < blocked_until;
        let held = config.paused || reason.is_some() || !consented || cooling;
        let old_target = previous;
        if sampled || config.range != last_range || held != last_held || stopping {
            previous = budget::target(&config.range, cpu, own, previous, held || stopping);
            last_range = config.range.clone();
            last_held = held;
        }
        if previous != old_target {
            pace_ms = 0.0;
        }
        shared_budget.set(previous);
        let paused = previous <= 0.0;
        if paused
            && (!queue.is_empty()
                || active
                    .as_ref()
                    .is_some_and(|j| !j.budget.cancelled.load(Ordering::Relaxed)))
        {
            give_back.extend(
                queue
                    .iter()
                    .map(|(lease, task)| (lease.clone(), task["code"].as_str().map(String::from))),
            );
            if let Some(job) = &active {
                give_back.push((
                    job.lease.clone(),
                    job.task["code"].as_str().map(String::from),
                ));
                job.budget.cancelled.store(true, Ordering::Relaxed);
            }
            queue.clear();
            releasing = true;
        }
        let mut phase = if stopping {
            "stopping"
        } else if !consented {
            "consent required"
        } else if config.paused {
            "paused"
        } else if cooling {
            "contribution cooldown"
        } else if let Some(r) = reason {
            r
        } else if paused {
            "CPU budget is zero"
        } else if active.is_some() {
            "computing"
        } else if engine.is_none() {
            "loading engine"
        } else {
            "waiting for work"
        }
        .to_string();
        if last_error.is_some() && now < next_retry {
            phase = "retrying".into();
        }

        if now >= next_retry || stopping {
            let result = (|| -> Result<()> {
                for (lease, code) in &give_back {
                    if let Some(code) = code {
                        net.post(
                            &format!("/api/appraise/{code}/renew"),
                            &json!({"verifier":id,"lease":lease,"release":true}),
                        )?;
                    }
                }
                // Appraisal results have their own door and are retried from disk.
                let mut at = 0;
                while at < out.len() {
                    if out[at].path == "/api/board/sync" {
                        at += 1;
                        continue;
                    }
                    let reply = match net.post(&out[at].path, &out[at].body) {
                        Ok(reply) => reply,
                        Err(error)
                            if error
                                .downcast_ref::<crate::net::ExpiredAppraisal>()
                                .is_some() =>
                        {
                            eprintln!("The server has expired an appraisal; its pending answer was dropped.");
                            out.remove(at);
                            save(dir, &out)?;
                            continue;
                        }
                        Err(error) => return Err(error),
                    };
                    if reply["ok"] != true {
                        return Err(format!("result submission: {reply}").into());
                    }
                    out.remove(at);
                    save(dir, &out)?;
                }
                let engine_due = active.is_none()
                    && queue.is_empty()
                    && out.is_empty()
                    && (engine.is_none() || now >= next_engine);
                if engine_due && !stopping && !paused {
                    let release = net.get(&format!("{}/release.json", net.base))?.0;
                    let id = release["engine"]
                        .as_str()
                        .ok_or("release has no engine identity")?;
                    if !engine_id.is_empty() && (id != engine_id || engine.is_none()) {
                        return Err(EngineChanged.into());
                    }
                    if id != engine_id || engine.is_none() {
                        println!("Loading official engine {id}…");
                        engine = Some(Arc::new(Engine::load(&net, dir, id)?));
                        engine_id = id.into();
                    }
                    next_engine = now + Duration::from_secs(300);
                }
                if now >= next_signal && !stopping && !paused {
                    let url = if net.base == "https://wfsim.app" {
                        "https://cdn.wfsim.app/work.json".into()
                    } else {
                        format!("{}/work.json", net.base)
                    };
                    match net.get(&url) {
                        Ok((signal, date)) => {
                            let fresh = date
                                .zip(signal["at"].as_str().and_then(|s| {
                                    time::OffsetDateTime::parse(
                                        s,
                                        &time::format_description::well_known::Rfc3339,
                                    )
                                    .ok()
                                }))
                                .is_some_and(|(date, at)| {
                                    let signed = std::time::SystemTime::UNIX_EPOCH
                                        + Duration::from_secs(at.unix_timestamp().max(0) as u64);
                                    date.duration_since(signed)
                                        .map(|age| age <= Duration::from_secs(90))
                                        .unwrap_or(true)
                                });
                            signal_open = !fresh
                                || signal["mode"] == "ask"
                                || signal["open"].as_u64().unwrap_or(0) > 0
                                || signal["urgent"].as_u64().unwrap_or(0) > 0;
                            if signal["engine"].as_str().is_some_and(|id| id != engine_id) {
                                next_engine = now;
                            }
                            let poll = if fresh && signal["mode"] != "ask" {
                                millis(&signal, "poll_ms", 10_000)
                            } else {
                                120_000
                            };
                            next_signal = now + Duration::from_millis(jitter(poll.max(1000), 25)?);
                            if signal_open {
                                next_sync = next_sync.max(
                                    now + Duration::from_millis(random_below(millis(
                                        &signal,
                                        if signal["urgent"].as_u64().unwrap_or(0) > 0 {
                                            "urgent_jitter_ms"
                                        } else {
                                            "jitter_ms"
                                        },
                                        5000,
                                    ))?),
                                );
                            }
                        }
                        Err(_) => {
                            signal_open = true;
                            next_signal = now + Duration::from_millis(jitter(120_000, 25)?);
                        }
                    }
                }
                draining |= out.len() >= 100;
                let has_results = !cooling
                    && !out.is_empty()
                    && now >= result_gate
                    && (draining || now >= next_flush);
                let needs_work = !paused
                    && !stopping
                    && signal_open
                    && queue.is_empty()
                    && active.is_none()
                    && now >= next_sync;
                let renew = (active.is_some() || !queue.is_empty()) && now >= next_renew;
                let report = dials["report_ms"] != 0 && now >= next_report && !paused && !stopping;
                if has_results
                    || needs_work
                    || renew
                    || releasing
                    || stopping
                    || report
                    || !refused.is_empty()
                {
                    let mut renew_after =
                        millis(&dials, "sync_lease_ms", millis(&dials, "renew_ms", 300_000)) / 2;
                    let send_engine = out
                        .first()
                        .map(|p| p.engine.as_str())
                        .unwrap_or(&engine_id)
                        .to_string();
                    if send_engine.is_empty() {
                        return Ok(());
                    }
                    let rows: Vec<Value> = out
                        .iter()
                        .filter(|p| p.engine == send_engine && p.path == "/api/board/sync")
                        .take(100)
                        .map(|p| p.body.clone())
                        .collect();
                    let mut held_rows: Vec<Value> = queue
                        .iter()
                        .map(|(lease, t)| json!({"lease":lease,"rows":[t["question"]]}))
                        .collect();
                    hold_unsent(&out, &send_engine, &rows, &mut held_rows);
                    if let Some(job) = &active {
                        if !releasing
                            && !job.search
                            && !job.budget.cancelled.load(Ordering::Relaxed)
                        {
                            held_rows
                                .push(json!({"lease":job.lease,"rows":[job.task["question"]]}));
                        }
                    }
                    if let Some(job) = &active {
                        if job.search
                            && renew
                            && !releasing
                            && !stopping
                            && !job.budget.cancelled.load(Ordering::Relaxed)
                        {
                            let r = net.post(&format!("/api/appraise/{}/renew", job.task["code"].as_str().ok_or("missing appraisal code")?),
                            &json!({"verifier":id,"lease":job.lease,"release":releasing||stopping}))?;
                            if r["held"] == false {
                                job.budget.cancelled.store(true, Ordering::Relaxed);
                            }
                        }
                    }
                    if protocol == 8 {
                        let response = net.post("/api/board/sync", &json!({"verifier":id,"engine":send_engine,"protocol":8,
                            "consent":config.consent,"lanes":((cores as f64*previous/100.0).ceil() as usize).clamp(1,256),
                            "pace_ms":pace_ms.round() as u64,
                            "want_ms":if needs_work && send_engine==engine_id {millis(&dials,"pack8_ms",600_000)}else{0},
                            "held":held_rows,"results":rows,"refused":refused}))?;
                        if response["ok"] != true {
                            return Err(format!("sync: {response}").into());
                        }
                        if response["busy"] == true || response["paused"] == true {
                            backoff =
                                backoff.max(millis(&response, "next_ms", 20_000).div_ceil(1000));
                            return Err(
                                "server is temporarily paused; keeping pending answers".into()
                            );
                        }
                        if response["fallback"] == 7 {
                            protocol = 7;
                            next_sync = now;
                        } else {
                            if !rows.is_empty() {
                                next_flush = Instant::now()
                                    + Duration::from_millis(millis(&dials, "flush_ms", 600_000));
                                let acknowledged = response["taken"]
                                    .as_array()
                                    .is_some_and(|xs| !xs.is_empty())
                                    || response["dropped"]
                                        .as_array()
                                        .is_some_and(|xs| !xs.is_empty());
                                result_gate = Instant::now()
                                    + Duration::from_millis(if acknowledged {
                                        0
                                    } else {
                                        millis(&response, "next_ms", 20_000).clamp(1000, 120_000)
                                    });
                            }
                            if needs_work
                                && !response["pack"].is_object()
                                && !response["work"].is_object()
                            {
                                signal_open = false;
                            }
                            acknowledge(&mut out, &send_engine, &response);
                            draining &= !out.is_empty();
                            refused.clear();
                            save(dir, &out)?;
                            for h in response["held"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .filter(|h| h["held"] == false)
                            {
                                queue.retain(|(_, t)| t["question"] != h["question"]);
                                if let Some(job) = &active {
                                    if job.task["question"] == h["question"] {
                                        job.budget.cancelled.store(true, Ordering::Relaxed);
                                    }
                                }
                            }
                            if response["dials"].is_object() {
                                dials = response["dials"].clone();
                            }
                            if let Some(pack) = response["pack"].as_object() {
                                let lease = pack
                                    .get("lease")
                                    .and_then(Value::as_str)
                                    .ok_or("pack has no lease")?
                                    .to_string();
                                for t in pack
                                    .get("tasks")
                                    .and_then(Value::as_array)
                                    .ok_or("pack has no tasks")?
                                {
                                    queue.push_back((lease.clone(), t.clone()));
                                }
                                let elapsed = millis(&response["pack"], "lease_ms", 300_000);
                                renew_after = (elapsed / 2).max(1000);
                            }
                            if response["work"].is_object() {
                                let w = &response["work"];
                                queue.push_back((
                                    w["lease"].as_str().ok_or("work has no lease")?.into(),
                                    w.clone(),
                                ));
                                renew_after = 120_000;
                            }
                            if response["stale"] == true {
                                queue.clear();
                                if let Some(job) = &active {
                                    job.budget.cancelled.store(true, Ordering::Relaxed);
                                }
                                next_engine = now;
                                engine = None;
                            }
                            if response["banned"] == true {
                                eprintln!("Contribution cooldown until {}", response["until"]);
                                blocked_until = Instant::now()
                                    + net
                                        .until(response["until"].as_str().unwrap_or(""))
                                        .unwrap_or(Duration::from_secs(60));
                            }
                            next_sync = Instant::now()
                                + Duration::from_millis(millis(
                                    &response,
                                    "next_ms",
                                    millis(&dials, "sync_idle_ms", 20_000),
                                ));
                        }
                    } else {
                        // Protocol 7 remains the server-directed compatibility path.
                        refused.clear();
                        for answer in &rows {
                            let r=net.post("/api/board/answers",&json!({"verifier":id,"engine":send_engine,"lease":answer["lease"],"answers":[answer]}))?;
                            if r["ok"] != true {
                                return Err("protocol 7 submission failed".into());
                            }
                            out.retain(|p| {
                                p.engine != send_engine || p.body["question"] != answer["question"]
                            });
                            save(dir, &out)?;
                        }
                        if releasing || stopping {
                            for (lease, code) in &give_back {
                                if code.is_none() {
                                    net.post(
                                        "/api/board/release",
                                        &json!({"verifier":id,"lease":lease}),
                                    )?;
                                }
                            }
                            for h in &held_rows {
                                net.post(
                                    "/api/board/release",
                                    &json!({"verifier":id,"lease":h["lease"]}),
                                )?;
                            }
                        } else if renew {
                            for h in &held_rows {
                                for q in h["rows"].as_array().into_iter().flatten() {
                                    let r = net.post(
                                        "/api/board/renew",
                                        &json!({"verifier":id,"lease":h["lease"],"question":q}),
                                    )?;
                                    if r["held"] == false {
                                        if let Some(job) = &active {
                                            if job.task["question"] == *q {
                                                job.budget.cancelled.store(true, Ordering::Relaxed);
                                            }
                                        }
                                    }
                                }
                            }
                        } else if needs_work {
                            let r=net.post("/api/board/work",&json!({"verifier":id,"engine":engine_id,"protocol":7,"consent":config.consent,
                                "lanes":((cores as f64*previous/100.0).ceil() as usize).clamp(1,256),"pace_ms":pace_ms.round() as u64}))?;
                            if r["dials"].is_object() {
                                dials = r["dials"].clone();
                            }
                            if let Some(tasks) = r["pack"]["tasks"].as_array() {
                                renew_after = millis(&r["pack"], "lease_ms", 300_000) / 2;
                                for t in tasks {
                                    queue.push_back((
                                        r["pack"]["lease"].as_str().ok_or("missing lease")?.into(),
                                        t.clone(),
                                    ));
                                }
                            }
                            if r["work"].is_object() {
                                renew_after = 120_000;
                                queue.push_back((
                                    r["work"]["lease"].as_str().ok_or("missing lease")?.into(),
                                    r["work"].clone(),
                                ));
                            }
                            if r["stale"] == true {
                                engine = None;
                                next_engine = now;
                            }
                            if !r["pack"].is_object() && !r["work"].is_object() {
                                signal_open = false;
                            }
                            next_sync = Instant::now()
                                + Duration::from_millis(millis(&r, "next_ms", 10_000));
                        }
                    }
                    if active.as_ref().is_some_and(|j| j.search) {
                        renew_after = 120_000;
                    }
                    next_renew = Instant::now() + Duration::from_millis(renew_after.max(1000));
                    let report_ms = millis(&dials, "report_ms", 3_600_000);
                    next_report = now
                        + Duration::from_millis(if report_ms == 0 {
                            365 * 24 * 3600 * 1000
                        } else {
                            report_ms
                        });
                    releasing = false;
                    give_back.clear();
                }
                Ok(())
            })();
            match result {
                Err(error) if error.downcast_ref::<EngineChanged>().is_some() => {
                    save(dir, &out)?;
                    return Err(error);
                }
                Ok(()) => {
                    backoff = 2;
                    last_error = None;
                    next_retry = Instant::now();
                }
                Err(e) => {
                    let message = e.to_string();
                    let retry_after = e
                        .downcast_ref::<ureq::Error>()
                        .and_then(|e| match e {
                            ureq::Error::Status(_, r) => {
                                r.header("Retry-After").and_then(|s| s.parse::<u64>().ok())
                            }
                            _ => None,
                        })
                        .unwrap_or(0)
                        .min(86_400);
                    let delay = backoff.max(retry_after);
                    eprintln!("{message}; retrying in {delay}s (pending answers kept)");
                    last_error = Some(message);
                    phase = "retrying".into();
                    next_retry = Instant::now()
                        + Duration::from_secs(delay)
                        + Duration::from_millis(random_below(delay.saturating_mul(250))?);
                    backoff = (backoff * 2).min(120);
                }
            }
        }
        if stopping {
            if let Some(job) = &active {
                job.budget.cancelled.store(true, Ordering::Relaxed);
            }
            break;
        }
        if active.is_none() && !paused && out.len() < 300 {
            if let (Some(loaded), Some((lease, task))) = (&engine, queue.pop_front()) {
                let b = Arc::clone(&shared_budget);
                b.cancelled.store(false, Ordering::Relaxed);
                let e = Arc::clone(loaded);
                let budget = Arc::clone(&b);
                let t = task.clone();
                let search = t["kind"] == "riven_gain";
                let (tx, answer) = mpsc::channel();
                let (ptx, progress_rx) = mpsc::channel();
                let wake = main_thread.clone();
                std::thread::spawn(move || {
                    let r = if search {
                        compute::search(&e, &budget, &t, &ptx)
                    } else {
                        compute::row(&e, &budget, &t, &ptx)
                    };
                    let _ = tx.send(r);
                    wake.unpark();
                });
                active = Some(Active {
                    lease,
                    task,
                    search,
                    budget: b,
                    answer,
                    progress: progress_rx,
                    began: Instant::now(),
                });
            }
        }
        if let Some(job) = &active {
            phase = "computing".into();
            if !progress.is_empty() {
                phase = format!("computing: {progress}");
            }
            if job.search {
                phase = format!("optimizing: {progress}");
            }
        }
        if phase != previous_state
            && (std::io::stdout().is_terminal()
                || phase.split(':').next() != previous_state.split(':').next()
                || logged_at.elapsed() >= Duration::from_secs(30))
        {
            println!(
                "{phase} · CPU target {:.0}% · {completed} completed · {} pending",
                previous,
                out.len()
            );
            previous_state = phase.clone();
            logged_at = Instant::now();
        }
        if now >= next_status {
            state::write(
                &dir.join("status.json"),
                &json!({"run":nonce,"pid":std::process::id(),"at":state::timestamp(),
            "state":phase,"version":crate::BUILD,"engine":engine_id,"range":config.range,"target_percent":previous,"cpu_percent":own,
            "completed":completed,"pending":out.len(),"progress":progress,"error":last_error}),
            )?;
            next_status = now + Duration::from_secs(1);
        }
        std::thread::park_timeout(Duration::from_millis(250));
    }
    save(dir, &out)?;
    state::write(
        &dir.join("status.json"),
        &json!({"run":nonce,"state":"stopped","at":state::timestamp(),"pending":out.len(),"completed":completed}),
    )?;
    println!(
        "Stopped. {} pending answers kept for the next start.",
        out.len()
    );
    if !quit.load(Ordering::Relaxed)
        && !state::read::<Value>(&dir.join("stop.json"))
            .ok()
            .is_some_and(|s| s["run"] == nonce)
        && state::read::<Value>(&dir.join("host-update.json"))
            .ok()
            .is_some_and(|s| s["run"] == nonce)
    {
        return Err(crate::update::HostChanged.into());
    }
    Ok(())
}

pub fn acknowledge(out: &mut Vec<Pending>, engine: &str, reply: &Value) {
    let taken = reply["taken"].as_array().cloned().unwrap_or_default();
    let dropped: Vec<Value> = reply["dropped"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| v["question"].clone())
        .collect();
    out.retain(|p| {
        p.path != "/api/board/sync"
            || p.engine != engine
            || (!taken.contains(&p.body["question"]) && !dropped.contains(&p.body["question"]))
    });
}

fn hold_unsent(out: &[Pending], engine: &str, sent: &[Value], held: &mut Vec<Value>) {
    for p in out
        .iter()
        .filter(|p| p.engine == engine && p.path == "/api/board/sync")
    {
        if !sent.iter().any(|s| s["question"] == p.body["question"]) {
            held.push(json!({"lease":p.body["lease"],"rows":[p.body["question"]]}));
        }
    }
}

fn random_below(max: u64) -> Result<u64> {
    let mut b = [0; 8];
    getrandom::getrandom(&mut b).map_err(|e| format!("jitter: {e}"))?;
    Ok(if max == 0 {
        0
    } else {
        u64::from_le_bytes(b) % (max + 1)
    })
}
fn jitter(ms: u64, pct: u64) -> Result<u64> {
    let span = ms.saturating_mul(pct) / 100;
    Ok(ms.saturating_sub(span) + random_below(span.saturating_mul(2))?)
}

fn lower_priority() {
    #[cfg(unix)]
    {
        // SAFETY: raises this process's niceness without privileges.
        unsafe {
            libc::nice(10);
        }
    }
    #[cfg(windows)]
    {
        // SAFETY: operates only on the current process's pseudo handle.
        unsafe {
            windows_sys::Win32::System::Threading::SetPriorityClass(
                windows_sys::Win32::System::Threading::GetCurrentProcess(),
                windows_sys::Win32::System::Threading::BELOW_NORMAL_PRIORITY_CLASS,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lost_replies_and_engine_changes_do_not_erase_unacknowledged_results() {
        let p = |engine: &str, q: &str| Pending {
            engine: engine.into(),
            path: "/api/board/sync".into(),
            body: json!({"question":q}),
        };
        let mut rows = vec![p("old", "a"), p("new", "a"), p("new", "b"), p("new", "c")];
        acknowledge(&mut rows, "new", &json!({"taken":[],"dropped":[]}));
        assert_eq!(rows.len(), 4);
        acknowledge(
            &mut rows,
            "new",
            &json!({"taken":["a"],"dropped":[{"question":"b","why":"not_held"}]}),
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].engine, "old");
        assert_eq!(rows[1].body["question"], "c");
    }
    #[test]
    fn a_partial_outbox_batch_keeps_every_unsent_completed_row_leased() {
        let out: Vec<Pending> = (0..200)
            .map(|q| Pending {
                engine: "engine".into(),
                path: "/api/board/sync".into(),
                body: json!({"question":q,"lease":"lease"}),
            })
            .collect();
        let sent: Vec<Value> = out.iter().take(100).map(|p| p.body.clone()).collect();
        let mut held = Vec::new();
        hold_unsent(&out, "engine", &sent, &mut held);
        assert_eq!(held.len(), 100);
        for q in 100..200 {
            assert!(held.iter().any(|h| h["rows"][0] == q));
        }
    }
}
