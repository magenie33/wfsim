// ---- support: the donation channels -------------------------------------
// A channel is drawn only when it HAS a working link. An option that does not
// work yet is worse than one that is not offered, so an entry with an empty
// `url` renders nothing — and filling that url in is the whole of adding one.
//
// A CHANNEL A READER CANNOT PAY THROUGH IS NOT AN OPTION.
// Ko-fi wants a card or PayPal, which is not how anyone in mainland China pays
// for anything — so a Chinese reader was being shown one channel and offered
// none. The ones that match the display language come first.
//
// ORDERED, NEVER FILTERED, which is the rule the topbar's community links
// follow and for the same reason: a reader who can use the other one still has
// to be able to find it. `locale: null` means "works anywhere" and sorts
// between the two, and the per-locale ORDER is all that lives here.
const chRank = (c) => (c.locale === LANG ? 0 : c.locale ? 2 : 1);
const SUPPORT_CHANNELS = [
  {
    id: "bilibili",
    name: "Bilibili",
    // THE ONE CHANNEL A MAINLAND READER CAN ACTUALLY PAY THROUGH, which is the
    // whole reason `locale` exists: the other two want a card or PayPal, so a
    // Chinese reader was shown a page of options and offered none of them.
    //
    // THE AUTHOR'S SPACE PAGE, not a payment url. Bilibili's charge button
    // lives there and the flow never leaves an app the reader is already signed
    // into; a deep link into that flow is a url only Bilibili may build.
    url: "https://space.bilibili.com/1965302",
    locale: "zh",
    what: "One-off or monthly, in CNY, from inside Bilibili — no card, and no new account.",
  },
  {
    id: "kofi",
    // THE ACCOUNT IS THE PROJECT, AND THE PAGE BEHIND IT IS A PERSON. `ko-fi.com/wfsim` reads as the same thing as `wfsim.app` at
    // the one moment a reader is deciding whether they are in the right place;
    // an account named after the author does not, and that half-second sits
    // exactly where the money is. The FIRST-PERSON half is not lost — it moves
    // to the Ko-fi page's own bio, which is where poe.ninja puts it too.
    name: "Ko-fi",
    url: "https://ko-fi.com/wfsim",
    locale: "en",
    // ONE-OFF ONLY. Ko-fi's memberships are a subscription with perks, which
    // is the one shape DE's non-commercial rule does not allow — they stay
    // switched off in the account, and so do its shop and commissions.
    //
    // NO AMOUNT, HERE OR ON ANY CARD. Every channel shows its own minimum and
    // its own ladder at the moment of paying, which is one screen away and
    // always current; a number repeated here is a staler copy of that, and a
    // page that names a sum has set an expectation it did not mean to.
    what: "One-off, in USD. Card or PayPal, no account needed.",
  },
  {
    id: "patreon",
    name: "Patreon",
    // EMPTY UNTIL THE ACCOUNT EXISTS, which is the rule above: an option that
    // does not work yet is worse than one that is not offered, and filling this
    // url in is the whole of adding the channel.
    //
    // TWO CHANNELS, TWO JOBS. Ko-fi is the one-off and
    // Patreon is the month, and the split is what keeps either from being a
    // worse version of the other. There is no third: the donation-page
    // evidence is about REMOVING choices at the moment of decision, and a
    // developer-facing channel belongs in the README, where developers are.
    //
    // WHAT THE MONTH BUYS is stated on the page and bounded there: order and
    // company, never product — never the weapon queue, since a roster fills
    // and a perk that quietly expires is worse than none.
    url: "",
    locale: null,
    what: "Monthly, in USD. The channel where the work is discussed, and your reports read first.",
  },
];

/// HOW MUCH OF THIS THE READER HAS ACTUALLY USED — on their own machine, and
/// nowhere else.
///
/// The strongest thing this page can say is not what the project is, it is what
/// it has already done FOR the person reading, and that is a number the app can
/// count rather than claim. It is two integers in one key: engagements are what
/// the machine actually paid for (a run at the rulers' 1000 is a thousand of
/// them), and simulations are what the reader remembers doing.
///
/// IT NEVER LEAVES THE BROWSER, and the page says so where it prints it —
/// which is the same promise the board makes about a submission, made in the
/// one place a reader might reasonably suspect otherwise.
///
/// TWO INTEGERS, which is the whole of its storage budget: a measurement costs
/// its summary and never its replay, and a counter that grew per run would be
/// the same mistake one size down.
const SUPPORT_USE = "wfsim-use";
function supportUse() {
  try {
    const v = JSON.parse(localStorage.getItem(SUPPORT_USE) || "{}");
    return { sims: v.sims | 0, engagements: v.engagements | 0 };
  } catch (_) { return { sims: 0, engagements: 0 }; }
}
function noteSimRun(engagements) {
  const v = supportUse();
  v.sims += 1;
  v.engagements += Math.max(1, engagements | 0);
  try { localStorage.setItem(SUPPORT_USE, JSON.stringify(v)); } catch (_) { /* private mode */ }
}

/// THE ONE TIME THIS APP ASKS, AND IT ASKS WHERE THE ANSWER LANDED. /support
/// is a footer link, read only by somebody who went looking; the moment worth
/// asking in is the one straight after an answer the reader waited for.
///
/// ONCE PER BROWSER, EVER. A line that comes back is an advertisement, which
/// DE's Content Policy permits only while it stays out of the way of the
/// content — so it is one sentence, it carries no button, and the flag that
/// retires it is set the first time it is drawn.
///
/// AN OPTIMIZER RUN IS THE OCCASION, NEVER THE QUALIFICATION: it is one click
/// and thousands of engagements nobody watched, so qualifying on it would ask
/// a reader who has run nothing. `SUPPORT_USE`'s own count is the gate.
const SUPPORT_ASKED = "wfsim-asked";
const NUDGE_AFTER = 10;
function offerSupportOnce(host) {
  const u = supportUse();
  if (!host || u.sims < NUDGE_AFTER) return;
  // A BROWSER THAT CANNOT REMEMBER IS NEVER ASKED, which is the safe half of
  // the choice: the alternative asks it on every run.
  try {
    if (localStorage.getItem(SUPPORT_ASKED)) return;
    localStorage.setItem(SUPPORT_ASKED, "1");
  } catch (_) { return; }
  const p = document.createElement("p");
  p.className = "sup-nudge";
  p.innerHTML = `${escHtml(trF("{n} answers out of this so far, and not one of them cost you anything.",
    { n: u.sims.toLocaleString() }))} <a href="/support">${escHtml(tr("What it costs to run →"))}</a>`;
  host.appendChild(p);
}

/// WHO HAS CHIPPED IN, BY NAME — the only thing that ever leaves the ledger.
///
/// A FILE, NOT AN ENDPOINT. `scripts/publish_thanks.py` reads the ledger, works
/// out the order and writes `site/thanks.json`, which is committed the way
/// `site/board/` is. Nothing at the edge is bound to the ledger, so no request
/// to this site can ask what anybody gave.
///
/// ORDERED, NEVER NUMBERED, and no rank, no band, no size. The order combines
/// what somebody gave with how long ago they first gave it; printing a position
/// beside a name would turn a thank-you into a leaderboard, which is the one
/// thing the page above it promises it is not.
///
/// SILENT WHEN IT IS EMPTY, the rule every count on `/support` follows: a
/// heading over nothing is worse than no heading.
let thanksDoc = null;
const thanksWaiting = [];
function thanksAsk(then) {
  if (thanksDoc !== null && thanksDoc !== "asking") { then(); return; }
  thanksWaiting.push(then);
  if (thanksDoc === "asking") return;
  thanksDoc = "asking";
  const land = (v) => { thanksDoc = v; thanksWaiting.splice(0).forEach((f) => f()); };
  fetch("/thanks.json")
    .then((r) => (r.ok ? r.json() : null))
    // AN UNPUBLISHED LIST ARRIVES AS THE APP'S OWN HTML, with a 200: the SPA
    // fallback answers every unmatched path with index.html. `.json()` is what
    // tells a missing file from an empty one, so the catch IS the not-found.
    .then((j) => land(j && Array.isArray(j.supporters) ? j : "failed"))
    .catch(() => land("failed"));
}
function thanksNames() {
  return (thanksDoc && typeof thanksDoc === "object" && thanksDoc.supporters) || [];
}

/// HOW MANY NAMES `/support` SHOWS BEFORE IT DEFERS TO `/thanks`. The block
/// sits under the channels rather than over them: it is there to say that
/// people do this, not to be read instead of the thing above it.
const THANKS_PEEK = 24;
function drawThanks(block, list, limit) {
  const all = thanksNames();
  block.hidden = all.length === 0;
  if (!all.length) return;
  const shown = limit ? all.slice(0, limit) : all;
  // THE NAME AND NOTHING ELSE. `since` stays in the published file — it is
  // what the ordering is derived from and worth keeping as a record — but a
  // month printed beside a name IS a number beside a name, which is the one
  // thing /thanks tells the reader it does not do.
  list.innerHTML = shown.map((s) => `<li class="thx-one">`
    + `<span class="thx-name">${escHtml(s.name)}</span></li>`).join("");
  const more = block.querySelector(".thx-more");
  if (more) more.hidden = all.length <= shown.length;
}
function renderThanksPage() {
  const block = $("thanks-block");
  const list = $("thanks-list");
  if (!block || !list) return;
  const draw = () => {
    drawThanks(block, list, 0);
    const none = $("thanks-none");
    if (none) none.hidden = thanksNames().length > 0;
  };
  thanksAsk(draw);
  draw();
}

/// THE FACTS STRIP — what this repository holds, counted rather than claimed.
///
/// Two sources and the split is deliberate: anything the page can count for
/// itself is counted here and is right on the dev server too, and only what
/// lives outside the shipped data — tests, checks, measurements, commits —
/// comes from `PROJECT_FACTS`, which the site build writes. A figure that is
/// missing is DROPPED rather than drawn as a zero.
/// EVERYTHING THE ROSTER HOLDS, summed rather than listed — so a category
/// added to `META` counts itself and this function is not edited again. That
/// is the whole reason it is a sum: the roster grows sideways (frames today,
/// companions and Necramechs next) and a hand-written list of categories is a
/// figure that silently stops being true.
function rosterSize() {
  const mods = new Set();
  for (const pool of Object.values(META.mod_pools || {})) {
    for (const m of pool || []) mods.add(m.id);
  }
  // A weapon's evolutions arrive as TIERS, and what is modelled is the perks
  // inside them.
  let evolutions = 0;
  for (const w of META.weapons || []) {
    for (const tier of w.evolutions || []) evolutions += (tier.options || []).length;
  }
  const n = (k) => (META[k] || []).length;
  return n("weapons") + n("frames") + mods.size + evolutions
    + n("arcanes") + n("abilities") + n("auras") + n("shards") + n("enemies");
}

/// WHAT THE BOARD HAS BEEN ASKED, and what it answered.
///
/// SUBMISSIONS ARE ONE POOL AND SCORES ARE PER RULER, which is why these two
/// fields are summed differently: every ruler reports the SAME submission
/// count because they all read the same pool, so adding them would state the
/// uploads three times. `listed` is that ruler's own answers and does add up.
function boardCount(field) {
  const all = Object.values((BOARD_META && BOARD_META.boards) || {});
  if (!all.length) return 0;
  // BOTH BRANCHES READ `field`, which is also what keeps the one spelling of
  // `.submissions` in this file on the binding `check_release_identity` guards:
  // that field must be read from the FETCHED stamp, never the copy compiled
  // into the wasm, or the board dates itself by the build.
  return field === "submissions"
    ? Math.max(...all.map((b) => b[field] | 0))
    : all.reduce((t, b) => t + (b[field] | 0), 0);
}

function projectFacts() {
  // IDENTIFIED, because the home hero states these too. Picking by label would
  // break the moment a label is reworded, and a hero silently short of a
  // number is the failure.
  //
  // THREE ORDERS OF MAGNITUDE, ON PURPOSE: thousands, thousands, tens of
  // thousands read as three different facts, where two figures of the same
  // size read as one fact printed twice. They are also the three modules in
  // the order a reader meets them — what is modelled, what was built with it,
  // what came back out.
  const rows = [
    ["roster", rosterSize(), "items modelled"],
    ["builds", boardCount("submissions"), "builds players have uploaded"],
    ["evaluations", boardCount("listed"), "evaluations computed"],
  ];
  return rows.filter(([, n]) => n > 0).map(([id, n, what]) => ({ id, n, what }));
}

/// THE HERO'S NUMBERS — the same three the support page states, and all of
/// them. A figure whose source has not landed is DROPPED, so a dev server with
/// no board draws the roster alone rather than a zero or a placeholder.
const HERO_FACTS = ["roster", "builds", "evaluations"];

function renderHomeFacts() {
  const el = $("home-facts");
  if (!el) return;
  const rows = projectFacts().filter((f) => HERO_FACTS.includes(f.id));
  el.hidden = !rows.length;
  el.innerHTML = rows.map((f) => `<span class="hf"><b>${
    escHtml(f.n.toLocaleString())}</b> ${escHtml(tr(f.what))}</span>`).join("");
}

/// WHAT THIS CLIENT IS RUNNING, in the three identifiers of
/// docs/DISTRIBUTION.md §Identity: the release, the board, the shell.
///
/// EVERY LINE IS OMITTED WHEN IT WOULD BE A GUESS. The dev server has no
/// release and no board stamp, a browser has no shell version, and a line
/// saying `dev` beside two real digests is worse than three lines that are all
/// true — a reader quoting it would be quoting nothing.
///
/// ASYNC ONLY FOR THE SHELL, which is one IPC call away and may not answer at
/// all. The two lines that are already known are drawn first and the shell
/// appends itself, so a wedged bridge costs the shell line and not the block.
async function identityLines() {
  const el = $("support-identity");
  if (!el) return;
  const lines = [];
  if (RELEASE_ID !== "dev") {
    lines.push(trF("release {r} · commit {c}", { r: RELEASE_ID, c: BUILD_SHA }));
  }
  if (BOARD_META && BOARD_META.digest) {
    lines.push(trF("board {d} · scored {t} · {n} rows", {
      d: BOARD_META.digest.slice(0, 12),
      t: BOARD_META.scored_at
        ? new Date(BOARD_META.scored_at * 1000).toLocaleString()
        : "—",
      n: (BOARD_META.rows || 0).toLocaleString(),
    }));
  }
  const draw = () => {
    el.hidden = !lines.length;
    el.innerHTML = lines.map(escHtml).join("<br>");
  };
  draw();
  if (!window.__WFSIM_DESKTOP__ || !window.__TAURI_INTERNALS__) return;
  try {
    const v = await window.__TAURI_INTERNALS__.invoke("app_version");
    if (v) {
      lines.push(trF("shell {v}", { v }));
      draw();
    }
  } catch (_) { /* a shell that cannot say is a line that is not drawn */ }
}

function renderSupport() {
  const facts = $("support-facts");
  if (facts) {
    facts.innerHTML = projectFacts().map((f) => `
      <div class="sup-fact"><b>${escHtml(f.n.toLocaleString())}</b><span>${escHtml(tr(f.what))}</span></div>`).join("");
  }
  // WHEN IT STARTED AND HOW MUCH HAS HAPPENED SINCE. One line rather than two
  // more tiles: it is context for the strip above, not a sixth figure.
  const built = $("support-built");
  if (built) {
    const f = PROJECT_FACTS || {};
    const say = f.commits && f.first_commit_day;
    built.hidden = !say;
    if (say) {
      built.textContent = tr("Built in the open since {day} — {n} commits, every one of them public.")
        .replace("{day}", f.first_commit_day).replace("{n}", f.commits.toLocaleString());
    }
  }
  identityLines();
  const used = $("support-usage");
  if (used) {
    const u = supportUse();
    used.hidden = !u.sims;
    if (u.sims) {
      used.textContent = tr("You have run {n} simulations on this machine — {e} engagements. That number is in this browser and has never been sent anywhere.")
        .replace("{n}", u.sims.toLocaleString()).replace("{e}", u.engagements.toLocaleString());
    }
  }
  const box = $("support-channels");
  if (box) {
    box.innerHTML = SUPPORT_CHANNELS.filter((c) => c.url)
      .sort((a, b) => chRank(a) - chRank(b)).map((c) => `
      <a class="sup-card" href="${escHtml(c.url)}" target="_blank" rel="noopener">
        <div class="sup-name">${escHtml(c.name)}</div>
        <div class="sup-what">${escHtml(tr(c.what))}</div>
        <span class="run-btn">${escHtml(tr("Open"))} ↗</span>
      </a>`).join("");
  }
  const thanksBlock = $("support-thanks");
  const thanksList = $("support-thanks-list");
  if (thanksBlock && thanksList) {
    const draw = () => drawThanks(thanksBlock, thanksList, THANKS_PEEK);
    thanksAsk(draw);
    draw();
  }
}

