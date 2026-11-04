import assert from "node:assert/strict";
const { websiteRequest } = await import(process.env.WFSIM_WEBSITE_PROXY_MODULE || "../worker/website_proxy.js");
import { sameSite } from "../worker/accounts.js";
import { clientIp, viaRelay } from "../worker/verify.js";

const env = { WEBSITE_PROXY_SECRET: "website-test-secret", RELAY_SECRET: "relay-test-secret" };
const request = (extra = {}) => new Request("https://wfsim.example.workers.dev/api/recommendations?day=1", {
  method: "POST", body: '{"action":"recommend"}', headers: {
    "content-type": "application/json", origin: "https://wfsim.app", cookie: "wfsim_session=test",
    "x-wfsim-website": env.WEBSITE_PROXY_SECRET, "x-wfsim-website-ip": "203.0.113.9",
    "cf-connecting-ip": "198.51.100.2", "cf-ipcountry": "US", ...extra,
  },
});
const direct = new Request("https://wfsim.app/api/account");
assert.equal(websiteRequest(direct, env), direct);
assert.equal(websiteRequest(request(), {}).status, 403);
assert.equal(websiteRequest(request({ "x-wfsim-website": "forged" }), env).status, 403);
assert.equal(websiteRequest(request({ "x-wfsim-website-ip": "203.0.113.9, 1.1.1.1" }), env).status, 400);
const restored = websiteRequest(request(), env);
assert.equal(restored.url, "https://wfsim.app/api/recommendations?day=1");
assert.equal(restored.method, "POST");
assert.equal(await restored.text(), '{"action":"recommend"}');
assert.equal(restored.headers.get("cookie"), "wfsim_session=test");
assert(sameSite(restored));
assert(!sameSite(websiteRequest(request({ origin: "https://foreign.example" }), env)));
assert.equal(clientIp(restored, env), "203.0.113.9");
assert.equal(restored.cf.country, "XX");
assert.equal(restored.headers.get("cf-ipcountry"), "XX");
assert(!restored.headers.has("x-wfsim-website"));
const relay = websiteRequest(request({ "x-wfsim-relay": env.RELAY_SECRET,
  "x-wfsim-client-ip": "192.0.2.8", "x-wfsim-website-ip": "2001:db8::9" }), env);
assert.equal(clientIp(relay, env), "2001:db8::9");
assert(!viaRelay(relay, env));
console.log("website proxy: origin, cookies, body, client identity and forged headers checked");
