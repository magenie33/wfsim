// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const dir = mkdtempSync(join(tmpdir(), 'wfsim-site-setup-'));
const helper = resolve('.github/actions/site-tools/install.sh');
const log = join(dir, 'calls');
try {
  const stub = (name, body) => writeFileSync(join(dir, name), '#!/usr/bin/env bash\n' + body + '\n', { mode: 0o755 });
  stub('dpkg-query', 'if [ "$SETUP_INSTALLED" = yes ] || { [ "$SETUP_INSTALLED" = font ] && [[ "$*" == *fonts-noto-cjk ]]; }; then echo "install ok installed"; else exit 1; fi');
  stub('sudo', 'exec "$@"');
  stub('timeout', 'echo "timeout $*" >> "$SETUP_LOG"; shift; exec "$@"');
  stub('apt-get', 'echo "apt $*" >> "$SETUP_LOG"; echo "apt diagnostic sentinel"; if [[ " $* " == *" install "* ]]; then exit "$SETUP_FAIL"; fi');
  const run = (mode, installed = 'no', fail = '0') => {
    writeFileSync(log, '');
    const r = spawnSync('bash', [helper, mode, '22'], { encoding: 'utf8',
      env: { ...process.env, PATH: dir + ':' + process.env.PATH, SETUP_LOG: log, SETUP_INSTALLED: installed, SETUP_FAIL: fail } });
    return { ...r, calls: readFileSync(log, 'utf8') };
  };
  const build = run('true');
  assert.equal(build.status, 0, build.stderr);
  assert.match(build.calls, /fonts-noto-cjk lld-22/);
  assert.doesNotMatch(build.calls, /webkit|appindicator|librsvg/);
  assert.match(build.calls, /timeout 180s .*update/);
  assert.match(build.calls, /timeout 300s .*install/);
  assert.match(build.calls, /--no-install-recommends/);
  const publish = run('false');
  assert.equal(publish.status, 0, publish.stderr);
  assert.match(publish.calls, /libwebkit2gtk-4.1-dev/);
  assert.match(publish.calls, /libayatana-appindicator3-dev/);
  assert.match(publish.calls, /librsvg2-dev/);
  const installed = run('true', 'yes');
  assert.equal(installed.status, 0);
  assert.equal(installed.calls, '', 'installed packages need no network or sudo');
  const partial = run('true', 'font');
  assert.equal(partial.status, 0);
  assert.match(partial.calls, /lld-22/);
  assert.doesNotMatch(partial.calls, /fonts-noto-cjk/);
  const failed = run('true', 'no', '124');
  assert.equal(failed.status, 124, 'timeout remains a failed job');
  assert.match(failed.stdout, /apt diagnostic sentinel/, 'failed setup retains the package-manager diagnostic');
  assert.equal(run('unknown').status, 2);
  console.log('Site setup skips installed dependencies, separates GUI builds and retains bounded failures with diagnostics.');
} finally {
  rmSync(dir, { recursive: true, force: true });
}
