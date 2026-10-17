// A riven typed for `fx` reads every way a player writes a line, and says which
// side it is on the way the card does. No browser, no data: a fixture pool and
// locale, through the bot's own reader and the page's own query.
//
//   node scripts/check_fx_lines.mjs
import { readRivenLines } from "../bot/answer.mjs";
import { rivenQuery } from "../mcp/headless.js";

const pool = [
  { id: "multishot", text: "|val|% Multishot", base: 0.01 },
  { id: "critical_damage", text: "|val|% Critical Damage", base: 0.01 },
  { id: "zoom", text: "|val|% Zoom", base: 0.01 },
  { id: "weapon_recoil", text: "|val|% Weapon Recoil", base: -0.01 },
];
const zh = {
  riven_words: { 多重射击: ["multishot"], 多重: ["multishot"], 暴伤: ["critical_damage"], 变焦: ["zoom"], 后坐力: ["weapon_recoil"] },
  riven_query_words: { malus: ["负", "-"] },
};
const squash = (s) => s.normalize("NFKC").toLowerCase().replace(/\s+/g, "");
const read = (s) => readRivenLines(squash(s), pool, [zh], rivenQuery);
const line = (s) => { const r = read(s); return r.lines ? r.lines.map((x) => `${x.malus ? "-" : "+"}${x.id}:${x.value}`).join(" ") : `!${r.error}`; };

let failed = 0;
const want = (input, expected) => {
  const got = line(input);
  const ok = got === expected;
  if (!ok) failed += 1;
  console.log(`${ok ? "  ok  " : "  FAIL"}  ${input.padEnd(28)} ${got}${ok ? "" : `  (wanted ${expected})`}`);
};

// A bonus, however it is written.
for (const s of ["多重射击50", "多重射击+50", "正多重射击50", "多重射击 50", "多重射击 +50", "+多重射击50"]) want(s, "+multishot:50");
// A malus, by its word or by its sign against the stat's direction.
for (const s of ["变焦-50", "负变焦50", "负变焦-50", "-变焦50", "变焦 - 50"]) want(s, "-zoom:50");
// RECOIL IS BETTER DOWN: its minus is the bonus and its plus the malus.
want("后坐力-50", "+weapon_recoil:50");
want("后坐力+50", "-weapon_recoil:50");
want("负后坐力50", "-weapon_recoil:50");
// A whole card, spaced or not.
want("暴伤160.9 多重120.7 变焦-50", "+critical_damage:160.9 +multishot:120.7 -zoom:50");
want("暴伤160.9%多重+120.7%负变焦50%", "+critical_damage:160.9 +multishot:120.7 -zoom:50");
// …AND WHAT IS TOO FAR OFF IS SAID, never guessed.
want("负变焦+50", "!contradicts");
want("正后坐力+50", "!contradicts");
want("多重射击", "!format");
want("多重射击50 暴伤", "!format");
want("飞天50", "!unread");

console.log(failed ? `\n${failed} failed` : "\nfx reads a riven every way it is written");
process.exit(failed ? 1 : 0);
