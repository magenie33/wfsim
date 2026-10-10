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

export function makeAnswer({ run, meta, zh, headless, host }) {
  const t = (s, p = {}) => Object.entries(p).reduce((x, [k, v]) => x.split(`{${k}}`).join(String(v)),
    (zh.ui && zh.ui[s]) || s);
  const weaponName = (w) => (zh.weapons && zh.weapons[w.id]) || w.name;
  const modName = (id) => { const c = String(id).split("@")[0]; return (zh.mods && zh.mods[c]) || c; };
  const arcaneName = (id) => (zh.arcanes && zh.arcanes[id]) || id;
  const evoName = (id) => (zh.evolutions && zh.evolutions[id]) || id;
  // EVERY NAME A WEAPON GOES BY, longest first, so "Boar Prime" wins over "Boar".
  const weaponNames = (meta.weapons || []).flatMap((w) => [w.id, w.name, w.name_en, zh.weapons && zh.weapons[w.id]]
    .filter(Boolean).map((n) => ({ w, n: nameFold(n) }))).sort((a, b) => b.n.length - a.n.length);
  const rulerShort = (b) => t(b.name).split(" · ")[0];
  const statNames = (cls) => headless.rivenStatNames((meta.riven_stats || {})[cls] || [], [zh]);
  const statZh = (cls, id) => {
    const s = ((meta.riven_stats || {})[cls] || []).find((x) => x.id === id);
    if (!s) return id;
    const n = statNames(cls).find((x) => x.stat.id === id);
    return (n && n.names[1]) || headless.rivenStatNameEn(s);
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
    t("I am Nona, WFSim's assistant. Send zk or pz… not that I was waiting for you. (⁄ ⁄•⁄ω⁄•⁄ ⁄)"),
    t("zk weapon [ruler] [stats] [count]: the rivens the board has measured for this weapon, against its best build without one."),
    t("pz weapon [ruler] [riven] [count]: the best builds the board has measured for this weapon; add riven for builds that carry one."),
    t("fx weapon [ruler] each stat with its number: the gain of your own riven — the volunteers' devices search its best build together."),
    t("gx [name]: the compute contribution rankings — all time, this month, this week; with the name the ranking shows, that person's place on each."),
    t("For example: {a}, or {b}", { a: "zk 托里德 双暴 负任意 5", b: "pz 托里德 爆破使 紫卡 3" }),
  ].join("\n");
  const UNREAD = "I could not read “{word}”… Write a stat as the card does, or as short as 双暴, 暴伤 or 负任意. (・_・;)";
  const NO_WEAPON = "Which weapon? Put its name after the command, like {e}. (・_・;)";
  const NOT_FOUND = "No weapon by that name. Check it again — Chinese or English both work. (＞﹏＜)";
  const NOT_MEASURED = "Nobody has measured this one yet. Measure a build on wfsim.app… then I will remember it. (´；ω；`)";

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
    if (named.left) return t("I could not read “{word}”… The rulers are {list}; add riven for builds with one. (・_・;)", { word: named.left, list: rulerList() });
    const ruler = named.ruler || defaultRuler();
    const r = await run("builder.board.read", { weapon: hit.w.id, riven: said ? "with" : "without", limit: n || 3, distinct: true, pooled: true });
    if (r.ok === false) return t(NOT_MEASURED);
    const rows = r.rows.filter((x) => x.ruler_id === ruler.id);
    if (!rows.length) return t(NOT_MEASURED);
    const shown = rows.length;
    const card = `${SITE}${headless.headlessWeaponPath(meta.weapons || [], hit.w.id)}/card?kind=pz&ruler=${encodeURIComponent(ruler.id)}&n=${shown}${said ? "&rv=1" : ""}`;
    const text = [`${weaponName(hit.w)} · ${rulerShort(ruler)}`]
      .concat(rows.map((x) => `#${x.rank} ${x.score}  ${(x.build ? x.build.mods.map(modName) : x.mods).join("、")}${
        x.build && x.build.arcane.length ? ` | ${t("Arcane")}: ${x.build.arcane.map(arcaneName).join("、")}` : ""}${
        x.build && x.build.evolutions.length ? ` | ${t("Evolutions")}: ${x.build.evolutions.map(evoName).join("、")}` : ""}`))
      .join("\n");
    return { line: t(said ? "The top {n} riven builds of {w} under {ruler}. (￣ー￣)ゞ" : "The top {n} builds of {w} under {ruler}. (￣ー￣)ゞ",
      { w: weaponName(hit.w), ruler: rulerShort(ruler), n: shown }), card, text };
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
    if (!g) return t("No riven on the board has these stats yet. (￣^￣)");
    const top = headless.distinctTop(g.rivens, n || 3, () => "", (x) => x.score);
    const shown = top.length;
    const card = `${SITE}${headless.headlessWeaponPath(meta.weapons || [], hit.w.id)}/card?kind=zk&ruler=${encodeURIComponent(g.ruler_id)}&n=${shown}`
      + (ask.bonuses.length ? `&has=${ask.bonuses.map(encodeURIComponent).join(",")}` : "")
      + (ask.malus ? `&malus=${encodeURIComponent(ask.malus)}` : "");
    const line = (x) => `#${x.rank} ${x.stat_ids.bonuses.map((id) => "+" + statZh(cls, id)).join(" ")}${
      x.stat_ids.malus ? " −" + statZh(cls, x.stat_ids.malus) : ""}  ${x.score}${x.gain == null ? "" : `（${x.gain >= 0 ? "+" : "−"}${Math.abs(x.gain * 100).toFixed(1)}%）`}`;
    const text = [`${weaponName(hit.w)} · ${t(g.ruler).split(" · ")[0]}`,
      `${t("The board's best riven-free build")}: ${g.riven_free ? g.riven_free.score : "—"}`]
      .concat(top.map(line)).join("\n");
    return { line: t("These are {w}'s rivens under {ruler}, against the best build without one. I ran every one. (*/ω＼*)",
      { w: weaponName(hit.w), ruler: t(g.ruler).split(" · ")[0] }), card, text };
  }

  /// GX: THE COMPUTE RANKINGS, all three — or, after it, the exact name the
  /// ranking shows someone under (spaces and case as shown), their place on
  /// each. A number alone is how many rows a ranking shows. An anonymous
  /// contributor is never found by anything.
  async function gx(rest) {
    const ask = rest.trim();
    const count = /^\d+$/.test(ask) ? Math.max(1, Math.min(MAX_SHOWN, Number(ask))) : null;
    const name = count === null ? ask : "";
    const get = (q) => fetch(`${SITE}/api/contributors${q}`).then((r) => (r.ok ? r.json() : null)).catch(() => null);
    const periods = [["all", "points", "All-time ranking"], ["recent", "recent", "Monthly ranking"], ["week", "week", "Weekly ranking"]];
    const num = (x) => (x || 0).toLocaleString("en-US");
    if (name) {
      const r = await get(`?name=${encodeURIComponent(name)}`);
      if (!r) return t("Nona could not reach the ranking just now. Try again in a moment. (＞﹏＜)");
      const p = r.person;
      if (!p) return t("Nobody on the ranking shows the name “{name}”. A name appears there only once its owner chooses to show it. (＞﹏＜)", { name });
      const card = `${SITE}/contributors/card?name=${encodeURIComponent(name)}`;
      const cr = p.contributor_rank;
      const text = [t("{name}'s contribution", { name }) + (cr ? ` · ${t("Contributor rank {n}", { n: cr.rank })}` : "")].concat(periods.map(([id, key, label]) => `${t(label)}: ${
        p.ranks[id] ? `#${p.ranks[id]} · ${num(p[key])}` : t("Not on this ranking yet.")}`)).join("\n");
      return { line: t("{name}'s place on the three rankings. (*/ω＼*)", { name }), card, text };
    }
    const all = await Promise.all(periods.map(([id]) => get(id === "all" ? "" : `?period=${id}`)));
    if (!all[0]) return t("Nona could not reach the ranking just now. Try again in a moment. (＞﹏＜)");
    const n = count || 5;
    const card = `${SITE}/contributors/card?n=${n}`;
    const text = periods.map(([, key, label], i) => [t(label)].concat(((all[i] && all[i].contributors) || []).slice(0, n)
      .map((c, k) => `#${k + 1} ${c.name === null ? t("Anonymous contributor") : c.name}${
        c.contributor_rank ? ` [${c.contributor_rank.rank}]` : ""} · ${num(c[key])}`)).join("\n")).join("\n\n");
    const computing = all[0].computing || 0;
    return { line: computing
      ? t("The contribution rankings — all time, this month, this week. {n} devices are computing together right now. (￣ー￣)ゞ", { n: computing })
      : t("The contribution rankings — all time, this month, this week. (￣ー￣)ゞ"), card, text };
  }

  /// FX: THE ASKER'S OWN RIVEN, appraised by whoever opens its link — docs/AGENT.md
  /// §"Riven appraisal". Each stat is a word and the number on the card; the
  /// roll is that number over the engine's own value at roll 1, and a number the
  /// card could not show at this weapon's disposition is refused, not clamped.
  async function fx(rest, ctx) {
    const example = "fx 托里德 暴伤160.9 多重120.7 腐蚀120.7 负弹匣43.9";
    if (!rest) return t(NO_WEAPON, { e: example });
    const hit = weaponOf(rest);
    if (!hit) return t(NOT_FOUND);
    const named = rulerIn(hit.left);
    const ruler = named.ruler || defaultRuler();
    const cls = hit.w.riven_class;
    const pool = (meta.riven_stats || {})[cls] || [];
    const pairs = [...named.left.matchAll(/([^\d.+%]+?)\+?(-?\d+(?:\.\d+)?)%?/g)].map((x) => ({ word: x[1], value: Math.abs(Number(x[2])) }));
    const tail = named.left.replace(/([^\d.+%]+?)\+?(-?\d+(?:\.\d+)?)%?/g, "");
    if (!pairs.length || tail) return t("Each stat needs the number on the card, like {e}. (・_・;)", { e: example });
    const bonuses = [], maluses = [];
    for (const p of pairs) {
      const q = headless.rivenQuery(pool, [zh], p.word);
      if (q.unread.length) return t(UNREAD, { word: q.unread[0] });
      if (q.malus && q.malus !== "any" && q.malus !== "none" && !q.bonuses.length) maluses.push({ id: q.malus, value: p.value, word: p.word });
      else if (q.bonuses.length === 1 && !q.malus) bonuses.push({ id: q.bonuses[0], value: p.value, word: p.word });
      else return t("“{word}” names more than one stat; give each its own number. (・_・;)", { word: p.word });
    }
    if (bonuses.length < 2 || bonuses.length > 3 || maluses.length > 1) return t("A riven has two or three positive stats and at most one negative. (・_・;)");
    const rank = (meta.riven_rules || {}).max_rank ?? 8;
    const base = await host.api("/api/riven", { weapon: hit.w.id, rank, polarity: "madurai",
      bonuses: bonuses.map((b) => ({ id: b.id, roll: 1 })), malus: maluses[0] ? { id: maluses[0].id, roll: 1 } : null });
    if (!base || base.ok === false) return t(NOT_FOUND);
    if ((base.illegal || []).length) return t("This riven is not a legal one: {why} (＞﹏＜)", { why: base.illegal.join("; ") });
    const slots = bonuses.map((b, i) => ({ ...b, slot: String(i) })).concat(maluses.map((m) => ({ ...m, slot: "malus" })));
    const rolled = [];
    for (const sl of slots) {
      const st = (base.stats || []).find((x) => x.slot === sl.slot);
      const one = st ? Math.abs(Number(st.shown)) : 0;
      if (!one) return t(UNREAD, { word: sl.word });
      // THE CARD ROUNDS TO ITS OWN DECIMALS, so a number at the edge of the band
      // may read a hair past it.
      const slack = 0.5 * 10 ** -(st.decimals ?? 1) / one + 1e-9;
      const roll = sl.value / one;
      if (roll < 0.9 - slack || roll > 1.1 + slack) {
        const show = (x) => Math.abs(Number(x)).toFixed(st.decimals ?? 1);
        const [lo, hi] = [show(st.min), show(st.max)].sort((a, b) => a - b);
        return t("{stat} {value} is outside this weapon's range ({lo}–{hi}); the card may be from before a disposition change. (￣^￣)",
          { stat: statZh(cls, sl.id), value: sl.value, lo, hi });
      }
      rolled.push({ id: sl.id, roll: Math.round(Math.min(1.1, Math.max(0.9, roll)) * 1000) / 1000, malus: sl.slot === "malus" });
    }
    const riven = { bonuses: rolled.filter((x) => !x.malus).map(({ id, roll }) => ({ id, roll })),
      malus: rolled.filter((x) => x.malus).map(({ id, roll }) => ({ id, roll }))[0] || null, rank };
    const opened = ctx && ctx.openAppraisal ? await ctx.openAppraisal({ weapon: hit.w.id, ruler: ruler.id, riven }) : null;
    if (!opened || !opened.code) {
      return t(opened && /limit/.test(opened.error || "") ? "Nona is still busy with your earlier ones. Try again in a while. (´；ω；`)"
        : "Nona could not start the riven gain just now. Try again in a moment. (＞﹏＜)");
    }
    const card = `${SITE}${headless.headlessWeaponPath(meta.weapons || [], hit.w.id)}/card?kind=appraise&code=${opened.code}`;
    const line = rolled.map((x) => `${x.malus ? "−" : "+"}${statZh(cls, x.id)} ×${x.roll.toFixed(2)}`).join(" ");
    return { line: t("Nona read it. The volunteers' devices are computing it together — usually a minute or two — and Nona will post the answer here. Can't wait? Scan the code to run it on your own device, riven gain {code}. (๑•̀ㅂ•́)و✧", { code: opened.code }),
      card, text: `${weaponName(hit.w)} · ${rulerShort(ruler)} · ${line}\n${SITE}/appraise/${opened.code}` };
  }

  /// …AND WHILE IT WAITS: a computer has taken it.
  const started = (s) => t("A volunteer's device has taken riven gain {code} — usually a few minutes. Nona will post the answer here. (｀・ω・´)", { code: s.code });

  /// `{ text }`, or `{ line, card, text }` — the long image at `card` with
  /// `line` under it, and `text` the answer in words if the image cannot be made.
  async function answer(text, ctx) {
    const p = parse(text);
    const r = p.cmd === "zk" ? await zk(p.rest) : p.cmd === "pz" ? await pz(p.rest) : p.cmd === "fx" ? await fx(p.rest, ctx)
      : p.cmd === "gx" ? await gx(p.rest) : help();
    return typeof r === "string" ? { text: r } : r;
  }
  /// WHAT NONA SAYS WITH AN APPRAISAL'S ANSWER: the replayed number, its gain on
  /// the board's riven-free leader, and thanks; `late` when it arrives on someone
  /// else's message after the asker's own window closed.
  answer.told = (item, late) => {
    const w = (meta.weapons || []).find((x) => x.id === item.weapon) || { id: item.weapon, name: item.weapon };
    const v = item.verdict || {};
    const gain = typeof v.gain === "number" ? `${v.gain >= 0 ? "+" : "−"}${Math.abs(v.gain * 100).toFixed(1)}%` : "—";
    const said = t("{w} with this riven: {shown}, {gain} against the best build without one. Thanks to {who} for searching it! (⁄ ⁄•⁄ω⁄•⁄ ⁄)",
      { w: weaponName(w), shown: v.shown || "—", gain,
        who: item.thanks || (item.volunteer ? t("the volunteers' devices") : t("a kind someone in the chat")) });
    return late ? `${t("The riven gain from earlier is in.")} ${said}` : said;
  };
  /// The long image of an appraisal's answer — the page that replays it.
  answer.started = started;
  answer.answerCard = (item, resultId) => `${SITE}${headless.headlessWeaponPath(meta.weapons || [], item.weapon)}/card?kind=appraise&code=${item.code}&result=${resultId}`;
  return answer;
}
