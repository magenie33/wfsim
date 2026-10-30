// ---- Shared riven input: official names, typed cards and screenshots ----

/// A riven stat's English name off its card template — `|val|` and the unit go.
const rivenStatNameEn = (s) =>
  s.text.replace("|val|", "").replace(/^\s*[%s]\s*/, "").replace(/\s+/g, " ").trim();

/// Display spliced bonuses first and the malus last without changing stored slots or rolls.
function rivenDisplayOrder(pool, rows) {
  const spliced = new Set(pool.filter((s) => s.spliced).map((s) => s.id));
  const group = (s) => s.slot === "malus" || s.malus === true ? 2 : spliced.has(s.id) ? 0 : 1;
  return rows.slice().sort((a, b) => group(a) - group(b));
}

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
function rivenStatLabels(pool, locales) {
  const tables = (locales || []).map((l) => (l.effect_phrases || []).flatMap(([pat, out, flags]) => {
    try { return [[new RegExp(pat, flags || "gi"), out]]; } catch (_) { return []; }
  }));
  return pool.map((s) => {
    const line = s.text.replace("|val|", "+1");
    const names = [rivenStatNameEn(s)].concat(tables.map((t) =>
      t.reduce((x, [re, out]) => x.replace(re, out), line).replace(/^[+\-]?\d+(?:\.\d+)?(?:%|s)?\s*/, "")));
    return { stat: s, names: [...new Set(names.concat(names.map((n) => n.replace(/\s*[（(][^()（）]*[)）]\s*$/, ""))))] };
  });
}

/// OCR folds are only for screenshots; typed names retain their digits.
const rivenStatNames = (pool, locales) => rivenStatLabels(pool, locales).map(({ stat, names }) =>
  ({ stat, names: [...new Set(names.map(ocrFold).filter((n) => n.length >= 2))] }));

const rivenInputFold = (x) => String(x || "").normalize("NFKC").toLowerCase()
  .replace(/[−–]/g, "-").replace(/[^\p{L}\p{N}.%+×-]/gu, "");

/// Full card names win a collision with a shorthand. Longest names are atomic:
/// the 2 in Fire Rate (x2 for Bows) is part of the name, never a rolled value.
function rivenStatWords(pool, locales) {
  const words = rivenStatLabels(pool, locales).flatMap(({ stat, names }) =>
    names.map((name) => ({ w: rivenInputFold(name), ids: [stat.id], official: true })));
  for (const l of locales || []) for (const [name, ids] of Object.entries(l.riven_words || {})) {
    words.push({ w: rivenInputFold(name), ids: ids.filter((id) => pool.some((s) => s.id === id)), official: false });
  }
  return words.sort((a, b) => b.w.length - a.w.length || Number(b.official) - Number(a.official));
}

/// M71: a malus-only stat's base already points toward its malus. A faction
/// multiplier is neutral at 1; every other printed value is neutral at 0.
const rivenValueMalus = (stat, value) =>
  ((value > (stat.unit === "x" ? 1 : 0)) !== (stat.base > 0)) !== (stat.bonus === false);

/// A typed card in either order: stat + value, or the card's value + stat.
/// 正/负 and a sign before a name state the slot; a number's sign follows the
/// stat's direction. An unsigned multiplier states its direction on its own.
function readRivenLines(text, pool, locales) {
  const src = rivenInputFold(text), words = rivenStatWords(pool, locales), lines = [];
  const nameAt = (at) => words.find((w) => w.w && src.startsWith(w.w, at));
  const numberAt = (at) => {
    const m = src.slice(at).match(/^([+-]?)([x×]?)(\d+(?:\.\d+)?|\.\d+)(%?)/);
    return m && { sign: m[1], times: m[2], value: Number(m[3]), percent: m[4], end: at + m[0].length };
  };
  const no = (error, at) => ({ error, word: src.slice(at) });
  let at = 0;
  while (at < src.length) {
    const start = at;
    let said = null;
    if (/^[正负]/.test(src.slice(at))) said = src[at++] === "负";
    let n = numberAt(at), hit;
    if (n) {
      at = n.end;
      hit = nameAt(at);
      if (!hit && src[at] === "s") {
        const seconds = nameAt(at + 1);
        if (seconds && seconds.ids.every((id) => pool.find((s) => s.id === id).text.includes("|val|s"))) {
          at++;
          hit = seconds;
        }
      }
      if (!hit) return no(!src[at] || /^[\d.+-]/.test(src.slice(at)) ? "format" : "unread", at);
      at += hit.w.length;
    } else {
      if (/^[+-]/.test(src.slice(at))) {
        const role = src[at++] === "-";
        if (said !== null && said !== role) return no("contradicts", start);
        said = role;
      }
      hit = nameAt(at);
      if (!hit) return no("unread", at);
      at += hit.w.length;
      n = numberAt(at);
      if (!n) return no("format", at);
      at = n.end;
      if (src[at] === "s" && !nameAt(at)
          && hit.ids.every((id) => pool.find((s) => s.id === id).text.includes("|val|s"))) at++;
    }
    if (!hit.ids.length) return no("unread", start);
    if (hit.ids.length !== 1) return no("ambiguous", start);
    const stat = pool.find((s) => s.id === hit.ids[0]);
    if ((n.times && stat.unit !== "x") || (n.percent && stat.unit === "x")) return no("format", start);
    if (n.sign || stat.unit === "x") {
      const role = rivenValueMalus(stat, n.sign === "-" ? -n.value : n.value);
      if (said !== null && said !== role) return no("contradicts", start);
      said = role;
    }
    lines.push({ id: stat.id, value: n.value, malus: said ?? stat.bonus === false, word: hit.w });
  }
  return lines.length ? { lines } : no("format", 0);
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
  const words = [];
  for (const w of rivenStatWords(pool, locales)) words.push({ ...w, w: rivenQueryFold(w.w) });
  for (const l of locales || []) {
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
