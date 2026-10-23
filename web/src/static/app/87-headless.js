// ---- HEADLESS QUERIES -------------------------------------------------------
//
// THE ONE TABLE of what can be asked of WFSim with no page under it —
// docs/AGENT.md §"Headless queries". The door reads its queries from here, and
// so will every surface with no page: a query implemented anywhere else is a
// second answer that drifts from this one as WFSim changes.
//
// NO DOM AND NO PAGE GLOBAL below this line. What a query needs arrives on
// `host`, which the page (`HEADLESS_PAGE_HOST`) and a headless caller each
// provide; a build or a weapon a caller does not pass is the one on screen,
// and a host with no screen answers `screen_build`/`screen_weapon` with null.

/// The mod id a RIVEN takes in a board record — the bare word, because the
/// endpoint's ids are `[a-z0-9_]` and a riven's local name is one player's
/// label for their own item.
const BOARD_RIVEN_SLOT = "riven";

/// A match that ignores spaces — DE's Chinese names carry them and nobody types
/// them (`03-i18n.js` §squash states the same rule for the page's own lists).
const headlessTight = (s) => String(s || "").toLowerCase().replace(/\s+/g, "");
function headlessHit(names, q) {
  const want = headlessTight(q);
  return !want || names.some((n) => headlessTight(n).includes(want));
}

/// What a caller may have meant: the candidates sharing the longest opening
/// with `q`, at least three characters of it.
function headlessNear(q, xs, words) {
  const want = headlessTight(q);
  const head = (v) => { let i = 0; while (i < v.length && v[i] === want[i]) i++; return i; };
  return xs.map((x) => [x, Math.max(...words(x).map((w) => head(headlessTight(w))))])
    .filter(([, n]) => n >= 3).sort((a, b) => b[1] - a[1]).map(([x]) => x);
}

/// Every mod and arcane by id, for naming a board row's cards — from `meta`
/// itself, because a row names cards outside whatever pool is open.
let headlessNamesMemo = null;
function headlessNameOf(meta) {
  if (!headlessNamesMemo || headlessNamesMemo.meta !== meta) {
    const by = new Map();
    for (const pool of Object.values(meta.mod_pools || {})) for (const m of pool || []) by.set(m.id, m.name);
    for (const a of meta.arcanes || []) by.set(a.id, a.name);
    headlessNamesMemo = { meta, by };
  }
  return (id) => headlessNamesMemo.by.get(id) || id;
}

// The WIKI PAGE name behind a weapon's display name. A parenthesised
// qualifier is OURS — "Larkspur Prime (Atmosphere)" is one wiki page with two
// stat columns, and we ship the ground one — so it never reaches a URL.
// `build_site_app.py`'s `wiki_name` splits on the same " (".
const wikiWeaponName = (w) => (w.name_en || w.name).split(" (")[0];
const wikiSlug = (w) => wikiWeaponName(w).replace(/ /g, "_");

/// THE PATH SEGMENT A WEAPON LIVES AT — the wiki page name, and the ID where
/// that name is not this weapon's alone: two Kitgun slots are one wiki page and
/// two roster entries, and the loser of that collision would have no URL. THE
/// LOWEST ID KEEPS THE WIKI NAME, so the address does not follow roster order.
/// `url_slug` in `build_site_app.py` is the same rule and has to stay it, or a
/// link points at a page that was never written. Not remembered for an empty
/// roster, which would freeze every path to its id.
let slugOwnerMemo = null;
function weaponSlug(weapons, w) {
  if (weapons.length && (!slugOwnerMemo || slugOwnerMemo.weapons !== weapons)) {
    const owner = new Map();
    for (const x of weapons) {
      const g = wikiSlug(x);
      if (!owner.has(g) || x.id < owner.get(g)) owner.set(g, x.id);
    }
    slugOwnerMemo = { weapons, owner };
  }
  const g = wikiSlug(w);
  return !slugOwnerMemo || slugOwnerMemo.owner.get(g) === w.id ? g : w.id;
}
function headlessWeaponPath(weapons, id) {
  const w = weapons.find((x) => x.id === id);
  return "/weapons/" + (w ? weaponSlug(weapons, w) : id);
}

/// A refusal, in the door's shape (`agentNo`), for a caller that has no door.
function headlessNo(reason, detail = {}) {
  return { ok: false, reason, ...detail };
}

// ---- places: a tie is one place --------------------------------------------

/// A PLACE IS A SCORE, NOT A ROW. Builds on one exact score are tied — what
/// tells them apart moved no number in that fight, an exilus or an evolution
/// the ruler never reaches — so they share a place and the next score is the
/// next place: 1, 1, 2, never 1, 1, 3. `list` is best first and `group` names
/// the ranking an entry is in; each entry comes back with its `rank`. EVERY
/// RANKING OF BUILDS COUNTS THIS WAY, which is why it is one function.
function placed(list, score, group = () => "") {
  const last = new Map();
  return list.map((x) => {
    const g = group(x), s = score(x), was = last.get(g);
    const rank = !was ? 1 : was.s === s ? was.rank : was.rank + 1;
    last.set(g, { s, rank });
    return { ...x, rank };
  });
}
/// Where a score not yet on a ranking would stand: one past every higher score.
const placeAmong = (scores, s) => new Set(scores.filter((x) => x > s)).size + 1;

/// A BOARD ROW AS POSITIONS: where each part sits, so two tied builds can be
/// told apart by WHERE they differ. The mods are one position (a set); the
/// exilus, each arcane slot and each evolution tier are one each — two tier-2
/// evolutions are alternatives, a tier-2 and a tier-4 are not. `axis` is the
/// part's `/api/meta.build_axes` id; `id` keeps a mod's rank (`id@3`).
function boardRowParts(r) {
  const ex = r.exilus && r.exilus !== "none" ? r.exilus : null;
  const out = (r.mods || []).filter((id) => id && id !== ex)
    .map((id) => (id === BOARD_RIVEN_SLOT
      ? { axis: "rivens", pos: "riven", id: JSON.stringify([((r.riven || {}).bonuses || []).slice().sort(), (r.riven || {}).malus || ""]) }
      : { axis: "mods", pos: "mods", id }));
  if (ex) out.push({ axis: "mods", pos: "exilus", id: ex });
  (r.arcanes || []).forEach((id, i) => { if (id && id !== "none") out.push({ axis: "arcanes", pos: `arcanes#${i}`, id }); });
  (r.evolutions || []).forEach((id, i) => { if (id) out.push({ axis: "evolutions", pos: `evolutions#${i}`, id }); });
  if (r.grip) out.push({ axis: "assembly", pos: "assembly#grip", id: r.grip }, { axis: "assembly", pos: "assembly#loader", id: r.loader });
  if (r.valence) out.push({ axis: "valence", pos: "valence", id: r.valence });
  return out;
}

/// What the order below reads of `meta`: each card's drain at its full rank,
/// rank and name, each arcane's name, each evolution's tier and place in it.
let preferMemo = null;
function preferTables(meta) {
  if (!preferMemo || preferMemo.meta !== meta) {
    const mods = new Map(), arcanes = new Map(), evolutions = new Map();
    for (const pool of Object.values((meta && meta.mod_pools) || {})) for (const m of pool || []) mods.set(m.id, m);
    for (const a of (meta && meta.arcanes) || []) arcanes.set(a.id, a.name);
    for (const w of (meta && meta.weapons) || []) {
      for (const t of w.evolutions || []) (t.options || []).forEach((o, i) => evolutions.set(o.id, [t.tier, i + 1]));
    }
    preferMemo = { meta, mods, arcanes, evolutions };
  }
  return preferMemo;
}

/// OF TWO BUILDS ON ONE SCORE, the one a tie shows: the engine's order
/// (`engine::board::builds::preferred`, docs/OPTIMIZER.md §"A tie keeps the
/// preferred build") — more mod drain, then the cards' English names, the
/// arcanes' names and each evolution tier's own order — then the earlier
/// measured. Negative = `a`.
function preferredBuild(a, b, meta) {
  const t = preferTables(meta);
  const read = (r) => {
    const parts = boardRowParts(r);
    const cards = parts.filter((p) => p.axis === "mods").map((p) => {
      const [id, rank] = String(p.id).split("@");
      const m = t.mods.get(id);
      return { name: m ? m.name : id, drain: !m ? 0 : rank == null ? m.drain : m.drain - m.max_rank + Number(rank) };
    });
    const names = (xs) => xs.slice().sort();
    return {
      drain: cards.reduce((s, c) => s + c.drain, 0),
      cards: names(cards.map((c) => c.name)),
      arcanes: names(parts.filter((p) => p.axis === "arcanes").map((p) => t.arcanes.get(String(p.id).split("@")[0]) || p.id)),
      evolutions: parts.filter((p) => p.axis === "evolutions").map((p) => t.evolutions.get(p.id) || [0, 0])
        .sort((x, y) => x[0] - y[0]).map((x) => x[1]),
    };
  };
  const list = (x, y) => {
    for (let i = 0; i < Math.min(x.length, y.length); i++) if (x[i] !== y[i]) return x[i] < y[i] ? -1 : 1;
    return x.length - y.length;
  };
  const x = read(a), y = read(b);
  return y.drain - x.drain || list(x.cards, y.cards) || list(x.arcanes, y.arcanes) || list(x.evolutions, y.evolutions)
    || String(a.measured_at || "").localeCompare(String(b.measured_at || ""));
}

/// TIED BUILDS AS ONE ANSWER. Consecutive entries of `list` sharing a `key`
/// (the ranking and the place) are one group: `members` preferred first, `core`
/// the parts every member carries (as `pos=id`), `free` per position the
/// alternatives the members take there (`null`: the position left empty), and
/// `every` whether the members are every combination of those alternatives —
/// when not, only the members themselves are known to tie.
function tieGroups(list, key, rowOf, meta) {
  const out = [];
  for (const x of list) {
    const last = out[out.length - 1];
    if (last && last.key === key(x)) last.members.push(x);
    else out.push({ key: key(x), members: [x] });
  }
  return out.map((g) => {
    const members = g.members.slice().sort((a, b) => preferredBuild(rowOf(a), rowOf(b), meta));
    const each = members.map((x) => boardRowParts(rowOf(x)));
    const k = (p) => `${p.pos}=${p.id}`;
    const count = new Map();
    each.forEach((ps) => new Set(ps.map(k)).forEach((x) => count.set(x, (count.get(x) || 0) + 1)));
    const core = new Set([...count].filter(([, c]) => c === each.length).map(([x]) => x));
    const free = [];
    let multi = false;
    for (const pos of [...new Set(each.flat().map((p) => p.pos))]) {
      const alts = new Map();
      for (const ps of each) {
        const here = ps.filter((p) => p.pos === pos && !core.has(k(p)));
        if (here.length > 1) multi = true;
        if (here.length) here.forEach((p) => alts.set(p.id, p));
        else alts.set("", null);
      }
      if ([...alts.keys()].some(Boolean)) free.push([pos, [...alts.values()]]);
    }
    const product = free.reduce((t, [, v]) => t * v.length, 1);
    return { key: g.key, members, core, free, every: !multi && product === members.length };
  });
}

/// THE BOARD IN ITS READING ORDER, each row with its place inside its group.
///
/// Rulers in their declared order (the primary first), then modes and kinds by
/// their best row, strongest first — each is a control on the build bar and a
/// control's order is its own answer — then best first inside a group. A group
/// is one ruler, one mode, riven or not: those compete with each other for
/// nothing. `benchIds` is `META.benchmarks` in order; `modes` the weapon's own.
/// `rank` is the PLACE (`placed`), shared by a tie; `key` counts rows, so it
/// stays unique.
function rankBoard(rows, benchIds, modes) {
  const order = (id) => {
    const i = benchIds.indexOf(id);
    return i < 0 ? 99 : i;
  };
  const modeOrder = (m) => {
    const i = (modes || []).indexOf(m || "base");
    return i < 0 ? 99 : i;
  };
  const riven = (r) => !!(r && r.riven);
  const modeKey = (r) => `${r.benchmark}#${r.mode || "base"}`;
  const kindKey = (r) => `${modeKey(r)}#${riven(r) ? "r" : "p"}`;
  const best = {};
  for (const r of rows) {
    best[modeKey(r)] = Math.max(best[modeKey(r)] ?? -1, r.score || 0);
    best[kindKey(r)] = Math.max(best[kindKey(r)] ?? -1, r.score || 0);
  }
  const sorted = rows.slice().sort((a, b) =>
    order(a.benchmark) - order(b.benchmark)
    || (best[modeKey(b)] || 0) - (best[modeKey(a)] || 0)
    || modeOrder(a.mode) - modeOrder(b.mode)
    || (best[kindKey(b)] || 0) - (best[kindKey(a)] || 0)
    || (riven(a) ? 1 : 0) - (riven(b) ? 1 : 0)
    || (b.score || 0) - (a.score || 0));
  const seen = {};
  return placed(sorted.map((row) => {
    const k = kindKey(row);
    seen[k] = (seen[k] || 0) + 1;
    return { row, mode: row.mode || "base", riven: riven(row), key: `${k}#${seen[k]}` };
  }), (x) => x.row.score || 0, (x) => kindKey(x.row));
}

/// A weapon by id, or the refusal naming the ones a caller may have meant.
function headlessWeapon(host, id) {
  const all = host.meta().weapons || [];
  const w = all.find((x) => x.id === (id || host.screen_weapon()));
  if (w) return w;
  if (!id) return headlessNo("missing_argument", { argument: "weapon", wants: "weapon id" });
  return headlessNo("unknown_weapon", { alternatives: headlessNear(id, all, (x) => [x.id, x.name, x.name_en])
    .slice(0, 8).map((x) => x.id) });
}

/// An unmodelled reason in the reader's words — the contract `trGap` states.
function headlessGap(g, say) {
  if (typeof g === "string") return say(g);
  if (g && g.template) {
    const t = say(g.template);
    if (t !== g.template) {
      return Object.entries(g.params || {}).reduce((s, [k, v]) => s.split(`{${k}}`).join(v), t);
    }
  }
  return say((g && g.text) || "");
}

/// ONE STAT ROW as the panel draws it — the page's own wording, sources named.
/// A row is the server's answer, never recomputed here.
function headlessStat(x) {
  return {
    label: x.label, base: x.base, final: x.final,
    ...(x.note ? { note: x.note } : {}), ...(x.rule ? { rule: x.rule } : {}),
    ...(x.sources && x.sources.length
      ? { sources: x.sources.map((y) => `${y.mod} ${y.value}${y.note ? ` (${y.note})` : ""}`) } : {}),
  };
}

/// WHAT AN ARGUMENT MAY BE, by the `kind` it declares. The door adds `seat`,
/// which only a page with slots can answer.
const HEADLESS_KINDS = {
  string: (v) => typeof v === "string" && !!v,
  number: (v) => typeof v === "number" && Number.isFinite(v),
  object: (v) => !!v && typeof v === "object" && !Array.isArray(v),
  array: (v) => Array.isArray(v),
  boolean: (v) => typeof v === "boolean",
  scalar: (v) => typeof v === "boolean" || (typeof v === "number" && Number.isFinite(v)),
  any: (v) => v !== undefined,
};

/// The refusal for arguments a declaration does not allow, or null.
function headlessCheckArgs(a, args, kinds = HEADLESS_KINDS) {
  for (const [k, spec] of Object.entries(a.args || {})) {
    const has = k in args && !(args[k] === null && !spec.nullable);
    if (!has) {
      if (spec.required) return headlessNo("missing_argument", { argument: k, wants: spec.kind });
      continue;
    }
    if (args[k] === null && spec.nullable) continue;
    if (!kinds[spec.kind](args[k])) {
      return headlessNo("bad_argument", { argument: k, wants: spec.kind, got: args[k] });
    }
    if (spec.enum && !spec.enum().includes(args[k])) {
      return headlessNo("bad_argument", { argument: k, alternatives: spec.enum().slice(0, 8) });
    }
    if (spec.kind === "number" && (args[k] < spec.min || args[k] > spec.max)) {
      return headlessNo("out_of_range", { argument: k, min: spec.min, max: spec.max });
    }
  }
  for (const k of Object.keys(args)) {
    if (!(a.args || {})[k]) return headlessNo("unknown_argument", { argument: k, wants: Object.keys(a.args || {}) });
  }
  return null;
}

/// The arguments as the JSON Schema a model is handed — one reading for the
/// door's `tools()` and for every surface with no page.
function headlessSchema(args) {
  return {
    type: "object",
    properties: Object.fromEntries(Object.entries(args || {}).map(([k, s]) => [k, {
      // A NULLABLE argument says so in its type, or a model that follows the
      // schema can never send the null that empties a slot.
      // AN "any" ARGUMENT HAS NO TYPE in the schema; the action checks it.
      ...(s.kind === "any" ? {} : { type: ((ts) => (ts.length === 1 ? ts[0] : ts))(
        [].concat(s.kind === "seat" ? ["string", "integer"] : s.kind === "scalar" ? ["boolean", "number"] : s.kind,
          s.nullable ? ["null"] : [])) }),
      description: s.what,
      ...(s.kind === "array" ? { items: { type: "object" } } : {}),
      ...(s.enum ? { enum: s.enum() } : {}),
    }])),
    required: Object.entries(args || {}).filter(([, s]) => s.required).map(([k]) => k),
  };
}

/// WHAT WFSIM IS AND HOW TO QUOTE IT, for a surface that introduces the table
/// to a model (the MCP server's instructions, the skill, `llms.txt`).
const HEADLESS_ABOUT = "WFSim is a Warframe weapon calculator. It builds a loadout, simulates the "
  + "fight against a named enemy, and searches for the best mods. Every formula cites its source, "
  + "and every golden test is calibrated against a real in-game run. A board score belongs to its "
  + "ruler, the named benchmark fight: compare scores only under one ruler, quote the ruler with the "
  + "number, and give the reader the row's link, which opens the build in the calculator. How it "
  + "judges a build, and the in-game readings behind each rule, is its method: https://wfsim.app/method.";

/// Where a source file a query cites is read.
const HEADLESS_SOURCE = "https://github.com/magenie33/wfsim";

/// A query's name where a dot is not allowed (MCP, a model's function name) —
/// the spelling Nona's `toolName` uses, and wire once published.
const headlessToolName = (id) => id.replace(/\./g, "_");

/// A BOARD ROW AS THE BUILD WIRE `buildPayload()` sends — what
/// `builder.stats.read` takes, so a caller can read the panel of a row it was
/// handed. The valence is the weapon's maximum roll, the ruler's own term (see
/// `boardRowState`). Null for a riven row: the riven is a roll nobody else
/// holds, and its stats are not a build a caller can send.
function headlessBoardBuild(w, row) {
  if (row.riven) return null;
  const mods = (row.mods || []).filter(Boolean);
  if (row.exilus && row.exilus !== "none" && !mods.includes(row.exilus)) mods.push(row.exilus);
  return {
    weapon: w.id,
    mods,
    arcane: (row.arcanes || []).filter((x) => x && x !== "none"),
    evolutions: (row.evolutions || []).filter(Boolean),
    mode: row.mode || "base",
    ...(row.valence ? { valence_element: row.valence, valence_bonus: (w.valence || {}).max || 0 } : {}),
    ...(row.grip ? { assembly: { grip: row.grip, loader: row.loader } } : {}),
  };
}

// ---- a saved build and the build wire, both ways ------------------------------------
//
// THE ONE TRANSLATION between a saved build's state (`buildState`'s axes) and
// the wire `/api/panel` and `/api/simulate` read. The page's `seatPayload` and
// `stateFromBuild` are these plus what only a page knows — who holds the
// weapon — and a surface with no page reads and writes saved builds through
// them, so the two can never disagree about what a build is.

const HEADLESS_EXILUS = 8;
const HEADLESS_STANCE = 9;
/// Every mod card by id, from `meta`'s own tables.
let headlessModsMemo = null;
function headlessModOf(meta) {
  if (!headlessModsMemo || headlessModsMemo.meta !== meta) {
    headlessModsMemo = { meta, by: new Map(Object.values(meta.mod_pools || {}).flat().map((m) => [m.id, m])) };
  }
  return (id) => headlessModsMemo.by.get(id);
}
/// A card and a rank as one wire id: `<card>@<rank>` below its max, the card alone at it.
function headlessRankedId(meta, card, rank) {
  const m = card && headlessModOf(meta)(card);
  return m && !m.riven && rank != null && rank < m.max_rank ? `${card}@${rank}` : card;
}
const headlessSplitRank = (id) => {
  const m = typeof id === "string" && !id.startsWith("riven:") ? /^(.+)@(\d+)$/.exec(id) : null;
  return m ? [m[1], Number(m[2])] : [id, null];
};
const headlessArcaneList = (v, n) => {
  const a = Array.isArray(v) ? v.slice() : v == null ? [] : [v];
  while (a.length < n) a.push(undefined);
  return a.slice(0, n);
};

/// A SAVED BUILD AS THE WIRE, less who holds it — `wielder`, which only a page
/// with the frame's builds can resolve, is passed in, and absent means the
/// weapon's own default holder.
function headlessSeat(meta, st, wielder) {
  return {
    weapon: st.weapon,
    wielder,
    evolutions: Object.values(st.evoSel || {}).filter(Boolean),
    arcane: st.arcane,
    arcane_rank: st.arcaneRank,
    mods: (st.slots || []).filter((x) => x.mod).map((s) => headlessRankedId(meta, s.mod, s.rank)),
    mode: st.mode,
    valence_element: (st.valence || {}).element,
    valence_bonus: (st.valence || {}).bonus,
    ...(st.assembly ? { assembly: { ...st.assembly } } : {}),
  };
}

/// THE WIRE AS A SAVED BUILD'S AXES — every axis `buildState` names, the
/// holder unset. `exilusId` says which mod sat in the exilus slot, which a flat
/// list cannot; the stance tells itself apart by being one.
function headlessStateAxes(meta, p, weapon, exilusId) {
  const w = (meta.weapons || []).find((x) => x.id === weapon) || {};
  const card = headlessModOf(meta);
  const ids = (p.mods || []).filter(Boolean);
  const stanceId = ids.find((id) => (card(headlessSplitRank(id)[0]) || {}).stance);
  const main = ids.filter((id) => id !== exilusId && id !== stanceId);
  const sl = Array.from({ length: 10 }, () => ({ mod: null, pol: null, rank: null }));
  main.slice(0, 8).forEach((id, i) => { sl[i].mod = id; });
  if (exilusId && exilusId !== "none" && ids.includes(exilusId)) {
    sl[HEADLESS_EXILUS].mod = exilusId;
  } else if (main.length > 8) {
    sl[HEADLESS_EXILUS].mod = main[8];
  }
  if (stanceId) sl[HEADLESS_STANCE].mod = stanceId;
  // A RANK IS THE ONE THE ID NAMES, and the card's ceiling when it names none.
  sl.forEach((s) => {
    const [c, r] = headlessSplitRank(s.mod);
    const m = c && card(c);
    if (m) { s.mod = c; s.rank = r ?? m.max_rank; }
  });
  // EVERY TIER INSTALLED: one the build does not name holds its first option
  // (`evolutions::complete`, the rule the server scores it under).
  const evo = {};
  for (const t of w.evolutions || []) {
    const pick = (p.evolutions || []).find((id) => t.options.some((o) => o.id === id));
    evo[t.tier] = pick || (t.options[0] || {}).id || null;
  }
  const nPools = (w.arcane_pools || []).length;
  return {
    wielder: null,
    slots: sl,
    evoSel: evo,
    arcane: headlessArcaneList(p.arcane, nPools).map((x) => x || "none"),
    arcaneRank: headlessArcaneList(p.arcane_rank, nPools).map((x) => x ?? null),
    // `null`, never `undefined`, for "the weapon's own default": `undefined`
    // is dropped by `JSON.stringify` on the way into a preset.
    mode: p.mode || null,
    valence: p.valence_element ? { element: p.valence_element, bonus: p.valence_bonus } : null,
    assembly: p.assembly || null,
  };
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

// ---- the Riven Analyst --------------------------------------------------------
//
// docs/BOARD.md §"The Riven Analyst". GLOBAL: every riven row the board
// publishes for a weapon against that ruler's and mode's #1 riven-free row.
// Every number is a published score — nothing here measures anything.

/// A riven stat's English name off its card template — `|val|` and the unit go.
const rivenStatNameEn = (s) =>
  s.text.replace("|val|", "").replace(/^\s*[%s]\s*/, "").replace(/\s+/g, " ").trim();

// ---- reading a riven off a screenshot ---------------------------------------
//
// The OCR's lines in, a riven out: which stat each line names and the number it
// shows. A card's words are a CLOSED list — the stats of one pool — so a line
// is matched against every name each stat goes by rather than read as free
// text, and an OCR slip (Z00m, 暴击伤吉) still lands on its stat. The value goes
// to `/api/riven` as typed, which turns it into the roll it implies.

/// Lowercase, no spaces or punctuation, and the digits OCR puts for letters.
const ocrFold = (x) => String(x || "").normalize("NFKC").toLowerCase()
  .replace(/0/g, "o").replace(/1/g, "l").replace(/[^\p{L}]/gu, "");

/// EVERY NAME A STAT GOES BY: its English card template, and each locale's
/// phrase table run over it — the way the page translates the same line.
function rivenStatNames(pool, locales) {
  const tables = (locales || []).map((l) => (l.effect_phrases || []).flatMap(([pat, out, flags]) => {
    try { return [[new RegExp(pat, flags || "gi"), out]]; } catch (_) { return []; }
  }));
  return pool.map((s) => {
    const line = s.text.replace("|val|", "+1");
    const names = [rivenStatNameEn(s)].concat(tables.map((t) =>
      t.reduce((x, [re, out]) => x.replace(re, out), line).replace(/^[+\-\d.%x\s]+/, "")));
    return { stat: s, names: [...new Set(names.map(ocrFold).filter((n) => n.length >= 2))] };
  });
}

/// One line's sign and number, as printed: `+51.0%`, `-22.3%`, `x0.84`.
function ocrNumber(text) {
  const m = String(text).replace(/,/g, ".").match(/([+\-−–]?)\s*([x×]?)\s*(\d+(?:\.\d+)?)/i);
  if (!m) return null;
  const v = Number(m[3]);
  return { value: m[1] && m[1] !== "+" ? -v : v, rest: String(text).replace(m[0], "") };
}

/// THE RIVEN ON A SCREENSHOT, from the OCR's lines: `{ bonuses, malus }` with
/// each stat's id and shown value, and the lines that carried a number but
/// named no stat. A line is a malus when its number points the way a bonus
/// never does — below 1 on a multiplier, against the base's sign elsewhere.
function rivenFromOcr(pool, locales, lines) {
  const named = rivenStatNames(pool, locales);
  const found = [];
  const unread = [];
  for (const line of lines) {
    const n = ocrNumber(line);
    if (!n) continue;
    const rest = ocrFold(n.rest);
    let best = null;
    for (const { stat, names } of named) {
      for (const name of names) {
        const hit = rest === name ? 3 : rest.includes(name) ? 2 : name.includes(rest) && rest.length >= 3 ? 1 : 0;
        // THE LONGEST NAME, THEN THE EARLIEST: 暴击伤害 is Critical Damage and
        // not Damage, and 病毒伤害 is Viral, whose name leads the line.
        const at = Math.max(0, rest.indexOf(name));
        if (hit && (!best || hit > best.hit || (hit === best.hit
          && (name.length > best.len || (name.length === best.len && at < best.at))))) {
          best = { stat, hit, len: name.length, at };
        }
      }
    }
    if (!best) { unread.push(line); continue; }
    if (found.some((f) => f.stat.id === best.stat.id)) continue;
    found.push({ stat: best.stat, value: n.value });
  }
  const isBonus = (f) => (f.stat.unit === "x" ? f.value > 1 : f.value > 0) === (f.stat.base > 0);
  const bonuses = found.filter((f) => isBonus(f) && f.stat.bonus !== false).slice(0, 3);
  const malus = found.find((f) => !bonuses.includes(f) && f.stat.malus) || null;
  return {
    bonuses: bonuses.map((f) => ({ id: f.stat.id, value: f.value })),
    malus: malus ? { id: malus.stat.id, value: malus.value } : null,
    unread,
  };
}

// ---- every mode as one ranking --------------------------------------------------

/// A RULER'S MODES RANKED TOGETHER, with and without a riven apart: a short list
/// (the long image, the bots) answers "the best ways to play this weapon",
/// whichever mode that is. Takes `rankBoard`'s entries, keeps their keys, and
/// re-ranks each (ruler, riven) by score; the board itself ranks per mode.
function pooledRanking(ranked) {
  const by = new Map();
  for (const x of ranked) {
    const g = `${x.row.benchmark}#${x.riven}`;
    if (!by.has(g)) by.set(g, []);
    by.get(g).push(x);
  }
  return [...by.values()].flatMap((list) => placed(list.slice()
    .sort((a, b) => (b.row.score || 0) - (a.row.score || 0)), (x) => x.row.score || 0));
}

/// …AND ITS RIVENS: one row per riven shape, at the mode it scores best in,
/// against the best riven-free row of any mode — `{ ruler_id, top, rivens }`.
function pooledRivens(meta, w, rows, ruler) {
  const ranked = pooledRanking(rankBoard(rows.filter((r) => r.benchmark === ruler), [ruler], w.modes));
  const top = ranked.find((x) => !x.riven) || null;
  const seen = new Set();
  const rivens = placed(ranked.filter((x) => {
    if (!x.riven) return false;
    const k = JSON.stringify([x.row.riven.bonuses.slice().sort(), x.row.riven.malus || ""]);
    if (seen.has(k)) return false;
    seen.add(k);
    return true;
  }), (x) => x.row.score || 0).map((x) => ({ ...x, gain: top ? x.row.score / top.row.score - 1 : null }));
  return { ruler_id: ruler, top, rivens };
}

// ---- the top N, one per score ------------------------------------------------

/// THE FIRST `n` PLACES of each ranking, one entry each: a tie is one answer,
/// shown by its preferred build (`preferredBuild`), with `tied` saying how many
/// builds share it. `list` carries `placed` ranks; `group` names the ranking.
function distinctTop(list, n, group, meta) {
  const pick = new Map();
  for (const x of list) {
    if (x.rank > n) continue;
    const k = `${group(x)}#${x.rank}`;
    const had = pick.get(k);
    if (!had) pick.set(k, { x, tied: 1 });
    else {
      had.tied += 1;
      if (x.row && had.x.row && preferredBuild(x.row, had.x.row, meta) < 0) had.x = x;
    }
  }
  const won = new Map([...pick.values()].map((p) => [p.x, p.tied]));
  return list.filter((x) => won.has(x)).map((x) => ({ ...x, tied: won.get(x) }));
}

// ---- the heads: what a short answer reads ------------------------------------

/// HOW DEEP A SHORT ANSWER GOES: `builder.board.read`'s largest `limit`, and
/// past the bots' longest image (`CARD_MAX`).
const BOARD_HEADS_PLACES = 20;

/// THE HEADS OF A WEAPON'S BOARD — `board/heads/<id>.json`, docs/BOARD.md
/// §"The heads": every row holding one of the first `k` places of its ruler,
/// mode and riven-ness (`rankBoard`), and each ruler's newest-measured row. A
/// row scoring above a kept row in its ranking is kept too, so a ranking run on
/// the heads places its first `k` as on every row, ties and their preferred
/// build included, and `boardMeasuredAt` of a ruler is unchanged. The modes
/// pooled (`pooledRanking`) need nothing more: a row's place among its own
/// mode is never below its place among them all.
function boardHeads(rows, k = BOARD_HEADS_PLACES) {
  const keep = new Set(rankBoard(rows, [], []).filter((x) => x.rank <= k).map((x) => x.row));
  const newest = new Map();
  for (const r of rows) {
    const had = newest.get(r.benchmark);
    if (r.measured_at && (!had || r.measured_at > had.measured_at)) newest.set(r.benchmark, r);
  }
  for (const r of newest.values()) keep.add(r);
  return rows.filter((r) => keep.has(r));
}

// ---- when a number was measured ---------------------------------------------

/// WHEN A WEAPON'S NUMBERS LAST MOVED: the newest of its rows' `measured_at`,
/// the scores table's own clock published to the minute, or "" where none says.
const boardMeasuredAt = (rows) =>
  (rows || []).reduce((m, r) => (r && r.measured_at && r.measured_at > m ? r.measured_at : m), "");

/// `2026-10-05T13:52Z` in the reader's own clock, to the minute: `2026-10-05 21:52`.
function measuredText(at) {
  const d = new Date(at);
  if (!at || Number.isNaN(d.getTime())) return "";
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
}

// ---- asking for a riven in words ---------------------------------------------
//
// `双暴 负任意`: the stats a card must have and what its malus may be, read
// greedily, longest word first, against the same names a screenshot is read
// by plus each locale's `riven_words` (what players type) and
// `riven_query_words` (the grammar). Only stats of the weapon's pool count.

/// Folded like a card's line, with `-` kept for a malus.
const rivenQueryFold = (x) => String(x || "").normalize("NFKC").toLowerCase()
  .replace(/[−–]/g, "-").replace(/[^\p{L}-]/gu, "");

/// `{ bonuses: [ids], malus: id | "any" | "none" | null, unread }`.
function rivenQuery(pool, locales, text) {
  const inPool = (ids) => ids.filter((id) => pool.some((s) => s.id === id));
  const words = [];
  for (const { stat, names } of rivenStatNames(pool, locales)) for (const n of names) words.push({ w: n, ids: [stat.id] });
  for (const l of locales || []) {
    for (const [w, ids] of Object.entries(l.riven_words || {})) words.push({ w: rivenQueryFold(w), ids: inPool(ids) });
    for (const [role, ws] of Object.entries(l.riven_query_words || {})) for (const w of ws) words.push({ w: rivenQueryFold(w), role });
  }
  words.sort((a, b) => b.w.length - a.w.length);
  const q = rivenQueryFold(text);
  const out = { bonuses: [], malus: null, unread: [] };
  let next = "bonus";
  for (let i = 0; i < q.length;) {
    const hit = words.find((x) => x.w && q.startsWith(x.w, i));
    if (!hit) { out.unread.push(q[i]); i += 1; continue; }
    i += hit.w.length;
    if (hit.role === "malus") { next = "malus"; continue; }
    if (hit.role) { out.malus = hit.role === "any_malus" ? "any" : "none"; continue; }
    if (!hit.ids.length) { out.unread.push(hit.w); continue; }
    if (next === "malus") out.malus = hit.ids[0];
    else for (const id of hit.ids) if (!out.bonuses.includes(id)) out.bonuses.push(id);
    next = "bonus";
  }
  out.unread = out.unread.join("") ? [out.unread.join("")] : [];
  return out;
}

/// Whether a riven `{ bonuses, malus }` of ids answers a query's ask.
const rivenMatches = (r, ask) => (ask.bonuses || []).every((id) => r.bonuses.includes(id))
  && (!ask.malus || (ask.malus === "any" ? !!r.malus : ask.malus === "none" ? !r.malus : r.malus === ask.malus));

/// THE ANALYSIS of `w`'s board rows: one group per ruler and mode that has a
/// riven row, best riven row first. `gain` is null where no riven-free row
/// stands to be compared against.
function rivenGroups(meta, w, rows) {
  const benches = (meta.benchmarks || []).map((b) => b.id);
  const ranked = rankBoard(rows, benches, w.modes);
  const groups = new Map();
  for (const x of ranked) {
    const k = `${x.row.benchmark}#${x.mode}`;
    const g = groups.get(k) || { ruler_id: x.row.benchmark, mode: x.mode, top: null, rivens: [] };
    if (x.riven) g.rivens.push(x);
    else if (!g.top) g.top = x;
    groups.set(k, g);
  }
  return [...groups.values()].filter((g) => g.rivens.length).map((g) => ({
    ...g, rivens: g.rivens.map((x) => ({ ...x, gain: g.top ? x.row.score / g.top.row.score - 1 : null })),
  }));
}

const HEADLESS_QUERIES = [
  {
    id: "builder.weapons.find",
    what: "Find weapons by name, in any language WFSim speaks.",
    anchor: "#weapon",
    args: {
      query: { kind: "string", required: true, what: "name words, in any language WFSim speaks" },
      limit: { kind: "number", min: 1, max: 40, what: "rows to return, default 12" },
    },
    run({ query, limit = 12 }, host) {
      const locales = host.names();
      const hits = (host.meta().weapons || []).filter((w) =>
        headlessHit([w.id, w.name, w.name_en].concat(locales.map((l) => (l.weapons || {})[w.id])), query));
      return {
        found: hits.length,
        rows: hits.slice(0, limit).map((w) => ({ id: w.id, name: w.name, class: w.class,
          url: host.origin + headlessWeaponPath(host.meta().weapons || [], w.id) })),
        ...(hits.length > limit ? { more: hits.length - limit } : {}),
      };
    },
  },
  {
    id: "builder.board.read",
    what: "Read a weapon's leaderboard: the measured best builds per ruler (the benchmark fight), mode, and with or without a riven, ranked. Each row carries the link that opens it, and a riven-free row its `build`, which the stats read takes.",
    page: "A row's key opens it with shell.preset.open (bar \"build\").",
    anchor: "#build-finder",
    args: {
      weapon: HEADLESS_WEAPON_ARG,
      riven: { kind: "string", what: "\"without\" (default), \"with\" or \"any\"", enum: () => ["without", "with", "any"] },
      mode: { kind: "string", what: "only this mode id" },
      limit: { kind: "number", min: 1, max: 20, what: "rows per group, default 3" },
      distinct: { kind: "boolean", what: "one row per place, its preferred build; `limit` counts places" },
      pooled: { kind: "boolean", what: "rank a ruler's modes together" },
    },
    async run({ weapon, riven = "without", mode: m, limit = 3, distinct = false, pooled = false }, host) {
      const w = headlessWeapon(host, weapon);
      if (w.ok === false) return w;
      const rows = await host.board(w.id);
      if (!rows) return headlessNo("board_not_loaded", { because: "this weapon has no board rows yet" });
      const benches = host.meta().benchmarks || [];
      const name = headlessNameOf(host.meta());
      const evos = (w.evolutions || []).flatMap((t) => t.options || []);
      const evo = (id) => (evos.find((x) => x.id === id) || { name: id }).name;
      const ranked = rankBoard(rows, benches.map((b) => b.id), w.modes);
      const scoped = (pooled ? pooledRanking(ranked) : ranked)
        .filter((x) => (riven === "any" || x.riven === (riven === "with")) && (!m || x.mode === m));
      const picked = distinct
        ? distinctTop(scoped, limit, (x) => `${x.row.benchmark}#${pooled ? "" : x.mode}#${x.riven}`, host.meta())
        : scoped.filter((x) => x.rank <= limit);
      return {
        weapon: w.id,
        rows: picked.map(({ row: r, mode, riven: rv, rank, key, tied }) => {
          const bench = benches.find((b) => b.id === r.benchmark);
          return {
            key, rank, ...(tied > 1 ? { tied } : {}), ruler: bench ? host.tr(bench.name) : r.benchmark, ruler_id: r.benchmark,
            mode, riven: rv, score: String(r.shown != null ? r.shown : (r.score || 0).toFixed(4)),
            ...(r.measured_at ? { measured_at: r.measured_at } : {}), ...(r.measured_by ? { measured_by: r.measured_by } : {}),
            mods: (r.mods || []).filter((x) => x && x !== BOARD_RIVEN_SLOT).map(name),
            ...(r.exilus && r.exilus !== "none" ? { exilus: name(r.exilus) } : {}),
            ...((r.arcanes || []).some((x) => x && x !== "none")
              ? { arcanes: r.arcanes.filter((x) => x && x !== "none").map(name) } : {}),
            ...((r.evolutions || []).some(Boolean) ? { evolutions: r.evolutions.filter(Boolean).map(evo) } : {}),
            ...(r.riven ? { riven_stats: r.riven } : { build: headlessBoardBuild(w, r) }),
            link: `${host.origin}${headlessWeaponPath(host.meta().weapons || [], w.id)}?bench=${encodeURIComponent(r.benchmark)}`
              + `&mode=${encodeURIComponent(mode)}&riven=${rv ? 1 : 0}`,
          };
        }),
      };
    },
  },
  {
    id: "builder.stats.read",
    what: "Read the stats panel for a build: every stat per form and part, base and final, with the mod each change came from, and what the weapon's model does not cover.",
    anchor: "#stats-rows",
    args: {
      build: { kind: "object", what: "the build: {weapon, mods: [mod ids], arcane: [arcane ids], evolutions: [ids], mode} — a board row's `build` is one; on the page, the one on screen when omitted" },
    },
    async run({ build }, host) {
      const b = build || host.screen_build();
      if (!b) return headlessNo("missing_argument", { argument: "build", wants: "object" });
      const w = headlessWeapon(host, b.weapon);
      if (w.ok === false) return w;
      const r = await host.api("/api/panel", b);
      if (!r || r.ok === false) return headlessNo("panel_failed", { because: r ? r.error : "no answer" });
      const gaps = ((w.unmodeled_parts && w.unmodeled_parts.length) ? w.unmodeled_parts : (w.unmodeled || []))
        .map((g) => headlessGap(g, host.tr));
      if (w.passive_unmodeled) gaps.unshift(host.tr("this weapon's passive is not modelled yet"));
      return {
        policy: r.policy,
        not_modelled: gaps,
        forms: (r.forms || []).map((f) => ({
          label: f.label, meta: f.meta,
          stats: (f.stats || []).map(headlessStat),
          elements: (f.elements || []).map(headlessStat),
          indirect: (f.indirect || []).map(headlessStat),
          parts: (f.parts || []).map((pt) => ({
            label: pt.label, meta: pt.meta, damage_total: pt.damage_total, damage: pt.damage,
            stats: (pt.stats || []).map(headlessStat),
          })),
        })),
        conditionals: r.conditionals, buffs: r.buffs,
      };
    },
  },
  {
    id: "builder.rivens.read",
    what: "The Riven Analyst: each riven the board measured for a weapon, its best score against the #1 riven-free build of the same ruler and mode — what the riven is worth. Only submitted rivens appear.",
    anchor: "#riven-analyst",
    args: {
      weapon: HEADLESS_WEAPON_ARG,
      ruler: { kind: "string", what: "only this benchmark id" },
      mode: { kind: "string", what: "only this mode id" },
      bonuses: { kind: "array", what: "stat ids each riven has as bonuses" },
      malus: { kind: "string", what: "the malus: a stat id, \"any\" or \"none\"" },
      pooled: { kind: "boolean", what: "a ruler's modes as one ranking" },
    },
    async run({ weapon, ruler, mode: m, bonuses = [], malus = null, pooled = false }, host) {
      const w = headlessWeapon(host, weapon);
      if (w.ok === false) return w;
      const rows = await host.board(w.id);
      if (!rows) return headlessNo("board_not_loaded", { because: "this weapon has no board rows yet" });
      const meta = host.meta();
      const name = headlessNameOf(meta);
      const pool = (meta.riven_stats || {})[w.riven_class] || [];
      const stat = (id) => { const d = pool.find((x) => x.id === id); return d ? rivenStatNameEn(d) : id; };
      const benches = meta.benchmarks || [];
      const shown = (r) => String(r.shown != null ? r.shown : (r.score || 0).toFixed(4));
      const link = (r, rv) => `${host.origin}${headlessWeaponPath(meta.weapons || [], w.id)}?bench=${encodeURIComponent(r.benchmark)}`
        + `&mode=${encodeURIComponent(r.mode || "base")}&riven=${rv ? 1 : 0}`;
      const mods = (r) => (r.mods || []).filter((x) => x && x !== BOARD_RIVEN_SLOT).map(name);
      return {
        weapon: w.id,
        groups: (pooled
          ? benches.map((b) => ({ ...pooledRivens(meta, w, rows, b.id), mode: null })).filter((g) => g.rivens.length)
          : rivenGroups(meta, w, rows)).filter((g) => (!ruler || g.ruler_id === ruler) && (!m || !g.mode || g.mode === m))
          .map((g) => ({
            ruler: host.tr((benches.find((b) => b.id === g.ruler_id) || { name: g.ruler_id }).name),
            ruler_id: g.ruler_id, mode: g.mode,
            riven_free: g.top ? { score: shown(g.top.row), mods: mods(g.top.row), link: link(g.top.row, false) } : null,
            rivens: placed(g.rivens.filter((x) => rivenMatches(x.row.riven, { bonuses, malus })), (x) => x.row.score || 0).map((x) => ({
              rank: x.rank, mode: x.row.mode || "base",
              bonuses: x.row.riven.bonuses.map(stat), malus: x.row.riven.malus ? stat(x.row.riven.malus) : null,
              stat_ids: { bonuses: x.row.riven.bonuses, malus: x.row.riven.malus || null },
              score: shown(x.row), gain: x.gain, mods: mods(x.row), link: link(x.row, true),
              ...(x.row.measured_at ? { measured_at: x.row.measured_at } : {}),
              ...(x.row.measured_by ? { measured_by: x.row.measured_by } : {}),
            })),
          })),
      };
    },
  },
  {
    id: "shell.method.read",
    what: "How WFSim judges a build: each rule, why it holds, how to apply it, and the readings behind it. Read it before comparing builds or explaining why one is better.",
    anchor: "a[href=\"/method\"]",
    args: {
      concept: { kind: "string", what: "one rule's id; every rule when omitted" },
    },
    run({ concept }, host) {
      const meta = host.meta();
      const m = meta.method;
      if (!m) return headlessNo("method_not_loaded", { because: "this engine carries no method" });
      const all = m.concepts || [];
      const picked = concept ? all.filter((c) => c.id === concept) : all;
      if (concept && !picked.length) return headlessNo("unknown_concept", { alternatives: all.map((c) => c.id) });
      const source = (s) => `${HEADLESS_SOURCE}/blob/main/${s}`;
      const rulers = (meta.benchmarks || []).filter((b) => !b.cell_dir)
        .map((b) => ({ id: b.id, name: host.tr(b.name), ...(b.standard ? { standard: host.tr(b.standard) } : {}) }));
      return {
        about: host.tr(m.about),
        url: `${host.origin}/method`,
        concepts: picked.map((c) => ({
          id: c.id, title: host.tr(c.title), rule: host.tr(c.rule),
          why: c.why.map((s) => host.tr(s)), apply: c.apply.map((s) => host.tr(s)),
          evidence: (c.evidence || []).map((e) => ({ kind: e.kind, shows: host.tr(e.shows), source: source(e.source) })),
          ...((c.see || []).length ? { see: c.see.map((s) => ({ shows: host.tr(s.shows), source: source(s.source) })) } : {}),
          ...(c.rulers ? { rulers } : {}),
          url: `${host.origin}/method#${c.id}`,
        })),
      };
    },
  },
];
