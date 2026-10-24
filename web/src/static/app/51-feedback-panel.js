// ---- feedback to the owner -----------------------------------------------------
//
// docs/ACCOUNTS.md §"Feedback". A mark on the bar beside the bell opens a
// panel with one box: the reader says what they see, pastes or drops game
// screenshots, and sends. Nothing is asked of them but the words — where they
// were (the page, the build's share link, the release) goes with it unasked.

const FEEDBACK_TEXT_MAX = 4000;
const FEEDBACK_IMAGES_MAX = 3;
/// UNDER THE WORKER'S 1.5 MB, with room: a screenshot past it is drawn smaller.
const FEEDBACK_IMAGE_BYTES = 1_400_000;
/// The long side a screenshot is drawn at first; 1440p reads every number.
const FEEDBACK_IMAGE_SIDE = 2560;

/// What is being written: the words, the screenshots as JPEG data URLs, and
/// the line under the box (an error, or sending).
let feedbackDraft = { images: [], note: null, sending: false };

/// A SCREENSHOT AS THE WORKER TAKES IT: a JPEG, its long side at most
/// `FEEDBACK_IMAGE_SIDE`, drawn smaller until it fits.
async function feedbackJpeg(blob) {
  const bitmap = await createImageBitmap(blob);
  let side = Math.min(FEEDBACK_IMAGE_SIDE, Math.max(bitmap.width, bitmap.height));
  for (let tries = 0; tries < 6; tries++) {
    const k = side / Math.max(bitmap.width, bitmap.height);
    const c = new OffscreenCanvas(Math.round(bitmap.width * k), Math.round(bitmap.height * k));
    c.getContext("2d").drawImage(bitmap, 0, 0, c.width, c.height);
    const out = await c.convertToBlob({ type: "image/jpeg", quality: 0.85 });
    if (out.size <= FEEDBACK_IMAGE_BYTES) {
      return await new Promise((ok) => { const r = new FileReader(); r.onload = () => ok(r.result); r.readAsDataURL(out); });
    }
    side = Math.round(side * 0.75);
  }
  return null;
}

async function feedbackAdd(files) {
  const images = [...files].filter((f) => f.type.startsWith("image/"));
  for (const f of images) {
    if (feedbackDraft.images.length >= FEEDBACK_IMAGES_MAX) { feedbackDraft.note = tr("Up to 3 screenshots."); break; }
    const url = await feedbackJpeg(f).catch(() => null);
    if (url) feedbackDraft.images.push(url);
  }
  renderFeedbackShots();
}

/// WHERE THE READER WAS: the page, and on a weapon the build on screen as its
/// long share link — self-contained, so nothing is written to the link store.
async function feedbackWhere() {
  const where = { route: location.pathname + location.search, release: RELEASE_ID, lang: LANG };
  if (location.pathname.startsWith("/weapons/")) {
    try {
      const w = weaponInfo($("weapon").value);
      if (w) where.share = `${location.origin}${weaponPath(w.id)}?${SHARE_PARAM}=${await shareCode()}`;
    } catch (_) { /* sent without it */ }
  }
  return where;
}

function renderFeedbackShots() {
  const box = $("feedback-shots");
  if (!box) return;
  box.innerHTML = feedbackDraft.images.map((url, i) =>
    `<span class="fbk-shot"><img src="${url}" alt=""><button type="button" class="fbk-x" data-fbk-drop="${i}" title="${aT("Remove")}" aria-label="${aT("Remove")}">✕</button></span>`).join("");
  const add = $("feedback-add");
  if (add) add.hidden = feedbackDraft.images.length >= FEEDBACK_IMAGES_MAX;
  const note = $("feedback-note");
  if (note) { note.textContent = feedbackDraft.note || ""; note.hidden = !feedbackDraft.note; }
}

function renderFeedbackPanel() {
  const panel = $("feedback-panel");
  if (!panel) return;
  const { account } = accountState;
  if (!account) {
    const here = location.pathname + location.search;
    panel.innerHTML = `<div class="fbk-body"><p class="fbk-lede">${aT("Sign in to send feedback.")}</p>
      <a class="run-btn btn-sm" href="/login?return=${encodeURIComponent(authKindOf(location.pathname) ? "/" : here)}">${aT("Sign in")}</a></div>`;
    return;
  }
  panel.innerHTML = `<div class="fbk-body">
      <textarea id="feedback-text" rows="5" maxlength="${FEEDBACK_TEXT_MAX}" placeholder="${aT("What's off, or what would you change?")}"></textarea>
      <div class="fbk-shots"><span id="feedback-shots"></span><label class="fbk-add ghost-btn btn-sm" id="feedback-add">${aT("Add game screenshots")}<input type="file" id="feedback-file" accept="image/*" multiple hidden></label></div>
      <div class="fine">${aT("Paste or drop up to 3. Only we see them; cover names and chat first.")}</div>
      <div class="auth-err" id="feedback-note" role="alert" hidden></div>
      <button type="button" class="run-btn btn-sm" id="feedback-send">${aT("Send")}</button>
    </div>`;
  renderFeedbackShots();
}

/// THE MARK ON THE BAR: wherever accounts are offered, as the account's own is.
function renderFeedbackEntry() {
  const box = $("feedback");
  if (!box) return;
  box.hidden = !accountState.providers.length && !accountState.account;
}

async function feedbackSend() {
  const text = (($("feedback-text") || {}).value || "").trim();
  if (!text || feedbackDraft.sending) return;
  feedbackDraft.sending = true;
  feedbackDraft.note = null;
  $("feedback-send").disabled = true;
  const r = await accountCall("POST", "/api/feedback", { text, images: feedbackDraft.images, ...(await feedbackWhere()) }, 30000);
  feedbackDraft.sending = false;
  if (r && r.ok) {
    feedbackDraft = { images: [], note: null, sending: false };
    feedbackOpen(false);
    renderFeedbackPanel();
    presetToast(tr("Received."));
    return;
  }
  feedbackDraft.note = accountSaid((r && r.reason) || "tell_failed");
  $("feedback-send").disabled = false;
  renderFeedbackShots();
}

function feedbackOpen(open) {
  const box = $("feedback");
  if (!box) return;
  const was = box.classList.contains("open");
  box.classList.toggle("open", open);
  $("feedback-btn").setAttribute("aria-expanded", open ? "true" : "false");
  // THE WORDS SURVIVE A CLOSE: the panel is drawn afresh only when it opens
  // from shut, and keeps what was typed by carrying it over.
  if (open && !was) {
    const typed = (($("feedback-text") || {}).value) || "";
    renderFeedbackPanel();
    const t = $("feedback-text");
    if (t) { t.value = typed; t.focus(); }
  }
}

(function () {
  const box = $("feedback");
  if (!box) return;
  $("feedback-btn").addEventListener("click", (e) => { e.stopPropagation(); feedbackOpen(!box.classList.contains("open")); });
  const panel = $("feedback-panel");
  panel.addEventListener("click", (e) => {
    e.stopPropagation();
    const drop = e.target.closest("[data-fbk-drop]");
    if (drop) { feedbackDraft.images.splice(Number(drop.dataset.fbkDrop), 1); feedbackDraft.note = null; renderFeedbackShots(); return; }
    if (e.target.closest("#feedback-send")) feedbackSend();
  });
  panel.addEventListener("change", (e) => {
    if (e.target.id === "feedback-file") { feedbackAdd(e.target.files); e.target.value = ""; }
  });
  // CTRL+ENTER SENDS, as in every box that sends a message; Enter is a new line.
  panel.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && (e.ctrlKey || e.metaKey) && e.target.id === "feedback-text" && !imeComposing(e)) { e.preventDefault(); feedbackSend(); }
  });
  panel.addEventListener("paste", (e) => {
    const files = [...(e.clipboardData || {}).files || []].filter((f) => f.type.startsWith("image/"));
    if (!files.length) return;
    e.preventDefault();
    feedbackAdd(files);
  });
  panel.addEventListener("dragover", (e) => { e.preventDefault(); });
  panel.addEventListener("drop", (e) => { e.preventDefault(); feedbackAdd((e.dataTransfer || {}).files || []); });
  document.addEventListener("click", (e) => { if (box.classList.contains("open") && !box.contains(e.target)) feedbackOpen(false); });
  document.addEventListener("keydown", (e) => { if (e.key === "Escape" && box.classList.contains("open") && !imeComposing(e)) feedbackOpen(false); });
})();
