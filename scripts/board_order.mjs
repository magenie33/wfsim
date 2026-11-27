// SPDX-License-Identifier: AGPL-3.0-or-later
// Operator CLI for declarative board order generation and partitioned queue control.
//
//   node scripts/board_order.mjs preview --tags=beam --zone=0
//   node scripts/board_order.mjs create  --tags=beam --zone=0 --why="beam ramp clock fix"
//   node scripts/board_order.mjs list
//   node scripts/board_order.mjs rezone  --order=<id> --zone=1
//   node scripts/board_order.mjs reorder --order=<id> --before=<other_id>
//   node scripts/board_order.mjs cancel  --order=<id>
import { parseArgs } from "node:util";
import { pathToFileURL } from "node:url";

function parseSpecFlags(values) {
  const specification = {};
  if (values.policy) specification.policy = values.policy;
  if (values.sort) specification.sort = values.sort;
  if (values.riven) specification.riven = values.riven;
  if (values.builds) specification.builds = values.builds;
  if (values["build-ids"]) specification.build_ids = values["build-ids"];
  if (values.weapons) specification.weapons = values.weapons;
  if (values.pin) specification.pinned_weapons = values.pin;
  if (values.tags) specification.tags = values.tags;
  if (values.classes) specification.classes = values.classes;
  if (values.rulers) specification.rulers = values.rulers;
  if (values.modes) specification.modes = values.modes;
  return specification;
}

async function callRemote(base, token, action, body = {}) {
  const url = `${base.replace(/\/$/, "")}/api/board/orders/${action}`;
  const res = await fetch(url, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      authorization: `Bearer ${token}`,
    },
    body: JSON.stringify(body),
  });
  const data = await res.json();
  if (!res.ok || data.ok === false) {
    throw new Error(data.reason || data.error || `HTTP ${res.status}`);
  }
  return data;
}

async function openDirectEnv() {
  const url = process.env.WFSIM_COMPUTE_ORDER_DATABASE_URL || process.env.DATABASE_URL;
  if (!url) return null;
  const [{ default: postgres }, { pgDb }] = await Promise.all([
    import("../worker/vendor/postgres/src/index.js"),
    import("../worker/db.js"),
  ]);
  const sql = postgres(url, {
    max: 1,
    types: { bigint: { to: 20, from: [20], parse: (x) => Number(x), serialize: (x) => String(x) } },
  });
  const client = {
    query: async (text, params) => {
      const rows = await sql.unsafe(text, params);
      return { rows, affected: rows.count ?? 0 };
    },
    transaction: async (fn) => sql.begin(async (tx) => fn({
      query: async (text, params) => {
        const rows = await tx.unsafe(text, params);
        return { rows, affected: rows.count ?? 0 };
      },
    })),
  };
  return { env: { LIBRARY: pgDb(client) }, close: () => sql.end() };
}

export async function runCli(argv = process.argv.slice(2)) {
  const command = argv[0];
  const { values } = parseArgs({
    args: argv.slice(1),
    options: {
      order: { type: "string" },
      zone: { type: "string" },
      placement: { type: "string" },
      before: { type: "string" },
      after: { type: "string" },
      why: { type: "string" },
      engine: { type: "string" },
      policy: { type: "string" },
      sort: { type: "string" },
      riven: { type: "string" },
      builds: { type: "string" },
      "build-ids": { type: "string" },
      weapons: { type: "string" },
      pin: { type: "string" },
      tags: { type: "string" },
      classes: { type: "string" },
      rulers: { type: "string" },
      modes: { type: "string" },
      base: { type: "string" },
      json: { type: "boolean", default: false },
    },
  });

  const base = values.base || process.env.WFSIM_BASE_URL || "https://wfsim.app";
  const token = process.env.FACTORY_TOKEN || "";
  const direct = await openDirectEnv();

  const invoke = async (action, body) => {
    if (direct) {
      const mod = await import("../worker/board_orders.js");
      const now = Date.now();
      if (action === "list") return { ok: true, ...(await mod.inspectQueueAndOrders(direct.env, now)) };
      if (action === "preview") return mod.previewBoardOrder(direct.env, body);
      if (action === "create") return mod.createBoardOrder(direct.env, body, now);
      if (action === "admit") return mod.admitBoardOrderSlice(direct.env, body, now);
      if (action === "rezone") return mod.rezoneBoardOrder(direct.env, body, now);
      if (action === "reorder") return mod.reorderBoardOrder(direct.env, body, now);
      if (action === "sync") return mod.syncBoardOrderPositions(direct.env, body, now);
      if (action === "cancel") return mod.cancelBoardOrder(direct.env, body, now);
      if (action === "why") return mod.updateBoardOrderReason(direct.env, body, now);
      throw new Error(`unknown action: ${action}`);
    }
    if (!token) throw new Error("Set WFSIM_COMPUTE_ORDER_DATABASE_URL or FACTORY_TOKEN");
    return callRemote(base, token, action, body);
  };

  try {
    if (command === "preview") {
      const res = await invoke("preview", {
        zone: values.zone !== undefined ? Number(values.zone) : 0,
        engine: values.engine,
        specification: parseSpecFlags(values),
      });
      if (values.json) {
        console.log(JSON.stringify(res, null, 2));
      } else {
        const s = res.summary;
        console.log(`Preview [zone ${String(res.zone).padStart(2, "0")} · engine ${res.engine} · policy ${res.specification.policy} · sort ${res.specification.sort}]`);
        console.log(`  matched weapons: ${s.weapons_matched} (${s.weapons_with_items} with items)`);
        console.log(`  builds: ${s.builds_count} · total items: ${s.total_items} · distinct tasks: ${s.distinct_computations}`);
        if (s.by_ruler.length) console.log(`  by ruler: ${s.by_ruler.map((r) => `${r.ruler}=${r.count}`).join(", ")}`);
        if (s.by_weapon.length) console.log(`  top weapons: ${s.by_weapon.slice(0, 12).map((w) => `${w.weapon}(${w.count}, pop=${w.popularity})`).join(", ")}`);
        if (s.preview.length) {
          console.log("  first items:");
          for (const row of s.preview.slice(0, 15)) {
            console.log(`    #${row.ordinal} ${row.weapon} | ${row.ruler} | ${row.mode} | score=${row.score ?? "—"} | build=${row.identity.slice(0, 8)}`);
          }
        }
      }
      return;
    }

    if (command === "create") {
      let res = await invoke("create", {
        order_id: values.order,
        zone: values.zone !== undefined ? Number(values.zone) : 0,
        placement: values.placement || "tail",
        why: values.why,
        engine: values.engine,
        specification: parseSpecFlags(values),
      });
      if (!res.ok) throw new Error(res.reason || "create failed");
      const orderId = res.order_id;
      while (!res.done) {
        process.stderr.write(`  admitting ${orderId}: ${res.admitted_items}/${res.total_items}...\n`);
        res = await invoke("admit", { order_id: orderId });
      }
      console.log(values.json ? JSON.stringify(res, null, 2) : `Created & admitted ${orderId}: ${res.admitted_items}/${res.total_items} items (${res.state})`);
      return;
    }

    if (command === "list") {
      const res = await invoke("list", {});
      if (values.json) {
        console.log(JSON.stringify(res, null, 2));
        return;
      }
      console.log(`Queue at ${res.at} (open=${res.totals.open_questions}, leased=${res.totals.leased_questions}, waited=${res.totals.waited_questions}, active_orders=${res.totals.active_orders})`);
      for (const z of res.zones) {
        console.log(`  Zone ${String(z.zone).padStart(2, "0")}: open=${z.open} leased=${z.leased} waited=${z.waited} interactive=${z.interactive_open}`);
        for (const o of z.orders) {
          console.log(`    [order] ${o.id} base=${o.order_base} settled=${o.settled_items}/${o.total_items} open=${o.open_questions} policy=${o.policy} — ${o.why}`);
        }
        for (const l of z.legacy) {
          console.log(`    [${l.producer}] ${l.key} open=${l.open_questions}${l.why ? ` — ${l.why}` : ""}`);
        }
      }
      return;
    }

    if (command === "rezone") {
      let res = await invoke("rezone", {
        order_id: values.order,
        zone: Number(values.zone),
        placement: values.placement || "tail",
      });
      if (!res.ok) throw new Error(res.reason || "rezone failed");
      while (!res.done) res = await invoke("sync", { order_id: values.order });
      console.log(values.json ? JSON.stringify(res, null, 2) : `Moved ${values.order} to zone ${String(res.zone).padStart(2, "0")} (base=${res.order_base})`);
      return;
    }

    if (command === "reorder") {
      let res = await invoke("reorder", {
        order_id: values.order,
        placement: values.placement,
        before_order_id: values.before,
        after_order_id: values.after,
      });
      if (!res.ok) throw new Error(res.reason || "reorder failed");
      while (!res.done) res = await invoke("sync", { order_id: values.order });
      console.log(values.json ? JSON.stringify(res, null, 2) : `Reordered ${values.order} in zone ${String(res.zone).padStart(2, "0")} (base=${res.order_base})`);
      return;
    }

    if (command === "cancel") {
      let res = await invoke("cancel", { order_id: values.order });
      if (!res.ok) throw new Error(res.reason || "cancel failed");
      while (!res.done) res = await invoke("cancel", { order_id: values.order });
      console.log(values.json ? JSON.stringify(res, null, 2) : `Cancelled ${values.order}`);
      return;
    }

    if (command === "why") {
      const res = await invoke("why", { order_id: values.order, why: values.why });
      if (!res.ok) throw new Error(res.reason || "why failed");
      console.log(values.json ? JSON.stringify(res, null, 2) : `Updated reason on ${values.order}`);
      return;
    }

    throw new Error("usage: board_order.mjs <preview|create|list|rezone|reorder|cancel|why> [flags]");
  } finally {
    if (direct) await direct.close();
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  runCli().catch((err) => {
    console.error(`board_order: ${err.message || err}`);
    process.exit(1);
  });
}
