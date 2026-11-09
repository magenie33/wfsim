// SPDX-License-Identifier: AGPL-3.0-or-later
// A CHAT MESSAGE IN, A REPLY OUT — docs/AGENT.md §"The QQ bot". Commands are
// `zk` (rivens), `pz` (builds) and `gx` (the compute rankings), a trailing
// number is how many to show. Every
// number comes from the headless table; this only reads the words and lays the
// answer out, in the language overlay's words (`ui` keyed by English) and in
// Nona's voice (docs/NONA.md §"Her voice"): the line calm, the kaomoji her.

const MAX_SHOWN = 10;
const SITE = "https://wfsim.app";
const squash = (s) => String(s || "").normalize("NFKC").toLowerCase().replace(/\s+/g, "");
/// A WEAPON'S NAME as people type it: no spaces, hyphens or dots, any width —
/// "mk-1盗贼" and "MK1 盗贼" are both MK1-盗贼.
const nameFold = (s) => squash(s).replace(/[-_.·・'’]/g, "");

import { readRivenLines } from "../mcp/headless.js";
export { readRivenLines };

export function makeAnswer({ run, meta, zh, headless, host }) {
  const t = (s, p = {}) => Object.entries(p).reduce((x, [k, v]) => x.split(`{${k}}`).join(String(v)),
    (zh.ui && zh.ui[s]) || s);
  const say = headless.headlessSpeaker(meta, { zh }, "zh");
  const boardAttribution = async (weapon, url) => {
    try {
      if (!host?.board || !host?.computationNames) return {entries:[],names:new Map()};
      const boardRows = await host.board(weapon.id) || [];
      const selected = headless.headlessCardBoard(say, weapon, headless.headlessCardAsk(new URL(url).search), boardRows);
      return {...selected,boardRows,names:await host.computationNames(weapon.id,selected.rows)};
    } catch (_) { return {entries:[],names:new Map()}; }
  };
  const cardAttribution = attribution => ({
    cardVersion: (attribution.rows || []).map(row => headless.headlessWitnessesHtml(say,attribution.names.get(row),SITE)).join("\n"),
    ...(attribution.boardRows ? {cardData:{rows:attribution.boardRows,witnesses:attribution.names}} : {}),
  });
  const attributionText = (attribution, row) => headless.headlessWitnessesText(say, attribution.names.get(row));
  const weaponName = (w) => (zh.weapons && zh.weapons[w.id]) || w.name;
  const modName = (id) => { const c = String(id).split("@")[0]; return (zh.mods && zh.mods[c]) || c; };
  const arcaneName = (id) => (zh.arcanes && zh.arcanes[id]) || id;
  const evoName = (id) => (zh.evolutions && zh.evolutions[id]) || id;
  // EVERY NAME A WEAPON GOES BY, longest first, so "Boar Prime" wins over "Boar".
  const weaponNames = (meta.weapons || []).flatMap((w) => [w.id, w.name, w.name_en, zh.weapons && zh.weapons[w.id]]
    .filter(Boolean).flatMap((n) => {
      const full = nameFold(n);
      return (full.endsWith("prime") ? [full, full.slice(0, -5) + "p"] : [full]).map((n) => ({ w, n }));
    })).sort((a, b) => b.n.length - a.n.length);
  const rulerShort = (b) => t(b.name).split(" · ")[0];
  const statNames = (cls) => headless.rivenStatNames((meta.riven_stats || {})[cls] || [], [zh]);
  const statZh = (cls, id) => {
    const s = ((meta.riven_stats || {})[cls] || []).find((x) => x.id === id);
    if (!s) return id;
    const n = statNames(cls).find((x) => x.stat.id === id);
    return (n && n.names[1]) || headless.rivenStatNameEn(s);
  };
  const signedStatZh = (cls, id, malus = false) => {
    const s = ((meta.riven_stats || {})[cls] || []).find((x) => x.id === id);
    const negative = s ? (s.base < 0) !== (malus && s.bonus !== false) : malus;
    return (negative ? "−" : "+") + statZh(cls, id);
  };

  /// `zk 托里德 5` → { cmd: "zk", rest: "托里德 5" }. A COMMAND PANEL sends it
  /// as `/zk`, so a leading slash, either width, is the same command.
  function parse(text) {
    let s = String(text || "").replace(/<@!?[^>]*>/g, " ").trim();
    const m = s.match(/^[/／]?\s*(zk|pz|fx|gx|帮助|help|\?|？)\s*/i);
    const cmd = m ? m[1].toLowerCase() : "";
    if (m) s = s.slice(m[0].length);
    return { cmd: ["zk", "pz", "fx", "gx"].includes(cmd) ? cmd : cmd ? "help" : "", rest: s.trim() };
  }
  /// THE COUNT, read AFTER the weapon: "夜语者77" is a weapon and "77" in it is not
  /// a count. Whatever number ends what is left, spaced or not, held to 1–10.
  function countOf(left) {
    const num = left.match(/(\d+)$/);
    if (!num) return { n: null, left };
    return { n: Math.max(1, Math.min(MAX_SHOWN, Number(num[1]))), left: left.slice(0, num.index) };
  }
  /// The weapon the words start with, and what is left after it.
  function weaponOf(rest) {
    const q = nameFold(rest);
    const hit = weaponNames.find((x) => x.n && q.startsWith(x.n));
    if (!hit) return null;
    // WHAT FOLLOWS THE NAME keeps its signs ("-变焦" is a malus): walk the words
    // until as much of them as the name has been consumed.
    let i = 0;
    while (i < rest.length && nameFold(rest.slice(0, i)).length < hit.n.length) i += 1;
    return { w: hit.w, left: squash(rest.slice(i)) };
  }

  /// THE RULER NAMED in what is left, by its name in either language, and the
  /// words around it; `ruler` is null when none is named.
  const benches = meta.benchmarks || [];
  function rulerIn(left) {
    const names = benches.flatMap((b) => [squash(rulerShort(b)), squash(b.name.split(" · ")[0])].map((n) => ({ b, n })))
      .filter((x) => x.n).sort((a, z) => z.n.length - a.n.length);
    const hit = names.find((x) => left.includes(x.n));
    return hit ? { ruler: hit.b, left: left.replace(hit.n, "") } : { ruler: null, left };
  }
  const defaultRuler = () => benches.find((b) => b.primary) || benches[0];
  const rulerList = () => benches.map(rulerShort).join("、");

  const help = () => [
    t("Nona here. Builds, rivens, or your own riven gain? Pick a command. (￣ー￣)ゞ"),
    t("zk weapon [ruler] [stats] [count]: look up the riven ranking and its calculation witnesses."),
    t("pz weapon [ruler] [riven] [count]: look up measured builds; add riven for builds with one."),
    t("fx weapon [ruler] each stat with its number: submit your riven for community search; Nona replays the result before posting it."),
    t("gx [name or count]: look up lifetime, recent and weekly compute contributions; use a public nickname to find one person."),
    t("Try {a}, or {b}.", { a: "zk 托里德 双暴 负任意 5", b: "pz 托里德 爆破使 紫卡 3" }),
  ].join("\n");
  const UNREAD = "I could not read “{word}”. Use the card's stat name, or an alias such as 双暴, 暴伤 or 负任意. (・_・;)";
  const NO_WEAPON = "Which weapon? Add its name, like {e}. (・_・;)";
  const NOT_FOUND = "I could not find that weapon. Check the name; Chinese and English both work. (・_・;)";
  const NOT_MEASURED = "There are no published results for this weapon under that ruler yet. (・_・;)";

  async function pz(rest) {
    if (!rest) return t(NO_WEAPON, { e: "pz 托里德 3" });
    const hit = weaponOf(rest);
    if (!hit) return t(NOT_FOUND);
    const { n, left: afterCount } = countOf(hit.left);
    // "紫卡" anywhere after the weapon asks for riven builds; what is left names the ruler.
    const rivenWords = ((zh.riven_query_words || {}).with_riven || []).map(squash).sort((a, b) => b.length - a.length);
    const said = rivenWords.find((x) => x && afterCount.includes(x));
    const named = rulerIn(said ? afterCount.replace(said, "") : afterCount);
    // A WORD THAT IS NOT A RULER IS SAID, never quietly read as the default one.
    if (named.left) return t("I could not read “{word}”. Choose a ruler from {list}; add riven for a build with one. (・_・;)", { word: named.left, list: rulerList() });
    const ruler = named.ruler || defaultRuler();
    const r = await run("builder.board.read", { weapon: hit.w.id, riven: said ? "with" : "without", limit: n || 3, distinct: true, pooled: true });
    if (r.ok === false) return t(NOT_MEASURED);
    const rows = r.rows.filter((x) => x.ruler_id === ruler.id);
    if (!rows.length) return t(NOT_MEASURED);
    const shown = rows.length;
    const card = `${SITE}${headless.headlessWeaponPath(meta.weapons || [], hit.w.id)}/card?kind=pz&ruler=${encodeURIComponent(ruler.id)}&n=${shown}${said ? "&rv=1" : ""}`;
    const attribution = await boardAttribution(hit.w, card);
    const text = [`${weaponName(hit.w)} · ${rulerShort(ruler)}`]
      .concat(rows.map((x, i) => `#${x.rank}${x.tied > 1 ? `（${t("{n} builds tie", { n: x.tied })}）` : ""} ${x.score}  ${(x.build ? x.build.mods.map(modName) : x.mods).join("、")}${
        x.build && x.build.arcane.length ? ` | ${t("Arcane")}: ${x.build.arcane.map(arcaneName).join("、")}` : ""}${
        x.build && x.build.evolutions.length ? ` | ${t("Evolutions")}: ${x.build.evolutions.map(evoName).join("、")}` : ""}\n  ${attributionText(attribution,attribution.entries[i]?.row)}`))
      .join("\n");
    return { line: t(said ? "Found {n} riven builds for {w} under {ruler}. (￣ー￣)ゞ" : "Found {n} builds for {w} under {ruler}. (￣ー￣)ゞ",
      { w: weaponName(hit.w), ruler: rulerShort(ruler), n: shown }), card, text, ...cardAttribution(attribution) };
  }

  async function zk(rest) {
    if (!rest) return t(NO_WEAPON, { e: "zk 托里德 双暴" });
    const hit = weaponOf(rest);
    if (!hit) return t(NOT_FOUND);
    const { n, left: afterCount } = countOf(hit.left);
    const cls = hit.w.riven_class;
    const named = rulerIn(afterCount);
    const ask = headless.rivenQuery((meta.riven_stats || {})[cls] || [], [zh], named.left);
    if (ask.unread.length) return t(UNREAD, { word: ask.unread[0] });
    const r = await run("builder.rivens.read", { weapon: hit.w.id, bonuses: ask.bonuses, malus: ask.malus, pooled: true,
      ...(named.ruler ? { ruler: named.ruler.id } : {}) });
    if (r.ok === false || !r.groups.length) return t(NOT_MEASURED);
    const g = r.groups.find((x) => x.rivens.length);
    if (!g) return t("The board has no riven matching these stats yet. Try fewer filters. (・_・;)");
    const top = headless.distinctTop(g.rivens, n || 3, () => "", meta);
    const shown = top.length;
    const card = `${SITE}${headless.headlessWeaponPath(meta.weapons || [], hit.w.id)}/card?kind=zk&ruler=${encodeURIComponent(g.ruler_id)}&n=${shown}`
      + (ask.bonuses.length ? `&has=${ask.bonuses.map(encodeURIComponent).join(",")}` : "")
      + (ask.malus ? `&malus=${encodeURIComponent(ask.malus)}` : "");
    const attribution = await boardAttribution(hit.w, card);
    const line = (x) => `#${x.rank} ${x.stat_ids.bonuses.map((id) => signedStatZh(cls, id)).join(" ")}${
      x.stat_ids.malus ? " " + signedStatZh(cls, x.stat_ids.malus, true) : ""}  ${x.score}${x.gain == null ? "" : `（${x.gain >= 0 ? "+" : "−"}${Math.abs(x.gain * 100).toFixed(1)}%）`}`;
    const text = [`${weaponName(hit.w)} · ${t(g.ruler).split(" · ")[0]}`,
      `${t("The board's best riven-free build")}: ${g.riven_free ? g.riven_free.score : "—"}${g.riven_free ? "\n  " + attributionText(attribution,attribution.top?.row) : ""}`]
      .concat(top.map((x,i) => line(x) + "\n  " + attributionText(attribution,attribution.entries[i]?.row))).join("\n");
    return { line: t("Here is {w}'s riven ranking under {ruler}, compared with the best build without a riven. (*/ω＼*)",
      { w: weaponName(hit.w), ruler: t(g.ruler).split(" · ")[0] }), card, text, ...cardAttribution(attribution) };
  }

  /// GX: THE COMPUTE RANKINGS, all three — or, after it, the exact name the
  /// ranking shows someone under (spaces and case as shown), their place on
  /// each. A number alone is how many rows a ranking shows. An anonymous
  /// contributor is never found by anything.
  async function gx(rest) {
    const display = person => headless.headlessContributorText(headless.headlessSpeaker({}, { zh }, "zh"), person);
    const ask = rest.trim();
    const count = /^\d+$/.test(ask) ? Math.max(1, Math.min(MAX_SHOWN, Number(ask))) : null;
    const name = count === null ? ask : "";
    const get = (q) => fetch(`${SITE}/api/contributors${q}`).then((r) => (r.ok ? r.json() : null)).catch(() => null);
    const periods = [["all", "points", "All-time ranking"], ["recent", "recent", "Recent ranking"], ["week", "week", "Weekly ranking"]];
    const num = (x) => (x || 0).toLocaleString("en-US");
    if (name) {
      const r = await get(`?name=${encodeURIComponent(name)}`);
      if (!r) return t("The contribution ranking could not be loaded. Try again shortly. (・_・;)");
      const p = r.person;
      if (!p) return t("No public contributor named “{name}” was found. Check their public nickname. (・_・;)", { name });
      const card = `${SITE}/contributors/card?name=${encodeURIComponent(name)}`;
      const cr = p.contributor_rank;
      const text = [t("{name}'s contribution", { name: display(p) }) + (cr ? ` · ${t("Contributor rank {n}", { n: cr.rank })}` : "")].concat(periods.map(([id, key, label]) => `${t(label)}: ${
        p.ranks[id] ? `#${p.ranks[id]} · ${num(p[key])}` : t("Not on this ranking yet.")}`)).join("\n");
      return { line: t("{name}'s compute contribution record. (*/ω＼*)", { name: display(p) }), card, text, cardVersion:JSON.stringify(p) };
    }
    const all = await Promise.all(periods.map(([id]) => get(id === "all" ? "" : `?period=${id}`)));
    if (!all[0]) return t("The contribution ranking could not be loaded. Try again shortly. (・_・;)");
    const n = count || 5;
    const card = `${SITE}/contributors/card?n=${n}`;
    const text = periods.map(([, key, label], i) => [t(label)].concat(((all[i] && all[i].contributors) || []).slice(0, n)
      .map((c, k) => `#${k + 1} ${display(c)}${
        c.contributor_rank ? ` [${c.contributor_rank.rank}]` : ""} · ${num(c[key])}`)).join("\n")).join("\n\n");
    const computing = all[0].computing || 0;
    return { line: computing
      ? t("Lifetime, recent and weekly compute contributions. {n} devices are computing now. Thank you all! (￣ー￣)ゞ", { n: computing })
      : t("Here are the lifetime, recent and weekly compute contributions. Thank you all! (￣ー￣)ゞ"), card, text, cardVersion:JSON.stringify(all.map(r=>(r?.contributors || []).slice(0,n))) };
  }

  /// FX: THE ASKER'S OWN RIVEN, appraised by whoever opens its link — docs/AGENT.md
  /// §"Riven appraisal". The engine bounds each printed value and turns it into
  /// a roll. Numbers outside the card's rounded range are refused, not clamped.
  async function fx(rest, ctx) {
    const example = "fx 托里德 148.5暴伤 111.3多重 111.3腐蚀 -40.5弹匣";
    if (!rest) return t(NO_WEAPON, { e: example });
    const hit = weaponOf(rest);
    if (!hit) return t(NOT_FOUND);
    const named = rulerIn(hit.left);
    const ruler = named.ruler || defaultRuler();
    const cls = hit.w.riven_class;
    const pool = (meta.riven_stats || {})[cls] || [];
    const read = readRivenLines(named.left, pool, [zh]);
    if (read.error === "format") return t("Include each stat's card value, like {e}. (・_・;)", { e: example });
    if (read.error === "unread") return t(UNREAD, { word: read.word });
    if (read.error === "ambiguous") return t("“{word}” includes several stats. Write a separate value for each. (・_・;)", { word: read.word });
    if (read.error === "contradicts") return t("“{word}” has conflicting signs. Keep the sign shown on the card. (・_・;)", { word: read.word });
    const bonuses = read.lines.filter((x) => !x.malus);
    const maluses = read.lines.filter((x) => x.malus);
    if (bonuses.length < 2 || bonuses.length > 3 || maluses.length > 1) return t("Please check the card: a riven needs two or three bonuses and at most one malus. (・_・;)");
    const rank = (meta.riven_rules || {}).max_rank ?? 8;
    const base = await host.api("/api/riven", { weapon: hit.w.id, rank, polarity: "madurai",
      bonuses: bonuses.map((b) => ({ id: b.id, roll: 1 })), malus: maluses[0] ? { id: maluses[0].id, roll: 1 } : null });
    if (!base || base.ok === false) return t("This riven could not be checked. Try again shortly. (・_・;)");
    if ((base.illegal || []).length) return t("This riven could not pass the card checks: {why} (・_・;)", { why: base.illegal.join("; ") });
    const slots = bonuses.map((b, i) => ({ ...b, slot: String(i) })).concat(maluses.map((m) => ({ ...m, slot: "malus" })));
    const values = [];
    for (const sl of slots) {
      const st = (base.stats || []).find((x) => x.slot === sl.slot);
      if (!st) return t(UNREAD, { word: sl.word });
      const value = st.unit === "x" ? sl.value : Math.sign(Number(st.shown)) * sl.value;
      // THE CARD ROUNDS TO ITS OWN DECIMALS, so a number at the edge of the band
      // may read a hair past it.
      const slack = 0.5 * 10 ** -(st.decimals ?? 1) + 1e-9;
      const [min, max] = [Number(st.min), Number(st.max)].sort((a, b) => a - b);
      if (value < min - slack || value > max + slack) {
        const show = (x) => Math.abs(Number(x)).toFixed(st.decimals ?? 1);
        const [lo, hi] = [show(st.min), show(st.max)].sort((a, b) => a - b);
        return t("{stat} {value} is outside this weapon's range ({lo}–{hi}). Check the weapon and value. (・_・;)",
          { stat: statZh(cls, sl.id), value: sl.value, lo, hi });
      }
      values.push({ id: sl.id, value, malus: sl.slot === "malus" });
    }
    const malusValue = values.find((s) => s.malus);
    const resolved = await host.api("/api/riven", { weapon: hit.w.id, rank, polarity: "madurai",
      bonuses: values.filter((s) => !s.malus).map(({ id, value }) => ({ id, value })),
      malus: malusValue ? { id: malusValue.id, value: malusValue.value } : null });
    if (!resolved || resolved.ok === false) return t("This riven could not be checked. Try again shortly. (・_・;)");
    const rolled = slots.map((sl) => ({ id: sl.id, roll: (resolved.stats || []).find((s) => s.slot === sl.slot)?.roll,
      malus: sl.slot === "malus" }));
    if (rolled.some((s) => !Number.isFinite(s.roll))) return t("This riven could not be checked. Try again shortly. (・_・;)");
    const riven = { bonuses: rolled.filter((x) => !x.malus).map(({ id, roll }) => ({ id, roll })),
      malus: rolled.filter((x) => x.malus).map(({ id, roll }) => ({ id, roll }))[0] || null, rank };
    const opened = ctx && ctx.openAppraisal ? await ctx.openAppraisal({ weapon: hit.w.id, ruler: ruler.id, riven }) : null;
    if (!opened || !opened.code) {
      return t(opened && /limit/.test(opened.error || "") ? "Too many riven requests were submitted just now. Try again later. (・_・;)"
        : "The riven request could not be submitted. Try again shortly. (・_・;)");
    }
    const card = `${SITE}${headless.headlessWeaponPath(meta.weapons || [], hit.w.id)}/card?kind=appraise&code=${opened.code}`;
    const say = headless.headlessSpeaker({}, { zh }, "zh");
    const line = slots.map((sl) => say.tf(resolved.stats.find((s) => s.slot === sl.slot).text)).join(" ");
    return { line: t("Nona has your riven, code {code}. I will replay the result and post it here when it passes. (๑•̀ㅂ•́)و✧", { code: opened.code }),
      card, text: `${weaponName(hit.w)} · ${rulerShort(ruler)} · ${line}\n${SITE}/appraise/${opened.code}` };
  }

  /// …AND WHILE IT WAITS: a computer has taken it.
  const started = (s) => t("A device has started riven gain {code}. Nona will post the result after replay. (｀・ω・´)", { code: s.code });

  /// `{ text }`, or `{ line, card, text }` — the long image at `card` with
  /// `line` under it, and `text` the answer in words if the image cannot be made.
  async function answer(text, ctx) {
    try {
      const p = parse(text);
      const r = p.cmd === "zk" ? await zk(p.rest) : p.cmd === "pz" ? await pz(p.rest) : p.cmd === "fx" ? await fx(p.rest, ctx)
        : p.cmd === "gx" ? await gx(p.rest) : help();
      return typeof r === "string" ? { text: r } : r;
    } catch (error) {
      console.error("nona command failed", error?.message || "unknown error");
      return {text:t("Nona could not finish this request. Try again shortly. (・_・;)")};
    }
  }
  /// WHAT NONA SAYS WITH AN APPRAISAL'S ANSWER: the replayed number, its gain on
  /// the board's riven-free leader, and thanks; `late` when it arrives on someone
  /// else's message after the asker's own window closed.
  answer.told = (item, late) => {
    const w = (meta.weapons || []).find((x) => x.id === item.weapon) || { id: item.weapon, name: item.weapon };
    const v = item.verdict || {};
    const gain = typeof v.gain === "number" ? `${v.gain >= 0 ? "+" : "−"}${Math.abs(v.gain * 100).toFixed(1)}%` : "—";
    const who = headless.headlessContributorText(headless.headlessSpeaker({}, { zh }, "zh"), item.calculator);
    // GRADED BY THE REPLAY (45-card-page.js `rivenGrade`); an answer judged
    // before the grade existed carries none and is told without one.
    const said = typeof v.grade === "number"
      ? t("Replay passed. {w}: {shown}, {gain} against the best riven-free build; grade {grade}/100 against {n} board rivens. Delivered by {who}. Thank you! (*/ω＼*)",
        { w: weaponName(w), shown: v.shown || "—", gain, grade: v.grade, n: v.of || 0, who })
      : t("Replay passed. {w}: {shown}, {gain} against the best riven-free build. Delivered by {who}. Thank you! (*/ω＼*)",
        { w: weaponName(w), shown: v.shown || "—", gain, who });
    return late ? `${t("The earlier riven result is here.")} ${said}` : said;
  };
  /// The long image of an appraisal's answer — the page that replays it.
  answer.started = started;
  answer.answerCard = (item, resultId) => `${SITE}${headless.headlessWeaponPath(meta.weapons || [], item.weapon)}/card?kind=appraise&code=${item.code}&result=${resultId}`;
  return answer;
}
