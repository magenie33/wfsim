import { openApp } from "./cdp.mjs";

const app = await openApp({ lang: "zh" });
const { evaluate, check, send, sleep } = app;
const route = "/utility/dictionary";
const entries = `Array.from(document.querySelectorAll("[data-dictionary-entry]"), (card) => ({
  id: card.dataset.dictionaryEntry,
  title: card.querySelector("h2 [lang]").textContent,
  names: Array.from(card.querySelectorAll("dd [lang]"), (el) => el.textContent),
}))`;

async function lookup(text, browser = app) {
  return browser.evaluate(`(() => {
    const input = document.getElementById("dictionary-search");
    input.value = ${JSON.stringify(text)};
    input.dispatchEvent(new Event("input", { bubbles: true }));
    return ${entries};
  })()`);
}

async function waitForCards(browser = app) {
  return browser.evaluate(`(async () => {
    for (let i = 0; i < 100; i += 1) {
      if (document.querySelector("[data-dictionary-entry]")) return ${entries};
      if (!document.getElementById("dictionary-retry").hidden) break;
      await new Promise((resolve) => setTimeout(resolve, 100));
    }
    return [];
  })()`);
}

check("home boot does not download the dictionary", await evaluate(`
  !performance.getEntriesByType("resource").some((r) => r.name.includes("warframe-dictionary"))`));
await app.load(route);
await lookup("  ARCANE ENERGIZE  ");
const exact = await waitForCards();
const [first] = exact;
check("the Dictionary route has a Chinese heading", await evaluate(`
  !document.getElementById("utility-dictionary").hidden
    && document.getElementById("h-utility").textContent === "词典"`));
check("English search displays one entity with its Chinese heading and every other name",
  exact.length === 1 && first.title === "赋能·充沛" && first.names.length === 14
    && first.names.includes("Arcane Energize") && first.names.includes("充沛賦能"), JSON.stringify(first));

const reverse = await lookup("充沛賦能");
check("a different official name finds exactly the same entity",
  reverse.length === 1 && !!first && reverse[0].id === first.id);

await evaluate(`document.getElementById("dictionary-search").select()`);
await send("Input.imeSetComposition", { text: "chong", selectionStart: 5, selectionEnd: 5 });
check("results wait while the input method composes", (await evaluate(entries))[0]?.title === "赋能·充沛");
await send("Input.insertText", { text: "能量枢纽" });
await sleep(100);
check("committed Chinese input finds the English mod", (await evaluate(entries))[0]?.names.includes("Energy Nexus"));

let found = await lookup("WTB [Wisp Prime Neuroptics Blueprint] and [Lith V8 Relic] 10p each");
check("pasted links display separate cards for blueprints and relics",
  found.some((card) => card.title === "Wisp Prime 头部神经光元 蓝图")
    && found.some((card) => card.title === "古纪 V8 遗物"), JSON.stringify(found));
found = await lookup("WTB arcane energize 20p");
check("unbracketed chat finds the complete name", found[0]?.title === "赋能·充沛", JSON.stringify(found));

found = await lookup("Apollodorus");
check("star chart nodes are searchable", found.some((card) => card.title === "Apollodorus"), JSON.stringify(found));
found = await lookup("qzxvwunmatchedstring");
check("unknown input has an empty result", !found.length && await evaluate(`
  document.getElementById("dictionary-status").textContent === "没有找到匹配的游戏名称。"`));

await send("Emulation.setDeviceMetricsOverride", { width: 360, height: 760, deviceScaleFactor: 1, mobile: true });
await lookup("Corrupted Heavy Gunner");
check("the full card fits a narrow phone", await evaluate(`
  document.documentElement.scrollWidth <= window.innerWidth
    && document.querySelector("[data-dictionary-share]")?.getBoundingClientRect().right <= window.innerWidth
    && document.querySelectorAll(".dictionary-translations dt").length === 14`));

const copied = await evaluate(`(async () => {
  let text;
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText(value) { text = value; } } });
  document.querySelector("[data-dictionary-copy] [lang=zh-Hant]")?.click();
  return text;
})()`);
check("any displayed name can be copied", copied === "墮落重型機槍兵", JSON.stringify(copied));

const carrierCards = await lookup("Carrier");
const [carrier] = carrierCards;
const shared = await evaluate(`(async () => {
  let url;
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText(value) { url = value; } } });
  document.querySelector("[data-dictionary-share]")?.click();
  return url;
})()`);
check("Copy link identifies the entity even when the input is ambiguous",
  carrierCards.length > 1 && shared === app.BASE + route + "?entry=" + carrier.id, shared);

if (shared) {
  const recipient = await openApp({ base: app.BASE, lang: "en" });
  try {
    const url = new URL(shared);
    await recipient.load(url.pathname + url.search);
    const linked = await waitForCards(recipient);
    check("the same share link opens one card in the recipient's own language",
      linked.length === 1 && linked[0].id === carrier.id && linked[0].title === "Carrier"
        && linked[0].names.includes(carrier.title), JSON.stringify(linked));
    await lookup("Arcane Energize", recipient);
    check("editing the search leaves the linked entry",
      await recipient.evaluate(`!new URLSearchParams(location.search).has("entry")`));
  } finally {
    await recipient.close();
  }
}
await app.finish("the Dictionary shows one shareable card per entity");
