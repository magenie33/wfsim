// ---- Riven appraisal: a chat's riven, searched in the reader's own browser ----
//
// docs/AGENT.md §"Riven appraisal". `/appraise/<code>` opens the weapon's
// OPTIMIZER under the official ruler with the appraisal's own built-in search
// (`riven_appraisal` in `data/search/presets.yaml`, read-only like every
// built-in) and starts it at once. The tab keeps nothing: its storage lives in memory, so none of it
// reaches the reader's own builds, rivens or optimizer. When it finishes the
// winner goes back to the appraisal and every finalist to the board; the page
// sends a BUILD and never a number — the chat's bot replays it before it says one.

/// The appraisal open on this page: `{ code, job, phase, first }`, or null.
let appraisal = null;
const APPRAISAL_NAME = "wfsim-appraisal-name";

const appraisalActive = () => !!(appraisal && appraisal.job);

/// THIS TAB KEEPS NOTHING: from here on its `localStorage` is a copy in memory,
/// read through to the real one for what it has not written. The appraisal's
/// riven, ruler and search live and die with the tab; the name to thank is the
/// one thing kept, so the next appraisal can offer it.
function isolateAppraisalStorage() {
  if (isolateAppraisalStorage.done) return;
  isolateAppraisalStorage.done = true;
  const real = { get: Storage.prototype.getItem, set: Storage.prototype.setItem, del: Storage.prototype.removeItem };
  const mem = new Map();
  const ours = (st) => { try { return st === window.localStorage; } catch (_) { return false; } };
  Storage.prototype.getItem = function (k) { return ours(this) && mem.has(k) ? mem.get(k) : real.get.call(this, k); };
  Storage.prototype.setItem = function (k, v) {
    if (ours(this) && k !== APPRAISAL_NAME) mem.set(k, String(v)); else real.set.call(this, k, v);
  };
  Storage.prototype.removeItem = function (k) { if (ours(this)) mem.set(k, null); else real.del.call(this, k); };
}

/// THE LINK, ANSWERED: read the appraisal, put the address on its weapon's
/// optimizer, draw that page, set it up and start. The search it runs is
/// `headlessRivenGainRequest`'s — the one the bot freezes with no page, so every
/// computer that runs this riven gain searches the same question — and the
/// controls set up beside it show that search. `freeze` stops before running,
/// writing `{ engine, request, context }` to `body[data-request]`.
async function openAppraisal(code, freeze = false) {
  isolateAppraisalStorage();
  let job = null;
  try {
    const r = await fetch(`/api/appraise/${encodeURIComponent(code)}`);
    job = r.ok ? await r.json() : null;
  } catch (_) { job = null; }
  const w = job && job.ok && weaponInfo(job.weapon);
  if (!w) {
    history.replaceState(null, "", "/");
    await route();
    presetToast(tr("This riven gain could not be found — it may have expired."));
    return;
  }
  appraisal = { code: job.code, job, phase: job.done ? "done-before" : "setting-up", first: null };
  history.replaceState(null, "", `${weaponPath(w.id)}/optimizer`);
  await route();
  renderAppraisal();
  if (job.done && !freeze) return;
  try {
    await setUpAppraisal(w, job);
    const asked = headlessRivenGainRequest(META, { weapon: w.id, ruler: job.ruler, card: job.riven || {},
      rivenId: appraisal.rivenMod.slice(RIVEN_PREFIX.length) });
    if (!asked) throw new Error(`unknown ruler ${job.ruler}`);
    if (freeze) {
      document.body.dataset.request = JSON.stringify({ engine: ENGINE_ID, ...asked });
      return;
    }
    appraisal.phase = "searching";
    renderAppraisal();
    await runSearch(() => asked.request);
  } catch (e) {
    appraisal.phase = "failed";
    appraisal.error = String((e && e.message) || e);
    if (freeze) document.body.dataset.request = JSON.stringify({ error: appraisal.error });
    renderAppraisal();
  }
}

/// THE SEARCH, as its preset states it: the official ruler, the riven as the
/// asker's card reads, and one start per preset row — the riven, pinned, beside
/// the row's 60/60 card this weapon can equip — answering with the preset's
/// count at its runs a candidate.
async function setUpAppraisal(w, job) {
  await agentDo("shell.preset.open", { bar: "scenario", preset: job.ruler });
  const rv = job.riven || {};
  const id = newRiven({ bonuses: (rv.bonuses || []).map((b) => ({ id: b.id, roll: b.roll })),
    malus: rv.malus ? { id: rv.malus.id, roll: rv.malus.roll } : null,
    rank: rv.rank != null ? rv.rank : rivenRules().max_rank, polarity: rv.polarity || "madurai" });
  // THE APPRAISAL'S OWN BUILT-IN SEARCH, opened like any other: read-only, so
  // the starts the asker's riven rides in cannot be taken out from under it.
  appraisal.rivenMod = RIVEN_PREFIX + id;
  const own = ((META.search_presets || {}).presets || []).find((p) => p.riven);
  if (own) pickPreset(optBarCfg(), "search:" + own.id);
}

/// THE SEARCH HAS ANSWERED: its winner goes back to the appraisal, with the
/// name to thank if one was given.
async function appraisalAnswered(r) {
  if (!appraisalActive() || appraisal.phase !== "searching") return;
  const best = (r.results || []).find((x) => x && (x.mods || []).length);
  if (!best) { appraisal.phase = "failed"; appraisal.error = tr("the search ranked nothing"); renderAppraisal(); return; }
  appraisal.phase = "sending";
  renderAppraisal();
  const thanks = appraisalName();
  try {
    const res = await fetch(`/api/appraise/${encodeURIComponent(appraisal.code)}/result`, {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ build: boardPayloadFromResult(best), thanks }),
    });
    const body = await res.json().catch(() => ({}));
    appraisal.phase = res.ok && body.ok ? "sent" : "failed";
    appraisal.first = !!body.first;
    if (!res.ok) appraisal.error = body.error || String(res.status);
  } catch (e) {
    appraisal.phase = "failed";
    appraisal.error = String((e && e.message) || e);
  }
  renderAppraisal();
}

/// THE NAME AS TYPED, kept as it is typed: the banner is redrawn as the search
/// moves on, and a name typed into the box it replaced must not be lost.
let appraisalTyped = null;
function appraisalName() {
  if (appraisalTyped != null) return appraisalTyped.trim();
  try { return (localStorage.getItem(APPRAISAL_NAME) || "").trim(); } catch (_) { return ""; }
}

/// THE BANNER over the optimizer: what this page is doing for whom, and how far.
function renderAppraisal() {
  const block = $("opt-block");
  if (!block) return;
  let box = $("appraisal-banner");
  if (!appraisalActive()) { if (box) box.remove(); return; }
  if (!box) {
    box = document.createElement("div");
    box.id = "appraisal-banner";
    box.className = "appraisal";
    block.prepend(box);
  }
  const p = appraisal.phase;
  const said = {
    "setting-up": tr("Setting up the search…"),
    searching: tr("Someone asked Nona about this riven in a chat. Your browser is finding its best build — a minute or a few. When it finishes, the build goes back to the chat and onto the board."),
    sending: tr("Sending the build back…"),
    sent: appraisal.first ? tr("Done — Nona will post the result in the chat. Thank you!")
      : tr("Someone finished first; your build still went onto the board. Thank you!"),
    "done-before": tr("This riven's gain has already been found. Thank you for coming!"),
    failed: `${tr("The riven gain could not finish")}: ${appraisal.error || ""}`,
  }[p] || "";
  const saved = appraisalName();
  const asking = p === "setting-up" || p === "searching";
  box.innerHTML = `<div class="appraisal-h"><b>${escHtml(tr("Riven gain"))}</b></div>
    <div class="appraisal-b">${escHtml(said)}</div>
    ${asking ? `<label class="appraisal-n">${escHtml(tr("Your name, to be thanked in the chat (optional)"))}
      <input id="appraisal-name" maxlength="24" value="${escHtml(saved)}" autocomplete="nickname"></label>` : ""}`;
  const input = $("appraisal-name");
  if (input) input.oninput = () => {
    appraisalTyped = input.value;
    try { localStorage.setItem(APPRAISAL_NAME, input.value.trim()); } catch (_) { /* this visit only */ }
  };
}
