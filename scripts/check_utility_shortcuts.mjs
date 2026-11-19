import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createContext, runInContext } from "node:vm";

const file = new URL("../web/src/static/app/27-utility.js", import.meta.url);
const source = readFileSync(file, "utf8").split("/// THE STATE WHERE THE PAGE CANNOT SHOW IT")[0];
const tabs = { innerHTML: "" };
const page = { hidden: false };
const accountState = { account: null };
const pending = [];
const context = createContext({
  $: (id) => id === "utility-tabs" ? tabs : page,
  LANG: "zh",
  tr: (text) => text,
  escHtml: (text) => text.replaceAll("&", "&amp;").replaceAll("<", "&lt;"),
  reminderHits: () => [],
  accountState,
  accountCall: (method, path) => {
    assert.equal(method, "GET");
    assert.equal(path, "/api/cloud/tools");
    return new Promise((resolve) => pending.push(resolve));
  },
});
runInContext(source + '\nutilityTab = "fissures"; renderUtilityTabs();', context);
const refresh = () => runInContext("refreshUtilityShortcuts()", context);
const privateUrl = "/api/cloud/example";
const answer = { ok: true, tools: [
  { path: privateUrl, names: { en: "Example", zh: "示例" } },
  { path: "//outside.test", names: { en: "Outside" } },
] };

refresh();
assert.equal(pending.length, 0);
accountState.account = { id: "owner" };
refresh();
refresh();
assert.equal(pending.length, 1);
pending[0](answer);
await Promise.resolve();
assert.match(tabs.innerHTML, /data-native href="\/api\/cloud\/example">示例/);
assert.doesNotMatch(tabs.innerHTML, /outside\.test/);

accountState.account = null;
refresh();
assert.doesNotMatch(tabs.innerHTML, /api\/cloud\/example/);
accountState.account = { id: "owner" };
refresh();
accountState.account = { id: "another-account" };
refresh();
pending[1](answer);
pending[2]({ ok: true, tools: [] });
await Promise.resolve();
assert.doesNotMatch(tabs.innerHTML, /api\/cloud\/example/);

accountState.account = null;
page.hidden = true;
refresh();
accountState.account = { id: "owner" };
refresh();
assert.equal(pending.length, 3);
console.log("Utility shortcuts use same-origin native links, clear on sign-out and discard another account's response.");
