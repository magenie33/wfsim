// SPDX-License-Identifier: AGPL-3.0-or-later
// Declarative board order planner: resolves weapon, build, ruler and mode
// scopes into a deterministic ordered list of canonical simulate tasks without
// touching the execution queue.
import { BOARD_ORDER_CATALOG } from "./board_order_catalog.js";
import { computationCanon } from "./computation_input.js";

const POPULARITY_DAYS = 30;
const BUILD_ID = /^[0-9a-f]{32}$/;
const SLUG = /^[a-z0-9_]{1,128}$/;
const VALID_TAGS = new Set(["beam", "continuous", "incarnon", "adversary", "aoe"]);
const VALID_CLASSES = new Set(["primary", "secondary", "melee", "archgun", "robotic"]);
const VALID_POLICIES = new Set(["rescore", "rescore_all", "missing", "all"]);
const VALID_SORTS = new Set(["popularity", "score", "weapon"]);
const VALID_RIVENS = new Set(["any", "rivenless", "with_riven"]);

function splitTokens(value) {
  if (Array.isArray(value)) return value.flatMap((v) => String(v ?? "").trim().split(/[\s,]+/)).filter(Boolean);
  if (typeof value === "string") return value.trim().split(/[\s,]+/).filter(Boolean);
  return [];
}

function uniqueTokens(value) {
  return [...new Set(splitTokens(value))];
}

async function sha256Hex32(text) {
  const bytes = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(text));
  return [...new Uint8Array(bytes)].map((b) => b.toString(16).padStart(2, "0")).join("").slice(0, 32);
}

export function catalogSummary(catalog = BOARD_ORDER_CATALOG) {
  const tagCounts = { beam: 0, continuous: 0, incarnon: 0, adversary: 0, aoe: 0 };
  const classCounts = {};
  const modeSet = new Set();
  for (const w of catalog.weapons) {
    for (const t of w.tags) if (t in tagCounts) tagCounts[t]++;
    if (w.weapon_class) classCounts[w.weapon_class] = (classCounts[w.weapon_class] || 0) + 1;
    for (const m of w.modes) modeSet.add(m);
  }
  return {
    weapons_total: catalog.weapons.length,
    tag_counts: tagCounts,
    class_counts: classCounts,
    rulers: catalog.rulers.map(({ id, name, primary }) => ({ id, name, primary })),
    modes: [...modeSet].sort(),
    weapons: catalog.weapons.map(({ id, name, weapon_class, tags, modes }) => ({ id, name, weapon_class, tags, modes })),
  };
}

export function normalizeOrderSpecification(raw = {}, catalog = BOARD_ORDER_CATALOG) {
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) throw new Error("bad_specification");
  const weaponById = new Map(catalog.weapons.map((w) => [w.id, w]));
  const rulerById = new Map(catalog.rulers.map((r) => [r.id, r]));

  const policy = String(raw.policy || "rescore").trim();
  if (!VALID_POLICIES.has(policy)) throw new Error(`bad_policy:${policy}`);

  const sort = String(raw.sort || "popularity").trim();
  if (!VALID_SORTS.has(sort)) throw new Error(`bad_sort:${sort}`);

  const riven = String(raw.riven || "any").trim();
  if (!VALID_RIVENS.has(riven)) throw new Error(`bad_riven:${riven}`);

  const weapons = uniqueTokens(raw.weapons);
  for (const id of weapons) if (!weaponById.has(id)) throw new Error(`unknown_weapon:${id}`);

  const pinnedWeapons = uniqueTokens(raw.pinned_weapons);
  for (const id of pinnedWeapons) if (!weaponById.has(id)) throw new Error(`unknown_weapon:${id}`);

  const tags = uniqueTokens(raw.tags);
  for (const tag of tags) if (!VALID_TAGS.has(tag)) throw new Error(`bad_tag:${tag}`);

  const classes = uniqueTokens(raw.classes);
  for (const cls of classes) if (!VALID_CLASSES.has(cls)) throw new Error(`bad_class:${cls}`);

  let rulers;
  if (raw.rulers === "primary") {
    rulers = catalog.rulers.filter((r) => r.primary).map((r) => r.id);
  } else if (!raw.rulers || raw.rulers === "all") {
    rulers = catalog.rulers.map((r) => r.id);
  } else {
    rulers = uniqueTokens(raw.rulers);
    if (!rulers.length) rulers = catalog.rulers.map((r) => r.id);
  }
  for (const r of rulers) if (!rulerById.has(r)) throw new Error(`unknown_ruler:${r}`);

  const modes = !raw.modes || raw.modes === "all" ? [] : uniqueTokens(raw.modes);
  for (const m of modes) if (!SLUG.test(m)) throw new Error(`bad_mode:${m}`);

  let builds = "scored";
  let buildIds = [];
  if (Array.isArray(raw.build_ids) || (typeof raw.build_ids === "string" && raw.build_ids.trim())) {
    buildIds = uniqueTokens(raw.build_ids);
    for (const id of buildIds) if (!BUILD_ID.test(id)) throw new Error(`bad_build_id:${id}`);
    builds = "explicit";
  } else if (raw.builds === "library" || raw.builds === "scored") {
    builds = raw.builds;
  } else {
    builds = policy === "missing" || policy === "all" ? "library" : "scored";
  }

  return {
    policy,
    sort,
    riven,
    weapons,
    pinned_weapons: pinnedWeapons,
    tags,
    classes,
    rulers,
    modes,
    builds,
    build_ids: buildIds,
  };
}

function matchesWeapon(weaponMeta, spec) {
  if (spec.weapons.length && !spec.weapons.includes(weaponMeta.id)) return false;
  if (spec.tags.length && !spec.tags.every((t) => weaponMeta.tags.includes(t))) return false;
  if (spec.classes.length && !spec.classes.includes(weaponMeta.weapon_class)) return false;
  return true;
}

function matchesRiven(record, rivenFilter) {
  if (rivenFilter === "any") return true;
  const hasRiven = Array.isArray(record.riven_pos) && record.riven_pos.length > 0;
  return rivenFilter === "with_riven" ? hasRiven : !hasRiven;
}

function admitsBuildForRuler(record, weaponMeta, rulerMeta, stanceMods, mainSlots) {
  if (!weaponMeta || !weaponMeta.board_calibrated) return false;
  const mods = Array.isArray(record.mods) ? record.mods : [];
  const mainCount = mods.filter((id) => typeof id === "string" && !stanceMods.has(id.split("@")[0])).length;
  if (rulerMeta.requires_full_mods && mainCount !== mainSlots) return false;
  if (record.exilus && !rulerMeta.allows_exilus) return false;
  if (rulerMeta.requires_full_arcanes) {
    const arcanes = Array.isArray(record.arcanes) ? record.arcanes : [];
    const filled = arcanes.filter((a) => typeof a === "string" && a && a !== "none").length;
    if (filled !== weaponMeta.arcane_seats) return false;
  }
  return true;
}

async function readWeaponPopularity(db) {
  const anchor = await db.prepare(
    `SELECT COALESCE(
       (SELECT MAX(day) FROM record_daily_usage_summaries WHERE event = 'visitors' AND subject = 'all'),
       (SELECT MAX(day) FROM record_daily_usage_summaries WHERE event = 'tested')
     ) AS d`
  ).first();
  if (!anchor?.d) return new Map();
  const after = new Date(Date.parse(`${anchor.d}T00:00:00Z`) - POPULARITY_DAYS * 86_400_000).toISOString().slice(0, 10);
  const { results } = await db.prepare(
    `SELECT subject, SUM(visitors) AS n FROM record_daily_usage_summaries
     WHERE event = 'tested' AND day > ?1::text GROUP BY subject`
  ).bind(after).all();
  return new Map(results.filter((r) => Number(r.n) > 0).map((r) => [r.subject, Number(r.n)]));
}

export async function planBoardOrder(db, rawSpec, { engine = "", catalog = BOARD_ORDER_CATALOG, previewLimit = 25 } = {}) {
  const specification = normalizeOrderSpecification(rawSpec, catalog);
  const weaponById = new Map(catalog.weapons.map((w) => [w.id, w]));
  const rulerById = new Map(catalog.rulers.map((r) => [r.id, r]));
  const rulerRank = new Map(catalog.rulers.map((r, idx) => [r.id, idx]));
  const stanceMods = new Set(catalog.stance_mods || []);
  const mainSlots = catalog.main_slots || 8;

  const matchedWeapons = catalog.weapons.filter((w) => matchesWeapon(w, specification));
  const matchedWeaponIds = matchedWeapons.map((w) => w.id);
  if (!matchedWeaponIds.length) {
    return {
      specification,
      items: [],
      summary: {
        weapons_matched: 0,
        weapons_with_items: 0,
        builds_count: 0,
        total_items: 0,
        distinct_computations: 0,
        by_weapon: [],
        by_ruler: [],
        preview: [],
      },
    };
  }

  const popularity = await readWeaponPopularity(db);
  const pinIndex = new Map(specification.pinned_weapons.map((w, idx) => [w, idx]));
  const weaponJson = JSON.stringify(matchedWeaponIds);
  const rulerJson = JSON.stringify(specification.rulers);

  const candidates = [];
  const parsedRecords = new Map();
  const canonRecords = new Map();
  const maxScoreByBuild = new Map();

  const rememberBuild = (id, rawRecord) => {
    let rec = parsedRecords.get(id);
    if (!rec) {
      rec = typeof rawRecord === "string" ? JSON.parse(rawRecord) : rawRecord;
      parsedRecords.set(id, rec);
    }
    return rec;
  };

  if (specification.policy === "rescore" || specification.policy === "rescore_all") {
    const buildFilter = specification.builds === "explicit"
      ? "AND b.id IN (SELECT value FROM jsonb_array_elements_text((?3::text)::jsonb))"
      : "";
    const binds = specification.builds === "explicit"
      ? [weaponJson, rulerJson, JSON.stringify(specification.build_ids)]
      : [weaponJson, rulerJson];
    const { results: rows } = await db.prepare(
      `SELECT s.identity, s.ruler, s.mode, s.score, s.measured_by, s.review_id, b.weapon, b.record
       FROM state_board_measurement_selection s
       JOIN entity_weapon_builds b ON b.id = s.identity
       WHERE b.weapon IN (SELECT value FROM jsonb_array_elements_text((?1::text)::jsonb))
         AND s.ruler IN (SELECT value FROM jsonb_array_elements_text((?2::text)::jsonb))
         ${buildFilter}`
    ).bind(...binds).all();

    for (const r of rows) {
      const s = Number(r.score);
      if (Number.isFinite(s)) {
        const prev = maxScoreByBuild.get(r.identity);
        if (prev === undefined || s > prev) maxScoreByBuild.set(r.identity, s);
      }
    }

    for (const r of rows) {
      const wMeta = weaponById.get(r.weapon);
      if (!wMeta) continue;
      if (specification.modes.length && !specification.modes.includes(r.mode)) continue;
      const rec = rememberBuild(r.identity, r.record);
      if (!matchesRiven(rec, specification.riven)) continue;
      if (specification.policy === "rescore" && engine
          && (r.measured_by === engine || r.measured_by === `verified:${engine}`)
          && r.review_id == null) {
        continue;
      }
      candidates.push({
        identity: r.identity,
        weapon: r.weapon,
        ruler: r.ruler,
        mode: r.mode,
        score: Number.isFinite(Number(r.score)) ? Number(r.score) : -1,
        force_recalculation: true,
      });
    }
  } else {
    const buildWhere = specification.builds === "explicit"
      ? "b.id IN (SELECT value FROM jsonb_array_elements_text((?2::text)::jsonb)) AND b.weapon IN (SELECT value FROM jsonb_array_elements_text((?1::text)::jsonb))"
      : specification.builds === "scored"
        ? "b.weapon IN (SELECT value FROM jsonb_array_elements_text((?1::text)::jsonb)) AND EXISTS (SELECT 1 FROM state_board_measurement_selection s0 WHERE s0.identity = b.id)"
        : "b.weapon IN (SELECT value FROM jsonb_array_elements_text((?1::text)::jsonb))";
    const buildBinds = specification.builds === "explicit"
      ? [weaponJson, JSON.stringify(specification.build_ids)]
      : [weaponJson];

    const [{ results: buildRows }, { results: scoreRows }] = await db.batch([
      db.prepare(`SELECT b.id, b.weapon, b.record FROM entity_weapon_builds b WHERE ${buildWhere}`).bind(...buildBinds),
      db.prepare(
        `SELECT s.identity, s.ruler, s.mode, s.score, s.measured_by, s.review_id
         FROM state_board_measurement_selection s
         JOIN entity_weapon_builds b ON b.id = s.identity
         WHERE b.weapon IN (SELECT value FROM jsonb_array_elements_text((?1::text)::jsonb))
           AND s.ruler IN (SELECT value FROM jsonb_array_elements_text((?2::text)::jsonb))`
      ).bind(weaponJson, rulerJson),
    ]);

    const existing = new Map();
    for (const s of scoreRows) {
      const num = Number(s.score);
      existing.set(`${s.identity}|${s.ruler}|${s.mode}`, s);
      if (Number.isFinite(num)) {
        const prev = maxScoreByBuild.get(s.identity);
        if (prev === undefined || num > prev) maxScoreByBuild.set(s.identity, num);
      }
    }

    for (const b of buildRows) {
      const wMeta = weaponById.get(b.weapon);
      if (!wMeta || !wMeta.board_calibrated) continue;
      const rec = rememberBuild(b.id, b.record);
      if (!matchesRiven(rec, specification.riven)) continue;
      const weaponModes = specification.modes.length
        ? wMeta.modes.filter((m) => specification.modes.includes(m))
        : wMeta.modes;
      if (!weaponModes.length) continue;

      for (const rulerId of specification.rulers) {
        const rMeta = rulerById.get(rulerId);
        if (!rMeta || !admitsBuildForRuler(rec, wMeta, rMeta, stanceMods, mainSlots)) continue;
        for (const mode of weaponModes) {
          const had = existing.get(`${b.id}|${rulerId}|${mode}`);
          if (specification.policy === "missing" && had) continue;
          const rowScore = had && Number.isFinite(Number(had.score))
            ? Number(had.score)
            : (maxScoreByBuild.get(b.id) ?? -1);
          candidates.push({
            identity: b.id,
            weapon: b.weapon,
            ruler: rulerId,
            mode,
            score: rowScore,
            force_recalculation: !!had,
          });
        }
      }
    }
  }

  candidates.sort((a, b) => {
    const pa = pinIndex.has(a.weapon) ? pinIndex.get(a.weapon) : 1e9;
    const pb = pinIndex.has(b.weapon) ? pinIndex.get(b.weapon) : 1e9;
    if (pa !== pb) return pa - pb;
    if (specification.sort === "popularity") {
      const popDiff = (popularity.get(b.weapon) || 0) - (popularity.get(a.weapon) || 0);
      if (popDiff !== 0) return popDiff;
      if (a.weapon !== b.weapon) return a.weapon < b.weapon ? -1 : 1;
      if (b.score !== a.score) return b.score - a.score;
    } else if (specification.sort === "score") {
      if (b.score !== a.score) return b.score - a.score;
      const popDiff = (popularity.get(b.weapon) || 0) - (popularity.get(a.weapon) || 0);
      if (popDiff !== 0) return popDiff;
      if (a.weapon !== b.weapon) return a.weapon < b.weapon ? -1 : 1;
    } else {
      if (a.weapon !== b.weapon) return a.weapon < b.weapon ? -1 : 1;
      if (b.score !== a.score) return b.score - a.score;
    }
    const ra = rulerRank.get(a.ruler) ?? 1e9;
    const rb = rulerRank.get(b.ruler) ?? 1e9;
    if (ra !== rb) return ra - rb;
    if (a.identity !== b.identity) return a.identity < b.identity ? -1 : 1;
    return a.mode < b.mode ? -1 : a.mode > b.mode ? 1 : 0;
  });

  const seenCells = new Set();
  const deduped = [];
  for (const c of candidates) {
    const cellKey = `${c.identity}|${c.ruler}|${c.mode}`;
    if (seenCells.has(cellKey)) continue;
    seenCells.add(cellKey);
    deduped.push(c);
  }

  for (const c of deduped) {
    if (!canonRecords.has(c.identity)) {
      canonRecords.set(c.identity, computationCanon(parsedRecords.get(c.identity)));
    }
  }

  const items = new Array(deduped.length);
  const HASH_BATCH = 512;
  for (let i = 0; i < deduped.length; i += HASH_BATCH) {
    const slice = deduped.slice(i, i + HASH_BATCH);
    const ids = await Promise.all(slice.map((c) => {
      const body = `simulate\n{"mode":${JSON.stringify(c.mode)},"record":${canonRecords.get(c.identity)},"ruler":${JSON.stringify(c.ruler)}}`;
      return sha256Hex32(body);
    }));
    for (let j = 0; j < slice.length; j++) {
      const c = slice[j];
      items[i + j] = {
        ordinal: i + j + 1,
        computation: ids[j],
        identity: c.identity,
        weapon: c.weapon,
        ruler: c.ruler,
        mode: c.mode,
        score: c.score,
        popularity: popularity.get(c.weapon) || 0,
        force_recalculation: c.force_recalculation,
      };
    }
  }

  const buildsSeen = new Set();
  const computationsSeen = new Set();
  const weaponTotals = new Map();
  const rulerTotals = new Map();
  for (const item of items) {
    buildsSeen.add(item.identity);
    computationsSeen.add(item.computation);
    weaponTotals.set(item.weapon, (weaponTotals.get(item.weapon) || 0) + 1);
    rulerTotals.set(item.ruler, (rulerTotals.get(item.ruler) || 0) + 1);
  }

  const byWeapon = [...weaponTotals.entries()].map(([weapon, count]) => ({
    weapon,
    name: weaponById.get(weapon)?.name || weapon,
    popularity: popularity.get(weapon) || 0,
    count,
  }));
  const byRuler = [...rulerTotals.entries()].map(([ruler, count]) => ({
    ruler,
    name: rulerById.get(ruler)?.name || ruler,
    count,
  }));

  return {
    specification,
    items,
    summary: {
      weapons_matched: matchedWeapons.length,
      weapons_with_items: weaponTotals.size,
      builds_count: buildsSeen.size,
      total_items: items.length,
      distinct_computations: computationsSeen.size,
      by_weapon: byWeapon.slice(0, 60),
      by_ruler: byRuler,
      preview: items.slice(0, Math.max(0, Math.min(100, previewLimit))).map((x) => ({
        ordinal: x.ordinal,
        weapon: x.weapon,
        weapon_name: weaponById.get(x.weapon)?.name || x.weapon,
        identity: x.identity,
        ruler: x.ruler,
        mode: x.mode,
        score: x.score >= 0 ? x.score : null,
        popularity: x.popularity,
        computation: x.computation,
      })),
    },
  };
}
