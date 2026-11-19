import { openApp } from "./cdp.mjs";

const app = await openApp({ lang: "zh" });
const { evaluate, check, send, sleep } = app;
const names = 'Array.from(document.querySelectorAll("[data-dictionary-name]"), (el) => el.textContent)';
const route = "/utility/dictionary";

check("home boot does not download the dictionary", await evaluate(`
  !performance.getEntriesByType("resource").some((r) => r.name.includes("warframe-dictionary"))`));
await app.load(route);
const loaded = await evaluate(`(async () => {
  for (let i = 0; i < 100; i += 1) {
    if (!document.getElementById("dictionary-language").disabled) return true;
    if (!document.getElementById("dictionary-retry").hidden) break;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  return false;
})()`);
check("the Dictionary route loads its 15 languages", loaded && await evaluate(`
  !document.getElementById("utility-dictionary").hidden
    && document.getElementById("h-utility").textContent === "词典"
    && document.getElementById("dictionary-language").options.length === 15`));

async function lookup(text) {
  return evaluate(`(() => {
    const input = document.getElementById("dictionary-search");
    input.value = ${JSON.stringify(text)};
    input.dispatchEvent(new Event("input", { bubbles: true }));
    return ${names};
  })()`);
}

let found = await lookup("  ARCANE ENERGIZE  ");
check("English names show the game's Simplified Chinese", found[0] === "赋能·充沛", JSON.stringify(found));

await evaluate(`document.getElementById("dictionary-search").select()`);
await send("Input.imeSetComposition", { text: "chong", selectionStart: 5, selectionEnd: 5 });
check("results wait while the input method composes", (await evaluate(names))[0] === "赋能·充沛");
await send("Input.insertText", { text: "能量枢纽" });
await sleep(100);
check("committed Chinese input finds the English mod", await evaluate(`
  document.querySelector("#dictionary-results [lang=en]")?.textContent === "Energy Nexus"`));

found = await lookup("WTB [Wisp Prime Neuroptics Blueprint] and [Lith V8 Relic] 10p each");
check("pasted links resolve both blueprints and relics",
  found.includes("Wisp Prime 头部神经光元 蓝图") && found.includes("古纪 V8 遗物"), JSON.stringify(found));
found = await lookup("WTB arcane energize 20p");
check("unbracketed chat finds the complete name", found[0] === "赋能·充沛", JSON.stringify(found));

found = await evaluate(`(() => {
  const language = document.getElementById("dictionary-language");
  language.value = "tc";
  language.dispatchEvent(new Event("change"));
  return ${names};
})()`);
check("switching output languages keeps the lookup", found[0] === "充沛賦能", JSON.stringify(found));
found = await lookup("充沛賦能");
check("the selected output language is searchable too", found[0] === "充沛賦能", JSON.stringify(found));

found = await lookup("Apollodorus");
check("star chart nodes are searchable", found.includes("Apollodorus"), JSON.stringify(found));
found = await lookup("qzxvwunmatchedstring");
check("unknown input has an empty result", !found.length && await evaluate(`
  document.getElementById("dictionary-status").textContent === "没有找到匹配的游戏名称。"`));
await send("Emulation.setDeviceMetricsOverride", { width: 360, height: 760, deviceScaleFactor: 1, mobile: true });
await lookup("Corrupted Heavy Gunner");
check("the result and copy button fit a narrow phone", await evaluate(`
  document.documentElement.scrollWidth <= window.innerWidth
    && document.querySelector("[data-dictionary-copy]").getBoundingClientRect().right <= window.innerWidth`));

const copied = await evaluate(`(async () => {
  let text;
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText(value) { text = value; } } });
  document.querySelector("[data-dictionary-copy]").click();
  return text;
})()`);
check("Copy uses the selected client language", copied === "墮落重型機槍兵", JSON.stringify(copied));
await app.finish("the Dictionary searches official localized names");
