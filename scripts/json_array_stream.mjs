// SPDX-License-Identifier: AGPL-3.0-or-later
// Preserve one JSON value's bytes at a time, including whitespace inside it.
import { createReadStream } from 'node:fs';

export async function* jsonArrayRows(path, { maxBytes = 8 * 1024 ** 2, offset = 0, withOffsets = false } = {}) {
  let opened = offset > 0, ended = false, active = false, quoted = false, escaped = false, depth = 0;
  let allowEnd = !offset, parts = [], bytes = 0, chunkOffset = offset, last;
  const append = text => {
    parts.push(text); bytes += Buffer.byteLength(text);
    if (bytes > maxBytes) throw Error('JSON array value exceeds byte bound');
  };
  for await (const chunk of createReadStream(path, { start: offset, encoding: 'utf8', highWaterMark: 65536 })) {
    let start = active ? 0 : -1;
    for (let i = 0; i < chunk.length; i++) {
      const c = chunk[i];
      if (ended) { if (!/\s/.test(c)) throw Error('JSON data after array'); continue; }
      if (!opened) {
        if (/\s/.test(c)) continue;
        if (c !== '[') throw Error('JSON array required');
        opened = true; continue;
      }
      if (!active) {
        if (/\s/.test(c)) continue;
        if (c === ']') {
          if (!allowEnd) throw Error('JSON trailing comma');
          ended = true; continue;
        }
        if (c === ',') throw Error('JSON missing array value');
        active = true; start = i;
      }
      if (quoted) {
        if (escaped) escaped = false;
        else if (c === '\\') escaped = true;
        else if (c === '"') quoted = false;
      } else if (c === '"') quoted = true;
      else if (c === '[' || c === '{') depth++;
      else if (depth > 0 && (c === ']' || c === '}')) depth--;
      else if (depth === 0 && (c === ',' || c === ']')) {
        append(chunk.slice(start, i));
        const raw = parts.join('').trim(); JSON.parse(raw);
        const row = withOffsets ? { raw, offset: chunkOffset + Buffer.byteLength(chunk.slice(0, i + 1)), done: c === ']' } : raw;
        if (c === ']') last = row; else yield row;
        parts = []; bytes = 0; active = false; start = -1;
        allowEnd = c === ']'; ended = allowEnd;
      }
    }
    if (active && start >= 0) append(chunk.slice(start));
    chunkOffset += Buffer.byteLength(chunk);
  }
  if (!opened || !ended || active || quoted || depth) throw Error('incomplete JSON array');
  if (last !== undefined) yield last;
}
