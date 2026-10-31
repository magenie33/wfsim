// The typed riven reader against every real pool and the official locale.
// Names, values and signs work in either card order; aliases cannot replace
// an official name. No browser or network. Requires Python with PyYAML.
//
//   node scripts/check_fx_lines.mjs
import { execFileSync } from "node:child_process";
import { readRivenLines, makeAnswer } from "../bot/answer.mjs";
import * as headless from "../mcp/headless.js";

const { pools, zh } = JSON.parse(execFileSync(process.env.PYTHON || "python3", ["-c", `
import json
from pathlib import Path
import yaml
pools = {}
for path in sorted(Path("data/rivens").glob("*.yaml")):
    table = yaml.safe_load(path.read_text())
    if not isinstance(table, dict) or "stats" not in table:
        continue
    for stat in table["stats"]:
        stat["unit"] = "x" if stat["kind"] == "faction_damage_bonus" else "%" if "%" in stat["text"] else ""
    pools[table["class"]] = table["stats"]
zh = yaml.safe_load(Path("data/i18n/zh/ui.yaml").read_text())
zh.update(yaml.safe_load(Path("data/i18n/zh/riven_words.yaml").read_text()))
print(json.dumps({"pools": pools, "zh": zh}))
`], { cwd: new URL("..", import.meta.url), encoding: "utf8" }));
const tf = (s) => headless.headlessSpeaker({}, { zh }, "zh").tf(s);
const read = (s, pool = pools.rifle, locales = [zh]) => readRivenLines(s, pool, locales);
const officialOnly = [{ ...zh, riven_words: {} }];
const compact = (r) => r.lines ? r.lines.map((x) => `${x.malus ? "-" : "+"}${x.id}:${x.value}`).join(" ") : `!${r.error}`;
let failed = 0, checked = 0;
const check = (name, ok, detail = "") => {
  checked++;
  if (ok) return;
  failed++;
  if (failed <= 16) console.log(`FAIL  ${name}: ${detail}`);
};
const want = (input, expected, pool, locales) => {
  const got = compact(read(input, pool, locales));
  check(input, got === expected, `${got}, wanted ${expected}`);
};

for (const s of ["多重射击50", "多重射击+50", "正多重射击50", "多重射击 50", "多重射击 +50", "+多重射击50", "+50% 多重射击", "50多重射击"]) want(s, "+multishot:50");
for (const s of ["变焦-50", "负变焦50", "负变焦-50", "-变焦50", "变焦 - 50", "-50% 变焦"]) want(s, "-zoom:50");
for (const s of ["武器后坐力-50", "后坐力-50", "负后坐力50", "负武器后坐力50", "负50武器后坐力", "负后坐力-50", "-后坐力50", "Weapon Recoil-50", "-50% 武器后坐力", "-50% Weapon Recoil"]) want(s, "+weapon_recoil:50");
for (const s of ["武器后坐力+50", "后坐力+50", "正后坐力50", "+后坐力50", "+50% Weapon Recoil"]) want(s, "-weapon_recoil:50");
want("暴伤160.9 多重120.7 变焦-50", "+critical_damage:160.9 +multishot:120.7 -zoom:50");
want("暴伤160.9%多重+120.7%负变焦50%", "+critical_damage:160.9 +multishot:120.7 -zoom:50");
want("+160.9% Critical Damage\n+120.7% Multishot\n+50% Weapon Recoil", "+critical_damage:160.9 +multishot:120.7 -weapon_recoil:50");
want("暴击伤害：１６０．９％，多重射击：＋１２０．７％；变焦：−５０％", "+critical_damage:160.9 +multishot:120.7 -zoom:50");
want("Fire Rate (x2 for Bows)50", "+fire_rate:50");
want("+50% Fire Rate (x2 for Bows)", "+fire_rate:50");
want("Critical Chance (x2 for Heavy Attacks)50", "+critical_chance:50", pools.melee);
want("暴击几率（重击时 x2）50", "+critical_chance:50", pools.melee);
want("暴击几率50", "+critical_chance:50", pools.melee);
want("连击持续时间6s", "+combo_duration:6", pools.melee);
want("+6s Combo Duration", "+combo_duration:6", pools.melee);
want("Combo Duration6sCritical Chance50", "+combo_duration:6 +critical_chance:50", pools.melee);
want("Combo Duration6SC50", "+combo_duration:6 +status_chance:50", pools.melee);
want("-50% Chance to Gain Combo Count", "-chance_to_gain_combo_count:50", pools.melee);
want("负连击数几率50", "-chance_to_gain_combo_count:50", pools.melee);
want("Damage to Corpus x1.5", "+damage_to_corpus:1.5");
want("x0.59 对 Corpus 的伤害", "-damage_to_corpus:0.59");
want("对 Corpus 的伤害0.59", "-damage_to_corpus:0.59");
for (const s of ["负变焦+50", "负后坐力+50", "正后坐力-50", "-后坐力+50", "正-50后坐力"]) want(s, "!contradicts");
want("正0.59对Corpus的伤害", "-damage_to_corpus:0.59");
want("110.1火焰伤害 110.9异常状态伤害 111.2多重射击 -0.64对Infested伤害",
  "+heat:110.1 +status_damage:110.9 +multishot:111.2 -damage_to_infested:0.64");
for (const faction of ["Grineer", "Corpus", "Infested"]) {
  for (const number of ["-0.64", "负0.64", "x0.64", "×0.64", "0.64"]) {
    want(`${number}对${faction}伤害`, `-damage_to_${faction.toLowerCase()}:0.64`);
    want(`对${faction}伤害${number.replace("负", "-")}`, `-damage_to_${faction.toLowerCase()}:0.64`);
  }
}
for (const s of ["多重射击", "多重射击50 暴伤", "50", "多重50.2.3", "变焦--50"]) want(s, "!format");
want("飞天50", "!unread");
want("双暴50", "!ambiguous");
want("100多 150爆击率 120爆击伤害 -40弹容", "+multishot:100 +critical_chance:150 +critical_damage:120 -magazine_capacity:40");
want("100MS 150CC 120CD -40mag", "+multishot:100 +critical_chance:150 +critical_damage:120 -magazine_capacity:40");
want("50换弹 -50recoil 60弹速", "+reload_speed:50 +weapon_recoil:50 +projectile_speed:60");
want("110触伤 70触几 -40触发时长", "+status_damage:110 +status_chance:70 -status_duration:40");
want("0.64I歧视", "-damage_to_infested:0.64");
want("12初连 6连击时长 100滑爆", "+initial_combo:12 +combo_duration:6 +critical_chance_for_slide_attack:100", pools.melee);

// A conflicting shorthand must lose to the official full name.
want("武器后坐力-50", "+weapon_recoil:50", pools.rifle,
  [{ ...zh, riven_words: { ...zh.riven_words, 武器后坐力: ["zoom"] } }]);
for (const [input, bonuses, malus] of [
  ["双暴负任意", ["critical_chance", "critical_damage"], "any"],
  ["病毒伤害无负", ["viral"], "none"],
  ["后坐力负弹匣", ["weapon_recoil"], "magazine_capacity"],
  ["负后坐力", ["weapon_recoil"], null],
  ["-武器后坐力", ["weapon_recoil"], null],
  ["正后坐力", [], "weapon_recoil"],
  ["+Weapon Recoil", [], "weapon_recoil"],
  ["负对Infested伤害", [], "damage_to_infested"],
]) {
  const q = headless.rivenQuery(pools.rifle, [zh], input);
  check(`query ${input}`, !q.unread.length && JSON.stringify(q.bonuses) === JSON.stringify(bonuses) && q.malus === malus, JSON.stringify(q));
}
for (const [input, bonuses, malus] of [
  ["-Weapon Recoil", ["weapon_recoil"], null],
  ["+Weapon Recoil", [], "weapon_recoil"],
]) {
  const q = headless.rivenQuery(pools.rifle, [], input);
  check(`English query ${input}`, !q.unread.length && JSON.stringify(q.bonuses) === JSON.stringify(bonuses) && q.malus === malus, JSON.stringify(q));
}
for (const input of ["负后坐力正", "负正后坐力"]) {
  check(`query rejects conflicting or dangling sign ${input}`, headless.rivenQuery(pools.rifle, [zh], input).unread.length > 0);
}

for (const [cls, pool] of Object.entries(pools)) {
  for (const [word, ids] of Object.entries(zh.riven_words)) {
    const held = ids.filter(id => pool.some(stat => stat.id === id));
    if (!held.length) continue;
    const query = headless.rivenQuery(pool, [zh], word);
    check(`${cls} alias query ${word}`, !query.unread.length && !query.malus
      && JSON.stringify(query.bonuses.slice().sort()) === JSON.stringify(held.slice().sort()), JSON.stringify(query));
    if (held.length > 1) {
      want(`50${word}`, "!ambiguous", pool);
      continue;
    }
    const stat = pool.find(stat => stat.id === held[0]);
    const value = stat.unit === "x" ? 1.5 : 50;
    const number = stat.unit === "x" ? `x${value}` : `${stat.base < 0 ? "-" : "+"}${value}`;
    const expected = `${stat.bonus === false ? "-" : "+"}${stat.id}:${value}`;
    want(`${number}${word}`, expected, pool);
    want(`${word}${number}`, expected, pool);
  }
  for (const stat of pool) {
    const value = stat.unit === "x" ? 1.5 : 50;
    const sign = stat.base < 0 ? "-" : "+";
    const number = stat.unit === "x" ? `x${value}` : `${sign}${value}`;
    const names = [headless.rivenStatNameEn(stat), tf(stat.text.replace("|val|", number))
      .replace(/^[+\-]?x?\d+(?:\.\d+)?(?:%|s)?\s*/, "")];
    for (const name of names) {
      const expected = `${stat.bonus === false ? "-" : "+"}${stat.id}:${value}`;
      want(`${name}${number}`, expected, pool);
      want(`${number}${stat.unit === "%" ? "%" : ""} ${name}`, expected, pool);
      want(`${number}${stat.unit === "%" ? "%" : ""} ${name}`, expected, pool, officialOnly);
      const query = headless.rivenQuery(pool, [zh], name);
      check(`${cls} query ${name}`, query.unread.length === 0 && query.bonuses.length === 1 && query.bonuses[0] === stat.id && !query.malus, JSON.stringify(query));
      for (const [sign, word] of [["-", "负"], ["+", "正"]]) {
        const amount = stat.unit === "x" ? (sign === "-" ? 0.64 : 1.5) : value;
        const malus = ((sign === "+") !== (stat.base > 0)) !== (stat.bonus === false);
        const signedExpected = `${malus ? "-" : "+"}${stat.id}:${amount}`;
        for (const prefix of [sign, word]) {
          want(`${prefix}${name}${amount}`, signedExpected, pool);
          want(`${prefix}${amount}${name}`, signedExpected, pool);
          want(`${prefix}${name}${sign}${amount}`, signedExpected, pool);
        }
        want(`${name}${word}${amount}`, signedExpected, pool);
        const query = headless.rivenQuery(pool, [zh], `${word}${name}`);
        check(`${cls} query ${word}${name}`, !query.unread.length && (malus
          ? query.malus === stat.id && !query.bonuses.length
          : query.malus === null && query.bonuses.length === 1 && query.bonuses[0] === stat.id), JSON.stringify(query));
      }
    }
  }
}

// P is the Prime suffix in every chat command, including when the base weapon
// is another candidate; canonical names and family ids remain unchanged.
for (const name of ["冰凇p", "冰凇 P", "Verglas p", "VERGLASP", "verglas_prime", "冰凇prime"]) {
  const calls = [];
  const meta = { weapons: [
    { id: "verglas", name: "Verglas", riven_class: "rifle" },
    { id: "verglas_prime", name: "Verglas Prime", riven_class: "rifle" },
  ], benchmarks: [{ id: "test", name: "Test", primary: true }], riven_stats: pools };
  const answer = makeAnswer({ meta, zh: { ...zh, weapons: { verglas: "冰凇", verglas_prime: "冰凇 Prime" } }, headless,
    run: async (id, args) => { calls.push(args); return { ok: false }; },
    host: { api: async (path, body) => { calls.push(body); return { ok: false }; } } });
  for (const command of [`fx ${name} 100多 150暴伤`, `zk ${name} 双暴`, `pz ${name}`]) {
    const before = calls.length;
    await answer(command);
    check(`Prime shorthand ${command}`, calls.length === before + 1 && calls.at(-1)?.weapon === "verglas_prime", JSON.stringify(calls));
  }
}

// The command delegates printed-value inversion to the engine, including x0.xx
// maluses. These sentinel rolls are the engine's answer, not a second formula.
const cases = [
  { text: "fx Torid +102.3% Multishot +151.5% Critical Damage x0.58 Damage to Corpus", malus: "damage_to_corpus", shown: 0.6,
    min: 0.56, max: 0.64, unit: "x", value: 0.58, roll: 1.051234 },
  { text: "fx Torid 多重102.3 暴伤151.5 武器后坐力+50", malus: "weapon_recoil", shown: 50,
    min: 45, max: 55, unit: "%", value: 50, roll: 1.000123 },
];
for (const c of cases) {
  const stats = [
    { slot: "0", id: "multishot", shown: 100, min: 90, max: 110, decimals: 1, unit: "%", roll: 1.023456 },
    { slot: "1", id: "critical_damage", shown: 150, min: 135, max: 165, decimals: 1, unit: "%", roll: 1.01 },
    { slot: "malus", id: c.malus, shown: c.shown, min: c.min, max: c.max, decimals: 2, unit: c.unit, roll: c.roll },
  ];
  for (const stat of stats) {
    const name = headless.rivenStatNameEn(pools.rifle.find(s => s.id === stat.id));
    stat.text = `${stat.unit === "x" ? "x" : stat.shown < 0 ? "" : "+"}${stat.shown}${stat.unit === "%" ? "%" : ""} ${name}`;
  }
  const requests = [], opened = [];
  const meta = { weapons: [{ id: "torid", name: "Torid", riven_class: "rifle" }],
    benchmarks: [{ id: "test", name: "Test", primary: true }], riven_stats: pools };
  const answer = makeAnswer({ meta, zh, headless, host: { api: async (path, body) => {
    requests.push(body);
    return { ok: true, stats, illegal: [] };
  } } });
  const reply = await answer(c.text, { openAppraisal: async (job) => { opened.push(job); return { code: "TEST1" }; } });
  if (c.malus === "weapon_recoil") check("recoil malus prints its literal plus sign before the value and name", reply.text?.includes("+50% 武器后坐力"), reply.text);
  check("the reply prints engine values before stat names", reply.text?.includes("+100% 多重射击"), reply.text);
  check(`${c.malus}: the command creates one appraisal`, opened.length === 1, JSON.stringify(opened));
  check(`${c.malus}: the engine reads the shown values`, requests.length === 2 && requests[1].bonuses[0].value === 102.3
    && requests[1].bonuses[1].value === 151.5 && requests[1].malus.value === c.value, JSON.stringify(requests));
  check(`${c.malus}: the card keeps the engine's exact rolls`, opened[0]?.riven.bonuses[0].roll === stats[0].roll
    && opened[0]?.riven.malus.roll === c.roll, JSON.stringify(opened));
  await answer(c.text.replace("102.3", "200"), { openAppraisal: async (job) => { opened.push(job); return { code: "TEST2" }; } });
  check(`${c.malus}: an impossible printed value creates no appraisal`, opened.length === 1);
}

console.log(`${checked} checks, ${failed} failed${failed > 16 ? ` (${failed - 16} more failures)` : ""}`);
process.exit(failed ? 1 : 0);
