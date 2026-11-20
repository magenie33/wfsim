import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createContext, runInContext } from "node:vm";

const file = new URL("../web/src/static/app/27-utility.js", import.meta.url);
const source = readFileSync(file, "utf8").split("/// THE STATE WHERE THE PAGE CANNOT SHOW IT")[0];
const tabClasses = new Set();
const tabs = { innerHTML: "", classList: { toggle: (value, enabled) => enabled ? tabClasses.add(value) : tabClasses.delete(value) } };
const page = { hidden: false };
const elements = { "utility-page": page, "utility-tabs": tabs };
const accountState = { account: null };
const pending = [];
const context = createContext({
  $: (id) => elements[id] ??= {},
  LANG: "zh",
  tr: (text) => text,
  escHtml: (text) => text.replaceAll("&", "&amp;").replaceAll("<", "&lt;"),
  reminderHits: () => [],
  renderBell: () => {},
  location: { search: "" },
  URLSearchParams,
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
  { id: "example", path: privateUrl, module: privateUrl + "/panel.js", names: { en: "Example tab", zh: "示例标签" } },
  { id: "invalid", path: privateUrl, module: "/api/cloud/other/panel.js", names: { en: "Invalid tab" } },
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
assert.match(tabs.innerHTML, /<div class="utility-shortcuts"><a class="ghost-btn" data-native href="\/api\/cloud\/example">示例/);
assert.ok(tabClasses.has("has-utility-shortcuts"));
assert.match(tabs.innerHTML, /class="mtab" href="\/utility\/fissures\?tool=example">示例标签/);
assert.doesNotMatch(tabs.innerHTML, /tool=invalid/);
assert.doesNotMatch(tabs.innerHTML, /outside\.test/);

accountState.account = null;
refresh();
assert.doesNotMatch(tabs.innerHTML, /api\/cloud\/example/);
assert.doesNotMatch(tabs.innerHTML, /tool=example/);
assert.ok(!tabClasses.has("has-utility-shortcuts"));
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
console.log("Utility uses same-origin account tabs and shortcuts, clears them on sign-out and discards another account's response.");
