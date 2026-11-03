// SPDX-License-Identifier: AGPL-3.0-or-later
// The private service sends the owner's notifications and suppresses mentions.
export async function cloudTell(env, text, files = []) {
  if (!env.CLOUD) return false;
  const attached = files.map((f) => ({ name: f.name, type: f.type,
    base64: btoa(Array.from(f.bytes, (c) => String.fromCharCode(c)).join("")) }));
  try {
    const r = await env.CLOUD.fetch(new Request("https://cloud.internal/internal/tell", {
      method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify(attached.length ? { text, files: attached } : { text }) }));
    return r.ok && (await r.json()).ok === true;
  } catch (_) {
    return false;
  }
}
