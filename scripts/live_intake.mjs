// SPDX-License-Identifier: AGPL-3.0-or-later
// The live loop uses the queue's bounded intake and the same strict ACK boundary.
import { resolve, dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { intakeBatches, runIntakeCommand } from './intake_batches.mjs';
import { BoardPipeline } from './board_pipeline.mjs';

export async function liveIntake(file, { work, bin, incremental, run = runIntakeCommand, ...options } = {}) {
  if (![0,1].includes(incremental)) throw Error('live intake mode must be zero or one');
  const here = dirname(fileURLToPath(import.meta.url));
  const pipeline = new BoardPipeline(work);
  try {
  if (pipeline.paused('intake')) return { paused: true };
  const result = await intakeBatches(file, { ...options, run, intake: join(bin, 'wfsim-intake'),
    onCommitted: incremental === 0 ? builds => run('flock', [join(work, 'mailbox.lock'), process.execPath, join(here, 'live_mailbox.mjs'), 'append', work, builds]) : undefined,
  });
  pipeline.progress('intake',result); return result;
  } finally { pipeline.close(); }
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const [work, bin, mode, file] = process.argv.slice(2);
  if (!work || !bin || !file) throw Error('usage: live_intake.mjs <work> <bin> <mode> <inbox.ndjson>');
  await liveIntake(file, { work, bin, incremental: Number(mode), doneFile: file + '.done' });
}
