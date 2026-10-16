/// THE NAME AN ACCOUNT GOES BY, ON THE PAGE (docs/ACCOUNTS.md §"Names").
///
/// The account API is faked inside the page; the rules themselves are
/// `check_accounts`'. What it holds: a born `user_` name reads as not chosen,
/// the profile form saves both names and the top bar shows the display name at
/// once, a refusal is said in the card, and inside the day after a change the
/// username field is shut while the display name still saves.
import { openApp } from "./cdp.mjs";

const app = await openApp({ boot: 12000, base: process.env.WFSIM_BASE });
const { evaluate, check } = app;

const r = await evaluate(`(async () => {
  const sleep = (ms) => new Promise(r => setTimeout(r, ms));
  const acct = { id: 'a1', created_at: '2026-09-01T00:00:00Z', username: 'user_k3x9q2', display_name: null,
    rename_after: null, identities: [{ provider: 'email', label: 'a@x' }] };
  const sent = [];
  let agents = [{ id: 'g1', name: 'Claude', claimed_at: '2026-09-20T00:00:00Z', last_used_at: '2026-09-29T00:00:00Z' }];
  const revoked = [];
  const realFetch = window.fetch;
  window.fetch = async (url, o = {}) => {
    const path = String(url);
    const reply = (j, status = 200) => new Response(JSON.stringify(j), { status, headers: { 'content-type': 'application/json' } });
    if (path === '/api/account') return reply({ ok: true, providers: ['email'], account: acct });
    if (path === '/api/account/profile') {
      const b = JSON.parse(o.body);
      sent.push(b);
      if (b.username === 'taken_one') return reply({ ok: false, reason: 'username_taken' }, 409);
      if ('username' in b && b.username !== acct.username) { acct.username = b.username.toLowerCase(); acct.rename_after = new Date(Date.now() + 864e5).toISOString(); }
      acct.display_name = b.display_name || null;
      return reply({ ok: true, account: acct });
    }
    if (path === '/api/account/agents') return reply({ ok: true, agents });
    if (path === '/api/account/agents/revoke') { const id = JSON.parse(o.body).id; revoked.push(id); agents = agents.filter((g) => g.id !== id); return reply({ ok: true }); }
    if (path.startsWith('/api/billing') || path.startsWith('/api/cloud')) return reply({ ok: true, configured: false });
    return realFetch(url, o);
  };
  history.pushState({}, '', '/account'); route(); await loadAccount(); await sleep(800);
  const out = {};
  const block = () => document.querySelector('#profile');
  out.born = block() && block().textContent.includes('@user_k3x9q2') && !!block().querySelector('.tag.muted');
  block().querySelector('[data-auth=open][data-open=profile]').click(); await sleep(300);
  document.getElementById('auth-username').value = 'taken_one';
  block().querySelector('[data-auth=profile]').click(); await sleep(500);
  out.refused = !!(block().querySelector('.auth-err') || {}).textContent;
  document.getElementById('auth-username').value = 'Tenno_Ada';
  document.getElementById('auth-display').value = '阿达';
  block().querySelector('[data-auth=profile]').click(); await sleep(800);
  out.shown = block().textContent.includes('@tenno_ada') && block().textContent.includes('阿达');
  out.topBar = (document.querySelector('#acct-menu .who b') || {}).textContent || '';
  out.handle = (document.querySelector('#acct-menu .who span:not(.avatar)') || {}).textContent || '';
  block().querySelector('[data-auth=open][data-open=profile]').click(); await sleep(300);
  out.shut = document.getElementById('auth-username').disabled === true;
  document.getElementById('auth-display').value = 'Ada';
  block().querySelector('[data-auth=profile]').click(); await sleep(800);
  out.lastSent = sent[sent.length - 1];
  out.topBar2 = (document.querySelector('#acct-menu .who b') || {}).textContent || '';
  const ag = () => document.querySelector('#agents');
  out.agentListed = !!ag() && ag().textContent.includes('Claude') && !!ag().querySelector('[data-auth=agent-revoke]');
  ag().querySelector('[data-auth=agent-revoke]').click(); await sleep(800);
  out.revoked = JSON.stringify(revoked);
  out.agentGone = !!ag() && !ag().textContent.includes('Claude') && !ag().querySelector('[data-auth=agent-revoke]');
  window.fetch = realFetch;
  return out;
})()`);

check("a born user_ name shows, marked as not chosen yet", r.born === true);
check("a refused name is said in the card", r.refused === true);
check("saving shows both names in the profile", r.shown === true);
check("...and the top bar shows the display name at once, with the handle under it",
  r.topBar === "阿达" && r.handle === "@tenno_ada", JSON.stringify([r.topBar, r.handle]));
check("inside the day after a change the username field is shut", r.shut === true);
check("...and the display name still saves, sending no username",
  r.lastSent && !("username" in r.lastSent) && r.lastSent.display_name === "Ada" && r.topBar2 === "Ada",
  JSON.stringify([r.lastSent, r.topBar2]));

check("the account page lists the agents acting for it", r.agentListed === true);
check("...and one click disconnects one", r.revoked === '["g1"]' && r.agentGone === true, JSON.stringify([r.revoked, r.agentGone]));

// THE ONE YES AN ACCOUNT NEEDS (17-account.js `authConsentBox`): signing up, by
// email or by a provider, waits for the box that agrees to the privacy policy
// and to the account being kept outside mainland China.
const s = JSON.parse(await evaluate(`(async () => {
  const sleep = (ms) => new Promise((ok) => setTimeout(ok, ms));
  const realFetch = window.fetch;
  const sent = [];
  window.fetch = async (url, o = {}) => {
    const path = String(url);
    const reply = (j) => new Response(JSON.stringify(j), { headers: { 'content-type': 'application/json' } });
    if (path === '/api/account') return reply({ ok: true, providers: ['email', 'google'], account: null });
    if (path.startsWith('/api/auth/')) { sent.push(path); return reply({ ok: true, outcome: 'code_sent' }); }
    return realFetch(url, o);
  };
  await loadAccount(); history.pushState({}, '', '/signup'); route(); await sleep(300);
  const page = document.getElementById('auth-page');
  const out = { box: !!page.querySelector('#auth-consent'), mentions: /mainland China|中国大陆/.test(page.textContent) };
  page.querySelector('#auth-email').value = 'a@example.com';
  page.querySelector('#auth-password').value = 'longenough1';
  page.querySelector('[data-auth="register"]').click(); await sleep(200);
  out.blocked = sent.length === 0 && /Tick the box|请先勾选/.test(page.textContent);
  if (!page.querySelector('#auth-consent')) { window.fetch = realFetch; return JSON.stringify(out); }
  const before = location.href;
  const prov = page.querySelector('a.prov[data-needs-consent]');
  out.provHeld = !!prov && (prov.click(), await sleep(100), location.href === before);
  page.querySelector('#auth-consent').checked = true;
  page.querySelector('#auth-email').value = 'a@example.com';
  page.querySelector('#auth-password').value = 'longenough1';
  page.querySelector('[data-auth="register"]').click(); await sleep(300);
  out.sent = sent.length > 0;
  window.fetch = realFetch;
  return JSON.stringify(out);
})()`));
check("signing up asks for one yes: the privacy policy, and the account kept outside mainland China", s.box && s.mentions, JSON.stringify(s));
check("...nothing is sent until it is ticked", s.blocked, JSON.stringify(s));
check("...nor does a provider's button go anywhere", s.provHeld, JSON.stringify(s));
check("...and ticked, the sign-up goes through", s.sent, JSON.stringify(s));

// MERIT NOTHING SPENDS YET SAYS SO, and the note goes once priority is offered.
const merit = JSON.parse(await evaluate(`(() => {
  const base = { ok: true, on: true, balance: 1200, cap: 160000, minted_today: 200, daily: 16000, auto_priority: false };
  meritState = { ...base, priority: false }; const off = meritHtml();
  meritState = { ...base, priority: true }; const on = meritHtml();
  meritState = null;
  const note = aT("Nothing spends merit yet: it is kept for you until something does.");
  return JSON.stringify({ off: off.includes(note), on: on.includes(note) });
})()`));
check("merit says nothing spends it yet, until priority is offered", merit.off && !merit.on, JSON.stringify(merit));

await app.finish("an account's name is set on the page and shown where the account is");
