// ---- HEADLESS DRAWING ---------------------------------------------------------
//
// The headless part's drawings: a speaker's words, a mode's name, a board row as
// a build, a build as a card, the long image and an appraisal's replay — one for
// the page and every surface with no page. The rule is 87-headless.js's: no DOM
// and no page global; `mcp/headless.js` is this part and that one, joined.

// ---- words, for a surface with no page ---------------------------------------
//
// A SPEAKER is what a drawing needs to say anything: `{ meta, t, tf, dt, zone }`
// — `meta` with its names in the reader's language (`headlessApplyNames`), `t` a
// UI string, `tf` an engine-generated line, `dt` a damage type, `zone` the
// reader's clock (absent: this machine's). The page's is
// `tr`/`tf`/`DT` over its own `META`; one with no page builds the same from
// `/api/meta` and `/api/i18n` (`headlessSpeaker`).

/// A locale's NAME for `id` in table `table`, or the English one.
const headlessLN = (i18n, table, id, en) => (i18n && i18n[table] && i18n[table][id]) || en;

/// NAMES IN THE READER'S LANGUAGE, written into `meta` in place: every entity
/// keeps its English one as `name_en`, which URLs and wiki links are built from.
function headlessApplyNames(meta, i18n) {
  if (!i18n) return;
  const over = (x, table) => { x.name_en = x.name; x.name = headlessLN(i18n, table, x.id, x.name); };
  for (const w of meta.weapons || []) {
    over(w, "weapons");
    for (const t of w.evolutions || []) for (const o of t.options || []) over(o, "evolutions");
  }
  for (const e of meta.enemies || []) over(e, "enemies");
  for (const pool of Object.values(meta.mod_pools || {})) for (const m of pool) over(m, "mods");
  for (const a of meta.arcanes || []) over(a, "arcanes");
}

/// A DAMAGE TYPE'S NAME. The English fallback is capitalised: callers arrive
/// with "Void" off the wire or "void" off a yaml token, and one answer is owed.
const headlessDT = (i18n, ty) => {
  const k = String(ty).toLowerCase();
  return headlessLN(i18n, "damage_types", k, k.charAt(0).toUpperCase() + k.slice(1));
};

/// AN ENGINE-GENERATED LINE in the locale: a string the overlay states whole is
/// taken whole, and only one it does not is rewritten phrase by phrase
/// (`effect_phrases`, an ordered [regex, replacement(, flags)] table, compiled
/// once per locale table).
const headlessPhraseMemo = new WeakMap();
function headlessTf(i18n, x) {
  if (!i18n || typeof x !== "string") return x;
  if (i18n.ui && i18n.ui[x]) return i18n.ui[x];
  let res = headlessPhraseMemo.get(i18n);
  if (!res) {
    res = (i18n.effect_phrases || []).flatMap(([pat, cn, flags]) => {
      try { return [[new RegExp(pat, flags || "gi"), cn]]; } catch (_) { return []; }
    });
    headlessPhraseMemo.set(i18n, res);
  }
  let s = x;
  for (const [re, cn] of res) s = s.replace(re, cn);
  return s;
}

/// A SPEAKER WITH NO PAGE: `meta` copied and named in `lang` from `i18n` (the
/// whole `/api/i18n` answer), English being the source and needing no table.
function headlessSpeaker(meta, i18n, lang, zone) {
  const table = lang && lang !== "en" ? (i18n || {})[lang] || null : null;
  const m = JSON.parse(JSON.stringify(meta));
  headlessApplyNames(m, table);
  return { meta: m, lang: lang || "en", t: (s) => (table && table.ui && table.ui[s]) || s,
    tf: (x) => headlessTf(table, x), dt: (ty) => headlessDT(table, ty), zone };
}

/// Translate a template and fill its `{named}` holes.
const headlessTrF = (say, template, params) => Object.entries(params || {}).reduce(
  (s, [k, v]) => s.split(`{${k}}`).join(v), say.t(template));

/// ANY VALUE, NEVER A THROW: null and undefined are drawn as nothing.
const headlessEsc = (s) => String(s ?? "").replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c]));

/// A PICTURE'S ADDRESS, same-origin: `.webp` is appended, not substituted, since
/// six names in the cache differ only by extension (`Sobek.png` the shotgun,
/// `Sobek.jpg` Shattering Justice).
const headlessImg = (name) => {
  if (!name) return null;
  const s = String(name);
  return "/img/" + encodeURIComponent((s.startsWith("wiki:") ? s.slice(5) : s) + ".webp");
};
/// A picture in a list: lazy, since a list draws hundreds, and hidden when it fails.
const headlessImgTag = (src, cls) => src ? `<img class="${cls || ""}" src="${src}" loading="lazy" onerror="this.style.visibility='hidden'"/>` : `<span class="${cls || ""}"></span>`;

/// WHAT A SCENARIO IS JUDGED BY, from the table `meta` publishes, never asked
/// as "is it dps": an unknown id is the default, and `fallback` is the one
/// named before any table has arrived.
const headlessMetricOf = (meta, id, fallback = null) => {
  const all = meta.metrics || [];
  return all.find((m) => m.id === (id || meta.metric_default))
    || all.find((m) => m.id === meta.metric_default)
    || fallback;
};

/// Kill score PER MINUTE. The score is whole kills plus the fraction of the
/// current target's pool already drained, so it grows with the engagement —
/// dividing by the clock is what makes two runs of different length
/// comparable, exactly as DPS does for damage.
/// A MISSING score or clock is NaN, never 0: an unknown drawn as "0 KPM" reads as
/// a build that killed nothing (a search row once read for a field it lacks).
const headlessKpm = (score, duration) => (score == null || !(Number(duration) >= 0) ? NaN
  : duration > 0 ? (Number(score) * 60) / duration : 0);

/// A RUN'S NUMBER IN METRIC `m`. `score` off the wire is kill PROGRESS over the
/// whole engagement, so a per-minute metric turns it into a rate; a metric that
/// is already a rate reads its own field and is left alone. A result without
/// that field is NaN, which every formatter draws as "—".
const headlessMetricValue = (m, r) => {
  const raw = (r || {})[m.field];
  if (raw == null) return NaN;
  return m.per_minute ? headlessKpm(raw, (r || {}).duration) : Number(raw);
};

/// A NUMBER NOT KNOWN is "—", never a zero that looks measured.
const HEADLESS_UNKNOWN = "—";
/// A SCORE, SPELLED THE WAY THE BOARD SPELLS IT (`data::boards::format_score`):
/// four significant figures with four decimals as the floor.
const headlessFmtScore = (x) => {
  const v = Number(x);
  if (x == null || !Number.isFinite(v)) return HEADLESS_UNKNOWN;
  if (v === 0) return (0).toFixed(4);
  const mag = Math.floor(Math.log10(Math.abs(v)));
  return v.toFixed(Math.min(12, Math.max(4, 3 - mag)));
};

const headlessPrettify = (id) => id.replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());

// ---- a mode's name -------------------------------------------------------------

const HEADLESS_TRIGGER_WORD = {
  auto: "fully automatic", semi_auto: "semi-auto", burst: "burst",
  charge: "charged", held: "held", projectile: "projectile",
};

/// A FORM, NAMED WITH WHAT TELLS IT FROM THE FORM BESIDE IT: its trigger, or
/// its draw. A form named after its trigger does not say it twice — compared on
/// the source strings, both checked non-empty, since a Chinese string strips to
/// "" and `"".includes("")` is true.
function headlessFiredForm(say, f) {
  const word = f && HEADLESS_TRIGGER_WORD[f.trigger];
  const nm = say.t((f || {}).name || "");
  if (!word) return nm;
  if (f.charge_seconds) {
    return headlessTrF(say, "{form} ({secs}s charge)", { form: nm, secs: (+f.charge_seconds).toFixed(2) });
  }
  const flat = (x) => String(x || "").toLowerCase().replace(/[^a-z]/g, "");
  const said = (a, b) => !!flat(a) && !!flat(b) && flat(a).includes(flat(b));
  const how = say.t(word);
  return said(f.name, word) || said(nm, how) ? nm : headlessTrF(say, "{form} ({how})", { form: nm, how });
}

/// Is this mode a CYCLE — one form filling a gauge the other spends?
const headlessIsCycleMode = (id) => id === "cycle" || String(id).endsWith("_cycle");

/// THE FORM A CYCLE SPENDS — the gauge-fed one.
const headlessEarnedForm = (w) =>
  ((w || {}).forms || []).find((x) => !x.is_default && x.gauge_switched) || null;

/// THE FORM A MODE IS ABOUT: the one it fires, and for a cycle the one it fills
/// the gauge in — `<mode>_cycle`, the bare `cycle` being the arsenal's own.
function headlessModeForm(w, id) {
  const forms = (w || {}).forms || [];
  const def = forms.find((x) => x.is_default) || forms[0] || null;
  const declared = ((w || {}).declared_modes || {})[id];
  if (declared) return forms.find((x) => x.id === declared.form) || def;
  // A DETONATED THROW FIRES THE THROW.
  const fed = id === "cycle" ? "base" : String(id).replace(/_cycle$/, "").replace(/_detonate$/, "");
  if (fed === "base") return def;
  if (fed === "transformed") return headlessEarnedForm(w);
  // MORE THAN ONE ALTERNATE — a tapped shot AND an Incarnon form — told apart by the gauge.
  if (fed === "alternate") return forms.find((x) => !x.is_default && !x.gauge_switched) || null;
  // A MODE ID THAT IS A FORM ID, LAST: the form KIND `base` is a bow's tapped
  // shot while the `base` MODE is the drawn one.
  return forms.find((x) => x.id === fed) || def;
}

/// A MODE'S NAME, which is its FORM's: a declared mode carries its own, a cycle
/// is named after the form it spends, and a stance never renames a mode.
function headlessBareModeLabel(say, w, id) {
  const declared = ((w || {}).declared_modes || {})[id];
  if (declared) return say.t(declared.name);
  if (headlessIsCycleMode(id)) {
    const f = headlessEarnedForm(w);
    return f ? headlessTrF(say, "{form} cycle", { form: say.t(f.name) }) : say.t("cycle");
  }
  const f = headlessModeForm(w, id);
  if (f && /_detonate$/.test(id)) return headlessTrF(say, "{form}, detonated", { form: say.t(f.name) });
  return f ? say.t(f.name) : say.t(id);
}

/// …TOLD FROM A SIBLING THAT READS THE SAME: a cycle by the form that fills its
/// gauge, anything else by how it is fired.
function headlessModeLabel(say, w, id) {
  const name = headlessBareModeLabel(say, w, id);
  const twin = ((w || {}).modes || []).some((o) => o !== id && headlessBareModeLabel(say, w, o) === name);
  const f = twin && headlessModeForm(w, id);
  if (!f) return name;
  return headlessIsCycleMode(id)
    ? headlessTrF(say, "{form} cycle ({base})", { form: say.t((headlessEarnedForm(w) || {}).name || ""), base: say.t(f.name) })
    : headlessFiredForm(say, f);
}

// ---- a board row as a build ------------------------------------------------------

/// A BOARD RIVEN'S LOCAL NAME, derived from its shape AND each roll, so one row
/// opened twice is one riven and two rows at two corners are two.
function headlessBoardRivenName(say, rv) {
  const stat = (id, i) => {
    const roll = (rv.rolls || [])[i];
    return roll == null ? id : `${id} x${roll}`;
  };
  const bonuses = rv.bonuses || [];
  return `${say.t("board")} · ${bonuses.map(stat).join(" / ")}`
    + (rv.malus ? ` − ${stat(rv.malus, bonuses.length)}` : "");
}
/// One board riven as a riven PRESET's state — the editor's own shape.
const headlessBoardRivenState = (rv) => ({
  bonuses: (rv.bonuses || []).map((id, i) => ({ id, roll: (rv.rolls || [])[i] ?? 1 })),
  malus: rv.malus
    ? { id: rv.malus, roll: (rv.rolls || [])[(rv.bonuses || []).length] ?? 1 }
    : null,
  rank: 8,
  polarity: "madurai",
});

/// A BOARD ROW AS A BUILD STATE's axes, its riven named \`riven:<board name>\` and
/// registered in `defs` (by that id) rather than written anywhere.
///
/// A row carries its mains in `mods` with a melee's stance appended, told apart
/// by being one, and its exilus as a field of its own; a row written before the
/// slot was recorded packed nine, the ninth the exilus by position. A row has
/// no wielder (every ruler scores in the Prototype's hands), and its valence is
/// at the roll's maximum, the ruler's own term.
function headlessBoardRowAxes(say, w, row, defs) {
  const card = headlessModOf(say.meta);
  const ids = (row.mods || []).slice();
  const si = ids.findIndex((id) => (card(headlessSplitRank(id)[0]) || {}).stance);
  const stance = si >= 0 ? ids.splice(si, 1)[0] : null;
  const ex = row.exilus && row.exilus !== "none" ? row.exilus : null;
  const main = ids.filter((id) => id !== ex);
  const out = Array.from({ length: 10 }, () => ({ mod: null, pol: null, rank: null }));
  main.slice(0, 8).forEach((id, i) => { out[i].mod = id; });
  out[HEADLESS_EXILUS].mod = ex || main[8] || null;
  if (stance) out[HEADLESS_STANCE].mod = stance;
  return {
    mode: row.mode || "base",
    wielder: null,
    slots: out.map((s) => {
      const id = s.mod;
      if (id !== BOARD_RIVEN_SLOT || !row.riven) {
        const [c, rank] = headlessSplitRank(id);
        return { mod: c, pol: null, rank };
      }
      const local = "riven:" + headlessBoardRivenName(say, row.riven);
      defs[local] = row.riven;
      return { mod: local, pol: null, rank: null };
    }),
    evoSel: (row.evolutions || []).reduce((m, id, k) => ({ ...m, [k + 1]: id }), {}),
    arcane: (row.arcanes || []).length ? row.arcanes : ["none"],
    arcaneRank: [null],
    valence: row.valence ? { element: row.valence, bonus: (w.valence || {}).max || 0 } : null,
    assembly: row.grip ? { grip: row.grip, loader: row.loader } : null,
  };
}

// ---- a build as a card -------------------------------------------------------------

/// A MODULAR WEAPON'S TWO PARTS AS CHIPS, named from its own assembly spec.
function headlessPartChips(w, grip, loader) {
  const s = (w || {}).assembly || {};
  const name = (list, id) => ((list || []).find((x) => x.id === id) || {}).name || headlessPrettify(id);
  return [grip && { label: name(s.grips, grip), key: "part:" + grip },
    loader && { label: name(s.loaders, loader), key: "part:" + loader }].filter(Boolean);
}

/// A WEAPON'S EVOLUTIONS AS CHIPS, tier first, by id.
function headlessEvoChips(w, ids) {
  return ((w || {}).evolutions || []).map((t) => {
    const o = t.options.find((x) => ids.includes(x.id));
    return o ? { img: o.icon ? headlessImg(o.icon) : null, label: `${t.tier} · ${o.name}`, key: "evo:" + o.id } : null;
  }).filter(Boolean);
}

/// A BUILD AS CHIPS — the one brief every surface that shows a build draws. It
/// takes a DESCRIPTOR, never live state; a section given as null is not drawn,
/// an empty one says so. The builder's own order: mode, mods, the riven spelled
/// out, parts, arcane, evolutions, and the valence last — its percentage with
/// the element, since 60% Heat and 25% Heat are two builds.
function headlessBuildCardHtml(say, d) {
  const esc = headlessEsc;
  const chip = (c) => `<span class="sb-chip${c.cls ? " " + c.cls : ""}"${c.title ? ` title="${esc(c.title)}"` : ""}>` +
    `${c.img ? headlessImgTag(c.img, "sb-img") : ""}<span>${esc(c.label)}</span>${c.rank != null ? `<span class="rk">R${c.rank}</span>` : ""}</span>`;
  const section = (head, chips, empty) => chips == null ? ""
    : `<div class="sb-h">${head}</div><div class="sb-chips">${chips.map(chip).join("") || `<span class="sb-empty">${empty}</span>`}</div>`;
  return [
    d.mode != null ? section(say.t("Mode"), [{ label: d.mode }]) : "",
    section(`${say.t("Mods")} · ${d.mods.length}`, d.mods, say.t("no mods equipped")),
    d.riven || "",
    d.parts ? section(say.t("Parts"), d.parts) : "",
    section(say.t("Arcane"), d.arcanes, say.t("no arcane")),
    section(say.t("Evolutions"), d.evolutions, say.t("none selected")),
    d.valence ? section(say.t("Valence"), [{ label: d.valence }]) : "",
  ].join("");
}

/// A RIVEN SECTION'S HEADING: the reader's name for the card, then the name the
/// game generates from its stats.
const headlessRivenCardHead = (say, name, generated) => `<div class="sb-h">${headlessEsc(say.t("Riven"))}${
  [name, generated].filter(Boolean).map((x) => ` · ${headlessEsc(x)}`).join("")}</div>`;
/// RED IS THE MALUS, by its slot: a sign says nothing on recoil, whose bonus is a minus.
const headlessRivenCardChips = (stats) => stats.map((x) => `<span class="sb-chip${x.malus ? " neg" : ""}"><span>${
  headlessEsc(x.label)}</span>${x.roll != null ? `<span class="rk">×${Number(x.roll).toFixed(2)}</span>` : ""}</span>`).join("");
const headlessRivenCardBody = (say, name, got) => headlessRivenCardHead(say, name, got.generated)
  + `<div class="sb-chips">${headlessRivenCardChips(got.stats)}</div>`;
/// A RIVEN AS `/api/riven` PRINTED IT: the name the game gives it and each stat
/// line in the locale — the page never computes a riven value itself.
const headlessRivenAnswer = (say, a) => ({ generated: a.name || "",
  stats: (a.stats || []).map((x) => ({ label: say.tf(x.text), roll: x.roll, malus: x.slot === "malus" })) });
/// THE RIVEN SECTION OF A BUILD CARD for `r` (`{ name, spec }`), its stats `got`
/// (`headlessRivenAnswer`'s shape), keyed so a later answer can fill it in place.
const headlessRivenSection = (say, w, r, got) => !r ? ""
  : `<div data-riven-card="${headlessEsc(JSON.stringify([w.id, r.spec]))}" data-riven-name="${headlessEsc(r.name)}">${
    headlessRivenCardBody(say, r.name, got)}</div>`;

/// A BUILD STATE AS THE SIMULATOR'S CARD, drawn with ITS weapon's mods. `riven`
/// answers a riven id with `{ name, section }` — its name and its drawn section,
/// which only the caller can know; `pinned(key)` marks a position a start pins.
function headlessCardOfState(say, st, w, riven, pinned = () => "") {
  const card = headlessModOf(say.meta);
  const has = new Set(w.mods || []);
  const arcs = st.arcane || [];
  const ranks = st.arcaneRank || [];
  const rivenId = ((st.slots || []).find((s) => s && String(s.mod || "").startsWith("riven:")) || {}).mod;
  return headlessBuildCardHtml(say, {
    mode: headlessModeLabel(say, w, st.mode),
    mods: (st.slots || []).map((s, i) => {
      if (s && String(s.mod || "").startsWith("riven:")) return { label: riven(s.mod).name || say.t("Riven"), cls: pinned("mods:" + i) };
      const m = s && s.mod && has.has(s.mod) && card(s.mod);
      return m ? { img: headlessImg(m.image), label: m.name, rank: s.rank == null ? m.max_rank : s.rank,
        cls: pinned("mods:" + i) } : null;
    }).filter(Boolean),
    riven: rivenId ? riven(rivenId).section : "",
    arcanes: (w.arcane_slots || 0) >= 1
      ? arcs.map((id, i) => {
        const a = id !== "none" && (say.meta.arcanes || []).find((x) => x.id === id);
        return a ? { img: headlessImg(a.image), label: a.name, rank: ranks[i] ?? ((a.ranks || []).length - 1),
          cls: pinned("arcane:" + i) } : null;
      }).filter(Boolean)
      : null,
    parts: st.assembly ? headlessPartChips(w, st.assembly.grip, st.assembly.loader) : null,
    evolutions: w.uses_evo2 ? headlessEvoChips(w, Object.values(st.evoSel || {}).filter(Boolean)) : null,
    valence: w.valence && st.valence
      ? `${say.dt(st.valence.element)} +${Math.round(st.valence.bonus * 1000) / 10}%` : null,
  });
}

// ---- the long image ------------------------------------------------------------------
//
// docs/AGENT.md §"The QQ bot". A weapon's answer laid out to be sent as one
// picture, from the page's own components — the simulator's build card and the
// board's published numbers — so a chat image and the site are one design. The
// page's `/weapons/<Name>/card` draws it with this, and so does the bot with no
// page; only what each must FETCH differs (`headlessLongCard`'s `got`).

const HEADLESS_CARD_MAX = 10;
/// A BUILD'S OWN RECORD: when it was measured and by which engine of WFSim
/// (`measured_by`, the engine id — the one version every row states). Each
/// build carries its own: a rescore replaces them one by one.
function headlessMeasuredRecord(say, row, words = true) {
  const r = row || {};
  const at = r.measured_at ? measuredText(r.measured_at, say.zone) : "";
  return [
    at ? headlessEsc(words ? headlessTrF(say, "measured {t}", { t: at }) : at) : "",
    r.measured_by ? `${words ? headlessEsc(say.t("engine")) + " " : ""}${headlessEsc(r.measured_by)}` : "",
  ].filter(Boolean).join(" · ");
}

/// WHAT A CARD ADDRESS ASKS FOR, clamped to what a card shows.
function headlessCardAsk(search) {
  const p = new URLSearchParams(search);
  const n = Math.max(1, Math.min(HEADLESS_CARD_MAX, Math.round(Number(p.get("n"))) || 3));
  const kind = ["zk", "appraise"].includes(p.get("kind")) ? p.get("kind") : "pz";
  return { kind, ruler: p.get("ruler") || "", n, code: (p.get("code") || "").toUpperCase(), result: Number(p.get("result")) || 0,
    riven: { bonuses: (p.get("has") || "").split(",").filter(Boolean), malus: p.get("malus") || null },
    withRiven: p.get("rv") === "1" };
}

/// THE GRADE, 0 to 100 and linear in the gain: 100 × (card − the riven-free
/// leader) ÷ (the board's #1 − that leader), rounded down. At or past #1 is 100,
/// so the first riven measured where the board has none is 100 too; no better
/// than going without one is 0. `best` is the #1 the grade was read against.
function headlessRivenGrade(score, free, riven) {
  const best = Math.max(free, riven);
  const grade = score >= best ? 100 : score <= free ? 0 : Math.floor(100 * (score - free) / (best - free));
  return { grade, best: headlessFmtScore(best), best_riven: riven > free };
}

/// WHAT THE GRADE STOOD ON, every term of it: the formula, the two numbers it
/// divides, how many rivens the board had, when, and what is approximate.
function headlessRivenGradeNotes(say, v) {
  const locale = say.lang === "zh" ? "zh-CN" : "en-US";
  const when = (t) => {
    const zone = new Intl.DateTimeFormat(locale, { timeZoneName: "short", timeZone: say.zone }).formatToParts(new Date(t)).find((x) => x.type === "timeZoneName");
    return `${measuredText(t, say.zone)}${zone ? ` (${zone.value})` : ""}`;
  };
  return [
    say.t("Grade = 100 × (this card − the best build without a riven) ÷ (the board's #1 − the best build without a riven), linear and rounded down; at or past the #1 is 100, no better than going without a riven is 0."),
    !v.of ? say.t("The board has measured no riven of this weapon under this ruler: this card is the first, so it is 100.")
      : headlessTrF(say, "Read against: the board's #1 {best} ({which}), the best build without a riven {free}, and the {n} rivens the board has measured under this ruler, each at its god roll (every positive stat at its ceiling, the negative at its floor).",
        { best: v.best, which: v.best_riven ? say.t("with a riven") : say.t("without a riven"), free: v.top == null ? "—" : v.top, n: v.of }),
    v.board_at ? headlessTrF(say, "The board as last measured {t}; graded {g}.", { t: when(v.board_at), g: when(v.at) }) : headlessTrF(say, "Graded {g}.", { g: when(v.at) }),
    headlessTrF(say, "Approximate: this card's build is a quick search ({k} starts, {c} fights a candidate, one build kept), not proven its best, so the grade is a floor. Its number is the official fight, {runs} fights, at the card's own rolls, the same fight the board's rows are measured in.",
      { k: v.starts, c: v.candidate_runs, runs: v.runs }),
  ];
}

const headlessCardShown = (r) => String(r.shown != null ? r.shown : (r.score || 0).toFixed(4));
const headlessCardBox = (head, body) => `<section class="lc-box"><div class="lc-head">${head}</div>${body}</section>`;

/// THE REPLAY of an appraisal's answer `job` (`/api/appraise/<code>?result=`) on
/// weapon `w`: the board's own door judges the build, then the official fight
/// runs it at the ruler's runs with the asker's card at its real rolls and rank,
/// scored in the ruler's metric. Against the board's `rows` of the weapon: its
/// riven-free leader for this ruler, and where the card would stand among the
/// rivens it has measured. `host.check(build)` answers `/api/board/check` and
/// `host.simulate(body)` `/api/simulate` — the page spreads a simulation over
/// its fleet and the bot runs it on its own engine, the same answer either way;
/// `host.now()`, when given, is the clock the verdict is dated by.
async function headlessAppraisalReplay(say, w, job, rows, host) {
  const meta = say.meta;
  const build = job.result.build || {};
  const check = await host.check(build).catch(() => null);
  if (!check || check.ok === false || check.accepted === false) {
    return { ok: false, reason: (check && check.reason) || say.t("the board could not judge this build") };
  }
  const bench = (meta.benchmarks || []).find((b) => b.id === job.ruler);
  const met = bench && headlessMetricOf(meta, (bench.scenario || {}).metric);
  if (!met) return { ok: false, reason: say.t("the board could not judge this build") };
  // THE ASKER'S CARD as a riven of its own, nameless so it reads "Riven" as a
  // board row's does.
  const rv = job.riven || {};
  const id = headlessNewRivenId();
  const riven = { ...headlessWithDrafts(meta, { bonuses: rv.bonuses || [], malus: rv.malus || null,
    rank: rv.rank != null ? rv.rank : (meta.riven_rules || {}).max_rank, polarity: rv.polarity || "madurai" }), __weapon: w.id };
  const mod = "riven:" + id;
  const state = { weapon: w.id, ...headlessBoardRowAxes(say, w,
    { ...build, mods: (build.mods || []).map((m) => (m === BOARD_RIVEN_SLOT ? mod : m)) }, {}) };
  // THE RULER'S OWN FIGHT AND RUNS, not a reader's: the grade divides this
  // number by the board's, and a board row is fought at the ruler's runs.
  const contact = 2 * (Number(meta.body_radius_m) || 0.25);
  const sim = { ...headlessScenarioOnto(meta, bench.scenario, contact, met.id), __weapon: w.id };
  const runs = (bench.scenario || {}).runs;
  const r = await host.simulate({ ...headlessSeat(meta, state, undefined), rivens: [{ id, name: "", spec: riven }],
    ...sim, runs, custom_enemies: [], buffs: sim.buffs || {}, also_acting: [] });
  if (!r || r.ok === false) return { ok: false, reason: (r && r.error) || say.t("the replay did not run") };
  const score = headlessMetricValue(met, r);
  const mine = rows.filter((x) => x.benchmark === job.ruler);
  const free = pooledRanking(rankBoard(mine.filter((x) => !x.riven), [job.ruler], w.modes))[0];
  const rivens = pooledRivens(meta, w, mine, job.ruler).rivens;
  const preset = ((meta.search_presets || {}).presets || []).find((p) => p.riven) || {};
  return { ok: true, score, shown: headlessFmtScore(score), state, riven, metric: met.id,
    top: free ? headlessCardShown(free.row) : null, gain: free ? score / free.row.score - 1 : null,
    of: rivens.length, rank: rivens.filter((x) => x.row.score > score).length + 1, runs,
    // THE STARTS THE SEARCH WAS GIVEN, counted by the function that gave them
    // (`headlessRivenGainRequest`).
    starts: headlessBuiltinStarts(meta, w.id, mod).length, candidate_runs: preset.candidate_runs || null,
    board_at: boardMeasuredAt(mine) || null, at: (host.now ? host.now() : new Date()).toISOString(),
    ...headlessRivenGrade(score, free ? free.row.score : 0, rivens.length ? rivens[0].row.score : 0) };
}

/// WHAT A VERDICT REPORTS to the appraisal (worker/appraise.js `judged`): its
/// numbers and their terms, never the build or the card it drew.
const headlessVerdictRecord = ({ ok, score, shown, gain, top, of, rank, reason, grade, best, best_riven, runs, starts,
  candidate_runs, board_at, at }) => ({ ok, score, shown, gain, top, of, rank, reason, grade, best, best_riven, runs, starts,
  candidate_runs, board_at, at });

/// THE LONG IMAGE'S INSIDE for weapon `w` and `ask` (`headlessCardAsk`), in two
/// steps because its QR code encodes its link: `{ link, wayIn, draw(qrSvg) }`,
/// `draw` answering the HTML of `#card-page` and `wayIn` whether the appraisal's
/// first picture is drawn (its code drawn large, people long-press it).
///
/// `got` is everything fetched: `rows` (the weapon's board rows, every scene),
/// `job` (an appraisal, or null), `verdict` (its replay's, or null), `origin`
/// (the live site), `riven(w, spec)` (a riven's stats, `headlessRivenAnswer`'s
/// shape, or the placeholder a page draws while the engine answers).
function headlessLongCard(say, w, ask, got) {
  const esc = headlessEsc, trF = (s, p) => headlessTrF(say, s, p);
  const { kind, n, riven: rivenAsk, withRiven, code } = ask;
  const { rows = [], job = null, verdict = null, origin } = got;
  const meta = say.meta;
  const want = job ? job.ruler : ask.ruler;
  const benches = meta.benchmarks || [];
  const bench = benches.find((b) => b.id === want) || benches.find((b) => b.primary) || benches[0];
  const ruler = bench ? bench.id : "";
  const metric = say.t((headlessMetricOf(meta, ((bench || {}).scenario || {}).metric) || {}).label || "");
  const pct = (x) => (x >= 0 ? "+" : "−") + Math.abs(x * 100).toFixed(1) + "%";
  const record = (row) => {
    const t = headlessMeasuredRecord(say, row);
    return t ? `<span class="lc-when">${t}</span>` : "";
  };
  // A BOARD ROW'S RIVEN is registered by its row and named by its shape; a
  // replayed one is the asker's card (`verdict.riven`), nameless so it reads
  // "Riven" as a row's does.
  const defs = {};
  const rivenOf = (id) => {
    const rv = defs[id];
    const r = rv ? { name: "", spec: headlessBoardRivenState(rv) } : verdict && verdict.riven ? { name: "", spec: verdict.riven } : null;
    return { name: r ? r.name : "", section: r ? headlessRivenSection(say, w, r, got.riven(w, r.spec)) : "" };
  };
  const ofRow = (row) => headlessCardOfState(say, headlessBoardRowAxes(say, w, row, defs), w, rivenOf);
  const path = headlessWeaponPath(meta.weapons || [], w.id);
  let title = "", link = "", body = () => "";
  if (verdict) {
    title = trF("{w} · Riven gain", { w: w.name });
    link = `${origin}${path}/riven-analyst`;
    const who = job.result.thanks || say.t("a kind someone in the chat");
    body = () => !verdict.ok ? `<p class="sim-empty">${esc(verdict.reason || "")}</p>`
      : headlessCardBox(`<b class="lc-score">${esc(verdict.shown)}</b><span class="sb-empty">${esc(metric)}</span>${
        verdict.gain == null ? "" : `<b class="lc-gain">${pct(verdict.gain)}</b>`}`, headlessCardOfState(say, verdict.state, w, rivenOf))
      + headlessCardBox(`<span class="sb-h">${esc(say.t("Riven grade"))}</span><b class="lc-score">${verdict.grade}</b><span class="sb-empty">/ 100</span>`,
        headlessRivenGradeNotes(say, verdict).map((x) => `<p class="lc-note">${esc(x)}</p>`).join(""))
      + headlessCardBox(`<span class="sb-h">${esc(say.t("Against the board"))}</span>`,
        `<p class="lc-note">${esc(verdict.top == null ? say.t("The board has no riven-free build of this weapon yet.")
          : trF("The board's best riven-free build: {s}", { s: verdict.top }))}</p>`
        + (verdict.of ? `<p class="lc-note">${esc(trF("Among the {n} rivens the board has measured (each at its best rolls), this card would rank #{k}.", { n: verdict.of, k: verdict.rank }))}</p>` : "")
        + `<p class="sb-empty">${esc(trF("Searched by {who}. Thank you!", { who }))}</p>`);
  } else if (kind === "appraise") {
    // THE ASKER'S CARD AND THE WAY IN: whoever scans the code searches it.
    title = trF("{w} · Riven gain", { w: w.name });
    link = `${origin}/appraise/${code}`;
    body = () => {
      if (!job) return "";
      const spec = { ...job.riven, polarity: job.riven.polarity || "madurai" };
      return headlessCardBox(`<span class="sb-h">${esc(say.t("Riven gain code"))}</span><b class="lc-score">${esc(code)}</b>`,
        headlessRivenSection(say, w, { name: "", spec }, got.riven(w, spec))
        + `<p class="lc-note">${esc(say.t("Scan the code below: your own browser searches this riven's best build, and Nona posts the result in the chat."))}</p>`
        + `<p class="sb-empty">${esc(link.replace(/^https?:\/\//, ""))}</p>`);
    };
  } else if (kind === "pz") {
    const ranked = pooledRanking(rankBoard(rows.filter((r) => r.benchmark === ruler && !r.riven === !withRiven), [ruler], w.modes));
    const shown = distinctTop(ranked, n, () => "", meta);
    const mode = (shown[0] || {}).mode;
    title = trF(withRiven ? "{w} · top {n} riven builds" : "{w} · top {n} builds", { w: w.name, n: shown.length });
    link = `${origin}${path}?bench=${encodeURIComponent(ruler)}&mode=${encodeURIComponent(mode || "base")}&riven=${withRiven ? 1 : 0}`;
    body = () => shown.map((x) => headlessCardBox(`<b class="lc-rank">#${x.rank}</b><b class="lc-score">${esc(headlessCardShown(x.row))}</b><span class="sb-empty">${esc(metric)}${
      x.tied > 1 ? ` · ${esc(trF("{n} builds tie", { n: x.tied }))}` : ""}</span>${record(x.row)}`, ofRow(x.row))).join("");
  } else {
    const g = pooledRivens(meta, w, rows, ruler);
    title = trF("{w} · Riven Analyst", { w: w.name });
    link = `${origin}${path}/riven-analyst`;
    body = () => !g.rivens.length ? "" : (g.top ? headlessCardBox(`<span class="sb-h">${esc(say.t("The board's best riven-free build"))}</span><b class="lc-score">${esc(headlessCardShown(g.top.row))}</b>${record(g.top.row)}`,
      ofRow(g.top.row)) : "")
      + distinctTop(placed(g.rivens.filter((x) => rivenMatches(x.row.riven, rivenAsk)), (x) => x.row.score || 0), n, () => "", meta)
        .map((x) => headlessCardBox(`<b class="lc-rank">#${x.rank}</b><b class="lc-score">${esc(headlessCardShown(x.row))}</b>${
        x.gain == null ? "" : `<b class="lc-gain">${pct(x.gain)}</b>`}${record(x.row)}`, ofRow(x.row))).join("");
  }
  const updated = kind === "appraise" ? "" : measuredText(boardMeasuredAt(rows.filter((r) => r.benchmark === ruler)), say.zone);
  // THE SITE'S OWN WORDMARK, as the topbar draws it: the brand is WFSim, and
  // Nona speaks in the line the bot sends with the image, not on it.
  const draw = (qrSvg) => {
    const cards = body();
    return `<header class="lc-top"><span class="brand">WF<span>Sim</span></span><span class="sb-empty">wfsim.app</span></header>
    <h1 class="lc-title">${esc(title)}</h1>
    <div class="sb-empty lc-sub">${esc([bench ? say.t(bench.name) : "", updated ? trF("updated {t}", { t: updated }) : ""]
      .filter(Boolean).join(" · "))}</div>
    ${cards || `<p class="sim-empty">${esc(say.t("Nothing measured for this weapon yet."))}</p>`}
    <footer class="lc-foot"><div class="lc-qr">${qrSvg ? qrSvg.replace(/^<\?xml[^>]*>/, "") : ""}</div>
      <div><b class="lc-slogan">${esc(say.t("The real Simulacrum Prime."))}</b><div class="sb-empty">wfsim.app</div></div></footer>`;
  };
  return { link, wayIn: kind === "appraise" && !verdict, draw };
}

/// NAMES WFSIM HAS RETIRED, old id -> the query that replaces it, or null
/// when nothing does. A caller that learned the old name is told where it went
/// (docs/AGENT.md §"Headless queries"); the row goes whenever WFSim decides.
const HEADLESS_RETIRED = {};

/// The refusal for a name that is not a query: where a retired one went, or
/// that there is no such name.
function headlessUnknown(id) {
  if (Object.prototype.hasOwnProperty.call(HEADLESS_RETIRED, id)) {
    const use = HEADLESS_RETIRED[id];
    return headlessNo("retired", use ? { use, tool: headlessToolName(use) } : {});
  }
  return null;
}

const HEADLESS_WEAPON_ARG = { kind: "string", what: "weapon id; on the page, the one open when omitted" };
