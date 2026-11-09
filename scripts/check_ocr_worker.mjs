// SPDX-License-Identifier: AGPL-3.0-or-later
// A failed OCR worker releases every caller and the next read can start fresh.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const workers = [];
let failConstruct = false, failPost = false, closed = 0;
class Worker extends EventTarget {
  constructor() {
    super();
    if (failConstruct) throw new Error('worker creation failed');
    this.sent = [];
    workers.push(this);
  }
  postMessage(message) {
    if (failPost) throw new Error('image transfer failed');
    this.sent.push(message);
  }
  terminate() { this.stopped = true; }
  reply(index, extra) {
    this.dispatchEvent(new MessageEvent('message', {data: {id: this.sent[index].id, ...extra}}));
  }
}
const context = vm.createContext({Worker, Blob, URL, location: {href: 'https://wfsim.test/weapons/Braton/rivens'},
  createImageBitmap: async () => ({close() { closed++; }})});
vm.runInContext(readFileSync(process.env.WFSIM_OCR_SOURCE
  || new URL('../web/src/static/app/25-riven-ocr.js', import.meta.url), 'utf8'), context);
const read = () => vm.runInContext('ocrLines(new Blob())', context);
const settle = async () => { await Promise.resolve(); await Promise.resolve(); };
const bounded = promise => Promise.race([promise, new Promise((_, reject) => {
  const timer = setTimeout(() => reject(new Error('OCR caller never settled')), 500);
  promise.finally(() => clearTimeout(timer)).catch(() => {});
})]);
const succeeds = async () => {
  const result = read();
  await settle();
  const worker = workers.at(-1);
  worker.reply(worker.sent.length - 1, {lines: [{text: '+100% Damage'}]});
  assert.equal((await bounded(result))[0].text, '+100% Damage');
  return worker;
};

const first = read(), second = read();
await settle();
assert.equal(workers.length, 1);
workers[0].reply(1, {lines: [{text: 'second'}]});
workers[0].reply(0, {lines: [{text: 'first'}]});
assert.equal((await bounded(first))[0].text, 'first');
assert.equal((await bounded(second))[0].text, 'second');
console.log('ok  concurrent reads share one worker and match response ids');

for (const failure of ['message', 'error', 'messageerror']) {
  const pending = [read(), read()];
  const results = Promise.allSettled(pending);
  await settle();
  const worker = workers.at(-1);
  if (failure === 'message') worker.reply(worker.sent.length - 2, {error: 'model failed'});
  else worker.dispatchEvent(Object.assign(new Event(failure, {cancelable: true}), {message: 'worker failed'}));
  assert((await bounded(results)).every(r => r.status === 'rejected'), failure);
  assert(worker.stopped, failure);
  const fresh = await succeeds();
  assert.notEqual(fresh, worker);
  console.log(`ok  ${failure} failure rejects every caller and a fresh worker succeeds`);
}
failPost = true;
await assert.rejects(bounded(read()), /image transfer failed/);
assert(workers.at(-1).stopped);
failPost = false;
failConstruct = true;
await assert.rejects(bounded(read()), /worker creation failed/);
failConstruct = false;
await succeeds();
assert.equal(closed, 2);
console.log('ok  failed worker creation and image transfer close images and remain retryable');
