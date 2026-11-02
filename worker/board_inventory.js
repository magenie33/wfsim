// SPDX-License-Identifier: AGPL-3.0-or-later
// Accept complete board scans through the factory's authenticated producer port.
import { factoryAuthed } from "./factory.js";
import { BOARD_KINDS } from "./board_kinds.js";
import { inventoryWrite } from "./task_inventory.js";

export async function boardInventoryRoute(request, env) {
  const reply = (body, status = 200) => Response.json(body, { status, headers: { "cache-control": "no-store" } });
  if (request.method !== "POST") return reply({ ok: false, error: "POST only" }, 405);
  if (!factoryAuthed(request, env)) return reply({ ok: false, error: "unauthorized" }, 401);
  let write;
  try {
    const plan = await request.json();
    write = inventoryWrite({ ...plan, kind: "board", verb: "simulate", covers: BOARD_KINDS });
  } catch (_) { return reply({ ok: false, error: "incomplete task inventory" }, 400); }
  const db = env.LIBRARY;
  await db.batch([db.prepare("SET LOCAL statement_timeout = 180000"), db.prepare(write.sql).bind(...write.params)]);
  return reply({ ok: true });
}
