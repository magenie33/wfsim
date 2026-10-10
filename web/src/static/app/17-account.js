// ---- ACCOUNTS ON THE PAGE -----------------------------------------------------------
//
// docs/ACCOUNTS.md. An account is optional and changes nothing a reader without
// one can do. Nothing here draws unless `/api/account` names a way in — the
// site with its secrets set — so the dev server and the desktop shell show no
// account control at all.
//
// THE SHAPE A SERIOUS PRODUCT HAS (docs/ACCOUNTS.md §"On the page"): one entry
// in the top bar — "Sign in", or an avatar with a short menu — and pages of
// their own for everything else: /login, /signup, /reset, /account. Mail goes
// out only to prove an address; signing in with a password sends nothing.

const ACCOUNT_SLOTS = [
  { id: "google", name: "Google" },
  { id: "discord", name: "Discord" },
  { id: "github", name: "GitHub" },
  { id: "email", name: "Email" },
];
const AUTH_PATHS = { "/login": "login", "/signup": "signup", "/reset": "reset", "/account": "account",
  "/account/sync": "sync", "/contributors": "contributors", "/compute": "compute" };
/// …and the pages an extension mounts (`EXT.pages`), routed the same way.
const authKindOf = (path) => AUTH_PATHS[path.replace(/\/$/, "")] || extKindOf(path.replace(/\/$/, ""));
/// EACH PAGE ITS OWN `app.view` KIND, so the way in can be read step by step;
/// the settings pages carry `account_`, since `sync` alone could be any sync.
const AUTH_VIEWS = { login: "login", signup: "signup", reset: "reset",
  account: "account", sync: "account_sync", contributors: "contributors", compute: "compute" };
const authView = (kind) => AUTH_VIEWS[kind] || (EXT.pages[kind] || {}).view || "other";
/// The pages of a signed-in account; every other kind is a way in.
const isSettings = (kind) => kind === "account" || kind === "sync" || !!(EXT.pages[kind] || {}).settings;

/// WHAT THE SERVER SAID, in the reader's words — an outcome or a refusal.
const ACCOUNT_SAYS = {
  created: "Account created.",
  signed_in: "Signed in.",
  linked: "Connected.",
  replaced: "Replaced.",
  password_reset: "Password reset. You are signed in, and signed out everywhere else.",
  password_changed: "Password changed. Other devices are signed out.",
  taken: "That sign-in already belongs to another WFSim account. Sign in with it and disconnect it there first.",
  email_taken: "That email already has an account. Sign in, or reset its password.",
  not_signed_in: "Sign in first.",
  state: "The sign-in expired. Try again.",
  cancelled: "Sign-in was cancelled.",
  provider: "The provider did not answer. Try again.",
  unavailable: "This way to sign in is not set up yet.",
  bad_email: "That is not an email address.",
  bad_password: "A password needs at least 8 characters.",
  wrong_credentials: "The email or the password is not right.",
  wrong_password: "The current password is not right.",
  locked: "Too many wrong passwords. Wait 15 minutes, or reset the password.",
  too_soon: "A code was just sent. Wait a minute before asking again.",
  send_failed: "The mail could not be sent. Try again later.",
  rate_limited: "Too many tries. Wait a minute.",
  bad_code: "The code is six digits.",
  expired: "That code has expired. Ask for a new one.",
  wrong_code: "That code is not right.",
  too_many_attempts: "Too many wrong codes. Ask for a new one.",
  last_slot: "This is your last way to sign in. Disconnecting it deletes the account — do that below if you mean to.",
  bad_username: "A username is 3 to 20 letters, digits or underscores.",
  username_reserved: "That username is reserved.",
  username_taken: "That username is taken.",
  rename_too_soon: "A username can change once a day.",
  bad_display_name: "A display name is at most 32 characters.",
  name_not_allowed: "That name cannot be used. A name cannot carry a link, contact details or trading.",
  needs_consent: "Tick the box above to agree first.",
};
const accountSaid = (key) => tr(ACCOUNT_SAYS[key] || key);

// The brands' own marks, drawn inline so nothing is fetched from their hosts.
const ACCOUNT_ICONS = {
  google: `<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="#4285F4" d="M22.5 12.3c0-.8-.1-1.5-.2-2.2H12v4.2h5.9a5 5 0 0 1-2.2 3.3v2.7h3.6c2.1-1.9 3.2-4.8 3.2-8z"/><path fill="#34A853" d="M12 23c3 0 5.5-1 7.3-2.7l-3.6-2.7c-1 .7-2.2 1.1-3.7 1.1-2.9 0-5.3-1.9-6.2-4.5H2.1v2.8A11 11 0 0 0 12 23z"/><path fill="#FBBC05" d="M5.8 14.2a6.6 6.6 0 0 1 0-4.3V7.1H2.1a11 11 0 0 0 0 9.9l3.7-2.8z"/><path fill="#EA4335" d="M12 5.4c1.6 0 3.1.6 4.2 1.7l3.2-3.2A11 11 0 0 0 2.1 7.1l3.7 2.8C6.7 7.3 9.1 5.4 12 5.4z"/></svg>`,
  discord: `<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="#5865F2" d="M20.3 4.4A19.8 19.8 0 0 0 15.4 3l-.6 1.3a18.4 18.4 0 0 0-5.6 0L8.6 3a19.7 19.7 0 0 0-4.9 1.5C.6 9.2-.3 13.8.1 18.3a19.9 19.9 0 0 0 6 3l1.3-2.1c-.7-.3-1.4-.6-2-1l.5-.4a14.2 14.2 0 0 0 12.2 0l.5.4c-.6.4-1.3.7-2 1l1.3 2.1a19.8 19.8 0 0 0 6-3c.5-5.2-.8-9.7-3.6-13.9zM8 15.5c-1.2 0-2.2-1.1-2.2-2.4S6.8 10.6 8 10.6s2.2 1.1 2.2 2.5-1 2.4-2.2 2.4zm8 0c-1.2 0-2.2-1.1-2.2-2.4s1-2.5 2.2-2.5 2.2 1.1 2.2 2.5-1 2.4-2.2 2.4z"/></svg>`,
  github: `<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="M12 .5a11.5 11.5 0 0 0-3.6 22.4c.6.1.8-.3.8-.6v-2.2c-3.2.7-3.9-1.4-3.9-1.4-.5-1.3-1.3-1.7-1.3-1.7-1-.7.1-.7.1-.7 1.2.1 1.8 1.2 1.8 1.2 1 1.8 2.8 1.3 3.4 1 .1-.8.4-1.3.8-1.6-2.6-.3-5.3-1.3-5.3-5.7 0-1.3.5-2.3 1.2-3.1-.1-.3-.5-1.5.1-3.1 0 0 1-.3 3.2 1.2a11 11 0 0 1 5.8 0c2.2-1.5 3.2-1.2 3.2-1.2.6 1.6.2 2.8.1 3.1.8.8 1.2 1.9 1.2 3.1 0 4.4-2.7 5.4-5.3 5.7.4.4.8 1.1.8 2.2v3.3c0 .3.2.7.8.6A11.5 11.5 0 0 0 12 .5z"/></svg>`,
  email: `<svg viewBox="0 0 24 24" aria-hidden="true"><path fill="none" stroke="currentColor" stroke-width="1.8" d="M3 6h18v12H3zM3 7l9 6 9-6"/></svg>`,
};

/// WHERE A TWO-STEP PAGE IS: its step, the address a code went to and when,
/// and the error to show in the card. One at a time; leaving a page resets it.
let authFlow = { kind: null, step: 1, email: "", sentAt: 0, error: null, open: null };
let authTimer = null;

let accountState = { providers: [], account: null, loaded: false };
let accountLoading = null;
/// THE AGENTS ACTING FOR THIS ACCOUNT, as `/api/account/agents` last said.
let agentsState = [];
/// THIS ACCOUNT'S DEVICES AND THEIR POINTS, as `/api/account/devices` last
/// said; and the public ranking, as `/api/contributors` did.
let devicesState = null;
let contributorsState = null;

async function accountCall(method, path, body) {
  try {
    const r = await fetch(path, {
      method, credentials: "same-origin",
      headers: body ? { "content-type": "application/json" } : {},
      ...(body ? { body: JSON.stringify(body) } : {}),
    });
    return await r.json();
  } catch (_) {
    return null;
  }
}

function loadAccount() {
  accountLoading = (async () => {
    if (!extSettled) await extReady;
    const r = await accountCall("GET", "/api/account");
    const was = `${!!accountState.account}:${accountState.providers.length > 0}`;
    accountState = r && r.ok
      ? { providers: r.providers || [], account: r.account, loaded: true }
      : { providers: [], account: null, loaded: true };
    // THE CLOUD ON A CHIP is drawn from who is signed in, so a bar on screen
    // drawn before this answer is drawn again. Only one on screen: a bar whose
    // page is not open has nothing to draw from.
    if (META && was !== `${!!accountState.account}:${accountState.providers.length > 0}`) {
      for (const { domain } of COLLECTIONS) {
        const doc = presetDoc(domain), el = $(domain === RIVENS ? "riven-all" : "preset-bar-" + domain);
        if (doc && el && el.offsetParent) doc.rerender();
      }
    }
    const at = authKindOf(location.pathname);
    // EVERY SETTINGS PAGE ASKS EVERY EXTENSION PAGE: the navigation shows a
    // page only once it says it is available.
    if (accountState.account && isSettings(at)) await Promise.all([loadAgents(), loadDevices(), ...extLoads()]);
    else if ((EXT.pages[at] || {}).open) await Promise.all(extLoads(at));
    renderAccountEntry();
    const kind = authKindOf(location.pathname);
    if (kind) renderAuthPage(kind);
    if (accountState.account) syncSoon(0);
  })();
  return accountLoading;
}

async function loadDevices() {
  const r = await accountCall("GET", "/api/account/devices");
  devicesState = r && r.ok ? r : null;
}

async function loadAgents() {
  const r = await accountCall("GET", "/api/account/agents");
  agentsState = (r && r.ok && r.agents) || [];
}

/// A path on this site to go back to after signing in, or `/` — never another
/// sign-in page, which would send a signed-in reader round in a circle.
function authReturn() {
  const r = new URLSearchParams(location.search).get("return");
  const kind = r && authKindOf(r.split("?")[0]);
  return r && /^\/(?!\/)/.test(r) && (!kind || isSettings(kind)) ? r : "/";
}
const authStart = (p, intent, back) =>
  `/api/auth/${p}/start?intent=${intent}&return=${encodeURIComponent(back)}`;
/// WHAT THE SITE CALLS AN ACCOUNT: its display name, or its username where
/// there is none or the server hides it (worker/names.js `shownName`).
const accountName = (a) => (a && ((!a.display_name_hidden && a.display_name) || a.username)) || "WFSim";
const accountInitial = (a) => (accountName(a).replace(/[^\p{L}\p{N}]/gu, "")[0] || "W").toUpperCase();
const aT = (s) => escHtml(tr(s));
const accountLocale = () => (LANG === "zh" ? "zh-CN" : "en-US");
/// What each extension page needs before it is drawn — one kind's, or all.
const extLoads = (kind) => Object.entries(EXT.pages)
  .filter(([k, p]) => p.load && (!kind || k === kind)).map(([, p]) => p.load().catch(() => {}));

// ---- the top bar: one entry --------------------------------------------------------------

function renderAccountEntry() {
  const box = $("account");
  if (!box) return;
  const { providers, account } = accountState;
  box.hidden = !providers.length && !account;
  if (box.hidden) return;
  const here = location.pathname + location.search;
  box.innerHTML = account
    ? `<button class="avatar${account.display_name_hidden ? " noted" : ""}" id="account-toggle" aria-haspopup="menu" aria-expanded="false" title="${aT("Account")}">${escHtml(accountInitial(account))}</button>
      <div class="acct-menu" id="acct-menu" role="menu" hidden>
        <div class="who"><span class="avatar">${escHtml(accountInitial(account))}</span><div><b>${escHtml(accountName(account))}</b>
          <span>@${escHtml(account.username || "")}</span></div></div>
        <a href="/account" role="menuitem">${aT("Account settings")}</a>
        <a href="/account/sync" role="menuitem">${aT("Cloud sync")}</a>
        ${Object.values(EXT.pages).filter((p) => p.menu).map((p) => `<a href="${escHtml(p.path)}?from=menu" role="menuitem">${aT(p.menu)}</a>`).join("")}
        <hr><a href="#" role="menuitem" data-acct="logout">${aT("Sign out")}</a>
      </div>`
    // SIGNED OUT, an extension's page anyone may open stands beside "Sign in",
    // a plain link at the bar's weight: the menu that names it is not there yet.
    : `${Object.values(EXT.pages).filter((p) => p.menu && p.open).map((p) =>
        `<a class="topbar-page" href="${escHtml(p.path)}?from=topbar">${aT(p.menu)}</a>`).join("")}`
      + `<a class="signin-btn" href="/login?return=${encodeURIComponent(authKindOf(location.pathname) ? "/" : here)}">${aT("Sign in")}</a>`;
}

(function () {
  const box = $("account");
  if (!box) return;
  const menu = (open) => {
    const m = $("acct-menu"), b = $("account-toggle");
    if (!m || !b) return;
    m.hidden = !open;
    b.setAttribute("aria-expanded", open ? "true" : "false");
  };
  box.addEventListener("click", async (e) => {
    if (e.target.closest("#account-toggle")) { e.stopPropagation(); menu($("acct-menu").hidden); return; }
    if (e.target.closest("[data-acct=logout]")) {
      e.preventDefault();
      menu(false);
      // Off the settings page first: redrawn signed out, it would send the
      // reader to sign in again.
      const leave = authKindOf(location.pathname) === "account";
      await accountCall("POST", "/api/auth/logout", {});
      presetToast(tr("Signed out."));
      accountState = { ...accountState, account: null };
      if (leave) nav("/");
      await loadAccount();
      return;
    }
    if (e.target.closest(".acct-menu a[href]")) menu(false);
  });
  document.addEventListener("click", (e) => { if (!e.target.closest("#account")) menu(false); });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && !imeComposing(e)) menu(false); });
  // AN OAUTH ROUND TRIP COMES BACK WITH ITS OUTCOME ON THE ADDRESS, said once
  // as a toast — or, on a sign-in page, in the card — and taken off.
  const q = new URLSearchParams(location.search);
  const outcome = q.get("auth"), refused = q.get("auth_error");
  if (outcome || refused) {
    if (authKindOf(location.pathname) && refused) authFlow.error = refused;
    else presetToast(accountSaid(outcome || refused));
    q.delete("auth");
    q.delete("auth_error");
    history.replaceState(null, "", location.pathname + (q.toString() ? `?${q}` : ""));
  }
  loadAccount();
})();

// ---- the pages -------------------------------------------------------------------------


function authField(id, label, type, auto, extra = "") {
  return `<div class="field"><label for="${id}">${aT(label)}${extra}</label>
    <input id="${id}" type="${type}" autocomplete="${auto}"${type === "password" ? ' minlength="8" maxlength="128"' : ""}></div>`;
}
const authError = () => (authFlow.error
  ? `<div class="auth-err" role="alert">${escHtml(accountSaid(authFlow.error))}</div>` : "");
const authVal = (id) => (($(id) || {}).value || "").trim();

function authProviders(verb, back, needsConsent = false) {
  const on = ACCOUNT_SLOTS.filter((s) => s.id !== "email" && accountState.providers.includes(s.id));
  if (!on.length) return "";
  const or = accountState.providers.includes("email") ? `<div class="or">${aT("or with email")}</div>` : "";
  return on.map((s) => `<a class="prov" data-native${needsConsent ? " data-needs-consent" : ""} href="${authStart(s.id, "login", `/login?return=${encodeURIComponent(back)}`)}">${
    ACCOUNT_ICONS[s.id]}${escHtml(tr(verb).replace("{p}", s.name))}</a>`).join("") + or;
}

/// SIX BOXES FOR A SIX-DIGIT CODE: typing moves on, backspace moves back, a
/// pasted code fills them all, and the last digit submits.
function authCodeStep(title, button, extra = "") {
  const left = Math.max(0, 60 - Math.floor((Date.now() - authFlow.sentAt) / 1000));
  return `<div class="mail-ico">${ACCOUNT_ICONS.email}</div>
    <h2>${aT(title)}</h2>
    <p class="lede">${escHtml(tr("We sent a 6-digit code to {email}. It works for 10 minutes.")).replace("{email}", `<b>${escHtml(authFlow.email)}</b>`)}</p>
    ${authError()}
    <div class="code-boxes">${Array.from({ length: 6 }, (_, i) =>
      `<input data-code="${i}" inputmode="numeric" maxlength="1" autocomplete="${i ? "off" : "one-time-code"}" aria-label="${aT("Digit")} ${i + 1}">`).join("")}</div>
    ${extra}
    <button class="run-btn" data-auth="verify">${aT(button)}</button>
    <div class="auth-foot">${aT("Didn't get it?")} ${left
      ? `<span class="muted">${aT("Resend")} (0:${String(left).padStart(2, "0")})</span>`
      : `<a href="#" data-auth="resend">${aT("Resend")}</a>`} · <a href="#" data-auth="back">${aT("Use another email")}</a></div>
    <div class="fine">${aT("Check the spam folder too.")}</div>`;
}
const authCode = () => [...document.querySelectorAll("[data-code]")].map((i) => i.value).join("");

function authLoginCard() {
  const back = authReturn();
  return `<h2>${aT("Sign in to WFSim")}</h2>
    <p class="lede">${aT("An account is optional. WFSim works the same without one.")}</p>
    ${authError()}${authProviders("Continue with {p}", back)}
    ${accountState.providers.includes("email") ? `${authField("auth-email", "Email", "email", "email")}
    ${authField("auth-password", "Password", "password", "current-password",
      ` <a href="/reset?return=${encodeURIComponent(back)}">${aT("Forgot password?")}</a>`)}
    <button class="run-btn" data-auth="login">${aT("Sign in")}</button>` : ""}
    <div class="auth-foot">${aT("New to WFSim?")} <a href="/signup?return=${encodeURIComponent(back)}">${aT("Create an account")}</a></div>
    <div class="fine">${aT("By continuing you have read the")} <a data-native href="/privacy">${aT("privacy policy")}</a>${
      aT(", and agree that a new account is kept outside mainland China.")}</div>`;
}

/// THE ONE YES AN ACCOUNT NEEDS: the privacy policy, and that the account is kept
/// by Cloudflare outside mainland China — a transfer abroad that the law asks to
/// be agreed to on its own. Nothing creates the account until it is ticked.
const authConsentBox = () => `<label class="fine" style="display:flex;gap:8px;align-items:flex-start;text-align:left">
    <input type="checkbox" id="auth-consent"${authFlow.consent ? " checked" : ""}>
    <span>${aT("I have read the")} <a data-native href="/privacy">${aT("privacy policy")}</a>${
      aT(" and agree that my account is kept by Cloudflare outside mainland China.")}</span></label>`;

function authSignupCard() {
  const back = authReturn();
  if (authFlow.step === 2) return authCodeStep("Check your email", "Create account");
  return `<h2>${aT("Create a WFSim account")}</h2>
    <p class="lede">${aT("Your builds follow you across devices.")}</p>
    ${authError()}${authConsentBox()}${authProviders("Sign up with {p}", back, true)}
    ${accountState.providers.includes("email") ? `${authField("auth-email", "Email", "email", "email")}
    ${authField("auth-password", "Password", "password", "new-password")}<span class="hint">${aT("At least 8 characters")}</span>
    <button class="run-btn" data-auth="register">${aT("Continue")}</button>` : ""}
    <div class="auth-foot">${aT("Already have an account?")} <a href="/login?return=${encodeURIComponent(back)}">${aT("Sign in")}</a></div>`;
}

function authResetCard() {
  if (authFlow.step === 2) {
    return authCodeStep("Check your email", "Set new password",
      authField("auth-password", "New password (8+ characters)", "password", "new-password"));
  }
  return `<h2>${aT("Reset your password")}</h2>
    <p class="lede">${aT("Enter the email you signed up with, and we will send a code.")}</p>
    ${authError()}${authField("auth-email", "Email", "email", "email")}
    <button class="run-btn" data-auth="reset">${aT("Send code")}</button>
    <div class="auth-foot"><a href="/login?return=${encodeURIComponent(authReturn())}">${aT("Back to sign in")}</a></div>`;
}

// ---- /account ------------------------------------------------------------------------------

function accountMethods(a) {
  const rows = ACCOUNT_SLOTS.map((s) => {
    const id = a.identities.find((i) => i.provider === s.id);
    const offered = accountState.providers.includes(s.id);
    if (!id && !offered) return "";
    const act = s.id === "email"
      ? `<a class="ghost-btn btn-sm" href="#email-password">${aT(id ? "Manage" : "Add")}</a>`
      : id
        ? `<button class="ghost-btn btn-sm" data-auth="unlink" data-p="${s.id}">${aT("Disconnect")}</button>`
        : `<a class="ghost-btn btn-sm" data-native href="${authStart(s.id, "link", "/account")}">${aT("Connect")}</a>`;
    return `<div class="method"><span class="ic">${ACCOUNT_ICONS[s.id]}</span>
      <div><b>${s.id === "email" ? aT("Email and password") : s.name}</b><span>${id ? escHtml(id.label) : aT("Not connected")}</span></div>
      <div class="acts">${id ? `<span class="tag ok">${aT("Connected")}</span>` : ""}${act}</div></div>`;
  }).join("");
  return `<div class="block" id="sign-in-methods"><div class="bh"><h2>${aT("Ways to sign in")}</h2>
    <span class="sub">${aT("Connected")} ${a.identities.length} / ${ACCOUNT_SLOTS.length}</span></div>
    <div class="bb" style="padding:0">${rows}</div></div>
    <p class="set-note">${aT("Keep at least one way to sign in. A way to sign in belongs to one account only; WFSim never merges accounts.")}</p>`;
}

/// THE NAME THE ACCOUNT GOES BY: a username, which is the site's handle for
/// it, and a display name, which is free text. The first username change is
/// at once, each later one a day after the last (docs/ACCOUNTS.md §"Names").
function accountProfileBlock(a) {
  const after = a.rename_after
    ? tr("The username can change again after {time}.").replace("{time}",
      new Date(a.rename_after).toLocaleString(accountLocale(), { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" }))
    : "";
  const body = authFlow.open === "profile"
    ? `<div class="inline-form">
        <div class="field"><label for="auth-username">${aT("Username")}</label>
          <input id="auth-username" type="text" autocomplete="username" maxlength="20" spellcheck="false" value="${escHtml(a.username)}"${a.rename_after ? " disabled" : ""}></div>
        <div class="field"><label for="auth-display">${aT("Display name")}</label>
          <input id="auth-display" type="text" autocomplete="nickname" maxlength="32" value="${escHtml(a.display_name || "")}" placeholder="${escHtml(a.username)}"></div>
        <button class="run-btn" data-auth="profile">${aT("Save")}</button></div>${authError()}
      <p class="set-note">${aT("Letters, digits and underscores for the username; anything for the display name. Everything you sign shows your current name.")}${after ? " " + escHtml(after) : ""}</p>`
    : `<dl class="kvs"><div class="kv"><dt>${aT("Username")}</dt><dd>@${escHtml(a.username)}${
        a.username.startsWith("user_") ? ` <span class="tag muted">${aT("not chosen yet")}</span>` : ""}</dd>
        <button class="ghost-btn btn-sm" data-auth="open" data-open="profile">${aT("Edit")}</button></div>
      <div class="kv"><dt>${aT("Display name")}</dt><dd>${a.display_name ? escHtml(a.display_name) : `<span class="set-note">${aT("Not set — the username is shown")}</span>`}${
        a.display_name_hidden ? ` <span class="tag warn">${aT("hidden")}</span>` : ""}</dd><span></span></div></dl>${
      a.display_name_hidden ? `<p class="set-note">${aT("Others see your username: a name cannot carry a link, contact details or trading.")}</p>` : ""}`;
  return `<div class="block" id="profile"><div class="bh"><h2>${aT("Profile")}</h2></div><div class="bb">${body}</div></div>`;
}

function accountEmailBlock(a) {
  const slot = a.identities.find((i) => i.provider === "email");
  if (!accountState.providers.includes("email") && !slot) return "";
  const open = authFlow.open;
  let body;
  if (open === "email" && authFlow.step === 2) {
    body = `<div class="inline-code">${authCodeStep("Check your email", "Confirm")}</div>`;
  } else {
    const kv = slot
      ? `<dl class="kvs"><div class="kv"><dt>${aT("Email")}</dt><dd>${escHtml(slot.label)} <span class="tag ok">${aT("Verified")}</span></dd>
          <button class="ghost-btn btn-sm" data-auth="open" data-open="email">${aT("Change email")}</button></div>
        <div class="kv"><dt>${aT("Password")}</dt><dd>${aT(slot.has_password ? "Set" : "Not set yet")}</dd>
          <button class="ghost-btn btn-sm" data-auth="open" data-open="password">${aT(slot.has_password ? "Change password" : "Set password")}</button></div></dl>`
      : `<p class="set-note" style="margin:0">${aT("No email yet. Add one to sign in with an email and a password.")}</p>
        <button class="ghost-btn btn-sm" data-auth="open" data-open="email" style="margin-top:8px">${aT("Add email and password")}</button>`;
    const form = open === "email"
      ? `<div class="inline-form">${authField("auth-email", slot ? "New email" : "Email", "email", "email")}
          ${authField("auth-password", "Password (8+ characters)", "password", "new-password")}
          <button class="run-btn" data-auth="link">${aT("Send code")}</button></div>`
      : open === "password"
        ? `<div class="inline-form">${slot && slot.has_password ? authField("auth-current", "Current password", "password", "current-password") : ""}
            ${authField("auth-password", "New password (8+ characters)", "password", "new-password")}
            <button class="run-btn" data-auth="password">${aT("Save")}</button></div>
          <p class="set-note">${aT("Other devices are signed out when the password changes.")}</p>`
        : "";
    body = kv + (open ? authError() : "") + form;
  }
  return `<div class="block" id="email-password"><div class="bh"><h2>${aT("Email and password")}</h2></div>
    <div class="bb">${body}</div></div>`;
}

/// THE SETTINGS PAGES' OWN NAVIGATION: who is signed in, then one link per page.
function settingsNav(a, here) {
  const since = new Date(a.created_at).toLocaleDateString(accountLocale(), { year: "numeric", month: "long" });
  const link = (href, kind, label) => `<a href="${href}"${here === kind ? ' class="on" aria-current="page"' : ""}>${aT(label)}</a>`;
  return `<nav class="set-side" aria-label="${aT("Account settings")}">
    <div class="me"><span class="avatar avatar-lg">${escHtml(accountInitial(a))}</span>
      <div><b>${escHtml(accountName(a))}</b><span>${escHtml(tr("Joined {date}").replace("{date}", since))}</span></div></div>
    ${link("/account", "account", "Account")}
    ${link("/account/sync", "sync", "Cloud sync")}
    ${Object.entries(EXT.pages).filter(([, p]) => p.settings && (!p.available || p.available()))
      .map(([k, p]) => link(p.path, k, p.nav)).join("")}</nav>`;
}

/// THE SYNC ROW says where `syncStatus` stands and offers the one action that
/// state calls for (docs/UI.md §"Build sync").
function syncRowHtml() {
  const s = syncStatus;
  const row = (dd, act = "<span></span>") => `<dt>${aT("Build sync")}</dt><dd>${dd}</dd>${act}`;
  if (s.state === "on") {
    const when = new Date(s.at).toLocaleTimeString(accountLocale(), { hour: "2-digit", minute: "2-digit" });
    return row(`<span class="tag ok">${aT("On")}</span> ${escHtml(tr("last synced {time}").replace("{time}", when))}${
      s.full ? ` <span class="tag warn">${aT("Full: new items stay on this browser")}</span>` : ""}${syncUnsyncedHtml(s.unsynced)}`,
    `<button class="ghost-btn btn-sm" data-auth="sync-now">${aT("Sync now")}</button>`);
  }
  if (s.state === "other") {
    return row(aT("This browser holds items synced with another account."),
      `<button class="ghost-btn btn-sm" data-auth="sync-adopt">${aT("Add them to this account")}</button>`);
  }
  if (s.state === "error") {
    return row(`${aT("Could not sync just now.")}${s.reason ? ` <span class="set-note">(${escHtml(s.reason)})</span>` : ""}`,
      `<button class="ghost-btn btn-sm" data-auth="sync-now">${aT("Try again")}</button>`);
  }
  if (s.state === "not_included") return row(`<span class="tag muted">${aT("Not available for this account")}</span>`);
  return row(`<span class="tag muted">${aT("Checking…")}</span>`);
}
/// WHAT SYNCS: the default for a new item, and how much each pool holds —
/// against the allowance where the account has one.
function syncChoiceHtml() {
  const n = syncedCounts();
  const of = (pool) => (syncAllowance && syncAllowance[pool] != null ? ` / ${syncAllowance[pool]}` : "");
  return `<dt>${aT("What syncs")}</dt><dd>${escHtml(tr("presets {a} · customs {b}")
    .replace("{a}", n.presets + of("presets")).replace("{b}", n.customs + of("customs")))}</dd>
    <a class="ghost-btn btn-sm" href="/account/sync">${aT("Manage")}</a>`;
}
/// WHAT THE SERVER WOULD NOT TAKE, by name and why. Each stays on this browser.
const SYNC_REJECTED = { bad_body: "too large to sync", bad_change: "cannot be synced" };
function syncUnsyncedHtml(list) {
  if (!list || !list.length) return "";
  const here = syncLocal();
  const name = (u) => ((here.get(u.id) || {}).p || {}).name || u.id;
  return `<br><span class="tag warn">${escHtml(tr("{n} items did not sync").replace("{n}", list.length))}</span>
    <span class="set-note">${list.slice(0, 8).map((u) => `${escHtml(name(u))} — ${aT(SYNC_REJECTED[u.reason] || u.reason)}`).join("; ")}${
      list.length > 8 ? " …" : ""}</span>`;
}
function renderSyncStatus() {
  const el = $("sync-row");
  if (el) el.innerHTML = syncRowHtml();
  const c = $("sync-choice");
  if (c) c.innerHTML = syncChoiceHtml();
  if (authKindOf(location.pathname) === "sync" && $("cloud-items")) renderAuthPage("sync");
}

/// THE AGENTS THAT ACT FOR THIS ACCOUNT — each one claimed with a code mailed
/// here, each one revoked in one click, the way a mature product lists the
/// apps it has let in (docs/ACCOUNTS.md §"Agents").
function accountAgentsBlock() {
  const day = (d) => (d ? new Date(d).toLocaleDateString(accountLocale(), { year: "numeric", month: "short", day: "numeric" }) : "—");
  const rows = agentsState.map((g) => `<div class="kv"><dt>${escHtml(g.name)}</dt>
      <dd>${escHtml(tr("connected {date}").replace("{date}", day(g.claimed_at)))} · ${escHtml(tr("last used {date}").replace("{date}", day(g.last_used_at)))}</dd>
      <button class="ghost-btn btn-sm" data-auth="agent-revoke" data-id="${escHtml(g.id)}">${aT("Disconnect")}</button></div>`).join("");
  return `<div class="block" id="agents"><div class="bh"><h2>${aT("Connected agents")}</h2></div><div class="bb">${
    rows ? `<dl class="kvs">${rows}</dl>` : `<p class="set-note" style="margin:0">${aT("No agent acts for this account.")}</p>`}
    <p class="set-note">${aT("An AI agent connects by asking you for a code WFSim mails to your address. Disconnecting it stops its key at once.")}
      <a data-native href="/auth.md">auth.md</a></p></div></div>`;
}

/// WHAT THIS ACCOUNT'S MACHINES COMPUTED FOR THE BOARD, and whether its name
/// is on the ranking (docs/BOARD.md §"Contribution").
function accountComputeBlock() {
  const d = devicesState;
  if (!d) return "";
  const n = d.devices.length;
  return `<div class="block" id="compute"><div class="bh"><h2>${aT("Board compute")}</h2></div><div class="bb"><dl class="kvs">
    <div class="kv"><dt>${aT("Points")}</dt><dd>${escHtml(d.points.toLocaleString(accountLocale()))} · ${
      escHtml((n === 1 ? tr("{n} device") : tr("{n} devices")).replace("{n}", String(n)))}</dd>
      <a class="ghost-btn btn-sm" href="/compute">${aT("Compute")}</a>
      <a class="ghost-btn btn-sm" href="/contributors">${aT("Ranking")}</a></div>
    <div class="kv"><dt>${aT("On the ranking")}</dt><dd>${aT(d.named ? "Your name is shown" : "Anonymous")}</dd>
      ${contributionNameButton(d)}</div>${computeHonourHtml(d)}</dl>
    <p class="set-note">${aT("A browser you are signed in on counts its compute here, and it is on the public ranking without your name unless you choose to show it. What it computes is free for everyone and never sold.")}</p></div></div>`;
}

/// THE ONE SWITCH for whether the ranking names this account — the same on the
/// account page and the ranking.
function contributionNameButton(d) {
  return `<button class="ghost-btn btn-sm" data-auth="contribution-named" data-named="${d.named ? "no" : "yes"}">${
    aT(d.named ? "Hide my name" : "Show my name")}</button>`;
}

/// THE CONTRIBUTION RANKING — public, every account with a claimed device, by
/// all their points or the last thirty days'. A row is anonymous unless its
/// account agreed to show its name. A named row's mark comes with it from the
/// server and the extension draws it (`contributorMark`); it never moves a
/// place: the order is the points alone.
let contributorsPeriod = "all";
let devicesAskedFor = null;
/// THE CONTRIBUTOR RANK, as the server computes it (worker/contribution.js
/// `contributorRank`): the page draws it and never derives it from points.
const contributorRankBadge = (cr) => (cr && Number.isFinite(cr.rank)
  ? `<span class="cr-badge${cr.rank >= 30 ? " cr-30" : ""}" title="${escHtml(tr("Contributor rank {n}").replace("{n}", cr.rank))}">${cr.rank}</span>` : "");
/// …AND THE WAY TO THE NEXT ONE, in experience.
function contributorRankBar(cr) {
  if (!cr || !Number.isFinite(cr.rank)) return "";
  const n = (x) => Number(x).toLocaleString(accountLocale());
  const span = Math.max(1, cr.xp_at_next - cr.xp_at_rank);
  const pct = Math.max(0, Math.min(100, ((cr.xp - cr.xp_at_rank) * 100) / span));
  return `<div class="cr-line">${contributorRankBadge(cr)}<div class="cr-to"><div class="cr-bar" role="progressbar" aria-valuemin="0" aria-valuemax="100"
    aria-valuenow="${Math.round(pct)}"><i style="width:${pct.toFixed(1)}%"></i></div><span class="set-note">${escHtml(tr("{x} / {y} experience to rank {n}")
    .replace("{x}", n(cr.xp)).replace("{y}", n(cr.xp_at_next)).replace("{n}", cr.rank + 1))}</span></div></div>`;
}

function contributorsPage() {
  const list = contributorsState;
  // THE COLUMN EACH RANKING ORDERS ON, and its heading (worker/contribution.js `PERIODS`).
  const col = { all: ["points", "Points"], recent: ["recent", "Last 30 days"], week: ["week", "Last 7 days"] }[contributorsPeriod];
  const n = (x) => escHtml(x.toLocaleString(accountLocale()));
  const who = (c) => (c.name === null ? `<span class="rank-name muted">${aT("Anonymous contributor")}</span>`
    : `<span class="rank-name">${escHtml(c.name)}</span>${extHookNow("contributorMark", c.mark) || ""}`);
  const honour = (c) => (c.volunteer ? `<span class="contrib-volunteer">${aT("WFSim Volunteer")}</span>` : "");
  const rows = (list || []).map((c, i) => `<li class="rank-row${i < 3 ? " top" : ""}${c.you ? " you" : ""}">
      <span class="rank-n">${i + 1}</span><span class="rank-who">${contributorRankBadge(c.contributor_rank)}${who(c)}${honour(c)}${
      c.you ? `<span class="rank-you">${aT("(you)")}</span>` : ""}</span><span class="rank-pts">${n(c[col[0]] || 0)}</span></li>`).join("");
  const head = `<li class="rank-row rank-head" aria-hidden="true"><span class="rank-n">#</span><span class="rank-who">${
    aT("Contributor")}</span><span class="rank-pts">${aT(col[1])}</span></li>`;
  const tab = (id, label) => `<button class="seg${contributorsPeriod === id ? " on" : ""}" data-auth="contributors-period"
      data-period="${id}" aria-pressed="${contributorsPeriod === id}">${aT(label)}</button>`;
  return `<div class="settings solo"><div class="set-main"><h1 class="page">${aT("Contributors")}</h1>
    <p class="set-note">${aT("The volunteers whose devices compute WFSim's free features together. Everything they compute is free for every player, and WFSim never makes money from it.")}</p>
    ${contributorsYouHtml()}
    <div class="block"><div class="bh"><span class="oseg">${tab("all", "All-time ranking")} ${tab("recent", "Monthly ranking")} ${tab("week", "Weekly ranking")}</span></div><div class="bb">${list == null ? ""
      : rows ? `<ol class="rank-list">${head}${rows}</ol>` : `<p class="set-note" style="margin:0">${aT("Nobody yet.")}</p>`}</div></div>
    <p class="set-note">${aT("Points count verified compute and nothing else. A membership adds none.")}
      ${aT("The badge is the contributor rank: ten points are one experience, and the ranks climb as Mastery Rank does, past 30 without end.")}</p></div></div>`;
}

/// THE READER'S OWN LINE: signed out, how to be on it; signed in and not yet
/// asked, the one question whether to show their name; after, their points and
/// the switch.
function contributorsYouHtml() {
  if (!accountState.providers.length) return "";
  if (!accountState.account) {
    return `<p class="set-note">${aT("Signed in, the board compute your browser does counts under your name.")}
      <a href="/login?return=${encodeURIComponent("/contributors")}">${aT("Sign in")}</a></p>`;
  }
  const d = devicesState;
  if (!d) return "";
  if (!d.decided) {
    return `<div class="block"><div class="bb"><p class="set-note" style="margin:0 0 8px">${
      escHtml(tr("You have {n} points, on the ranking without your name. Show your name there?").replace("{n}", d.points.toLocaleString(accountLocale())))}</p>
      <button class="ghost-btn btn-sm" data-auth="contribution-named" data-named="yes">${aT("Show my name")}</button>
      <button class="ghost-btn btn-sm" data-auth="contribution-named" data-named="no">${aT("Keep it anonymous")}</button>
      <p class="set-note" style="margin:8px 0 0">${aT("Shown, it is your display name, or your username if you have none, with any mark your account carries. You can change it any time.")}</p></div></div>`;
  }
  return `<div class="block"><div class="bb"><dl class="kvs"><div class="kv"><dt>${aT("Your points")}</dt>
      <dd>${escHtml(d.points.toLocaleString(accountLocale()))} · ${aT(d.named ? "Your name is shown" : "Anonymous")}</dd>
      ${contributionNameButton(d)}</div>${d.contributor_rank ? `<div class="kv"><dt>${aT("Contributor rank")}</dt><dd>${contributorRankBar(d.contributor_rank)}</dd></div>` : ""}</dl></div></div>`;
}

function accountDataBlock() {
  return `<div class="block" id="data-privacy"><div class="bh"><h2>${aT("Data and privacy")}</h2></div><div class="bb"><dl class="kvs">
    <div class="kv" id="sync-row">${syncRowHtml()}</div>
    <div class="kv" id="sync-choice">${syncChoiceHtml()}</div>
    <div class="kv"><dt>${aT("Your data")}</dt><dd>${aT("The account, its ways to sign in, and what it syncs")}</dd>
      <button class="ghost-btn btn-sm" data-auth="export">${aT("Download my data")}</button></div>
    <div class="kv"><dt>${aT("Privacy policy")}</dt><dd>${aT("What WFSim keeps, and why")}</dd>
      <a class="ghost-btn btn-sm" data-native href="/privacy">${aT("View")}</a></div></dl></div></div>`;
}

function accountDangerBlock() {
  return `<div class="block danger" id="delete-account"><div class="bh"><h2>${aT("Delete account")}</h2></div><div class="bb">
    ${authFlow.error === "last_slot" ? `<div class="auth-err" role="alert">${escHtml(accountSaid("last_slot"))}</div>` : ""}
    <p class="set-note" style="margin:0">${aT("Deletes this account, every way to sign in to it and everything it syncs, for good.")}
      ${accountDeleteNote}</p>
    <div class="confirm"><label for="auth-delete" class="set-note">${aT("Type DELETE to confirm:")}</label>
      <input id="auth-delete" autocomplete="off" spellcheck="false">
      <button class="btn-danger" data-auth="delete" disabled>${aT("Delete account for good")}</button></div></div></div>`;
}

/// WHAT ELSE A DELETION ENDS, as an extension says it; set before the page is drawn.
let accountDeleteNote = "";
function accountPage(a) {
  return `<div class="settings">${settingsNav(a, "account")}
    <div class="set-main"><h1 class="page">${aT("Account settings")}</h1>
      ${accountProfileBlock(a)}${accountMethods(a)}${accountEmailBlock(a)}${accountAgentsBlock()}${accountComputeBlock()}${accountDataBlock()}${accountDangerBlock()}</div></div>`;
}

// ---- drawing a page ------------------------------------------------------------------------

function renderAuthPage(kind) {
  const main = $("auth-page");
  if (!main) return;
  // A NEW PAGE STARTS CLEAN — except the first, which may carry an OAuth refusal.
  if (authFlow.kind !== kind) {
    authFlow = { kind, step: 1, email: "", sentAt: 0, error: authFlow.kind === null ? authFlow.error : null, open: null };
  }
  clearInterval(authTimer);
  if (!accountState.loaded) { main.innerHTML = ""; return; }
  // THE COMPUTE PAGE IS EVERYONE'S TOO: this browser's half needs no account.
  if (kind === "compute") {
    computeDraw(main);
    return;
  }
  // THE RANKING IS EVERYONE'S, signed in or not.
  if (kind === "contributors") {
    main.innerHTML = contributorsPage();
    // THE READER'S OWN LINE is asked once per account, however they arrived —
    // signing in on this page included.
    const who = accountState.account && accountState.account.id;
    if (who && !devicesState && devicesAskedFor !== who) {
      devicesAskedFor = who;
      loadDevices().then(() => { if (authKindOf(location.pathname) === "contributors") renderAuthPage(kind); });
    }
    if (contributorsState === null) {
      contributorsState = undefined;
      const period = contributorsPeriod;
      accountCall("GET", period === "all" ? "/api/contributors" : `/api/contributors?period=${period}`).then((r) => {
        if (period !== contributorsPeriod) return;
        contributorsState = (r && r.ok && r.contributors) || [];
        if (authKindOf(location.pathname) === "contributors") renderAuthPage(kind);
      });
    }
    return;
  }
  const { account, providers } = accountState;
  // A SIGNED-IN READER HAS NO SIGN-IN PAGE, and a signed-out one no settings.
  const ext = EXT.pages[kind];
  if (account && !isSettings(kind) && !(ext && ext.open)) { nav(authReturn()); return; }
  if (!account && isSettings(kind)) {
    history.replaceState(null, "", `/login?return=${encodeURIComponent(location.pathname)}`); route(); return;
  }
  // AN EXTENSION PAGE THAT IS NOT AVAILABLE IS NOT A PAGE: the reader is on
  // the account page instead.
  if (ext && ext.available && !ext.available()) { history.replaceState(null, "", "/account"); route(); return; }
  if (ext && ext.open) { main.innerHTML = ext.render(account); if (ext.shown) ext.shown(main); return; }
  if (!account && !providers.length) {
    main.innerHTML = `<div class="auth-page"><div class="auth-card"><h2>${aT("Accounts are not available here")}</h2>
      <p class="lede">${aT("Sign in on wfsim.app.")}</p></div></div>`;
    return;
  }
  if (kind === "account") accountDeleteNote = extHookNow("deleteNote") || "";
  main.innerHTML = kind === "account" ? accountPage(account) : ext ? ext.render(account)
    : kind === "sync" ? cloudPage(account)
    : `<div class="auth-page"><div class="auth-card" data-auth-kind="${kind}">${
      kind === "signup" ? authSignupCard() : kind === "reset" ? authResetCard() : authLoginCard()}</div></div>`;
  if (ext && ext.shown) ext.shown(main);
  if (kind === "account") {
    const del = $("auth-delete");
    if (del) del.addEventListener("input", () => { main.querySelector("[data-auth=delete]").disabled = del.value.trim() !== "DELETE"; });
  }
  // THE RESEND COUNTDOWN ticks while a code step is on screen, and only then.
  if (authFlow.step === 2 && Date.now() - authFlow.sentAt < 60000) {
    authTimer = setInterval(() => {
      const f = main.querySelector(".auth-foot .muted");
      const left = Math.max(0, 60 - Math.floor((Date.now() - authFlow.sentAt) / 1000));
      if (!f || !left) { clearInterval(authTimer); if (f) renderAuthPage(kind); return; }
      f.textContent = `${tr("Resend")} (0:${String(left).padStart(2, "0")})`;
    }, 1000);
  }
  const first = main.querySelector("[data-code]") || main.querySelector(".auth-card input, .inline-form input");
  if (first && !isSettings(kind)) first.focus();
  else if (first && authFlow.open) first.focus();
}

/// AN ANSWER THAT SIGNS SOMEBODY IN: said, the account reloaded, and the reader
/// taken back to where they came from.
async function authSignedIn(r) {
  // WHERE TO GO IS READ BEFORE ANYTHING MOVES: reloading the account redraws
  // this page, which sends a signed-in reader on and takes `?return=` with it.
  const back = authReturn();
  presetToast(accountSaid(r.outcome));
  authFlow = { kind: null, step: 1, email: "", sentAt: 0, error: null, open: null };
  await loadAccount();
  if (authKindOf(location.pathname)) nav(back);
}

/// The box ticked, or the card says it must be — kept across a redraw.
function authConsented() {
  const box = $("auth-consent");
  authFlow.consent = !!(box && box.checked);
  if (authFlow.consent) return true;
  authFlow.error = "needs_consent";
  renderAuthPage(authFlow.kind);
  return false;
}

async function authAct(el) {
  const what = el.dataset.auth;
  if (what === "register" && !authConsented()) return;
  if (what.startsWith("compute-") || what.startsWith("device-")) return computeAct(el, what);
  const kind = authFlow.kind;
  const main = $("auth-page");
  authFlow.error = null;
  const fail = (r) => { authFlow.error = (r && r.reason) || "send_failed"; renderAuthPage(kind); };
  const mailed = (email) => { authFlow = { ...authFlow, step: 2, email, sentAt: Date.now() }; renderAuthPage(kind); };
  el.disabled = true;
  try {
    if (what === "login") {
      const r = await accountCall("POST", "/api/auth/email/login", { email: authVal("auth-email"), password: $("auth-password").value });
      return r && r.ok ? authSignedIn(r) : fail(r);
    }
    if (what === "register" || what === "link") {
      const email = authVal("auth-email");
      const r = await accountCall("POST", `/api/auth/email/${what}`, { email, password: $("auth-password").value });
      return r && r.ok ? mailed(email) : fail(r);
    }
    if (what === "reset" || what === "resend") {
      const email = what === "resend" ? authFlow.email : authVal("auth-email");
      if (what === "resend" && kind !== "reset") { authFlow.step = 1; return renderAuthPage(kind); }
      const r = await accountCall("POST", "/api/auth/email/reset", { email });
      return r && r.ok ? mailed(email) : fail(r);
    }
    if (what === "back") { authFlow.step = 1; return renderAuthPage(kind); }
    if (what === "verify") {
      const r = await accountCall("POST", "/api/auth/email/verify", {
        email: authFlow.email, code: authCode(), ...(kind === "reset" ? { password: $("auth-password").value } : {}),
      });
      if (!(r && r.ok)) return fail(r);
      if (kind === "account") {
        presetToast(accountSaid(r.outcome));
        authFlow = { ...authFlow, step: 1, open: null };
        return loadAccount();
      }
      return authSignedIn(r);
    }
    if (what === "open") {
      authFlow = { ...authFlow, step: 1, open: authFlow.open === el.dataset.open ? null : el.dataset.open };
      return renderAuthPage(kind);
    }
    if (what === "password") {
      const r = await accountCall("POST", "/api/account/password",
        { current: ($("auth-current") || {}).value || "", password: $("auth-password").value });
      if (!(r && r.ok)) return fail(r);
      presetToast(accountSaid(r.outcome));
      authFlow.open = null;
      return loadAccount();
    }
    if (what === "unlink") {
      const r = await accountCall("POST", "/api/account/unlink", { provider: el.dataset.p });
      if (r && r.reason === "last_slot") {
        authFlow.error = "last_slot";
        renderAuthPage(kind);
        $("delete-account").scrollIntoView({ behavior: "smooth", block: "center" });
        return;
      }
      if (!(r && r.ok)) return fail(r);
      presetToast(tr("Disconnected."));
      return loadAccount();
    }
    if (what === "delete") {
      const r = await accountCall("POST", "/api/account/delete", {});
      if (!(r && r.ok)) return fail(r);
      // WHAT IT SYNCED WENT WITH IT; this browser keeps its own copy, and the
      // next account to sign in here takes it as new rather than as another's.
      try { localStorage.removeItem(SYNC_KEY); } catch (_) { /* nothing to forget */ }
      presetToast(tr("Account deleted."));
      await loadAccount();
      return nav("/");
    }
    // AN ACTION THIS FILE DOES NOT KNOW is an extension's, which says whether it took it.
    if (await extHook("act", el, kind)) return;
    if (what === "profile") {
      const a = accountState.account;
      const r = await accountCall("POST", "/api/account/profile", {
        ...(a.rename_after ? {} : { username: authVal("auth-username") }), display_name: authVal("auth-display") });
      if (!(r && r.ok)) return fail(r);
      presetToast(tr("Saved."));
      authFlow = { ...authFlow, open: null, error: null };
      return loadAccount();
    }
    if (what === "agent-revoke") {
      const r = await accountCall("POST", "/api/account/agents/revoke", { id: el.dataset.id });
      if (!(r && r.ok)) return fail(r);
      presetToast(tr("Disconnected."));
      await loadAgents();
      return renderAuthPage(kind);
    }
    if (what === "contributors-period") {
      contributorsPeriod = ["recent", "week"].includes(el.dataset.period) ? el.dataset.period : "all";
      contributorsState = null;
      return renderAuthPage(kind);
    }
    if (what === "contribution-named") {
      const r = await accountCall("POST", "/api/account/contribution", { named: el.dataset.named === "yes" });
      if (!(r && r.ok)) return fail(r);
      await loadDevices();
      contributorsState = null;
      return renderAuthPage(kind);
    }
    if (what === "sync-now") { await syncNow(); return; }
    if (what === "sync-adopt") { await syncAdopt(); return; }
    if (what === "export") {
      const r = await accountCall("POST", "/api/account/export", {});
      if (!(r && r.ok)) return fail(r);
      const a = document.createElement("a");
      a.href = URL.createObjectURL(new Blob([JSON.stringify(r, null, 2)], { type: "application/json" }));
      a.download = "wfsim-account.json";
      a.click();
      URL.revokeObjectURL(a.href);
    }
  } finally {
    if (el.isConnected) el.disabled = false;
    if (main && what === "delete") el.disabled = ($("auth-delete") || {}).value !== "DELETE";
  }
}

(function () {
  const main = $("auth-page");
  if (!main) return;
  main.addEventListener("click", (e) => {
    const jump = e.target.closest("a[data-jump]");
    if (jump) {
      e.preventDefault();
      const t = $(jump.getAttribute("href").slice(1));
      if (t) t.scrollIntoView({ behavior: "smooth", block: "start" });
      return;
    }
    const email = e.target.closest('a[href="#email-password"]');
    if (email) { e.preventDefault(); $("email-password").scrollIntoView({ behavior: "smooth", block: "start" }); return; }
    // A WAY TO SIGN UP WAITS FOR THE YES, as the email form does.
    const prov = e.target.closest("a.prov[data-needs-consent]");
    if (prov && !authConsented()) { e.preventDefault(); return; }
    const el = e.target.closest("[data-auth]");
    if (el) { e.preventDefault(); authAct(el); }
  });
  main.addEventListener("change", (e) => {
    if (e.target.id === "sync-auto") setSyncAuto(e.target.checked);
  });
  // ENTER SUBMITS what it is typed into, as the button under it would.
  main.addEventListener("keydown", (e) => {
    if (imeComposing(e)) return;
    if (e.key === "Enter" && e.target.matches("input")) {
      const box = e.target.closest(".auth-card, .inline-form, .inline-code, .confirm");
      const go = box && box.querySelector(".run-btn, .btn-danger");
      if (go && !go.disabled) { e.preventDefault(); authAct(go); }
      return;
    }
    if (e.key === "Backspace" && e.target.matches("[data-code]") && !e.target.value) {
      const prev = main.querySelector(`[data-code="${Number(e.target.dataset.code) - 1}"]`);
      if (prev) prev.focus();
    }
  });
  main.addEventListener("input", (e) => {
    if (!e.target.matches("[data-code]")) return;
    const boxes = [...main.querySelectorAll("[data-code]")];
    const digits = e.target.value.replace(/\D/g, "");
    const at = Number(e.target.dataset.code);
    // A PASTED OR AUTOFILLED CODE arrives in one box; it is spread across all.
    [...digits].slice(0, 6 - at).forEach((d, i) => { boxes[at + i].value = d; });
    if (!digits) e.target.value = "";
    const next = boxes[Math.min(at + Math.max(digits.length, 1), 5)];
    if (digits && next) next.focus();
    // THE LAST DIGIT SUBMITS — unless a new password still has to be typed.
    const pw = $("auth-password");
    const go = main.querySelector("[data-auth=verify]");
    if (authCode().length === 6 && go && !(pw && !pw.value)) authAct(go);
  });
})();
