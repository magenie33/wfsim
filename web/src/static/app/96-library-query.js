// Both filter interfaces edit this query; projections never execute a second rule.
const LIBRARY_ATTRIBUTES = [
  ["crit_chance", "Crit Chance", "%", 100], ["crit_damage", "Crit Multiplier", "×", 1],
  ["status_chance", "Status Chance", "%", 100], ["multishot", "Multishot", "", 1],
  ["fire_rate", "Fire Rate", "/s", 1], ["magazine_size", "Magazine", "", 1],
];
const LIBRARY_NUMERIC_FIELDS = [...LIBRARY_ATTRIBUTES.map(([id, label]) => [id, label]), ["score", "Score"], ["drain", "Base MOD drain"]];
const LIBRARY_QUERY_FIELDS = [...LIBRARY_NUMERIC_FIELDS, ["equipment", "Equipment"], ["mode", "Mode"], ["riven", "Riven"], ["trigger", "Trigger"]];
const LIBRARY_ORDER_FIELDS = [...LIBRARY_NUMERIC_FIELDS, ["measured_at", "Recently calculated"]];
const LIBRARY_SIMPLE_ORDERS = { score: ["score", "desc"], "score-asc": ["score", "asc"], recent: ["measured_at", "desc"], drain: ["drain", "asc"] };
const libraryStatCache = new Map();
let libraryStatRun = null;
const libraryStatKey = (w, p) => JSON.stringify([ENGINE_ID, w.id, p.mode, boardRowIdentity(p.board), p.board.riven]);
const libraryBlankQuery = () => ({ where: { op: "and", children: [] }, orderBy: [{ field: "score", direction: "desc" }] });
const libraryQueryLeaves = (n = finder.query?.where) => !n ? [] : n.children ? n.children.flatMap((c) => libraryQueryLeaves(c)) : [n];
const libraryStaticField = (id) => LIBRARY_ATTRIBUTES.find(([key]) => key === id);
const libraryStatBounds = () => [...new Set([...libraryQueryLeaves().filter((n) => n.value != null && n.value !== "").map((n) => n.field), ...(finder.query?.orderBy || []).map((n) => n.field)])]
  .filter(libraryStaticField).map((id) => [id, {}]);
function librarySimpleTerms() {
  const leaf = (field, cmp, value) => ({ field, cmp, value });
  return [...[...finder.req].map((v) => leaf("equipment", "=", v)), ...[...finder.exc].map((v) => leaf("equipment", "!=", v)),
    ...[...finderOff()].map((v) => leaf("trigger", "!=", v)),
    ...(finder.mo && finder.mo !== FINDER_EVERY_MODE ? [leaf("mode", "=", finder.mo)] : []),
    ...(finder.rv !== "all" ? [leaf("riven", "=", finder.rv === "riven" ? "with" : "without")] : []),
    ...(finder.minScore != null ? [leaf("score", ">=", finder.minScore)] : []),
    ...(finder.maxDrain != null ? [leaf("drain", "<=", finder.maxDrain)] : [])];
}
function librarySimpleTerm(n) {
  if (n.value == null || n.value === "") return false;
  if (["mode", "riven", "score", "drain"].includes(n.field) && finder.query?.where.op === "and"
    && finder.query.where.children.some((other) => other !== n && other.field === n.field && (other.cmp !== n.cmp || other.value !== n.value))) return false;
  return !n.children && ((n.field === "equipment" && ["=", "!="].includes(n.cmp))
    || (n.field === "trigger" && n.cmp === "!=") || (["mode", "riven"].includes(n.field) && n.cmp === "=")
    || (n.field === "score" && n.cmp === ">=") || (n.field === "drain" && n.cmp === "<="));
}
const libraryComplexTerms = () => finder.query.where.op === "and"
  ? finder.query.where.children.filter((n) => !librarySimpleTerm(n)) : [finder.query.where];
function libraryProjectQuery() {
  Object.assign(finder, { req: new Set(), exc: new Set(), off: new Set(), mo: FINDER_EVERY_MODE, rv: "all", minScore: null, maxDrain: null });
  for (const n of finder.query.where.op === "and" ? finder.query.where.children : []) {
    if (!librarySimpleTerm(n)) continue;
    if (n.field === "equipment") (n.cmp === "=" ? finder.req : finder.exc).add(n.value);
    if (n.field === "trigger") finder.off.add(n.value);
    if (n.field === "mode") finder.mo = n.value;
    if (n.field === "riven") finder.rv = n.value === "with" ? "riven" : "plain";
    if (n.field === "score") finder.minScore = n.value;
    if (n.field === "drain") finder.maxDrain = n.value;
  }
  finder.simpleSnapshot = JSON.stringify(librarySimpleTerms());
  finder.sort = Object.entries(LIBRARY_SIMPLE_ORDERS).find(([, [field, direction]]) => finder.query.orderBy.length === 1
    && finder.query.orderBy[0].field === field && finder.query.orderBy[0].direction === direction)?.[0] || "custom";
}
function librarySyncQuery() {
  if (!finder.query) finder.query = libraryBlankQuery();
  const simple = librarySimpleTerms(), snapshot = JSON.stringify(simple);
  if (snapshot !== finder.simpleSnapshot) {
    finder.query.where = { op: "and", children: [...simple, ...libraryComplexTerms()] };
    finder.queryDraft = null; finder.queryError = "";
    libraryProjectQuery();
  }
}
function libraryReadAttributes(q) {
  finder.query = libraryBlankQuery(); finder.query.where.children = librarySimpleTerms();
  const [field, direction] = LIBRARY_SIMPLE_ORDERS[finder.sort] || LIBRARY_SIMPLE_ORDERS.score;
  finder.query.orderBy = [{ field, direction }];
  finder.advanced = q.get("advanced") === "1"; finder.advancedSort = q.get("sort_advanced") === "1"; finder.sortDraft = null; finder.sortError = ""; finder.queryDraft = null; finder.queryError = "";
  if (q.has("filter")) {
    try { finder.query = libraryParseQuery(q.get("filter")); }
    catch (e) { finder.queryError = e.message; finder.queryDraft = q.get("filter"); finder.advanced = true; }
  }
  if (finder.advanced) finder.filters = true;
  libraryProjectQuery();
}
function libraryWriteAttributes(q) {
  const text = libraryQueryLanguage(finder.query);
  if (text) q.set("filter", text);
  if (finder.advanced) q.set("advanced", "1");
  if (finder.advancedSort) q.set("sort_advanced", "1");
}
function libraryQueryValue(p, field) {
  if (libraryStaticField(field)) {
    const v = libraryStatCache.get(libraryStatKey(weaponInfo(finder.weapon), p))?.values?.[field];
    return Number.isFinite(v) ? v * libraryStaticField(field)[3] : undefined;
  }
  if (field === "score") return libraryScore(p);
  if (field === "drain") return libraryBaseDrain(p);
  if (field === "measured_at") return Date.parse(p.board.measured_at) || 0;
  if (field === "mode") return p.mode;
  if (field === "riven") return p.riven ? "with" : "without";
}
function libraryCompareValue(a, cmp, b) {
  if (a === undefined || !Number.isFinite(a) && typeof a === "number") return undefined;
  const equal = typeof a === "number" ? Math.abs(a - b) <= 1e-9 * Math.max(1, Math.abs(a), Math.abs(b)) : a === b;
  return cmp === "=" ? equal : cmp === "!=" ? !equal : cmp === ">" ? a > b && !equal
    : cmp === "<" ? a < b && !equal : cmp === ">=" ? a > b || equal : a < b || equal;
}
function libraryEvaluateQuery(n, p, tokens, triggers) {
  if (n.children) {
    const values = n.children.map((c) => libraryEvaluateQuery(c, p, tokens, triggers)).filter((v) => v !== null);
    if (!values.length) return null;
    if (n.op === "and" && values.includes(false)) return false;
    if (n.op === "or" && values.includes(true)) return true;
    return values.includes(undefined) ? undefined : n.op === "and";
  }
  if (n.value == null || n.value === "") return null;
  if (n.field === "equipment" || n.field === "trigger") {
    const has = n.field === "equipment" ? tokens.includes(n.value) : triggers.has(n.value);
    return n.cmp === "=" ? has : !has;
  }
  return libraryCompareValue(libraryQueryValue(p, n.field), n.cmp, n.value);
}
function libraryAttributePasses(p, tokens = finderTokens(p), triggers = finderTriggers(p)) {
  const value = libraryEvaluateQuery(finder.query.where, p, tokens, triggers);
  return value === true || value === null;
}
function libraryQueryCompare(a, b) {
  for (const term of finder.query.orderBy) {
    const av = libraryQueryValue(a, term.field), bv = libraryQueryValue(b, term.field);
    const am = !Number.isFinite(av), bm = !Number.isFinite(bv);
    if (am !== bm) return am ? 1 : -1;
    if (am) continue;
    const d = av - bv;
    if (d) return term.direction === "asc" ? d : -d;
  }
  return b.board.score - a.board.score || (a.mode + boardRowIdentity(a.board)).localeCompare(b.mode + boardRowIdentity(b.board));
}
function libraryAttributeCard(w, p) {
  const got = libraryStatCache.get(libraryStatKey(w, p));
  if (!got?.values || !libraryStatBounds().length) return "";
  const chips = libraryStatBounds().map(([id]) => {
    const [, label, unit, scale] = libraryStaticField(id);
    return `<span class="sb-chip"><span>${escHtml(tr(label))} ${escHtml(Number((got.values[id] * scale).toFixed(3)) + unit)}</span></span>`;
  });
  return `<div class="sb-h">${escHtml(tr("Static attributes"))}</div><div class="sb-chips">${chips.join("")}</div>`;
}
function libraryAttributeChips() {
  const nodes = libraryComplexTerms().filter((n) => libraryQueryLeaves(n).some((c) => c.value != null));
  if (!nodes.length) return "";
  const text = libraryExpressionText({ op: "and", children: nodes });
  return `<span class="fd-tok req library-expression-chip" title="${escHtml(text)}"><i>${escHtml(tr(finder.advanced ? "Advanced filters" : "Advanced conditions retained"))}</i> <b>${escHtml(text)}</b>`
    + `<button type="button" class="library-expression-edit" data-library-filter-edit>${escHtml(tr("Edit"))}</button>`
    + `<button type="button" data-library-remove="expression" aria-label="${escHtml(tr("remove"))}">×</button></span>`;
}
function libraryCommitQuery(area = "filter") {
  libraryProjectQuery();
  if (area === "sort") { finder.sortDraft = null; finder.sortError = ""; }
  else { finder.queryDraft = null; finder.queryError = ""; }
  finder.open = null; finder.shown = FINDER_FIRST; finder.ties.clear(); renderBuildFinder();
}
