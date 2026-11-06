// SPDX-License-Identifier: AGPL-3.0-or-later
// Convert the keyed library snapshot to an array without holding every record.
import { createReadStream, createWriteStream, renameSync, rmSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { once } from 'node:events';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export async function libraryArray(input, output) {
  const partial = `${output}.partial-${process.pid}`, out = createWriteStream(partial);
  let failed = null, count = 0;
  out.on('error', error => { failed = error; });
  const write = async text => {
    if (failed) throw failed;
    if (!out.write(text)) await once(out, 'drain');
  };
  try {
    await write('[');
    for await (const line of createInterface({ input: createReadStream(input), crlfDelay: Infinity })) {
      if (!line.trim()) continue;
      const row = JSON.parse(line);
      await write((count ? ',' : '') + JSON.stringify(row.v ?? null));
      count++;
    }
    await write(']\n');
    out.end(); await once(out, 'finish');
    if (failed) throw failed;
    renameSync(partial, output);
    return count;
  } finally { out.destroy(); rmSync(partial, { force: true }); }
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const [input, output] = process.argv.slice(2);
  if (!input || !output) throw new Error('usage: library_array.mjs <keyed.ndjson> <array.json>');
  console.log(await libraryArray(input, output));
}
