/// THE ARENA AS AN EDITOR — an infinite canvas you pan, zoom and lay a fight
/// out on: it should feel like Miro or n8n, not like a picture with chips
/// beside it.
///
/// WHAT AN EDITOR NEEDS AND A PICTURE DOES NOT: a floor that extends as far as
/// you drag, a tool that paints bodies where the pointer goes, a marquee that
/// selects them, and a drag that moves the whole selection. A fixed viewBox
/// framing itself to its contents rescales a formation under your hand and
/// leaves nowhere to put a body outside the frame.
///
/// THE TRUTH IS STILL `sim`. There is no scene state — every gesture writes
/// `player_at`, `target_at`, `aim_at` or `formation` and redraws from them, so
/// this is a VIEW of the fight in exactly the way the old one was and nothing
/// downstream (the payload, the share link, the checks that read the scenario)
/// has to learn that the picture changed.
///
/// NOTHING MOVES A BODY FOR YOU unless you asked, and the two things that can
/// are switches on screen: SNAP puts what you place on the grid you can SEE,
/// off by default; COLLIDE keeps two bodies out of the same ground, on by
/// default because it is the engine's own contact rule.
function mountArenaCanvas(host, s, en, opts) {
  const ro = () => opts.readonly || officialScenarioActive();

  // THE TARGET'S OWN CONTROLS ARE ALREADY HERE, rendered by the caller into
  // this host. They are MOVED into the scene rather than re-serialised: an
  // `innerHTML` round trip would drop `pick.onclick`, every `data-k` listener
  // the scenario binding attached, and the identity of every node something
  // else holds a reference to. Detach, build, re-attach.
  const side = host.querySelector(".arc-side");
  if (side) side.remove();
  // THE LOGIC LINE: LEFT IS THE VERB, RIGHT IS THE NOUN.
  //
  //   · LEFT — what you are about to DO. The tools, and directly under them
  //     what the active tool acts WITH: the place tool's brush is an enemy, so
  //     the enemy's own card, level and Steel Path live there. Brush settings
  //     belong with the brush. Under those, the two switches, which are about
  //     how the doing behaves.
  //   · RIGHT — what you have SELECTED. Click a body and this says which one,
  //     where it stands, how far off it is and whether the shot is on it. With
  //     nothing picked it describes the fight instead, which is the honest
  //     answer to "what am I looking at".
  //   · BOTTOM — the read-outs, which are nobody's control.
  //
  // A card floating top-right is BOTH of those things at once: it names the
  // enemy you are placing and it is the only thing the right-hand side says.
  // One panel doing two jobs is why neither is findable.
  host.innerHTML =
    `<div class="arc-wrap">
       <canvas class="arc-cv"></canvas>
       <div class="arc-left">
         <div class="arc-rail" role="toolbar" aria-label="${escHtml(tr("Tools"))}"></div>
         <div class="arc-brush"></div>
         <div class="arc-opts"></div>
         <div class="arc-chips"></div>
       </div>
       <div class="arc-insp"></div>
       <div class="arc-read num"></div>
     </div>
     `;
  const wrap = host.querySelector(".arc-wrap");
  // THE BRUSH IS WHAT YOU PLACE, so the caller's enemy card and its fields go
  // into the LEFT column beside the place tool rather than floating on their
  // own. Moved, never re-serialised — see above.
  if (side) host.querySelector(".arc-brush").appendChild(side);
  const cv = host.querySelector(".arc-cv");
  const ctx = cv.getContext("2d");
  const rail = host.querySelector(".arc-rail");
  const optbox = host.querySelector(".arc-opts");
  const readout = host.querySelector(".arc-read");
  const chipbox = host.querySelector(".arc-chips");
  const insp = host.querySelector(".arc-insp");

  // ---- the view ---------------------------------------------------------
  // METRES ARE THE UNIT and `k` is pixels per metre. Nothing here is stored in
  // the scenario: where you are LOOKING is not part of the fight.
  const view = { x: 0, y: 0, k: 26 };
  let W = 0, H = 0;
  const px = (p) => [W / 2 + (p[0] - view.x) * view.k, H / 2 - (p[1] - view.y) * view.k];
  const mt = (x, y) => [view.x + (x - W / 2) / view.k, view.y - (y - H / 2) / view.k];

  let tool = "select";
  let snapGrid = false, collide = true;
  const sel = new Set();          // indices into `arenaBodies(s)`
  let space = false, panning = false, dragging = false, marquee = null;
  let mode = null, last = null, dragFrom = null, painted = null;
  const touches = new Map();
  let pinch = null;

  const gridStep = () => (view.k < 9 ? 10 : 2);
  const snapTo = (p) => {
    if (!snapGrid) return p;
    const g = gridStep();
    return [Math.round(p[0] / g) * g, Math.round(p[1] / g) * g];
  };
  const bodyAt = (i) => (i === 0 ? s.target_at : (s.formation[i - 1] || {}).at);

  // ---- drawing ----------------------------------------------------------
  const cssv = (n) => getComputedStyle(document.documentElement)
    .getPropertyValue("--" + n).trim() || "#888";

  function fit() {
    const pts = [...arenaBodies(s), s.player_at, arenaAim(s)];
    const xs = pts.map((p) => p[0]), ys = pts.map((p) => p[1]);
    const pad = 2;
    const w = Math.max(Math.max(...xs) - Math.min(...xs) + pad * 2, 6);
    const h = Math.max(Math.max(...ys) - Math.min(...ys) + pad * 2, 6);
    view.k = Math.min(160, Math.max(1.6, Math.min(W / w, H / h)));
    view.x = (Math.min(...xs) + Math.max(...xs)) / 2;
    view.y = (Math.min(...ys) + Math.max(...ys)) / 2;
  }

  function grid() {
    const step = gridStep();
    const [x0, y1] = mt(0, 0), [x1, y0] = mt(W, H);
    ctx.lineWidth = 1;
    for (let pass = 0; pass < 2; pass++) {
      const g = pass ? step * 5 : step;
      if (pass === 0 && view.k * g < 11) continue;
      ctx.strokeStyle = pass ? cssv("arc-grid2") : cssv("arc-grid");
      ctx.beginPath();
      for (let x = Math.ceil(x0 / g) * g; x <= x1; x += g) {
        const q = Math.round(px([x, 0])[0]) + 0.5;
        ctx.moveTo(q, 0); ctx.lineTo(q, H);
      }
      for (let y = Math.ceil(y0 / g) * g; y <= y1; y += g) {
        const q = Math.round(px([0, y])[1]) + 0.5;
        ctx.moveTo(0, q); ctx.lineTo(W, q);
      }
      ctx.stroke();
    }
  }

  function draw() {
    if (!W || !H) return;
    ctx.clearRect(0, 0, W, H);
    ctx.fillStyle = cssv("panel3"); ctx.fillRect(0, 0, W, H);
    grid();

    const r = Math.max(2.5, BODY_R_M * view.k);
    const bodies = arenaBodies(s);
    const aim = arenaAim(s);
    const struck = arenaFirstHit(s);
    const you = px(s.player_at), aimS = px(aim);

    // THE MUZZLE, and the line from it. A straight-line weapon does not stop
    // where you point — the aim marker is a DIRECTION, so
    // the line runs through it and off the floor.
    const span = Math.hypot(aim[0] - s.player_at[0], aim[1] - s.player_at[1]) || 1;
    const u = [(aim[0] - s.player_at[0]) / span, (aim[1] - s.player_at[1]) / span];
    const mz = px([s.player_at[0] + u[0] * BODY_R_M, s.player_at[1] + u[1] * BODY_R_M]);
    const su = [u[0], -u[1]];
    ctx.save();
    ctx.strokeStyle = cssv("accent"); ctx.lineWidth = 1.6;
    ctx.beginPath(); ctx.moveTo(mz[0], mz[1]);
    ctx.lineTo(mz[0] + su[0] * 4000, mz[1] + su[1] * 4000); ctx.stroke();
    ctx.restore();

    for (let i = 0; i < bodies.length; i++) {
      const [x, y] = px(bodies[i]);
      if (x < -r * 3 || x > W + r * 3 || y < -r * 3 || y > H + r * 3) continue;
      const on = sel.has(i);
      // ITS UNIT'S COLOUR (see `unitHue`), so a mixed formation reads as one
      // without clicking through it. SELECTION still wins the outline, because
      // "which ones am I about to move" is the more urgent question and the hue
      // is still visible in the fill underneath.
      const col = unitColor(unitAt(s, i));
      ctx.beginPath(); ctx.arc(x, y, r, 0, 7);
      ctx.fillStyle = col;
      ctx.globalAlpha = i === struck ? 0.62 : on ? 0.5 : 0.3; ctx.fill();
      ctx.globalAlpha = 1;
      ctx.lineWidth = on || i === struck ? 2 : 1.25;
      ctx.strokeStyle = on ? cssv("accent") : col;
      ctx.stroke();
      // THE ONE THE SHOT IS ON, ringed: aiming at a place means the answer is
      // not always the body nearest the cursor.
      if (i === struck) {
        ctx.beginPath(); ctx.arc(x, y, r + 4, 0, 7);
        ctx.strokeStyle = cssv("accent"); ctx.lineWidth = 1.3;
        ctx.setLineDash([3, 3]); ctx.stroke(); ctx.setLineDash([]);
      }
    }

    ctx.beginPath(); ctx.arc(you[0], you[1], Math.max(3, r), 0, 7);
    ctx.fillStyle = cssv("gold"); ctx.globalAlpha = 0.45; ctx.fill(); ctx.globalAlpha = 1;
    ctx.lineWidth = 2; ctx.strokeStyle = cssv("gold"); ctx.stroke();
    // …and a short arrow past the muzzle, so being turned by your own aim is
    // visible rather than implied.
    ctx.beginPath(); ctx.moveTo(mz[0], mz[1]);
    ctx.lineTo(mz[0] + su[0] * 11, mz[1] + su[1] * 11); ctx.stroke();

    // THE AIM MARKER, always drawn: it is a place you set, not a lock that
    // finds a body.
    ctx.beginPath(); ctx.arc(aimS[0], aimS[1], 6.5, 0, 7);
    ctx.strokeStyle = cssv("accent"); ctx.lineWidth = 2; ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(aimS[0] - 10, aimS[1]); ctx.lineTo(aimS[0] + 10, aimS[1]);
    ctx.moveTo(aimS[0], aimS[1] - 10); ctx.lineTo(aimS[0], aimS[1] + 10);
    ctx.lineWidth = 1; ctx.stroke();

    if (marquee) {
      const [x0, y0] = marquee.a, [x1, y1] = marquee.b;
      const [lx, ly] = [Math.min(x0, x1), Math.min(y0, y1)];
      const [w, h] = [Math.abs(x1 - x0), Math.abs(y1 - y0)];
      ctx.fillStyle = cssv("accent"); ctx.globalAlpha = 0.1;
      ctx.fillRect(lx, ly, w, h); ctx.globalAlpha = 1;
      ctx.strokeStyle = cssv("accent"); ctx.lineWidth = 1;
      ctx.setLineDash([4, 3]); ctx.strokeRect(lx + 0.5, ly + 0.5, w, h); ctx.setLineDash([]);
    }
    paintReadout();
    paintInspector();
  }

  /// WHAT YOU HAVE PICKED, in the top right and nowhere else.
  ///
  /// A body is identified by its NAME (`e1`, `e2`, …) because that is what the
  /// roll call, the heat map and the debuff table use — a scene that called it
  /// something else would be the fourth spelling of the same body.
  function paintInspector() {
    const bodies = arenaBodies(s);
    const struck = arenaFirstHit(s);
    const nameOf = (i) => (i === 0
      ? tr("the aimed target")
      : (s.formation[i - 1] || {}).id || "e" + (i + 1));
    const row = (k, v) => `<div class="ai-row"><span>${escHtml(k)}</span>`
      + `<span class="num">${escHtml(String(v))}</span></div>`;
    const gapTo = (b) => Math.max(0,
      Math.hypot(b[0] - s.player_at[0], b[1] - s.player_at[1]) - CONTACT_M).toFixed(2) + " m";

    // WHO IT IS COMES FIRST. A body's NAME is `e3` — what
    // the roll call, the heat map and the debuff table call it, so the header
    // has to say it too or they are about different enemies. But `e3` alone
    // does not tell a reader WHICH of fifty bodies they just picked, and
    // finding that out is the whole reason to pick one. So the identity goes
    // above the geometry: the unit, its art, its LEVEL and what it is made of
    // at that level — `enemyMeta` is the enemy card's own line, which reads the
    // engine's answer once `loadTargetStats` has given it.
    //
    // A body placed on the floor carries no unit of its own (the place tool
    // pushes a position and nothing else), so it IS the scenario's enemy at the
    // scenario's level. Reading it that way is what makes this panel agree with
    // what the sim will actually field.
    const unitOf = (i) => {
      const f = i === 0 ? null : s.formation[i - 1] || {};
      const id = (f && f.enemy) || s.enemy;
      return { id, en: allEnemies().find((e) => e.id === id), level: (f && f.level) || s.level };
    };
    // THE SWATCH IS THE KEY TO THE FLOOR. The bodies are drawn in their unit's
    // hue (`unitHue`), which is only useful if the same hue is beside the name
    // somewhere — this is that somewhere.
    const swatch = (id) => `<i class="ai-sw" style="background:${unitColor(id)}"></i>`;
    if (sel.size === 1) {
      const i = [...sel][0];
      const b = bodies[i];
      if (b) {
        const { id, en, level } = unitOf(i);
        const art = IMG(en && en.image);
        insp.innerHTML = `<p class="ai-h">${escHtml(nameOf(i))}</p>`
          + `<div class="ai-who">`
          + (art ? `<img src="${escHtml(art)}" alt="">` : "")
          + `<div><b>${swatch(id)}${escHtml((en && en.name) || tr("Enemy"))}</b>`
          + `<span>${escHtml(tr("Level"))} ${escHtml(String(level))}`
          + `${s.steel_path ? " · " + escHtml(tr("Steel Path")) : ""}</span>`
          + `<span>${escHtml(enemyMeta(en))}</span></div></div>`
          + row(tr("at"), `${b[0].toFixed(1)}, ${b[1].toFixed(1)}`)
          + row(tr("from you"), gapTo(b))
          + row(tr("on the shot line"), i === struck ? tr("yes") : tr("no"));
        return;
      }
    }
    if (sel.size > 1) {
      const picked = [...sel].map((i) => bodies[i]).filter(Boolean);
      const xs = picked.map((q) => q[0]), ys = picked.map((q) => q[1]);
      insp.innerHTML =
        `<p class="ai-h">${escHtml(String(sel.size))} ${escHtml(tr("selected"))}</p>`
        + row(tr("centre"), `${((Math.min(...xs) + Math.max(...xs)) / 2).toFixed(1)}, `
          + `${((Math.min(...ys) + Math.max(...ys)) / 2).toFixed(1)}`)
        + row(tr("spread"), `${(Math.max(...xs) - Math.min(...xs)).toFixed(1)} x `
          + `${(Math.max(...ys) - Math.min(...ys)).toFixed(1)} m`)
        + row(tr("nearest to you"), gapTo(picked.reduce((a, q) =>
          Math.hypot(q[0] - s.player_at[0], q[1] - s.player_at[1])
            < Math.hypot(a[0] - s.player_at[0], a[1] - s.player_at[1]) ? q : a, picked[0])));
      return;
    }
    // NOTHING PICKED IS AN ANSWER TOO: describe the fight, since that is what
    // you are looking at.
    const { id, en, level } = unitOf(0);
    insp.innerHTML = `<p class="ai-h">${escHtml(tr("the fight"))}</p>`
      + `<div class="ai-who">`
      + (IMG(en && en.image) ? `<img src="${escHtml(IMG(en.image))}" alt="">` : "")
      + `<div><b>${swatch(id)}${escHtml((en && en.name) || tr("Enemy"))}</b>`
      + `<span>${escHtml(tr("Level"))} ${escHtml(String(level))}`
      + `${s.steel_path ? " · " + escHtml(tr("Steel Path")) : ""}</span></div></div>`
      + row(tr("enemy count"), bodies.length)
      + row(tr("range"), gapTo(bodies[Math.max(struck, 0)] || s.target_at))
      + row(tr("aim"), s.aim_at ? tr("a place of its own") : tr("on the target"))
      + `<p class="ai-hint">${escHtml(tr("click a body to inspect it"))}</p>`;
  }

  function paintReadout() {
    const n = arenaBodies(s).length;
    readout.textContent =
      `${n} ${n === 1 ? tr("enemy") : tr("enemies")}  ·  ${arenaDistance(s).toFixed(2)} m` +
      (sel.size ? `  ·  ${sel.size} ${tr("selected")}` : "");
  }

  // READ-ONLY IS ASKED LIVE, not baked in at mount: switching to an official
  // ruler must disable these without a re-render.
  const dis = () => (ro() ? " disabled" : "");

  // ---- WHAT THE CANVAS DID NOT REPLACE ----------------------------------
  //
  // There is no row of quick sets under the scene: the canvas is better to
  // use, so the controls beside it are gone. Each was a way to do something a
  // static scene could not: `contact/5/10/20/40 m` because you could not drag
  // to a
  // distance in a picture that reframed itself, `+1` and `+8` because there
  // was no way to place a body, `one enemy` because there was no way to
  // remove one. Dragging, painting and erasing are those four things, done
  // directly.
  //
  // TWO SURVIVE, because nothing replaced them, and they are canvas controls
  // rather than a row of chips:
  //
  //   · AIM AT THE TARGET. Aim becomes a place of its own the moment you click
  //     the floor, and there is no gesture that means "stop being a place" —
  //     so it needs a control or it is a one-way door.
  //   · THE MOVE MODE. Not an edit to the fight at all: it decides whether a
  //     FINGER belongs to the page's scroll or to this scene, and it is drawn
  //     only where a finger can be the pointer.
  function paintChips() {
    chipbox.innerHTML =
      (s.aim_at
        ? `<button class="arc-opt" data-unaim="1"${dis()}>${
          escHtml(tr("aim at the target"))}</button>`
        : "")
      // A FINGER SCROLLS; IT DOES NOT DRAG THE FIGHT. The
      // rule survived the move to a canvas and so did its reason: a browser
      // decides who owns a gesture at `pointerdown` and never gives it back.
      // `maxTouchPoints`, not a `pointer: coarse` query — a touchscreen laptop
      // reports a FINE pointer and has the same problem. Two fingers are still
      // the view whatever this says.
      + (navigator.maxTouchPoints > 0
        ? `<button class="arc-opt${arenaTouchDrag ? " on" : ""}" data-touchdrag="1"
           aria-pressed="${arenaTouchDrag}"
           title="${escHtml(tr("while this is on, dragging a body moves it instead of scrolling the page"))}"
           >✥ ${escHtml(tr("move"))}</button>`
        : "");
  }

  // ---- the tool rail ----------------------------------------------------
  const TOOLS = [
    ["select", "V", tr("Select"), '<path d="M4 3l6 14 2.2-5.6L18 9.2z"/>'],
    ["place", "B", tr("Place enemies"),
      '<circle cx="8" cy="8" r="3.1"/><circle cx="14.5" cy="13.5" r="3.1"/><path d="M4 16.5h2.6M5.3 15.2v2.6"/>'],
    // AIM IS A TOOL, because it stopped being a side effect. A bare click used
    // to aim, which meant every mis-click while selecting silently re-aimed the
    // weapon; and once it does not, a fight whose aim has never been moved has
    // NOTHING TO GRAB — the marker rides the target, so it sits exactly where
    // the body is and the body wins the grab. An explicit verb answers both:
    // pick it up and drag, and the marker stays draggable in Select afterwards
    // because by then it is a thing of its own standing on the floor.
    ["aim", "A", tr("Aim"),
      '<circle cx="11" cy="11" r="6.2"/><path d="M11 1.6v3.4M11 17v3.4M1.6 11H5m12 0h3.4"/>'],
    ["erase", "E", tr("Erase"),
      '<path d="M6.5 16.5h9"/><path d="M4.6 12.4l5.4-5.4a2 2 0 012.8 0l2.6 2.6a2 2 0 010 2.8l-3.6 3.6H7.4z"/>'],
    ["hand", "H", tr("Pan"),
      '<path d="M7 11V5.4a1.3 1.3 0 012.6 0V10m0-.6V4.6a1.3 1.3 0 012.6 0V10m0-.8V5.8a1.3 1.3 0 012.6 0v6.4c0 3.2-2 5.3-5.2 5.3-2.4 0-3.6-1-4.7-2.6L4.6 12a1.3 1.3 0 012.1-1.5z"/>'],
  ];
  function paintRail() {
    rail.innerHTML = TOOLS.map(([id, key, title, svg]) =>
      `<button class="arc-tool" data-tool="${id}" title="${escHtml(title)} (${key})"
         aria-label="${escHtml(title)}" aria-pressed="${id === tool}"${
        id === "hand" ? "" : dis()}>
         <svg viewBox="0 0 22 22">${svg}</svg></button>`).join("");
    optbox.innerHTML =
      `<button class="arc-opt" data-opt="snap" aria-pressed="${snapGrid}"${dis()}
         title="${escHtml(tr("put what you place and drag on the grid you can see"))}">${
        escHtml(tr("snap"))}</button>`
      + `<button class="arc-opt" data-opt="collide" aria-pressed="${collide}"${dis()}
         title="${escHtml(tr("keep two bodies out of the same ground - the engine's own contact rule"))}">${
        escHtml(tr("collide"))}</button>`
      + `<button class="arc-opt" data-opt="fit" title="${escHtml(tr("frame everything"))}">${
        escHtml(tr("fit"))}</button>`;
  }

  const changed = () => {
    paintChips(); paintRail(); draw();
    markScenarioDirty();
    if (opts.after) opts.after();
  };

  // ---- placement --------------------------------------------------------
  const clearAt = (p, skip) => !collide || arenaFree(s, p, skip);
  function placeAt(raw) {
    const p = snapTo(raw);
    const floor = collide ? 0 : BODY_R_M;
    if (painted && Math.hypot(painted[0] - p[0], painted[1] - p[1]) < floor) return;
    if (!arenaPlaceBody(s, p, collide)) return;
    painted = [p[0], p[1]];
    changed();
  }
  function eraseAt(p) {
    const i = hitBody(p);
    // THE TARGET IS NOT ERASABLE. It is the body the fight is scored against
    // and `arenaBodies` reads it from `target_at` — deleting it would be
    // deleting the fight, which is what "one enemy" is for in the other
    // direction.
    if (i === null || i === 0) return;
    s.formation.splice(i - 1, 1);
    sel.clear();
    changed();
  }

  // ---- hit tests --------------------------------------------------------
  // HOW CLOSE COUNTS AS ON IT. A body is small on screen when you are zoomed
  // out, so the grab is given a few pixels of slop — but NEVER more than half
  // the closest two bodies can ever be (`CONTACT_M / 2`). Without that cap the
  // player's slop swallowed the enemy standing at contact, and dragging the
  // enemy silently dragged YOU: at 24 px per metre the slop was 0.42 m and the
  // two of them are 0.4 m apart.
  const grab = () => Math.min(CONTACT_M / 2, Math.max(BODY_R_M, 9 / view.k));
  function hitBody(p) {
    const r = grab();
    const bodies = arenaBodies(s);
    let best = null, bd = Infinity;
    for (let i = 0; i < bodies.length; i++) {
      const d = Math.hypot(bodies[i][0] - p[0], bodies[i][1] - p[1]);
      if (d <= r && d < bd) { bd = d; best = i; }
    }
    return best;
  }
  const hitYou = (p) => Math.hypot(s.player_at[0] - p[0], s.player_at[1] - p[1]) <= grab();
  // THE MARKER IS GRABBABLE ONLY ONCE IT IS A PLACE OF ITS OWN.
  //
  // Until you put it somewhere, aim RIDES THE TARGET — which means the marker
  // sits exactly on the body, and hit-testing it first made the target
  // impossible to drag: every grab took the aim instead. It is set by CLICKING
  // BARE FLOOR (see `end`), which is the gesture the "aim is a direction" model
  // is for and the one the SVG scene had; after that it is a handle like any
  // other.
  const hitAim = (p) => {
    if (!s.aim_at) return false;
    const a = s.aim_at;
    return Math.hypot(a[0] - p[0], a[1] - p[1]) <= grab();
  };

  // ---- sizing -----------------------------------------------------------
  const dpr = () => Math.min(window.devicePixelRatio || 1, 2);
  function resize() {
    const b = cv.getBoundingClientRect();
    if (!b.width || !b.height) return;          // on another tab: nothing to do
    const first = !W;
    W = b.width; H = b.height;
    cv.width = Math.round(W * dpr()); cv.height = Math.round(H * dpr());
    ctx.setTransform(dpr(), 0, 0, dpr(), 0, 0);
    if (first) fit();
    draw();
  }
  new ResizeObserver(resize).observe(cv);

  // ---- gestures ---------------------------------------------------------
  const cursor = () => {
    cv.className = "arc-cv arc-" + (space || tool === "hand" ? "hand" : tool)
      + (panning ? " arc-panning" : "") + (dragging ? " arc-moving" : "");
    // TOUCH-ACTION FOLLOWS THE MODE. `pan-y` keeps the page scrollable through
    // the scene; `none` hands every gesture here — a finger scrolls, it does
    // not drag the fight.
    cv.style.touchAction = arenaTouchDrag ? "none" : "pan-y";
  };

  rail.addEventListener("click", (e) => {
    const b = e.target.closest("[data-tool]");
    if (!b || b.disabled) return;
    tool = b.dataset.tool; paintRail(); cursor();
  });
  optbox.addEventListener("click", (e) => {
    const b = e.target.closest("[data-opt]");
    if (!b || b.disabled) return;
    if (b.dataset.opt === "snap") snapGrid = !snapGrid;
    else if (b.dataset.opt === "collide") collide = !collide;
    else if (b.dataset.opt === "fit") fit();
    paintRail(); draw();
  });
  chipbox.addEventListener("click", (e) => {
    const b = e.target.closest("button");
    if (!b) return;
    // THE MOVE MODE IS NOT AN EDIT to the fight, so it is answered before the
    // official-ruler guard: a ruler's scene refuses to MOVE, and offering a
    // dead toggle beside it would say the opposite.
    if (b.dataset.touchdrag) { arenaTouchDrag = !arenaTouchDrag; paintChips(); cursor(); return; }
    if (ro()) return;
    if (b.dataset.unaim) { s.aim_at = null; changed(); }
  });

  const pinchState = () => {
    const [a, b] = [...touches.values()];
    return { d: Math.hypot(a[0] - b[0], a[1] - b[1]),
             c: [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2] };
  };
  const local = (e) => {
    const b = cv.getBoundingClientRect();
    return [e.clientX - b.left, e.clientY - b.top];
  };

  cv.addEventListener("pointerdown", (e) => {
    const [lx, ly] = local(e);
    if (e.pointerType === "touch") {
      if (!arenaTouchDrag && touches.size === 0) return;   // the finger scrolls
      touches.set(e.pointerId, [lx, ly]);
      if (touches.size === 2) {
        if (mode === "paint" && painted) { s.formation.pop(); changed(); }
        mode = null; marquee = null; painted = null;
        const st = pinchState();
        pinch = { d: st.d, w: mt(st.c[0], st.c[1]) };
        return;
      }
    }
    cv.setPointerCapture(e.pointerId);
    const p = mt(lx, ly);
    last = [lx, ly];
    if (space || tool === "hand" || e.button === 1 || e.button === 2) {
      mode = "pan"; panning = true; cursor(); return;
    }
    if (ro()) return;                       // a ruler's fight is not editable
    if (tool === "place") { mode = "paint"; painted = null; placeAt(p); return; }
    if (tool === "erase") { mode = "erase"; eraseAt(p); return; }
    // …and with the aim tool the whole floor is the marker, which is the only
    // way to put an aim somewhere the first time.
    if (tool === "aim") { mode = "aim"; s.aim_at = snapTo(p); draw(); paintChips(); return; }
    // THE ORDER IS WHAT YOU REACH FOR FIRST: a placed aim marker, then an
    // ENEMY, then yourself. The enemy before the player, because at contact
    // they are one grab apart and the enemy is what a formation is made of.
    if (hitAim(p)) { mode = "aim"; return; }
    const i = hitBody(p);
    if (i === null && hitYou(p)) { mode = "you"; return; }
    if (i !== null) {
      if (e.shiftKey) (sel.has(i) ? sel.delete(i) : sel.add(i));
      else if (!sel.has(i)) { sel.clear(); sel.add(i); }
      mode = "move"; dragging = true; dragFrom = snapTo(p); cursor(); draw();
      return;
    }
    if (!e.shiftKey) sel.clear();
    mode = "marquee"; marquee = { a: [lx, ly], b: [lx, ly], base: new Set(sel) };
    draw();
  });

  cv.addEventListener("pointermove", (e) => {
    const [lx, ly] = local(e);
    if (e.pointerType === "touch" && touches.has(e.pointerId)) touches.set(e.pointerId, [lx, ly]);
    if (pinch && touches.size === 2) {
      const st = pinchState();
      view.k = Math.min(160, Math.max(1.6, view.k * (st.d / (pinch.d || 1))));
      const after = mt(st.c[0], st.c[1]);
      view.x += pinch.w[0] - after[0];
      view.y += pinch.w[1] - after[1];
      pinch.d = st.d; pinch.w = mt(st.c[0], st.c[1]);
      draw(); return;
    }
    if (!mode) return;
    const p = mt(lx, ly);
    if (mode === "pan") {
      view.x -= (lx - last[0]) / view.k;
      view.y += (ly - last[1]) / view.k;
      last = [lx, ly]; draw(); return;
    }
    if (mode === "paint") { placeAt(p); return; }
    if (mode === "erase") { eraseAt(p); return; }
    if (mode === "aim") { s.aim_at = snapTo(p); draw(); paintChips(); return; }
    if (mode === "you") {
      s.player_at = snapTo(p);
      s.target_at = keepApart(s.player_at, s.target_at);
      draw(); return;
    }
    if (mode === "move") {
      const to = snapTo(p);
      const dx = to[0] - dragFrom[0], dy = to[1] - dragFrom[1];
      if (!dx && !dy) return;
      // EVERY SELECTED BODY MOVES BY ONE DELTA, and each is settled on its own
      // so the block cannot be pushed through somebody standing outside it.
      for (const i of sel) {
        const b = bodyAt(i);
        if (!b) continue;
        const want = [b[0] + dx, b[1] + dy];
        const at = collide ? arenaSettle(s, want, i) : want;
        if (at === null) continue;
        if (i === 0) s.target_at = at; else s.formation[i - 1].at = at;
      }
      dragFrom = to; draw(); return;
    }
    if (mode === "marquee") {
      marquee.b = [lx, ly];
      const [ax, ay] = mt(marquee.a[0], marquee.a[1]);
      const [bx, by] = mt(marquee.b[0], marquee.b[1]);
      const lo = [Math.min(ax, bx), Math.min(ay, by)];
      const hi = [Math.max(ax, bx), Math.max(ay, by)];
      sel.clear(); marquee.base.forEach((i) => sel.add(i));
      arenaBodies(s).forEach((b, i) => {
        if (b[0] >= lo[0] && b[0] <= hi[0] && b[1] >= lo[1] && b[1] <= hi[1]) sel.add(i);
      });
      draw();
    }
  });

  const end = (e) => {
    if (e && e.pointerType === "touch") {
      touches.delete(e.pointerId);
      if (touches.size < 2) pinch = null;
      if (touches.size === 1) { mode = null; marquee = null; }
    }
    let wrote = mode && mode !== "pan" && mode !== "marquee";
    // A CLICK ON BARE FLOOR CLEARS THE SELECTION AND NOTHING ELSE. Aiming
    // there — on the reasoning that a body is dragged and a place has nothing
    // to grab — ignores that the aim marker is always on the floor and does
    // have something to grab, and a bare click that moves it means every
    // miss-click while selecting silently re-aims the
    // weapon. A fight that moved on a mis-click is a result that was for a
    // fight nobody was in.
    //
    // AIM IS DRAGGED, like everything else on this canvas. One rule for every
    // thing that has a position, and no gesture that edits the fight without
    // being asked to.
    if (mode === "marquee" && marquee) {
      const moved = Math.hypot(marquee.b[0] - marquee.a[0], marquee.b[1] - marquee.a[1]);
      if (moved < 4) { sel.clear(); }
      marquee = null;
    }
    mode = null; panning = false; dragging = false; painted = null;
    cursor();
    if (wrote) changed(); else draw();
  };
  cv.addEventListener("pointerup", end);
  cv.addEventListener("pointercancel", end);
  cv.addEventListener("contextmenu", (e) => e.preventDefault());

  cv.addEventListener("wheel", (e) => {
    e.preventDefault();
    const [lx, ly] = local(e);
    const before = mt(lx, ly);
    view.k = Math.min(160, Math.max(1.6, view.k * Math.exp(-e.deltaY * 0.0016)));
    const after = mt(lx, ly);
    view.x += before[0] - after[0];
    view.y += before[1] - after[1];
    draw();
  }, { passive: false });

  // KEYS ONLY WHILE THE SCENE HAS FOCUS, so typing a level into the box beside
  // it cannot delete the formation.
  wrap.tabIndex = 0;
  wrap.addEventListener("keydown", (e) => {
    if (e.code === "Space" && !space) { space = true; cursor(); e.preventDefault(); return; }
    const k = e.key.toLowerCase();
    if (k === "v" || k === "b" || k === "e" || k === "h") {
      if (ro() && k !== "h") return;
      tool = { v: "select", b: "place", e: "erase", h: "hand" }[k];
      paintRail(); cursor(); return;
    }
    if (ro()) return;
    if (k === "s") { snapGrid = !snapGrid; paintRail(); return; }
    if (k === "c") { collide = !collide; paintRail(); return; }
    if (k === "escape") { sel.clear(); draw(); return; }
    if (e.key === "Delete" || e.key === "Backspace") {
      const gone = [...sel].filter((i) => i > 0).sort((a, b) => b - a);
      if (!gone.length) return;
      gone.forEach((i) => s.formation.splice(i - 1, 1));
      sel.clear(); changed(); e.preventDefault(); return;
    }
    if (e.key.startsWith("Arrow") && sel.size) {
      const d = e.shiftKey ? 1 : 0.1;
      const dx = ((e.key === "ArrowRight") - (e.key === "ArrowLeft")) * d;
      const dy = ((e.key === "ArrowUp") - (e.key === "ArrowDown")) * d;
      for (const i of sel) {
        const b = bodyAt(i);
        if (!b) continue;
        const at = collide ? arenaSettle(s, [b[0] + dx, b[1] + dy], i) : [b[0] + dx, b[1] + dy];
        if (at === null) continue;
        if (i === 0) s.target_at = at; else s.formation[i - 1].at = at;
      }
      changed(); e.preventDefault();
    }
  });
  wrap.addEventListener("keyup", (e) => {
    if (e.code === "Space") { space = false; cursor(); }
  });

  // THE SCENE, ADDRESSABLE. A canvas has no per-body element, so anything that
  // needs to point at a body -- a check driving a drag, a caller re-framing the
  // view -- needs the same metres-to-pixels map the renderer uses. Exposing THAT
  // rather than a set of stand-in DOM nodes keeps one geometry: whatever it says
  // is where the body was drawn.
  host.__arena = {
    view,
    px, mt,
    fit: () => { fit(); draw(); },
    draw,
    tool: (id) => { tool = id; paintRail(); cursor(); },
    get sel() { return sel; },
    get snap() { return snapGrid; },
    get collide() { return collide; },
  };
  // READ-ONLY SAYS SO ON THE HOST, the same class the SVG scene set — it is
  // what tells a reader (and a check) that this copy is a picture rather than
  // an editor, and the optimizer's copy has always carried it.
  host.classList.toggle("ar-ro", !!opts.readonly);
  paintRail(); paintChips(); cursor(); resize();
  // THE HOST MAY BE ON ANOTHER TAB at mount, where the box is zero and there is
  // nothing to size to. The observer catches it the moment it is shown.
}

function mountArena(host, s, en, opts) {
  if (!host) return;
  // THE EDITOR IS A CANVAS as of 2026-08-18; the ANALYSIS mount is still the
  // SVG scene. Setting up a fight and reading one are two things, and only
  // one of them is an editor — the result panel's copy draws a fight that has
  // already been run, picks rather than drags, and shades by damage. Staged
  // deliberately: one renderer moves at a time, and the half a RESULT depends
  // on moves second.
  if (!opts.heat) return mountArenaCanvas(host, s, en, opts);
  // QUICK SETS, in the canvas. The scene is the one place a position is set, so the shortcuts live in it rather than in a second
  // control below that would be a second source of truth for the same fact.
  const dis = opts.readonly ? " disabled" : "";
  const chips = () => {
    const n = 1 + (s.formation || []).length;
    const jumps = ARENA_JUMPS.map((m) => {
      const on = Math.abs(arenaDistance(s) - m) < 0.05;
      const label = m === 0 ? tr("contact") : `${m} m`;
      return `<button class="ar-jump${on ? " on" : ""}" data-jump="${m}"${dis}>${escHtml(label)}</button>`;
    }).join("");
    // THE FORMATION'S OWN ROW. Separate from the distance shortcuts because
    // they answer different questions — one is where the target stands, the
    // other is how many bodies are on the floor.
    const full = n >= ARENA_MAX_BODIES();
    const crowd = `<button class="ar-jump" data-add="1"${dis || (full ? " disabled" : "")}>+1</button>`
      + `<button class="ar-jump" data-add="8"${dis || (full ? " disabled" : "")}>+8</button>`
      // DRAG-ON-TOUCH, off by default — see `arenaTouchDrag`. Drawn wherever a
      // finger CAN be the pointer, because on a mouse there is nothing to
      // choose. `maxTouchPoints` rather than a `pointer: coarse` query: a
      // touchscreen laptop reports a FINE pointer and still has the problem.
      + (navigator.maxTouchPoints > 0
        ? `<button class="ar-jump${arenaTouchDrag ? " on" : ""}" data-touchdrag="1"${dis}
           title="${escHtml(tr("while this is on, dragging a body moves it instead of scrolling the page"))}"
           >✥ ${escHtml(tr("move"))}</button>`
        : "")
      + `<button class="ar-jump" data-clear="1"${dis || (n < 2 ? " disabled" : "")}>${escHtml(tr("one enemy"))}</button>`
      + `<span class="ar-count">${n}${full ? " / " + ARENA_MAX_BODIES() : ""}</span>`
      + (s.aim_at
        ? `<button class="ar-jump" data-unaim="1"${dis}>${escHtml(tr("aim at the target"))}</button>`
        : "");
    return `<div class="ar-jumps">${jumps}</div><div class="ar-jumps ar-crowd">${crowd}</div>`;
  };
  // AN ANALYSIS MOUNT IS NOT AN EDITOR. `opts.heat` says this copy is the
  // RESULT's, so it draws the scene and nothing else — no distance shortcuts,
  // no +1/+8, no drag. Setting up a fight and reading one are two things and
  // the controls of one have no business in the other.
  const analysis = !!opts.heat;
  const paint = () => {
    host.innerHTML = arenaSvg(s, en, opts.heat, opts.selected)
      + (analysis ? "" : chips());
    // TOUCH-ACTION FOLLOWS THE MODE, because it is what the browser reads to
    // decide whether a gesture on this element is a scroll. `pan-y` keeps the
    // page scrollable through the scene; `none` hands every gesture here.
    const svg = host.querySelector(".ar-svg");
    if (svg) svg.style.touchAction = analysis || !arenaTouchDrag ? "pan-y" : "none";
    // THE SCENE, ADDRESSABLE — the same contract the canvas mount publishes on
    // `host.__arena`, so anything that needs to point AT a body reads the map
    // the renderer used rather than guessing from a DOM box. This one answers
    // in the SVG's viewBox, which is what `arenaSvg` draws in; a caller that
    // wants pixels converts through the svg's own client rect.
    host.__arena = {
      vb: arenaGeom(ARENA_VW, ARENA_VH, s.player_at, ...arenaBodies(s), arenaAim(s)).px,
      vw: ARENA_VW,
      vh: ARENA_VH,
    };
  };
  paint();
  if (opts.readonly) {
    host.classList.add("ar-ro");
    // AN ANALYSIS MOUNT IS READ-ONLY AND STILL CLICKABLE: it PICKS, it never
    // moves. Bound here rather than below because everything below belongs to
    // the EDITOR — the drag, the quick sets, the aim marker — and none of that
    // has any business on a picture of a fight that has already been run
    // Setting up a fight and reading one are two things.
    if (opts.onPick) {
      host.addEventListener("pointerdown", (e) => {
        const el = e.target.closest && e.target.closest("[data-drag]");
        const w = el && el.dataset.drag;
        if (w && w.startsWith("foe:")) opts.onPick(Number(w.slice(4)));
      });
    }
    return;
  }
  // DELEGATED, because `paint` replaces the markup on every move: listeners
  // bound to the circles die with the first repaint, which left the scene
  // draggable exactly once.
  host.addEventListener("click", (e) => {
    const b = e.target.closest && e.target.closest("button");
    if (!b) {
      // POINT AT A PLACE. Clicking bare floor aims there, which is the gesture
      // the whole "aim is a direction" model is for — a
      // body is dragged, but a PLACE has nothing to grab, so it is clicked.
      // Bodies and the marker itself are dragged instead, so they are skipped.
      const svg = e.target.closest && e.target.closest(".ar-svg");
      if (!svg || (e.target.closest && e.target.closest("[data-drag]"))) return;
      if (opts.readonly || officialScenarioActive()) return;
      const box = svg.getBoundingClientRect();
      if (!box.width || !box.height) return;
      const g = arenaGeom(ARENA_VW, ARENA_VH, s.player_at, ...arenaBodies(s), arenaAim(s));
      s.aim_at = g.m((e.clientX - box.left) * (ARENA_VW / box.width),
                     (e.clientY - box.top) * (ARENA_VH / box.height));
      paint();
      markScenarioDirty();
      if (opts.after) opts.after();
      return;
    }
    // THE TOUCH-DRAG MODE IS NOT AN EDIT to the fight, so it is answered before
    // the official-ruler guard: a ruler's scene refuses to MOVE, and offering a
    // dead toggle beside it would say the opposite.
    if (b.dataset.touchdrag) {
      arenaTouchDrag = !arenaTouchDrag;
      paint();
      return;
    }
    if (opts.readonly || officialScenarioActive()) return;
    if (b.dataset.jump !== undefined) setArenaDistance(s, Number(b.dataset.jump));
    else if (b.dataset.add) {
      for (let i = 0; i < Number(b.dataset.add); i++) if (!arenaAddFoe(s)) break;
    } else if (b.dataset.clear) {
      arenaReset(s);
    } else if (b.dataset.unaim) s.aim_at = null;
    else return;
    paint();
    markScenarioDirty();
    if (opts.after) opts.after();
  });
  host.addEventListener("pointerdown", (e) => {
    const el = e.target.closest && e.target.closest("[data-drag]");
    if (!el) return;
    // AN OFFICIAL RULER'S FIGHT IS NOT DRAGGABLE. The
    // benchmark pins its distance — both boards are scored at CONTACT — so a
    // drag there would silently move the fight off the standard while the bar
    // still said which ruler you were on.
    //
    // CHECKED AT THE GESTURE rather than baked in at mount: `lockOfficialScenario`
    // disables `input,select,button,textarea`, and the bodies here are SVG
    // circles, which that sweep does not reach. Asking live also means
    // switching scenarios needs no re-render to take effect.
    if (opts.readonly || officialScenarioActive()) return;
    // A FINGER SCROLLS UNLESS TOLD OTHERWISE. Returning here leaves the gesture
    // with the browser, which is the only way the page still scrolls.
    if (e.pointerType === "touch" && !arenaTouchDrag) return;
    e.preventDefault();
    const which = el.dataset.drag;
    const box = host.querySelector(".ar-svg").getBoundingClientRect();
    if (!box.width || !box.height) return;   // not on screen: nothing to drag
    const g = arenaGeom(ARENA_VW, ARENA_VH, s.player_at, ...arenaBodies(s), arenaAim(s));
    // CLIENT PIXELS -> the viewBox's own units, which is the one conversion
    // the fixed coordinate space costs and the reason it is worth it.
    const kx = ARENA_VW / box.width, ky = ARENA_VH / box.height;
    const move = (ev) => {
      const at = g.m((ev.clientX - box.left) * kx, (ev.clientY - box.top) * ky);
      if (which === "player_at") {
        s.player_at = at;
        s.target_at = keepApart(at, s.target_at);
      } else if (which === "aim") {
        // DRAGGING THE MARKER MAKES AIM A PLACE OF ITS OWN. Until you do, it
        // rides the target — which is what every scenario written before a
        // formation existed means, and what keeps this a strictly additive
        // feature.
        s.aim_at = at;
      } else if (which.startsWith("foe:")) {
        const i = Number(which.slice(4));
        // `null` = nowhere legal to go, so the body stays put. The gesture is
        // not cancelled — keep moving the finger and it follows again the
        // moment there is room.
        const settled = arenaSettle(s, at, i);
        if (settled === null) return;
        if (i === 0) s.target_at = settled;
        else if (s.formation[i - 1]) s.formation[i - 1].at = settled;
      }
      paint();
    };
    const up = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", up);
      markScenarioDirty();
      if (opts.after) opts.after();
    };
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", up);
  });
}


