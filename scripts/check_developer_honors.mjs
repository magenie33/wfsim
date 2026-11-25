import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { runInNewContext } from "node:vm";
import { testDb } from "./pg/test_db.mjs";
import { contributorAccounts } from "../worker/contributor_identity.js";
import { contributorProfilesRoute } from "../worker/contributor_profiles.js";
import { communityProjectsRoute } from "../worker/community_projects.js";

const ACCOUNTS = await testDb("accounts"), env = { ACCOUNTS };
const profile = "a".repeat(32);
await ACCOUNTS.run("INSERT INTO entity_accounts(id,created_at,username,profile_id) VALUES ('recipient','2026-01-01','developer',?)", profile);
await ACCOUNTS.run("INSERT INTO relation_account_login_identities(provider,subject,account,label,linked_at) VALUES ('github','54000348','recipient','SuperExboom','2026-01-01')");
await ACCOUNTS.run("INSERT INTO state_account_public_attribution(account,named,label,chosen_at) VALUES ('recipient',1,'fixture','2026-01-01')");
const grant = readFileSync(new URL("../worker/pg/grant_simfleet_honors.sql", import.meta.url), "utf8");
await ACCOUNTS.exec(grant);
await ACCOUNTS.exec(grant);
const account = (await contributorAccounts(env, ["recipient"])).get("recipient");
assert.deepEqual(account.developer_honors, ["community_developer", "core_developer"]);
assert.equal(account.label, "fixture");
const route = async () => (await communityProjectsRoute(new Request("https://wfsim.app/api/community/projects"), env, "/api/community/projects")).json();
assert.equal((await route()).projects[0].profile, profile);
const path = "/api/contributors/profile/" + profile;
const response = await contributorProfilesRoute(new Request("https://wfsim.app" + path + "?view=identity"), env, path);
assert.deepEqual((await response.json()).profile.developer_honors, account.developer_honors);
await ACCOUNTS.run("UPDATE state_account_public_attribution SET named=0 WHERE account='recipient'");
assert.equal((await route()).projects[0].profile, undefined);
assert.equal((await contributorProfilesRoute(new Request("https://wfsim.app" + path + "?view=identity"), env, path)).status, 404);
const source = readFileSync(new URL("../web/src/static/app/17-account.js", import.meta.url), "utf8");
const badge = runInNewContext(source.slice(source.indexOf("function developerHonors("), source.indexOf("async function communityProjectsPage")) + "\ndeveloperHonors", { aT: text => text });
assert.match(badge(account.developer_honors), /Core Developer.*Community Developer/);
assert.equal(badge(["unrecognized"]), "");
const slot = { isConnected: true, innerHTML: "" }, main = { innerHTML: "", querySelector: () => slot };
const render = runInNewContext(source.slice(source.indexOf("function developerHonors(")) + "\ncommunityProjectsPage", {
  aT: text => text, tr: text => text, LANG: "en", escHtml: text => text,
  userProfileLink: (id, name) => id ? '<a href="/user/' + id + '">' + name + '</a>' : name,
  accountCall: async () => ({ ok: true, projects: [{ ...(await route()).projects[0], profile }] }),
});
await render(main);
assert.match(main.innerHTML, /Community Projects/);
assert.match(main.innerHTML, /Apply for project listing/);
assert.match(slot.innerHTML, /Verified by WFSim/);
assert.match(slot.innerHTML, new RegExp("/user/" + profile));
assert.match(slot.innerHTML, /github.com\/superexboom\/simfleet/);
console.log("developer honors: grant, coexistence, profile, project link, anonymity and badge rendering pass");
