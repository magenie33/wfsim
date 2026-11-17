// SPDX-License-Identifier: AGPL-3.0-or-later
import assert from 'node:assert/strict';
import { createPrivateKey, createPublicKey, sign, verify } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export function publicKeys() {
  const source = readFileSync(new URL('../desktop/src/trust.rs', import.meta.url), 'utf8');
  const declaration = source.match(/PUBLIC_KEYS:[\s\S]*?=\s*&\[([\s\S]*?)\]/)?.[1];
  const keys = [...(declaration || '').matchAll(/"([0-9a-f]{64})"/g)].map(match => match[1]);
  assert(keys.length, 'the CLI uses the existing update-channel public keys');
  return keys;
}

const keyObject = hex => createPublicKey({ key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), Buffer.from(hex, 'hex')]), format: 'der', type: 'spki' });

export function verifyRelease(directory, keys = publicKeys()) {
  const body = readFileSync(path.join(directory, 'release.json'));
  const signature = readFileSync(path.join(directory, 'release.json.sig'), 'utf8').trim();
  assert(/^[0-9a-f]{128}$/.test(signature), 'malformed CLI release signature');
  assert(keys.some(key => verify(null, body, keyObject(key), Buffer.from(signature, 'hex'))), 'CLI release signature mismatch');
  const manifest = JSON.parse(body);
  assert(manifest.v === 1 && manifest.launch === 1, 'signed CLI launch contract is required');
  return manifest;
}

export function signRelease(directory, seed, keys = publicKeys()) {
  assert(/^[0-9a-f]{64}$/i.test(seed || ''), 'UPDATE_KEY must be the existing update-channel Ed25519 seed');
  const key = createPrivateKey({ key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), Buffer.from(seed, 'hex')]), format: 'der', type: 'pkcs8' });
  const publicKey = createPublicKey(key).export({ format: 'der', type: 'spki' }).subarray(-32).toString('hex');
  assert(keys.includes(publicKey), 'UPDATE_KEY does not match the update-channel public keys');
  const body = readFileSync(path.join(directory, 'release.json'));
  const manifest = JSON.parse(body);
  assert(manifest.v === 1 && manifest.launch === 1, 'signed CLI launch contract is required');
  writeFileSync(path.join(directory, 'release.json.sig'), sign(null, body, key).toString('hex') + '\n');
  return verifyRelease(directory, keys);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const verifying = process.argv[2] === '--verify';
    const directory = process.argv[verifying ? 3 : 2] || 'target/client-release';
    const manifest = verifying ? verifyRelease(directory) : signRelease(directory, process.env.UPDATE_KEY?.trim());
    console.log(`CLI ${manifest.version}: raw manifest signature ${verifying ? 'verified' : 'created and verified'}`);
  } catch (error) {
    console.error(`CLI signing: ${error.message}`);
    process.exitCode = 1;
  }
}
