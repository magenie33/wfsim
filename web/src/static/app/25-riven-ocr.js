// ---- Reading a riven off a screenshot ---------------------------------
//
// PP-OCRv4 (PaddleOCR's models, Apache-2.0) on onnxruntime-web (MIT), in a
// worker of its own, in this browser: the image never leaves it unless the
// reader offers it. The files are served same-origin under /ocr/, pinned by
// web/ocr/pins.json and fetched only the first time a screenshot is read.
// What the lines MEAN is `rivenFromOcr`'s (87-headless.js), shared with the bot.

const OCR_BASE = "/ocr/";

/// THE WORKER, as one function the page turns into a Blob: it reaches only
/// what a worker has, so the page and a bot's node can run the same body.
function ocrWorkerMain() {
  let ready = null;
  const boot = (base) => {
    importScripts(base + "ort-1.30.0.wasm.min.js");
    // A .js URL keeps the module's JavaScript MIME type on each static host.
    ort.env.wasm.wasmPaths = { mjs: base + "ort-wasm-simd-threaded-1.30.0.js",
      wasm: base + "ort-wasm-simd-threaded-1.30.0.wasm" };
    ort.env.wasm.numThreads = self.crossOriginIsolated ? Math.min(4, navigator.hardwareConcurrency || 1) : 1;
    return Promise.all([
      ort.InferenceSession.create(base + "ch-ppocrv4-det.onnx"),
      ort.InferenceSession.create(base + "ch-ppocrv4-rec.onnx"),
      fetch(base + "ppocr-keys-v1.txt").then((r) => r.text()),
    ]).then(([det, rec, keys]) => ({ det, rec, keys: [""].concat(keys.split(/\r?\n/).filter((k) => k !== ""), [" "]) }));
  };
  const pixels = (bitmap, sx, sy, sw, sh, w, h) => {
    const c = new OffscreenCanvas(w, h).getContext("2d");
    c.drawImage(bitmap, sx, sy, sw, sh, 0, 0, w, h);
    return c.getImageData(0, 0, w, h).data;
  };
  const tensor = (px, w, h, mean, std) => {
    const out = new Float32Array(3 * w * h);
    for (let i = 0; i < w * h; i++) {
      for (let c = 0; c < 3; c++) out[c * w * h + i] = (px[i * 4 + c] / 255 - mean[c]) / std[c];
    }
    return new ort.Tensor("float32", out, [1, 3, h, w]);
  };
  /// TEXT ROWS: the detector's map above 0.3, cut into rows, and a row cut
  /// again where a gap is wider than two line heights.
  const boxes = (map, W, H) => {
    const on = (x, y) => map[y * W + x] > 0.3;
    const out = [];
    for (let y = 0; y < H;) {
      let any = false;
      for (let x = 0; x < W && !any; x++) any = on(x, y);
      if (!any) { y++; continue; }
      const y0 = y;
      for (; y < H; y++) {
        let row = false;
        for (let x = 0; x < W && !row; x++) row = on(x, y);
        if (!row) break;
      }
      const cols = [];
      for (let x = 0; x < W; x++) for (let yy = y0; yy < y; yy++) if (on(x, yy)) { cols.push(x); break; }
      let a = cols[0];
      for (let i = 1; i <= cols.length; i++) {
        if (i === cols.length || cols[i] - cols[i - 1] > 2 * (y - y0)) { out.push([a, y0, cols[i - 1], y]); a = cols[i]; }
      }
    }
    return out;
  };
  const read = async (m, bitmap) => {
    const w = bitmap.width, h = bitmap.height;
    const s = 960 / Math.max(w, h);
    const W = Math.max(32, Math.floor((w * s) / 32) * 32), H = Math.max(32, Math.floor((h * s) / 32) * 32);
    const det = await m.det.run({ [m.det.inputNames[0]]: tensor(pixels(bitmap, 0, 0, w, h, W, H), W, H,
      [0.485, 0.456, 0.406], [0.229, 0.224, 0.225]) });
    const map = det[m.det.outputNames[0]].data;
    const lines = [];
    for (const [x0, y0, x1, y1] of boxes(map, W, H)) {
      const py = (y1 - y0) * 0.5, px = (y1 - y0) * 1.2;
      const bx = Math.max(0, (x0 - px) * w / W), by = Math.max(0, (y0 - py) * h / H);
      const bw = Math.min(w, (x1 + px) * w / W) - bx, bh = Math.min(h, (y1 + py) * h / H) - by;
      if (bw < 4 || bh < 4) continue;
      const rw = Math.min(1600, Math.max(16, Math.round((48 * bw) / bh)));
      const rec = await m.rec.run({ [m.rec.inputNames[0]]: tensor(pixels(bitmap, bx, by, bw, bh, rw, 48), rw, 48,
        [0.5, 0.5, 0.5], [0.5, 0.5, 0.5]) });
      const out = rec[m.rec.outputNames[0]];
      const [, T, C] = out.dims;
      let text = "", last = 0;
      for (let t = 0; t < T; t++) {
        let k = 0;
        for (let c = 1; c < C; c++) if (out.data[t * C + c] > out.data[t * C + k]) k = c;
        if (k !== last && k !== 0) text += m.keys[k] || "";
        last = k;
      }
      if (text.trim()) lines.push({ text: text.trim(), box: [bx, by, bw, bh] });
    }
    return lines;
  };
  onmessage = async (e) => {
    try {
      ready = ready || boot(e.data.base);
      const m = await ready;
      postMessage({ id: e.data.id, lines: await read(m, e.data.bitmap) });
    } catch (err) {
      postMessage({ id: e.data.id, error: String((err && err.message) || err) });
    } finally {
      e.data.bitmap.close();
    }
  };
}

let ocrWorker = null;
let ocrSeq = 0;
const ocrPending = new Map();
function ocrWorkerFailed(worker, error) {
  if (worker !== ocrWorker) return;
  ocrWorker = null;
  worker.terminate();
  for (const pending of ocrPending.values()) pending.reject(error);
  ocrPending.clear();
}
/// THE LINES ON AN IMAGE, each `{ text, box: [x, y, w, h] }` in its pixels.
async function ocrLines(blob) {
  const bitmap = await createImageBitmap(blob);
  if (!ocrWorker) {
    const url = URL.createObjectURL(new Blob([`(${ocrWorkerMain.toString()})()`], { type: "text/javascript" }));
    try { ocrWorker = new Worker(url); }
    catch (error) { bitmap.close(); throw error; }
    finally { URL.revokeObjectURL(url); }
    const worker = ocrWorker;
    worker.addEventListener("message", (e) => {
      if (worker !== ocrWorker) return;
      const pending = ocrPending.get(e.data.id);
      if (!pending) return;
      if (e.data.error) { ocrWorkerFailed(worker, new Error(e.data.error)); return; }
      ocrPending.delete(e.data.id);
      pending.resolve(e.data.lines);
    });
    worker.addEventListener("error", (e) => {
      e.preventDefault();
      ocrWorkerFailed(worker, new Error(e.message || "OCR worker failed"));
    });
    worker.addEventListener("messageerror", () => ocrWorkerFailed(worker, new Error("OCR worker message could not be read")));
  }
  const worker = ocrWorker;
  const id = ++ocrSeq;
  return new Promise((resolve, reject) => {
    ocrPending.set(id, { resolve, reject });
    try { worker.postMessage({ id, base: new URL(OCR_BASE, location.href).href, bitmap }, [bitmap]); }
    catch (error) { bitmap.close(); ocrWorkerFailed(worker, error); }
  });
}

/// THE CARD'S OWN RECTANGLE, around every line read, as a JPEG — what an
/// offered screenshot carries, so a chat or a name beside the card stays home.
async function ocrCardCrop(blob, lines) {
  const bitmap = await createImageBitmap(blob);
  if (!lines.length) return null;
  const x0 = Math.min(...lines.map((l) => l.box[0])), y0 = Math.min(...lines.map((l) => l.box[1]));
  const x1 = Math.max(...lines.map((l) => l.box[0] + l.box[2])), y1 = Math.max(...lines.map((l) => l.box[1] + l.box[3]));
  const pad = Math.round((y1 - y0) * 0.04);
  const sx = Math.max(0, x0 - pad), sy = Math.max(0, y0 - pad);
  const sw = Math.min(bitmap.width, x1 + pad) - sx, sh = Math.min(bitmap.height, y1 + pad) - sy;
  const c = new OffscreenCanvas(sw, sh);
  c.getContext("2d").drawImage(bitmap, sx, sy, sw, sh, 0, 0, sw, sh);
  return c.convertToBlob({ type: "image/jpeg", quality: 0.9 });
}

// ---- the editor's side ----------------------------------------------------
//
// READ, REVIEW, THEN CONFIRM. What the screenshot read becomes a new card, and
// a window opens holding the screenshot and the riven editor itself until OK.
// While the window is up an edit means "this was misread"; after OK it is a
// change of mind. So a correction is sent from OK and nowhere else, and Cancel
// deletes the card the way its ✕ does.

/// The card under review, or null: its id, what the screenshot read as, and
/// whether the reader ticked the box.
let ocrReview = null;
/// The status line under the tools: reading, a failure, sending, sent.
let ocrNote = null;

/// WHETHER THIS BROWSER SENDS CORRECTIONS — a preference of the browser, like
/// the board's. Unticked until the reader ticks it once; either answer is then
/// remembered, and the box shows it every time so it can be changed there.
const OCR_GIVE_KEY = "wfsim-ocr-give";
const ocrGivePref = () => { try { return localStorage.getItem(OCR_GIVE_KEY) === "1"; } catch (_) { return false; } };
const setOcrGivePref = (on) => { try { localStorage.setItem(OCR_GIVE_KEY, on ? "1" : "0"); } catch (_) {} };

async function ocrRead(blob) {
  const w = weaponInfo($("weapon").value);
  if (!w || !blob) return;
  ocrNote = { state: "reading", first: !ocrWorker };
  renderRivenOcr();
  try {
    const lines = await ocrLines(blob);
    const pool = (META.riven_stats || {})[w.riven_class] || [];
    const res = rivenFromOcr(pool, [I18N].concat(ALT_NAMES || []).filter(Boolean), lines.map((l) => l.text));
    const at = (x) => ({ id: x.id, roll: 1, value: x.value });
    const r = res.bonuses.length
      ? await api("/api/riven", { weapon: w.id, bonuses: res.bonuses.map(at),
        malus: res.malus ? at(res.malus) : null, rank: rivenRules().max_rank, polarity: "madurai" })
      : { stats: [] };
    const roll = (slot) => ((r.stats || []).find((s) => s.slot === slot) || { roll: 1 }).roll;
    // A CARD HAS TWO BONUSES AT LEAST; what the screenshot did not give, the
    // reader picks in the editor.
    const bonuses = res.bonuses.map((b, i) => ({ id: b.id, roll: roll(String(i)) }));
    while (bonuses.length < 2) bonuses.push({ id: null, roll: 1 });
    const id = newRiven({ bonuses, malus: res.malus ? { id: res.malus.id, roll: roll("malus") } : null,
      rank: rivenRules().max_rank, polarity: "madurai" });
    ocrReview = { id, blob, url: URL.createObjectURL(blob), lines, unread: res.unread, give: ocrGivePref(),
      first: { bonuses: res.bonuses, malus: res.malus },
      read: lines.filter((l) => ocrNumber(l.text) && !res.unread.includes(l.text)) };
    ocrNote = null;
    openOcrWindow(ocrReview);
  } catch (e) {
    ocrNote = { state: "error", note: String((e && e.message) || e) };
  }
  renderRivenOcr();
}

/// What the open card SHOWS, in the screenshot's own terms: each stat and the
/// number printed beside it.
function ocrCardAsShown() {
  const stats = (rivenResolved && rivenResolved.stats) || [];
  // AT THE CARD'S OWN PRECISION — the number a screenshot of it would print.
  const shown = (s) => ({ id: s.id, value: Number(Number(s.shown).toFixed(s.decimals ?? 1)) });
  return { bonuses: stats.filter((s) => s.bonus).map(shown),
    malus: (stats.find((s) => !s.bonus) && shown(stats.find((s) => !s.bonus))) || null,
    rank: riven ? riven.rank : null };
}

/// OK: the review ends, and — when ticked — the read and what the card now
/// shows are sent together.
async function ocrConfirm() {
  const v = ocrReview;
  if (!v) return;
  const confirmed = ocrCardAsShown();
  ocrReview = null;
  closeOcrWindow();
  ocrNote = v.give ? { state: "sending" } : null;
  renderRivenOcr();
  if (v.give) {
    ocrNote = { state: await ocrGive(v, confirmed) };
    renderRivenOcr();
  }
  URL.revokeObjectURL(v.url);
}

/// Cancel: the card goes, exactly as ✕ takes it.
function ocrCancel() {
  const v = ocrReview;
  if (!v) return;
  ocrReview = null;
  closeOcrWindow();
  URL.revokeObjectURL(v.url);
  deleteRiven(v.id);
  renderRivenOcr();
}

/// THE CORRECTION: the card's rectangle, every line read, what they read as and
/// what the reader confirmed. Answers how it went: sent, closed or failed.
async function ocrGive(v, confirmed) {
  try {
    const crop = await ocrCardCrop(v.blob, v.read.length ? v.read : v.lines);
    const image = await new Promise((res, rej) => {
      const f = new FileReader();
      f.onload = () => res(f.result);
      f.onerror = rej;
      f.readAsDataURL(crop);
    });
    const r = await fetch("/api/ocr/sample", { method: "POST", headers: { "content-type": "application/json" },
      body: JSON.stringify({ weapon: $("weapon").value, lang: LANG, image, lines: v.lines.map((l) => l.text),
        read: v.first, confirmed }) });
    return r.ok ? "sent" : r.status === 501 || r.status === 404 ? "closed" : "failed";
  } catch (_) {
    return "failed";
  }
}

/// THE STATUS LINE under the tools: reading, a failure, sending, sent.
function renderRivenOcr() {
  const box = $("riven-ocr");
  if (!box) return;
  const o = ocrNote;
  const text = !o ? "" : o.state === "reading"
    ? escHtml(tr("Reading the screenshot…"))
      + (o.first ? ` <span class="sb-empty">${escHtml(tr("the first time downloads the reader, about 30 MB"))}</span>` : "")
    : o.state === "error" ? `${escHtml(tr("The screenshot could not be read"))}: ${escHtml(o.note)}`
    : escHtml({ sending: tr("sending…"), sent: tr("correction sent — thank you"),
      closed: tr("not taken here"), failed: tr("could not be sent") }[o.state] || "");
  box.innerHTML = text
    ? `<div class="rv-ocr-in">${text}</div><button class="cu-btn rv-ocr-x" title="${escHtml(tr("close"))}">✕</button>` : "";
  const x = box.querySelector(".rv-ocr-x");
  if (x) x.onclick = () => { ocrNote = null; renderRivenOcr(); };
}

/// THE REVIEW WINDOW: the screenshot beside THE RIVEN EDITOR ITSELF, moved in
/// for as long as the read is being corrected — the same controls a card typed
/// by hand is made with, so the window owns nothing but OK and Cancel.
function openOcrWindow(v) {
  const win = $("riven-ocr-win");
  const title = escHtml(tr("What the screenshot reads as"));
  win.innerHTML = `<div class="rv-ocr-back"></div>
    <div class="rv-ocr-card" role="dialog" aria-modal="true" aria-label="${title}">
      <div class="jump-head"><span class="jh-t">${title}</span></div>
      <div class="rv-ocr-body">
        <div class="rv-ocr-side"><img class="rv-ocr-shot" src="${v.url}" alt="">
          ${v.unread.length ? `<div class="sb-empty">${escHtml(tr("not read"))}: ${v.unread.map(escHtml).join(" · ")}</div>` : ""}</div>
        <div><div class="sb-empty">${escHtml(tr("Correct anything misread, then press OK."))}</div>
          <div class="rv-ocr-slot"></div></div>
      </div>
      <div class="rv-ocr-foot">
        <label class="rv-ocr-give"><input type="checkbox" class="rv-ocr-givebox" ${v.give ? "checked" : ""}>
          <span>${escHtml(tr("Send this read and your corrections to WFSim, with the rectangle around the card's stats, to make reading better"))}</span></label>
        <button class="cu-btn rv-ocr-cancel">${escHtml(tr("Cancel"))}</button>
        <button class="cu-btn rv-ocr-ok">${escHtml(tr("OK"))}</button>
      </div>
    </div>`;
  win.querySelector(".rv-ocr-slot").appendChild($("riven-editor"));
  win.querySelector(".rv-ocr-givebox").onchange = (e) => { v.give = e.target.checked; setOcrGivePref(v.give); };
  win.querySelector(".rv-ocr-cancel").onclick = ocrCancel;
  win.querySelector(".rv-ocr-ok").onclick = ocrConfirm;
  win.hidden = false;
}

/// …and the editor back where it lives, under the status line.
function closeOcrWindow() {
  const win = $("riven-ocr-win");
  $("riven-ocr").after($("riven-editor"));
  closePopovers();
  win.hidden = true;
  win.innerHTML = "";
}

/// THREE WAYS IN, wired once: the button's file, Ctrl+V on the Rivens tab, and
/// a file dropped on its block.
let ocrWired = false;
function wireRivenOcr() {
  if (ocrWired || !$("riven-ocr-file")) return;
  ocrWired = true;
  $("riven-ocr-file").addEventListener("change", (e) => {
    const f = e.target.files && e.target.files[0];
    e.target.value = "";
    if (f) ocrRead(f);
  });
  document.addEventListener("paste", (e) => {
    if (!document.body.classList.contains("on-rivens") || ocrReview) return;
    const item = [...((e.clipboardData && e.clipboardData.items) || [])].find((x) => x.type.startsWith("image/"));
    if (!item) return;
    e.preventDefault();
    ocrRead(item.getAsFile());
  });
  const block = $("riven-block");
  block.addEventListener("dragover", (e) => { if ([...e.dataTransfer.types].includes("Files")) e.preventDefault(); });
  block.addEventListener("drop", (e) => {
    const f = [...(e.dataTransfer.files || [])].find((x) => x.type.startsWith("image/"));
    if (!f) return;
    e.preventDefault();
    ocrRead(f);
  });
}
