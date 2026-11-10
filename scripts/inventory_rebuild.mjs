// SPDX-License-Identifier: AGPL-3.0-or-later
// A request records intent and budget; only a matching explicit request may open a new generation.
import postgres from './pg/node_modules/postgres/src/index.js';
import { isAbsolute, join } from 'node:path';
import { InventoryControl, inspectInventoryTask } from './inventory_control.mjs';
import { inventoryContext } from './inventory_pipeline.mjs';
import { relationalInventoryContext } from './inventory_relational_pipeline.mjs';
import { openInventoryOperator } from './inventory_operator.mjs';

const [command, ...args] = process.argv.slice(2), options = {};
for (let i = 0; i < args.length; i += 2) {
  if (!/^--[a-z-]+$/.test(args[i]) || !args[i + 1] || args[i + 1].startsWith('--')) throw Error('options require explicit values');
  options[args[i].slice(2)] = args[i + 1];
}
if (!options.work || !isAbsolute(options.work)) throw Error('--work requires the absolute live working directory');
const root = join(options.work, 'inventory');
if (command === 'status') {
  console.log(JSON.stringify({ task: inspectInventoryTask(root) }));
} else {
  const control = new InventoryControl(root); let sql, operator;
  try {
    if (command === 'cancel') {
      console.log(JSON.stringify({ task: control.cancel(options.task, options.reason) }));
    } else if (command === 'request') {
      if (!options.bin || !isAbsolute(options.bin) || !['classic', 'relational'].includes(options.mode) || !process.env.PG_URL)
        throw Error('request requires --mode classic|relational, absolute --bin, and PG_URL');
      if (options.mode === 'relational') operator = await openInventoryOperator(join(options.bin, 'wfsim-board'));
      const context = options.mode === 'classic' ? await inventoryContext(options.bin) : await relationalInventoryContext(options.bin, operator);
      sql = postgres(process.env.PG_URL, { max: 1, fetch_types: false, connect_timeout: 15, onnotice: () => {}, connection: { application_name: 'wfsim-inventory-request', statement_timeout: 10000 } });
      const [{ status }] = await sql.unsafe('SELECT inventory_status() status');
      const task = control.request({ mode: options.mode, ...context, expectedGeneration: status?.generation || null,
        reason: options.reason, maxNewBytes: Number(options['max-new-bytes']) });
      console.log(JSON.stringify({ task, started: false }));
    } else throw Error('use request, status or cancel');
  } finally { await operator?.close(); await sql?.end({ timeout: 5 }); control.close(); }
}
