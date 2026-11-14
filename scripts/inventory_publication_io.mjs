// SPDX-License-Identifier: AGPL-3.0-or-later
// The database cursor describes only fsynced output; extra tail bytes are replayed.
import { createReadStream, openSync, closeSync, fsyncSync, ftruncateSync, writeSync, existsSync,
  mkdirSync, statSync, renameSync, rmSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname } from 'node:path';

export async function fileHash(path) {
  const hash = createHash('sha256');
  for await (const bytes of createReadStream(path)) hash.update(bytes);
  return hash.digest('hex');
}
export function syncDirectory(path) {
  const fd = openSync(dirname(path), 'r');
  try { fsyncSync(fd); } finally { closeSync(fd); }
}
export async function outputWriter(context, path, receipt = { size: 0, sha: null }) {
  const partial = `${path}.inventory-partial`;
  if (context.writer?.path === path && !context.writer.closed && context.writer.size === receipt.size
      && (!receipt.sha || context.writer.hash.copy().digest('hex') === receipt.sha)) return context.writer;
  context.writer?.close(); context.writer = null;
  mkdirSync(dirname(path), { recursive: true });
  if (!existsSync(partial) && receipt.size) {
    if (existsSync(path) && statSync(path).size === receipt.size && await fileHash(path) === receipt.sha)
      return { renamed: true, path, size: receipt.size, sha: receipt.sha };
    throw Error('durable publication prefix is unavailable');
  }
  const fd = openSync(partial, 'a+');
  const hash = createHash('sha256');
  try {
    if (statSync(partial).size < receipt.size) throw Error('durable publication prefix is truncated');
    ftruncateSync(fd, receipt.size);
    for await (const bytes of createReadStream(partial)) hash.update(bytes);
    if (receipt.sha && hash.copy().digest('hex') !== receipt.sha) throw Error('durable publication prefix changed');
    const writer = { path, partial, fd, hash, size: receipt.size, closed: false,
      append(text) {
        const bytes = Buffer.from(text); let offset = 0;
        while (offset < bytes.length) offset += writeSync(fd, bytes, offset);
        hash.update(bytes); this.size += bytes.length;
      },
      checkpoint() { fsyncSync(fd); return { size: this.size, sha: hash.copy().digest('hex') }; },
      close() { if (!this.closed) { closeSync(fd); this.closed = true; } },
      async finish() {
        const receipt = this.checkpoint(); this.close(); context.writer = null;
        const same = existsSync(path) && statSync(path).size === receipt.size && await fileHash(path) === receipt.sha;
        if (same) rmSync(partial); else renameSync(partial, path);
        syncDirectory(path); return { ...receipt, changed: !same };
      },
    };
    context.writer = writer; return writer;
  } catch (error) { closeSync(fd); throw error; }
}
