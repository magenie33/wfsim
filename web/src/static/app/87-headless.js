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

/// THE FIGHT EVERY SCENARIO IS APPLIED ONTO: the server's defaults
/// (`meta.defaults`) as a complete fight. `contact` is two bodies' radius, and
/// `metric` the one named before `meta` can name one.
function headlessDefaultScenario(meta, contact, metric) {
  const d = meta.defaults || {};
  return {
    enemy: d.enemy, level: d.level, steel_path: d.steel_path,
    // A GUARDIAN EXIMUS'S AURA over the target. OFF, and that is the honest
    // default rather than a convenience: every number this app has published
    // assumes nobody is shielding it.
    guardian_aura: d.guardian_aura === true,
    // …and the pool an Ancient Protector holds in front of it. OFF, same
    // reason.
    ancient_protector_aura: d.ancient_protector_aura === true,
    // NULL means "whatever this unit is by default", which the server resolves
    // to its `can_be_eximus`. Only an explicit choice is stored, so switching
    // targets keeps giving you the elite unit wherever one exists rather than
    // carrying a decision made about a different enemy.
    eximus: d.eximus ?? null,
    // HOW MANY THE TARGET WAS BROUGHT FOR. One unless a scenario says
    // otherwise — every fight this app has ever run is one player against one
    // enemy, and this changes the ENEMY, never the number of guns.
    squad_size: d.squad_size || 1,
    headshot_pct: d.headshot_pct, aiming: d.aiming !== false,
    // THE TOP ZOOM LEVEL unless a level was picked — see `zoomField`.
    scope_zoom: Number(d.scope_zoom) || 0,
    // WHERE THE TWO OF THEM STAND, metres. Contact by default — as close as
    // two bodies can be, which is what point blank means once they have a size.
    player_at: [...(d.player_at || [0, 0])],
    target_at: [...(d.target_at || [0, contact])],
    // THE REST OF THE FORMATION — every body that is not the one being aimed
    // at. Each is wholly its own: a place, and optionally
    // its own unit and level. What it omits it takes from the target above, so
    // nine identical enemies are nine positions and nothing else.
    formation: (Array.isArray(d.formation) ? d.formation : []).map((f) => ({
      at: [...(f.at || [0, 0])],
      enemy: f.enemy || "",
      level: f.level ?? null,
    })),
    // WHO ELSE IS FIRING — see `28-fight-roster.js`. Empty is the fight this
    // app has always run, one gun against the formation, and it is what every
    // official ruler means: a ruler never mentions this, so switching to one
    // empties the roster rather than carrying the last fight's squad into it.
    also_acting: (Array.isArray(d.also_acting) ? d.also_acting : [])
      .map((x) => ({ weapon: x.weapon, preset: x.preset || "" })),
    // WHERE THE WEAPON IS POINTED. `null` means "at the target", which is the
    // fight this app has always run and what keeps every stored scenario
    // meaning what it meant. Drag the marker and it becomes a place of its own.
    aim_at: d.aim_at ? [...d.aim_at] : null,
    invisible: !!d.invisible, airborne: !!d.airborne, sliding: !!d.sliding, aim_gliding: !!d.aim_gliding,
    overshields: !!d.overshields,
    channeling: !!d.channeling, melee_equipped: d.melee_equipped !== false,
    solo_weapon: !!d.solo_weapon,
    // KEPT WHOLE, unknown names included: an older page must not strip a
    // newer fight's terms.
    buff_triggers_off: [...(d.buff_triggers_off || [])],
    frame: d.frame || "",
    // ABSENT MEANS THE FLOOR. Only a key that is really there is an override,
    // so these are copied only when the saved scenario had them.
    ...(d.wf_health === undefined ? {} : { wf_health: d.wf_health }),
    ...(d.wf_armor === undefined ? {} : { wf_armor: d.wf_armor }),
    ...(d.wf_energy === undefined ? {} : { wf_energy: d.wf_energy }),
    ...(d.wf_sprint === undefined ? {} : { wf_sprint: d.wf_sprint }),
    infinite_ammo: d.infinite_ammo !== false, metric: d.metric || metric,
    // NO `form`: how the weapon is played belongs to the build.
    duration: d.duration, buffs: {},
    // WARFRAME ABILITY BUFFS. `ability_strength` is a TYPED OVERRIDE, a
    // fraction (1 = 100%), and NULL means the wielder's own — the linked
    // Warframe build's (`applyScenario` reads an older page's stored 1 as null).
    // `abilities` is what is ticked: `{id, secs}`, and `secs: null` is the
    // whole fight.
    ability_strength: null, abilities: [],
    // THE RULES THE PLAYER INSERTED INTO THE ACTION PRIORITY LIST, in the
    // engine's own shape (`data::apl::Rule`). Empty is every fight run before
    // it, which assumes everything ticked is up.
    apl: [],
    // THE FIGHT'S OWN STAT BONUSES — see `EXTRA_STAT_KEYS`. Empty is a fight
    // that hands this weapon nothing it did not earn, which is every ruler.
    extra_stats: { ...(d.extra_stats || {}) },
    // WHAT THE WARFRAME BRINGS. Both are the frame's rather than the weapon's,
    // so they travel with the fight exactly as `abilities` does — and empty is
    // a squad running no aura and a frame with bare sockets, which is every
    // ruler and every scenario stored before they existed.
    auras: (d.auras || []).map((a) => ({ id: a.id, count: a.count || 1 })),
    shards: (d.shards || []).map((x) => ({
      shard: x.shard, effect: x.effect, tauforged: !!x.tauforged,
    })),
  };
}

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

// ---- a riven gain's search, with no page -------------------------------------
//
// docs/AGENT.md §"Riven appraisal". The search a riven gain asks — the official
// ruler's fight, the asker's card, the built-in `riven_appraisal` search — is a
// function of those and `meta`, so the bot freezes it here, with no browser.
// `check_riven_request` holds it to the request a clean page sets up through
// its own controls (81-appraisal.js), on every weapon and ruler.

const headlessWeaponOf = (meta, id) => (meta.weapons || []).find((x) => x.id === id) || null;

/// Every board label follows its fight, enemy strength, operation and other axes.
function headlessBenchmarkParts(meta, id) {
  const all = meta.benchmarks || [], b = all.find((x) => x.id === id);
  if (!b) return [];
  const base = all.find((x) => x.id === (b.base || b.id)) || b;
  const axes = b.ladder || [];
  return [{ axis: "base", tag: base.id, name: base.name.split(" · ")[0], label: "Board scenario" },
    ...["scene", "shooting"].flatMap((axis) => axes.filter((x) => x.axis === axis)
      .map((x) => ({ ...x, label: axis === "scene" ? "Enemy strength" : "Operating conditions" }))),
    ...(axes.some((x) => !["scene", "shooting"].includes(x.axis))
      ? axes.filter((x) => !["scene", "shooting"].includes(x.axis)).map((x) => ({ ...x, label: "Other conditions" }))
      : [{ axis: "other", tag: "none", name: "No special conditions", label: "Other conditions" }])];
}
function headlessBenchmarkName(meta, id, translate = (x) => x) {
  const parts = headlessBenchmarkParts(meta, id);
  return parts.length ? parts.map((x) => translate(x.name)).join(" › ") : id || "—";
}

/// FIELDS A SCENARIO NO LONGER CARRIES, stripped in both directions so a stored
/// one — or a benchmark yaml — cannot reintroduce them: how the weapon is fired
/// belongs to the build, and how hard a fight is measured to the reader (the
/// official rulers' own runs are the scorer's, which no page setting moves).
const HEADLESS_DEAD_SCENARIO_FIELDS = ["form", "mode", "runs"];

/// A SCENARIO AS THE LIVE FIGHT, onto a COMPLETE fight — the defaults — and never
/// onto the fight before it: `st` states only what it has an opinion about, so
/// spreading it over the outgoing fight would keep that fight's other fields.
function headlessScenarioOnto(meta, st, contact, metric) {
  st = { ...(st || {}) };
  HEADLESS_DEAD_SCENARIO_FIELDS.forEach((k) => { delete st[k]; });
  const sim = { ...headlessDefaultScenario(meta, contact, metric), ...st, buffs: JSON.parse(JSON.stringify(st.buffs || {})),
    apl: JSON.parse(JSON.stringify(Array.isArray(st.apl) ? st.apl : [])) };
  // A STORED 1 IS NOT A CHOICE: an older page wrote it for everybody, before the
  // wielder's own strength reached the fight. Null reads the Warframe build.
  if (typeof sim.ability_strength !== "number" || sim.ability_strength === 1) sim.ability_strength = null;
  // A BODY PLACED WITH NO UNIT meant the aimed body's: written down here, once,
  // so it stops following the target from then on.
  (sim.formation || []).forEach((f) => { if (!f.enemy) f.enemy = sim.enemy; });
  return sim;
}

/// How a weapon is played when nothing says otherwise: `want` where it is one
/// of the weapon's modes, else its cycle, else its first.
function headlessDefaultMode(meta, weapon, want) {
  const ms = (headlessWeaponOf(meta, weapon) || {}).modes || ["base"];
  if (want && ms.includes(want)) return want;
  return ms.includes("cycle") ? "cycle" : (ms[0] || "base");
}

/// A BUILD'S VALENCE, cleaned against its weapon. An element the spec does not
/// offer is dropped — a preset from another weapon carries one — and the bonus
/// is clamped to the roll's own range.
function headlessDefaultValence(meta, weapon, st) {
  const s = (headlessWeaponOf(meta, weapon) || {}).valence || null;
  if (!s) return { element: "", bonus: 0 };
  // THE FIRST ELEMENT AT THE ROLL'S CEILING, not "none": every adversary weapon
  // comes out of its Lich carrying a bonus, every player can fuse to the
  // ceiling and the board scores it, and the wiki lists the elements in the
  // game's own order.
  const el = st && s.elements.includes(st.element) ? st.element : s.elements[0];
  const b = st && Number.isFinite(Number(st.bonus)) ? Number(st.bonus) : s.max;
  return { element: el, bonus: Math.min(Math.max(b, s.min), s.max) };
}

/// A KITGUN'S PARTS, cleaned against it; null on a weapon that takes none. A
/// build with none, or with the sibling slot's part, lands on the default
/// `/api/meta` states — the repair `assembly_of` makes on the wire, so the page
/// never shows one assembly while the server simulates another.
function headlessDefaultAssembly(meta, weapon, st) {
  const s = (headlessWeaponOf(meta, weapon) || {}).assembly || null;
  if (!s) return null;
  const d = s.default || {};
  const has = (list, v) => list.some((x) => x.id === v);
  return {
    grip: st && has(s.grips, st.grip) ? st.grip : d.grip,
    loader: st && has(s.loaders, st.loader) ? st.loader : d.loader,
  };
}

/// The four shapes a riven card can take, and an empty draft of one.
const HEADLESS_RIVEN_SHAPES = [
  { id: "3+1", bonuses: 3, malus: true },
  { id: "3", bonuses: 3, malus: false },
  { id: "2+1", bonuses: 2, malus: true },
  { id: "2", bonuses: 2, malus: false },
];
const headlessBlankDraft = (n, malus) => ({
  bonuses: Array.from({ length: n }, () => ({ id: null, roll: 1.0 })),
  malus: malus ? { id: null, roll: 1.0 } : null,
});

/// A RIVEN AS A SAVED CARD: a draft for every shape, its stats in its own.
/// A card saved before the wiki's Bonus/Malus wording says positives/curse.
function headlessWithDrafts(meta, r) {
  const out = JSON.parse(JSON.stringify(r || {}));
  out.rank = out.rank ?? (meta.riven_rules || {}).max_rank;
  out.polarity = out.polarity || "madurai";
  out.bonuses = out.bonuses || out.positives || [];
  out.malus = out.malus || out.curse || null;
  delete out.positives;
  delete out.curse;
  Object.values(out.drafts || {}).forEach((d) => {
    d.bonuses = d.bonuses || d.positives || [];
    d.malus = d.malus || d.curse || null;
    delete d.positives;
    delete d.curse;
  });
  out.shape = out.shape || `${out.bonuses.length || 2}${out.malus ? "+1" : ""}`;
  out.drafts = out.drafts || {};
  HEADLESS_RIVEN_SHAPES.forEach((s) => {
    if (!out.drafts[s.id]) out.drafts[s.id] = headlessBlankDraft(s.bonuses, s.malus);
  });
  if (out.bonuses.length) out.drafts[out.shape] = { bonuses: out.bonuses, malus: out.malus };
  const d = out.drafts[out.shape];
  out.bonuses = d.bonuses;
  out.malus = d.malus;
  return out;
}

/// A RIVEN'S ID, opaque: `Date.now()` orders them and the suffix separates two
/// made in the same millisecond.
const headlessNewRivenId = () => "r" + Date.now().toString(36) + Math.random().toString(36).slice(2, 5);

/// A riven as the board names it: its bonuses SORTED — a riven's stats do not
/// combine, so two players listing them in different orders describe one riven
/// and must make one row — and its malus.
function headlessRivenShape(spec) {
  const ids = (xs) => (xs || []).map((x) => x && x.id).filter(Boolean);
  return { riven_pos: ids(spec.bonuses || spec.positives).sort(), riven_neg: ((spec.malus || spec.curse) || {}).id || "" };
}

/// A START AS `/api/optimize` READS ONE: the build's slots, its axes, its pins.
function headlessStartPayload(meta, s) {
  const b = s.build;
  return {
    slots: (b.slots || []).map((x) => (x && x.mod ? headlessRankedId(meta, x.mod, x.rank) : null)),
    evolutions: Object.values(b.evoSel || {}).filter(Boolean),
    arcane: (b.arcane || []).slice(),
    arcane_rank: (b.arcaneRank || []).slice(),
    mode: b.mode || null,
    valence_element: (b.valence || {}).element || null,
    fixed: (s.fixed || []).map((k) => { const [kind, idx] = k.split(":"); return { kind, idx: Number(idx) }; }),
  };
}

/// A BUILT-IN SEARCH'S STARTS on `weapon`: one per element row, holding the
/// row's 60/60 card the weapon equips under evolutions `evoSel` (its first
/// ones when absent) — and `rivenMod` beside it, pinned. A row it takes none
/// of adds nothing new.
function headlessBuiltinStarts(meta, weapon, rivenMod, evoSel) {
  const w = headlessWeaponOf(meta, weapon) || {};
  const card = headlessModOf(meta);
  const at = (mods) => ({ weapon, ...headlessStateAxes(meta, { mods }, weapon) });
  const no = new Set(Object.values(evoSel || at([]).evoSel).flatMap((id) => (w.evo_forbids || {})[id] || []));
  const fits = new Set((w.mods || []).filter((id) => card(id) && !no.has(id)));
  const rows = ((meta.search_presets || {}).element_starts || [])
    .map((row) => [rivenMod, row.find((c) => fits.has(c))].filter(Boolean)).filter((mods) => mods.length);
  const unique = [...new Set(rows.map((r) => JSON.stringify(r)))].map((r) => JSON.parse(r));
  const starts = unique.map((mods) => ({ build: at(mods), fixed: rivenMod ? ["mods:0"] : [] }));
  return starts.length ? starts : [{ build: at([]), fixed: [] }];
}

/// What a search's limits may rule out, axis by axis.
const HEADLESS_LIMIT_AXES = ["mods", "arcanes", "evolutions", "modes", "valence"];
/// A search's limits on `weapon` with nothing ruled out: every card, the
/// exilus and every arcane seat.
function headlessBlankLimits(meta, weapon) {
  return { exclude: Object.fromEntries(HEADLESS_LIMIT_AXES.map((k) => [k, []])), mods: 8, exilus: true,
    arcane_seats: ((headlessWeaponOf(meta, weapon) || {}).arcane_pools || []).map(() => true) };
}

/// THE RIVEN GAIN'S SEARCH: `{ request, context }`, what a clean page builds for
/// `weapon` under `ruler` with `card` (`{ bonuses, malus, rank, polarity }`) as
/// the riven `rivenId` (a new one when absent) — the optimize request, and what
/// turns its result into a build (68-board-submit.js `boardBuildContext`).
/// `runs` is the final round's, a clean page's own. Null for an unknown weapon
/// or ruler.
function headlessRivenGainRequest(meta, { weapon, ruler, card, rivenId = headlessNewRivenId(), runs = 100 }) {
  const w = headlessWeaponOf(meta, weapon);
  const bench = (meta.benchmarks || []).find((b) => b.id === ruler);
  const preset = ((meta.search_presets || {}).presets || []).find((p) => p.riven);
  if (!w || !bench || !preset) return null;
  const contact = 2 * (Number(meta.body_radius_m) || 0.25);
  const sim = { ...headlessScenarioOnto(meta, bench.scenario, contact, "kpm"), __weapon: weapon };
  const mod = "riven:" + rivenId;
  const spec = headlessWithDrafts(meta, { bonuses: (card.bonuses || []).map((b) => ({ id: b.id, roll: b.roll })),
    malus: card.malus ? { id: card.malus.id, roll: card.malus.roll } : null,
    rank: card.rank != null ? card.rank : (meta.riven_rules || {}).max_rank, polarity: card.polarity || "madurai" });
  const mode = headlessDefaultMode(meta, weapon, null);
  const valence = headlessDefaultValence(meta, weapon, null);
  const assembly = headlessDefaultAssembly(meta, weapon, null);
  // A WEAPON WITH ONE STANCE holds it, and one locked to its frames (an Exalted
  // weapon) is held by the first of them, unbuilt (40-official-builds.js
  // `defaultWielder`); every other holder is the floor, which the wire omits.
  const stance = w.fixed_stance || "";
  const wielder = !w.sentinel && (w.wielders || []).length ? { frame: w.wielders[0] } : undefined;
  const request = {
    weapon,
    rivens: [{ id: rivenId, name: "riven 1", spec }],
    every_rank: meta.every_rank || { mods: [], arcanes: [] },
    mode,
    valence_element: valence.element,
    valence_bonus: valence.bonus,
    ...(assembly ? { assembly: { ...assembly } } : {}),
    stance,
    ...sim, runs, custom_enemies: [], buffs: sim.buffs || {}, also_acting: [],
    finalists: preset.finalists,
    strategy: "quick",
    starts: headlessBuiltinStarts(meta, weapon, mod).map((s) => headlessStartPayload(meta, s)),
    candidate_runs: preset.candidate_runs,
    limits: headlessBlankLimits(meta, weapon),
  };
  const context = { benchmark: bench.id, weapon, mode, stance, valence: valence.element,
    grip: (assembly && assembly.grip) || "", loader: (assembly && assembly.loader) || "",
    ...(wielder ? { wielder } : {}), rivens: { [mod]: headlessRivenShape(spec) } };
  return { request, context };
}

// ---- the Riven Analyst --------------------------------------------------------
//
// docs/BOARD.md §"The Riven Analyst". GLOBAL: every riven row the board
// publishes for a weapon against that ruler's and mode's #1 riven-free row.
// Every number is a published score — nothing here measures anything.

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
/// `zone` (an IANA name) is the reader's when it is not this machine's — a
/// bot drawing for a chat.
function measuredText(at, zone) {
  const d = new Date(at);
  if (!at || Number.isNaN(d.getTime())) return "";
  const p = (n) => String(n).padStart(2, "0");
  if (!zone) return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`;
  const part = Object.fromEntries(new Intl.DateTimeFormat("en-US", { timeZone: zone, hourCycle: "h23",
    year: "numeric", month: "2-digit", day: "2-digit", hour: "2-digit", minute: "2-digit" }).formatToParts(d).map((x) => [x.type, x.value]));
  return `${part.year}-${part.month}-${part.day} ${part.hour}:${part.minute}`;
}

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
      limit: { kind: "number", min: 1, max: BOARD_HEADS_PLACES, what: "rows per group, default 3" },
      distinct: { kind: "boolean", what: "one row per place, its preferred build; `limit` counts places" },
      pooled: { kind: "boolean", what: "rank a ruler's modes together" },
    },
    async run({ weapon, riven = "without", mode: m, limit = 3, distinct = false, pooled = false }, host) {
      const w = headlessWeapon(host, weapon);
      if (w.ok === false) return w;
      // THE HEADS ANSWER THIS AS EVERY ROW DOES (`boardHeads`, `limit` at most
      // `BOARD_HEADS_PLACES`); a host without them reads the whole board.
      const rows = await (host.boardHeads ? host.boardHeads(w.id) : host.board(w.id));
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
            key, rank, ...(tied > 1 ? { tied } : {}), ruler: headlessBenchmarkName(host.meta(), r.benchmark, host.tr), ruler_id: r.benchmark,
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
            ruler: headlessBenchmarkName(host.meta(), g.ruler_id, host.tr),
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
