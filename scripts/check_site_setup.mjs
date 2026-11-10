// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';

const dir = mkdtempSync(join(tmpdir(), 'wfsim-site-setup-'));
const helper = resolve('.github/actions/site-tools/install.sh');
const log = join(dir, 'calls');
const sources = join(dir, 'apt');
mkdirSync(join(sources, 'sources.list.d'), { recursive: true });
const ubuntu = 'Types: deb\nURIs: http://azure.archive.ubuntu.com/ubuntu/\nSuites: noble noble-updates\nComponents: main universe\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n';
const extra = 'deb [signed-by=/usr/share/keyrings/vendor.gpg] https://vendor.example/ stable main\n';
writeFileSync(join(sources, 'sources.list.d/ubuntu.sources'), ubuntu);
writeFileSync(join(sources, 'sources.list'), 'deb http://azure.archive.ubuntu.com/ubuntu noble main\n');
writeFileSync(join(sources, 'sources.list.d/vendor.list'), extra);
try {
  const stub = (name, body) => writeFileSync(join(dir, name), '#!/usr/bin/env bash\n' + body + '\n', { mode: 0o755 });
  stub('dpkg-query', 'if [ "$SETUP_INSTALLED" = yes ] || { [ "$SETUP_INSTALLED" = font ] && [[ "$*" == *fonts-noto-cjk ]]; }; then echo "install ok installed"; else exit 1; fi');
  stub('sudo', 'exec "$@"');
  stub('timeout', 'echo "timeout $*" >> "$SETUP_LOG"; shift; exec "$@"');
  stub('apt-get', [
    'echo "apt $*" >> "$SETUP_LOG"; echo "apt diagnostic sentinel"',
    'for arg; do if [[ "$arg" == Dir::Etc::sourcelist=* ]]; then cat "${arg#*=}" >> "$SETUP_LOG"; fi; if [[ "$arg" == Dir::Etc::sourceparts=* ]]; then cat "${arg#*=}"/* >> "$SETUP_LOG"; fi; done',
    'case "$SETUP_FAIL" in',
    '  first-download|both-download) if [[ " $* " == *" install "* && " $* " != *" --no-download "* ]]; then if [ "$SETUP_FAIL" = both-download ] || [ ! -f "$SETUP_STATE" ]; then touch "$SETUP_STATE"; exit 124; fi; fi ;;',
    '  first-update) if [[ " $* " == *" update "* ]] && [ ! -f "$SETUP_STATE" ]; then touch "$SETUP_STATE"; exit 100; fi ;;',
    '  signature) if [[ " $* " == *" update "* ]]; then echo "invalid archive signature"; exit 100; fi ;;',
    '  install) if [[ " $* " == *" --no-download "* ]]; then exit 100; fi ;;',
    'esac',
    'exit 0',
  ].join('\n'));
  const run = (mode, installed = 'no', fail = '0') => {
    writeFileSync(log, '');
    rmSync(join(dir, 'state'), { force: true });
    const r = spawnSync('bash', [helper, mode, '22'], { encoding: 'utf8',
      env: { ...process.env, PATH: dir + ':' + process.env.PATH, SETUP_LOG: log, SETUP_INSTALLED: installed,
        SETUP_FAIL: fail, SETUP_STATE: join(dir, 'state'), WFSIM_APT_SOURCE_DIR: sources } });
    return { ...r, calls: readFileSync(log, 'utf8') };
  };
  for (const failure of ['first-download', 'first-update']) {
    const recovered = run('true', 'no', failure);
    assert.equal(recovered.status, 0, recovered.stdout + recovered.stderr);
    assert.match(recovered.calls, /https:\/\/archive.ubuntu.com\/ubuntu/);
    assert.match(recovered.calls, /Signed-By: \/usr\/share\/keyrings\/ubuntu-archive-keyring.gpg/);
    assert(recovered.calls.includes(extra.trim()), 'other signed repositories are preserved');
    assert.match(recovered.calls, /--no-download/);
    assert.equal(recovered.calls.split('\n').filter(line => line.startsWith('apt ') && line.includes(' --no-download ')).length, 1,
      'package installation runs only after downloads recover');
    assert.match(recovered.stdout, /apt diagnostic sentinel/);
  }
  assert.equal(readFileSync(join(sources, 'sources.list.d/ubuntu.sources'), 'utf8'), ubuntu,
    'fallback does not rewrite system source files');
  const mirrorList = ubuntu.replace('http://azure.archive.ubuntu.com/ubuntu/', 'mirror+file:/etc/apt/apt-mirrors.txt')
    + '\nTypes: deb\nURIs: mirror+file:/etc/apt/apt-mirrors-security.txt\nSuites: noble-security\nComponents: main\nSigned-By: /usr/share/keyrings/ubuntu-archive-keyring.gpg\n';
  writeFileSync(join(sources, 'sources.list.d/ubuntu.sources'), mirrorList);
  const runnerMirror = run('true', 'no', 'first-download');
  assert.equal(runnerMirror.status, 0, runnerMirror.stdout + runnerMirror.stderr);
  assert.doesNotMatch(runnerMirror.calls, /mirror\+file:/, 'Ubuntu runner mirror lists must also switch to the official HTTPS archives');
  assert.match(runnerMirror.calls, /https:\/\/security.ubuntu.com\/ubuntu/);
  assert.match(runnerMirror.calls, /Suites: noble-security/);
  assert.equal(readFileSync(join(sources, 'sources.list.d/ubuntu.sources'), 'utf8'), mirrorList);
  writeFileSync(join(sources, 'sources.list.d/ubuntu.sources'), ubuntu);
  const build = run('true');
  assert.equal(build.status, 0, build.stderr);
  assert.match(build.calls, /fonts-noto-cjk lld-22/);
  assert.doesNotMatch(build.calls, /webkit|appindicator|librsvg/);
  assert.match(build.calls, /timeout 90s .*update/);
  assert.match(build.calls, /timeout 120s .*--download-only/);
  assert.match(build.calls, /timeout 300s .*--no-download/);
  assert.match(build.calls, /--no-install-recommends/);
  assert.match(build.calls, /APT::Update::Error-Mode=any/);
  assert.doesNotMatch(build.calls, /Dir::Etc::source/, 'successful downloads use the runner sources');
  assert.doesNotMatch(build.calls, /allow-unauthenticated|AllowInsecure|trusted=yes/);
  const publish = run('false');
  assert.equal(publish.status, 0, publish.stderr);
  assert.match(publish.calls, /libwebkit2gtk-4.1-dev/);
  assert.match(publish.calls, /libayatana-appindicator3-dev/);
  assert.match(publish.calls, /librsvg2-dev/);
  const prepared = run('publish');
  assert.equal(prepared.status, 0, prepared.stderr);
  assert.match(prepared.calls, /fonts-noto-cjk/);
  assert.match(prepared.calls, /lld-22/);
  assert.doesNotMatch(prepared.calls, /libwebkit|libayatana|librsvg/);
  const installed = run('true', 'yes');
  assert.equal(installed.status, 0);
  assert.equal(installed.calls, '', 'installed packages need no network or sudo');
  const partial = run('true', 'font');
  assert.equal(partial.status, 0);
  assert.match(partial.calls, /lld-22/);
  assert.doesNotMatch(partial.calls, /fonts-noto-cjk/);
  const failed = run('true', 'no', 'both-download');
  assert.equal(failed.status, 124, 'timeout remains a failed job');
  assert.match(failed.stdout, /apt diagnostic sentinel/, 'failed setup retains the package-manager diagnostic');
  assert.doesNotMatch(failed.calls, /--no-download/, 'failed downloads cannot reach package installation');
  const signature = run('true', 'no', 'signature');
  assert.equal(signature.status, 100);
  assert.match(signature.stdout, /invalid archive signature/);
  assert.doesNotMatch(signature.calls, /--no-download/);
  const install = run('true', 'no', 'install');
  assert.equal(install.status, 100);
  assert.doesNotMatch(install.calls, /Dir::Etc::source/, 'installation errors are not retried as mirror failures');
  assert.equal(run('unknown').status, 2);
  console.log('Site setup skips installed dependencies, recovers mirror failures before installation and retains bounded signed-archive failures.');
} finally {
  rmSync(dir, { recursive: true, force: true });
}
