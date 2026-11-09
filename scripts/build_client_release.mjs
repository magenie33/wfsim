// SPDX-License-Identifier: AGPL-3.0-or-later
import { createHash } from 'node:crypto';
import { copyFileSync, cpSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const args = Object.fromEntries(process.argv.slice(2).map(a => { const i = a.indexOf('='); return [a.slice(0,i), a.slice(i+1)]; }));
const version = args.version;
if (!/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/.test(version || '')) throw new Error('give version=<semver> artifacts=<directory> out=<directory>');
const artifacts = path.resolve(args.artifacts || 'target/client-artifacts');
const out = path.resolve(args.out || 'target/client-release');
mkdirSync(out, { recursive: true });
const files = {};
for (const target of ['x86_64-linux','x86_64-windows','aarch64-macos','x86_64-macos']) {
  const name = `wfsim-${target}${target.endsWith('windows') ? '.exe' : ''}`;
  const bytes = readFileSync(path.join(artifacts, name));
  const sha256 = createHash('sha256').update(bytes).digest('hex');
  files[target] = { name, sha256, size: bytes.length, urls: [
    `https://wfsim-1388973035.cos.ap-shanghai.myqcloud.com/cli/releases/${version}/${name}`,
    `https://github.com/magenie33/wfsim/releases/download/cli-${version}/${name}`,
  ] };
  copyFileSync(path.join(artifacts, name), path.join(out, name));
}
const manifest = { v: 1, version, npm: args.npm === 'true', files };
writeFileSync(path.join(out, 'release.json'), JSON.stringify(manifest, null, 2) + '\n');
writeFileSync(path.join(out, 'SHA256SUMS'), Object.values(files).map(f => `${f.sha256}  ${f.name}\n`).join(''));
const npm = path.join(out, 'npm');
cpSync('client/npm', npm, { recursive: true });
const pkg = JSON.parse(readFileSync(path.join(npm, 'package.json'), 'utf8')); pkg.version = version;
writeFileSync(path.join(npm, 'package.json'), JSON.stringify(pkg, null, 2) + '\n');
copyFileSync(path.join(out, 'release.json'), path.join(npm, 'release.json'));
copyFileSync('LICENSE', path.join(npm, 'LICENSE'));
copyFileSync('client/README.md', path.join(npm, 'README.md'));
console.log(`Prepared CLI ${version}: four native files, immutable manifest, checksums and npm launcher in ${out}`);
