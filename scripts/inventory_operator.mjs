// SPDX-License-Identifier: AGPL-3.0-or-later
// One request at a time; no engine process retains library-sized state.
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { once } from 'node:events';

export async function openInventoryOperator(binary, { stderr = 'inherit', timeoutMs = 60000 } = {}) {
  const child = spawn(binary, ['--inventory-operator'], { stdio: ['pipe', 'pipe', stderr] });
  const reader = createInterface({ input: child.stdout }), replies = reader[Symbol.asyncIterator]();
  let failed = null, closed = false, busy = false;
  child.on('error', error => { failed = error; });
  child.stdin.on('error', error => { failed = error; });
  const next = async () => {
    let timer;
    try {
      const reply = await Promise.race([replies.next(), new Promise((_, reject) => {
        timer = setTimeout(() => reject(Error('inventory operator deadline')), timeoutMs);
      })]);
      if (failed) throw failed;
      if (reply.done) throw Error('inventory operator closed');
      return JSON.parse(reply.value);
    } finally { clearTimeout(timer); }
  };
  const close = async () => {
    if (closed) return; closed = true;
    reader.close(); child.stdin.end();
    if (child.exitCode === null && child.signalCode === null) {
      let timer;
      try { await Promise.race([once(child, 'exit'), new Promise(resolve => { timer = setTimeout(resolve, 2000); })]); }
      finally { clearTimeout(timer); }
      if (child.exitCode === null && child.signalCode === null) { child.kill('SIGKILL'); await once(child, 'exit'); }
    }
  };
  try {
    const contract = await next();
    if (contract.v !== 1 || !contract.ready || contract.max_items !== 256) throw Error('invalid inventory operator contract');
    return { contract, close, pid: child.pid, call: async (op, items) => {
      if (busy || closed || failed) throw failed || Error('inventory operator unavailable');
      if (!Array.isArray(items) || items.length > contract.max_items) throw Error('inventory operator item bound');
      const body = JSON.stringify({ op, items }) + '\n';
      if (Buffer.byteLength(body) > 8 * 1024 * 1024) throw Error('inventory operator byte bound');
      busy = true;
      try {
        if (!child.stdin.write(body)) await once(child.stdin, 'drain');
        const response = await next();
        if (!response.ok) throw Error(response.error || 'inventory operator refused');
        if (!Array.isArray(response.items) || response.items.length !== items.length) throw Error('incomplete inventory operator response');
        return response.items;
      } catch (error) { failed = error; throw error; }
      finally { busy = false; }
    } };
  } catch (error) { await close(); throw error; }
}
