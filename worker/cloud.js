// ---- THE PAID HALF, FORWARDED ------------------------------------------------------
//
// Everything that is sold runs in a separate, private worker behind the CLOUD
// service binding (docs/ACCOUNTS.md §"Paid features"). This file is the whole
// of it on the public side: it forwards, and knows nothing of what is sold.
//
// THE ACCOUNT TRAVELS AS A HEADER ONLY THIS WORKER SETS. A browser's own
// `x-wfsim-account` is dropped before the session is read, and a state-changing
// call from another site never reaches the binding.

import { sessionAccount, sameSite, json, no } from "./accounts.js";
import { agentAccount } from "./agents.js";

const ACCOUNT_HEADER = "x-wfsim-account";
const WEBHOOK = "/api/stripe/webhook";

export const cloudPath = (path) =>
  path === "/api/billing" || path.startsWith("/api/billing/") || path.startsWith("/api/cloud/") || path.startsWith("/api/stripe/");

export async function cloudRoute(request, env, path) {
  if (!env.CLOUD) return path === "/api/billing" ? json({ ok: true, configured: false }) : no("unavailable", 503);
  const headers = new Headers(request.headers);
  headers.delete(ACCOUNT_HEADER);
  // THE WEBHOOK GOES THROUGH UNTOUCHED: Stripe signs the raw body, and is no reader.
  if (path !== WEBHOOK) {
    if (!["GET", "HEAD"].includes(request.method) && !sameSite(request)) return no("cross_site", 403);
    headers.delete("cookie");
    // A CLAIMED AGENT'S KEY acts for its account on a feature's route, and on
    // nothing of billing: an agent never buys, and never sees what was bought.
    const account = await sessionAccount(env, request)
      || (path.startsWith("/api/cloud/") ? await agentAccount(env, request) : null);
    headers.delete("authorization");
    if (account) headers.set(ACCOUNT_HEADER, account);
  }
  const body = ["GET", "HEAD"].includes(request.method) ? undefined : await request.arrayBuffer();
  return env.CLOUD.fetch(new Request(request.url, { method: request.method, headers, body }));
}

async function internal(env, what, account) {
  const r = await env.CLOUD.fetch(new Request(`https://cloud.internal/internal/${what}`,
    { method: "POST", headers: { [ACCOUNT_HEADER]: account } }));
  return r.json();
}

/// A WORD TO THE OWNER, as a Discord message from the paid half's bot. False
/// with no paid half bound, or when it could not be sent.
export async function cloudTell(env, text) {
  if (!env.CLOUD) return false;
  try {
    const r = await env.CLOUD.fetch(new Request("https://cloud.internal/internal/tell", {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ text }) }));
    return (await r.json()).ok === true;
  } catch (_) {
    return false;
  }
}

/// BEFORE AN ACCOUNT IS DELETED its subscriptions end, or it would go on being
/// charged. False when the paid half could not end them — and then the account
/// is kept. With no paid half bound there is nothing to end.
export async function cloudEnd(env, account) {
  if (!env.CLOUD) return true;
  try {
    return (await internal(env, "end", account)).ok === true;
  } catch (_) {
    return false;
  }
}

/// THE MARKS OF THE NAMED ACCOUNTS ON THE CONTRIBUTION RANKING — `{ account:
/// mark }` as the paid half proves them, passed through unread; empty with none
/// bound or none answering, and the ranking stands without them.
export async function cloudMarks(env, accounts) {
  if (!env.CLOUD || !accounts.length) return {};
  try {
    const r = await env.CLOUD.fetch(new Request("https://cloud.internal/internal/marks",
      { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ accounts }) }));
    const j = await r.json();
    return (j && j.ok && j.marks) || {};
  } catch (_) {
    return {};
  }
}

/// What the paid half holds about an account, for its export — `{ billing,
/// sync }` as it sends them; empty with none bound.
export async function cloudExport(env, account) {
  if (!env.CLOUD) return {};
  try {
    const { ok, ...held } = await internal(env, "export", account);
    return held;
  } catch (_) {
    return {};
  }
}
