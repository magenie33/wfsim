// Game legality and canonical ids belong to engine::board::equipment, in intake.
const ID = /^[a-z0-9_]{1,100}$/;
const KEY = /^[a-f0-9]{32}$/;
const reply = (value, status = 200) => new Response(JSON.stringify(value), {
  status, headers:{"content-type":"application/json", "cache-control":"no-store"},
});

export async function equipmentLibraryRoute(request, env, path) {
  if (request.method !== "POST") return reply({ok:false, error:"POST only"}, 405);
  if (!env.LIBRARY) return reply({ok:false, error:"the library is not configured"}, 503);
  const raw = await request.text();
  if (raw.length > 16384) return reply({ok:false, error:"payload too large"}, 413);
  let input;
  try { input = JSON.parse(raw); } catch { return reply({ok:false, error:"bad JSON"}, 400); }
  if (!input || !["warframe", "companion"].includes(input.kind)) return reply({ok:false, error:"bad kind"}, 400);
  if (path === "/api/equipment/submit") {
    if (!input.build || typeof input.build !== "object" || Array.isArray(input.build)) return reply({ok:false, error:"bad build"}, 400);
    const build = Object.fromEntries(["frame", "companion", "mods", "aura", "exilus", "arcanes", "shards", "helminth"]
      .filter(key => Object.hasOwn(input.build, key)).map(key => [key, input.build[key]]));
    const at = new Date().toISOString().slice(0, 10);
    await env.LIBRARY.prepare("INSERT INTO equipment_inbox (id, at, record) VALUES (?, ?, ?)")
      .bind("equipment:" + crypto.randomUUID(), at, JSON.stringify({kind:input.kind, build, at})).run();
    return reply({ok:true, queued:true});
  }
  if (!ID.test(input.equipment || "") || (input.after && !KEY.test(input.after))) return reply({ok:false, error:"bad scope or cursor"}, 400);
  const {results} = await env.LIBRARY.prepare("SELECT id, at, record FROM equipment_builds WHERE kind = ? AND equipment = ? AND id > ? ORDER BY id LIMIT 51")
    .bind(input.kind, input.equipment, input.after || "").all();
  const rows = results.slice(0, 50).map(row => ({id:row.id, at:row.at, ...JSON.parse(row.record)}));
  return reply({ok:true, builds:rows, next:results.length > 50 ? rows.at(-1).id : null});
}
