// SPDX-License-Identifier: AGPL-3.0-or-later
// Real OCR models must load through static hosting and recover from a bad module response.
import { createServer } from 'node:http';
import { Readable } from 'node:stream';
import { pipeline } from 'node:stream/promises';
import { fileURLToPath } from 'node:url';
import { openApp, serveSite } from './cdp.mjs';

const site = process.env.WFSIM_BASE ? null : await serveSite(fileURLToPath(new URL('../site', import.meta.url)));
const upstream = process.env.WFSIM_BASE || `http://127.0.0.1:${site.address().port}`;
let moduleRequests = 0, samples = 0;
const requests = [];
const proxy = createServer(async (q, s) => {
  try {
    if (q.url.startsWith('/api/ocr/sample')) { samples++; s.writeHead(403); s.end(); return; }
    const url = new URL(q.url, upstream);
    const response = await fetch(url, {method: q.method,
      ...(q.method !== 'GET' && q.method !== 'HEAD' ? {body: Readable.toWeb(q), duplex: 'half'} : {}),
      headers: q.headers});
    const headers = Object.fromEntries([...response.headers].filter(([k]) =>
      !['content-length', 'content-encoding', 'transfer-encoding', 'connection'].includes(k)));
    headers['cache-control'] = 'no-store';
    if (url.pathname.startsWith('/ocr/')) requests.push(url.pathname);
    if (/\/ocr\/ort-wasm-.*\.(mjs|js)$/.test(url.pathname)) {
      moduleRequests++;
      if (moduleRequests === 1 || url.pathname.endsWith('.mjs') || process.env.WFSIM_OCR_BROKEN === '1') {
        headers['content-type'] = 'application/octet-stream';
      }
    }
    s.writeHead(response.status, headers);
    await pipeline(Readable.fromWeb(response.body), s);
  } catch { if (!s.headersSent) s.writeHead(502); s.end(); }
});
await new Promise(resolve => proxy.listen(0, '127.0.0.1', resolve));
let app;
try {
  app = await openApp({base: `http://127.0.0.1:${proxy.address().port}`, boot: 12000, lang: 'en'});
  app.check('OCR models are not loaded until an image is read', requests.length === 0);
  const r = await app.evaluate(`(async () => {
    const canvas = document.createElement('canvas');
    canvas.width = 640; canvas.height = 240;
    const ctx = canvas.getContext('2d');
    ctx.fillStyle = '#222'; ctx.fillRect(0, 0, 640, 240);
    ctx.fillStyle = '#fff'; ctx.font = '32px sans-serif';
    ctx.fillText('+100.0% Damage', 40, 80);
    ctx.fillText('+80.0% Multishot', 40, 150);
    const blob = await new Promise(resolve => canvas.toBlob(resolve));
    const bounded = fn => Promise.race([fn(), new Promise(resolve =>
      setTimeout(() => resolve({error:'OCR timed out'}), 45000))]);
    const read = () => ocrLines(blob).then(lines => ({lines}), error => ({error:String(error)}));
    const first = await bounded(read);
    const second = await bounded(read);
    const third = second.lines ? await bounded(read) : {};
    const pool = META.riven_stats[META.weapons.find(w => w.id === 'braton').riven_class];
    const parsed = rivenFromOcr(pool, [I18N].filter(Boolean), (second.lines || []).map(l => l.text));
    let review = {};
    if (second.lines) {
      history.pushState({}, '', weaponPath('braton') + '/rivens');
      await route();
      const files = new DataTransfer();
      files.items.add(new File([blob], 'riven.png', {type:'image/png'}));
      const input = document.getElementById('riven-ocr-file');
      input.files = files.files;
      input.dispatchEvent(new Event('change', {bubbles:true}));
      for (let i = 0; i < 100 && !ocrReview && ocrNote?.state !== 'error'; i++) {
        await new Promise(resolve => setTimeout(resolve, 50));
      }
      review = {open:!!document.querySelector('#riven-ocr-win [role="dialog"]'),
        bonuses:ocrReview?.first.bonuses, give:ocrReview?.give, error:ocrNote};
      document.querySelector('#riven-ocr-win .rv-ocr-cancel')?.click();
      review.cancelled = !ocrReview;
    }
    return {first, second, third, parsed, review};
  })()`);
  app.check('a non-JavaScript module response rejects the first read',
    !!r.first.error && !r.first.error.includes('timed out'), JSON.stringify(r.first));
  app.check('a subsequent read starts fresh and recognizes both lines',
    r.second.lines?.length === 2, JSON.stringify(r.second));
  app.check('recognized text becomes two numeric riven stats',
    r.parsed.bonuses?.length === 2 && r.parsed.bonuses.some(s => s.value === 100)
      && r.parsed.bonuses.some(s => s.value === 80), JSON.stringify(r.parsed));
  app.check('a warm worker can recognize another image',
    r.third.lines?.length === 2, JSON.stringify(r.third));
  app.check('choosing an image opens the ordinary riven review with both stats',
    r.review.open && r.review.bonuses?.length === 2, JSON.stringify(r.review));
  app.check('review starts without sample consent and cancel discards the draft',
    r.review.give === false && r.review.cancelled);
  app.check('the runtime retries its module after the failed initialization', moduleRequests === 2, String(moduleRequests));
  app.check('recognition does not send an offered sample', samples === 0);
  await app.finish('Riven OCR loads real models and recovers after a failed read');
} finally {
  await app?.close();
  proxy.closeAllConnections();
  proxy.close();
  site?.close();
}
