// ---- THE ARENA, seen from above -------------------------------------------
//
// A fight is two bodies on a floor, so it is DRAWN as two bodies on a floor and
// you move them with your finger. The picture is not a
// decoration: what it shows is the scenario, and what you drag IS the distance
// the simulation runs at.
//
// It draws the bodies at their REAL radius, so "as close as they go" is
// something you can see rather than a rule you are told: the two circles touch
// and will not pass through each other.
//
// METRES ARE THE UNIT and the view fits itself to them, with a floor on the
// span so a contact-range fight does not zoom to two enormous discs.

// THE ENGINE'S OWN NUMBER, served at `/api/meta.body_radius_m` and adopted the
// moment it arrives — the literal is only what a scene drawn before the first
// fetch uses. The page kept its own 0.2 while the engine moved to 0.25, and a
// contact-range ruler then drew as 0.1 m apart and a 3 m crowd as 2.6.
let BODY_R_M = 0.25;
let CONTACT_M = 2 * BODY_R_M;
const adoptBodyRadius = (m) => {
  const r = m && Number(m.body_radius_m);
  if (!Number.isFinite(r) || r <= 0) return;
  BODY_R_M = r;
  CONTACT_M = 2 * r;
};
// A FIXED INTERNAL COORDINATE SPACE, and the CSS stretches it. The scene used
// to be laid out in whatever pixel width the host happened to have, which is
// ZERO while the panel is on another tab — so the geometry came out NaN and a
// drag wrote `[null, null]` into the fight. A viewBox has no such moment.
const ARENA_VW = 320;
const ARENA_VH = 176;

/// Metres <-> pixels for the current pair of positions.
function arenaGeom(w, h, ...pts) {
  const pad = 1.2;                       // metres of air around the bodies
  const floor = 5;                       // never zoom closer than a 5 m view
  // EVERY BODY IN FRAME. It fitted exactly two until a formation could hold
  // fifty, and a scene that framed two of them would put the rest
  // off the edge — which is the one thing a picture of a fight must not do.
  const xs = pts.map((q) => q[0]), ys = pts.map((q) => q[1]);
  const [lo, hi] = [Math.min(...xs), Math.max(...xs)];
  const [bo, to] = [Math.min(...ys), Math.max(...ys)];
  const spanX = Math.max(hi - lo + 2 * pad, floor);
  const spanY = Math.max(to - bo + 2 * pad, (floor * h) / w);
  const s = Math.min(w / spanX, h / spanY);
  const cx = (lo + hi) / 2, cy = (bo + to) / 2;
  return {
    s,
    // y grows AWAY from the viewer, so "further up the screen" is "further off"
    px: (m) => [w / 2 + (m[0] - cx) * s, h / 2 - (m[1] - cy) * s],
    m: (x, y) => [cx + (x - w / 2) / s, cy - (y - h / 2) / s],
  };
}

/// Push `b` out of `a` so two bodies never overlap — along the line between
/// them, so a drag that overshoots SLIDES rather than snapping to an axis.
function keepApart(a, b) {
  const dx = b[0] - a[0], dy = b[1] - a[1];
  const d = Math.hypot(dx, dy);
  if (d >= CONTACT_M) return b;
  if (d < 1e-9) return [a[0], a[1] + CONTACT_M];
  const k = CONTACT_M / d;
  return [a[0] + dx * k, a[1] + dy * k];
}

/// Put the target `want` metres away, ALONG THE LINE it already stands on.
///
/// Direction is preserved rather than snapped to an axis: a quick-set and a
/// drag are two ways to move the same body, so neither may quietly undo the
/// other's other axis. Below contact it is pushed back out, the same rule the
/// drag obeys.
function setArenaDistance(s, want) {
  const [ax, ay] = s.player_at;
  let dx = s.target_at[0] - ax, dy = s.target_at[1] - ay;
  const d = Math.hypot(dx, dy);
  if (d < 1e-9) { dx = 0; dy = 1; }
  const u = d < 1e-9 ? 1 : d;
  // `want` is a GAP, and the model holds centres — one contact apart.
  const m = Math.max(CONTACT_M, want + CONTACT_M);
  s.target_at = [ax + (dx / u) * m, ay + (dy / u) * m];
}

/// The distances worth one click. CONTACT is first because it is where both
/// boards are scored; the rest are the ranges a falloff window and a spread
/// cone actually change hands at.
const ARENA_JUMPS = [0, 5, 10, 20, 40];

/// CENTRE TO CENTRE — the model's own distance, never below CONTACT_M.
const arenaSpan = (s) =>
  Math.hypot(s.target_at[0] - s.player_at[0], s.target_at[1] - s.player_at[1]);

/// THE GAP — surface to surface, and what a reader is shown. Zero at contact,
/// which is what point blank means to a player. The 0.4 m
/// between the two centres is the model's business; nobody should subtract it
/// to find out how far away they are standing. Everything the page displays,
/// sets and marks is this number; `engine::rules::space::gap` is the same one.
const arenaDistance = (s) => Math.max(0, arenaSpan(s) - CONTACT_M);

/// EVERY BODY ON THE FLOOR, aimed one first — the order the api and the engine
/// both use, so nothing has to be reordered on the way out.
const arenaBodies = (s) => [s.target_at, ...(s.formation || []).map((f) => f.at)];

/// WHICH UNIT A BODY IS, AS A COLOUR — derived, never declared.
///
/// A formation can hold fifty bodies of several units and they were all one
/// red circle, so the only way to find out who was standing where was to click
/// each one. The cheap answer is the right one here: hash
/// the unit's id into a HUE. It costs no data file, no art, no translation and
/// no maintenance — a unit that arrives tomorrow already has a colour, and it
/// is the SAME colour on every machine and in every session, which a palette
/// handed out in load order would not be.
///
/// FNV-1a, because it is four lines and mixes short ASCII ids well; the id is
/// a stable English slug, so the colour is stable too. The saturation and
/// lightness are fixed so nothing lands on the floor's own grey or on the
/// accent, and the same hue is drawn as a swatch wherever a unit is NAMED —
/// the colour is only useful as a key if the key is somewhere.
const unitHue = (id) => {
  let h = 2166136261;
  for (let i = 0; i < String(id).length; i++) {
    h ^= String(id).charCodeAt(i);
    h = Math.imul(h, 16777619) >>> 0;
  }
  return h % 360;
};
const unitColor = (id, a) => `hsl(${unitHue(id)} 62% 58%${a === undefined ? "" : ` / ${a}`})`;
/// The unit a body is, by arena index — 0 is the aimed body, which is the
/// FIGHT's target, and the rest carry their own since 2026-08-18.
const unitAt = (s, i) => (i === 0
  ? s.enemy
  : ((s.formation || [])[i - 1] || {}).enemy || s.enemy);

/// WHERE THE WEAPON POINTS. `null` means "at the target", which is what every
/// scenario written before a formation existed means and what the app has
/// always done.
const arenaAim = (s) => s.aim_at || s.target_at;

/// FIFTY, the api's own cap — the sim pays for every body, so the page refuses
/// the same number the server refuses rather than a friendlier one.
// FROM THE ENGINE, never a copy of it (`formation::MAX_BODIES`, served at
// `/api/meta.max_bodies`). It was written out as 50 here, so the cap the server
// enforces and the cap the canvas offers were two numbers that happened to
// agree — until the ruler work raised one of them. The fallback is only for a
// meta that failed to load, where nothing else works either.
const ARENA_MAX_BODIES = () => (META && META.max_bodies) || 50;

/// WHICH BODY A SHOT CROSSES FIRST — `engine::rules::space::first_hit`, drawn rather
/// than computed for damage. The page needs it for ONE reason: to show which
/// body the beam is on, because aiming is a direction and the answer is not
/// always the nearest thing to the cursor.
function arenaFirstHit(s) {
  const bodies = arenaBodies(s);
  const aim = arenaAim(s);
  const [px, py] = s.player_at;
  const span = Math.hypot(aim[0] - px, aim[1] - py) || 1;
  const [ux, uy] = [(aim[0] - px) / span, (aim[1] - py) / span];
  const [mx, my] = [px + ux * BODY_R_M, py + uy * BODY_R_M];
  let best = null;
  bodies.forEach((b, i) => {
    const [dx, dy] = [b[0] - mx, b[1] - my];
    const along = dx * ux + dy * uy;
    if (along < 0) return;
    if (Math.abs(dx * uy - dy * ux) > BODY_R_M) return;
    if (!best || along < best[1]) best = [i, along];
  });
  return best ? best[0] : -1;
}

/// THE SCENE. `heat` shades each body by what it TOOK and `sel` marks the one
/// being examined — both absent when this is the scenario's own canvas.
///
/// SETTING UP A FIGHT AND READING ONE ARE TWO THINGS, and
/// the same picture serves both only because it is the same FIGHT. The
/// scenario's copy is where a body is placed and carries no result; the result
/// panel's copy is read-only, coloured by the answer, and clicking a body picks
/// it. Neither can be mistaken for the other, and neither is the other's
/// control.
function arenaSvg(s, en, heat, sel) {
  const w = ARENA_VW, h = ARENA_VH;
  const bodies = arenaBodies(s);
  const aim = arenaAim(s);
  const g = arenaGeom(w, h, s.player_at, ...bodies, aim);
  const [px, py] = g.px(s.player_at);
  const [tx, ty] = g.px(aim);
  const r = Math.max(3, BODY_R_M * g.s);
  // A GRID THAT STAYS READABLE: the step grows with the zoom so the lines
  // never crowd, and it is labelled, because a scale nobody can read is a
  // picture rather than a measurement.
  const step = [0.5, 1, 2, 5, 10, 20, 50, 100].find((k) => k * g.s >= 26) || 200;
  const lines = [];
  const [x0, y1] = g.m(0, 0), [x1, y0] = g.m(w, h);
  for (let x = Math.ceil(x0 / step) * step; x <= x1; x += step) {
    lines.push(`<line class="ar-grid" x1="${g.px([x, 0])[0].toFixed(1)}" y1="0" x2="${g.px([x, 0])[0].toFixed(1)}" y2="${h}"/>`);
  }
  for (let y = Math.ceil(y0 / step) * step; y <= y1; y += step) {
    const yy = g.px([0, y])[1].toFixed(1);
    lines.push(`<line class="ar-grid" x1="0" y1="${yy}" x2="${w}" y2="${yy}"/>`);
  }
  const d = arenaDistance(s);
  // THE MUZZLE — where the shot actually leaves, a point on the player's own
  // circumference facing the target (`engine::rules::space::muzzle`). Drawn because
  // it is load-bearing rather than decorative: the cone widens over the flight
  // from HERE, which is one radius shorter than the line between the two
  // bodies, and at contact it puts the muzzle on the enemy's surface — which
  // is why nothing misses there.
  // …AND IT FACES WHERE THE WEAPON POINTS, which may be a body or bare floor.
  const span = Math.hypot(tx - px, ty - py) || 1;
  const ux = (tx - px) / span, uy = (ty - py) / span;
  const mx = px + ux * r, my = py + uy * r;
  const struck = arenaFirstHit(s);
  // THE SHOT STOPS WHERE IT STOPS, and the line says so. Solid from the muzzle
  // to the body it actually reaches; DASHED on from there to wherever you are
  // pointing. Aiming past a body is legal and common — you point at the one
  // behind — and without this the scene showed a line running through a body it
  // cannot pass. No punch-through is modelled, so the first
  // body on the line is where it ends.
  const hitPx = struck >= 0 ? g.px(bodies[struck]) : null;
  const stop = hitPx
    ? [hitPx[0] - ux * r, hitPx[1] - uy * r]
    : [tx, ty];
  const sight = `<line class="ar-link" x1="${mx.toFixed(1)}" y1="${my.toFixed(1)}" `
    + `x2="${stop[0].toFixed(1)}" y2="${stop[1].toFixed(1)}"/>`
    + (hitPx && (Math.abs(stop[0] - tx) > 1 || Math.abs(stop[1] - ty) > 1)
      ? `<line class="ar-past" x1="${stop[0].toFixed(1)}" y1="${stop[1].toFixed(1)}" `
        + `x2="${tx.toFixed(1)}" y2="${ty.toFixed(1)}"/>`
      : "");
  const foes = bodies.map((b, i) => {
    const [bx, by] = g.px(b);
    // THE ONE THE BEAM IS ON is marked, because aiming at a place means the
    // answer is not always what the cursor is nearest to.
    const cls = (i === struck ? " ar-struck" : "")
      + (heat ? " ar-heat" : "")
      + (sel === i ? " ar-sel" : "");
    // HOW MUCH IT TOOK, as a share of the worst-hit body. A body that took
    // NOTHING keeps the outline it has on the scenario's canvas, which is what
    // makes "the chain reached nobody" visible at a glance rather than a table
    // to read.
    const fill = heat && heat[i] > 0
      ? ` style="fill-opacity:${(0.15 + 0.75 * heat[i]).toFixed(3)}"` : "";
    return `<circle class="ar-body ar-foe${cls}" data-drag="foe:${i}" data-foe="${i}"${fill} `
      + `cx="${bx.toFixed(1)}" cy="${by.toFixed(1)}" r="${r.toFixed(1)}"/>`;
  }).join("");
  // A short arrow past the muzzle says which way the body is FACING, so a
  // player being turned by their own aim is visible rather than implied.
  const ax = mx + ux * 9, ay = my + uy * 9;
  const nose = `<line class="ar-face" x1="${mx.toFixed(1)}" y1="${my.toFixed(1)}" x2="${ax.toFixed(1)}" y2="${ay.toFixed(1)}"/>
    <circle class="ar-muzzle" cx="${mx.toFixed(1)}" cy="${my.toFixed(1)}" r="2.2"/>`;
  return `<svg class="ar-svg" viewBox="0 0 ${w} ${h}" preserveAspectRatio="xMidYMid meet">
    ${lines.join("")}
    ${sight}
    ${foes}
    <circle class="ar-aim" data-drag="aim" cx="${tx.toFixed(1)}" cy="${ty.toFixed(1)}" r="${(r * 0.7).toFixed(1)}"/>
    <circle class="ar-body ar-you" data-drag="player_at" cx="${px.toFixed(1)}" cy="${py.toFixed(1)}" r="${r.toFixed(1)}"/>
    ${nose}
    <text class="ar-tag ar-tag-you" x="${px.toFixed(1)}" y="${(py + r + 13).toFixed(1)}">${escHtml(tr("You"))}</text>
    <text class="ar-tag ar-tag-foe" x="${g.px(bodies[Math.max(struck, 0)])[0].toFixed(1)}" y="${(g.px(bodies[Math.max(struck, 0)])[1] - r - 6).toFixed(1)}">${escHtml((en && en.name) || tr("Enemy"))}</text>
    <text class="ar-dist" x="${((px + tx) / 2).toFixed(1)}" y="${((py + ty) / 2 - 8).toFixed(1)}">${d.toFixed(2)} m</text>
    <text class="ar-scale" x="6" y="${h - 6}">${step} m ${escHtml(tr("grid"))}</text>
  </svg>`;
}

/// Draw the scene into `host` and let the two bodies be dragged.
///
/// The drag writes the LIVE scenario and repaints the SVG on every move — the
/// whole panel is not re-rendered until the finger comes up, because the panel
/// contains the thing being dragged and rebuilding it mid-gesture drops the
/// pointer capture.
/// THE NEXT UNUSED NAME in this scenario — `e2` upward, `e1` being the aimed
/// body's and not ours to give.
///
/// One PAST the highest ever used rather than one past the count, so deleting a
/// body never hands its name to the next one.
const nextFoeId = (s) => {
  const n = (s.formation || []).reduce((m, f) => {
    const k = Number(String(f.id || "").replace(/^e/, ""));
    return Number.isFinite(k) ? Math.max(m, k) : m;
  }, 1);
  return `e${n + 1}`;
};

/// ADD A BODY, somewhere it does not stand on anyone.
///
/// It walks OUTWARD in a ring around the target rather than dropping every new
/// enemy on one spot: a formation you have to untangle before you can read it
/// is worse than no formation. Three metres is the owner's own fixture spacing
/// and the distance a chain's step edges sit at.
/// Is this point clear of every body and of the player — one contact apart?
/// `skip` is a body's own index, so a body being dragged does not collide with
/// itself.
const arenaFree = (s, p, skip) => arenaBodies(s).every((b, i) => i === skip
  || Math.hypot(b[0] - p[0], b[1] - p[1]) >= CONTACT_M - 1e-9)
  && Math.hypot(s.player_at[0] - p[0], s.player_at[1] - p[1]) >= CONTACT_M - 1e-9;

/// ONE BODY PLACED AT A POINT, or false when the floor is full or (with
/// `collide`) the point is taken. A BODY IS THE UNIT IT WAS PLACED WITH: the
/// unit is STAMPED here and nothing afterwards moves it, so picking a Thrax to
/// place next does not turn every Gunner already down into a Thrax — a body
/// with no unit is read by the server as "the aimed body's". The LEVEL is not
/// stamped: it is a dial for the whole fight, and a body left blank follows it.
function arenaPlaceBody(s, p, collide = true) {
  if (arenaBodies(s).length >= ARENA_MAX_BODIES()) return false;
  if (collide && !arenaFree(s, p)) return false;
  s.formation = s.formation || [];
  s.formation.push({ id: nextFoeId(s), at: [p[0], p[1]], enemy: s.enemy });
  return true;
}

/// BACK TO ONE BODY, aimed at the target — the fight every golden value and
/// both boards are measured under.
function arenaReset(s) {
  s.formation = [];
  s.aim_at = null;
}

function arenaAddFoe(s) {
  if (1 + (s.formation || []).length >= ARENA_MAX_BODIES()) return false;
  const [cx, cy] = s.target_at;
  const taken = arenaBodies(s);
  const clear = (p) => taken.every((q) => Math.hypot(q[0] - p[0], q[1] - p[1]) > CONTACT_M + 1e-6);
  for (let ring = 1; ring <= 8; ring++) {
    for (let k = 0; k < ring * 8; k++) {
      const a = (k / (ring * 8)) * Math.PI * 2;
      const at = [cx + Math.cos(a) * ring * 3, cy + Math.sin(a) * ring * 3];
      if (clear(at)) {
        // NAMED WHERE IT IS CREATED. The server fills a blank id in by
        // POSITION, which is right for a scenario written before ids existed
        // and wrong for one being edited: delete the body in front and every
        // name behind it shifts, so a roll call, a heat map and a debuff table
        // would all be about somebody else.
        //
        // NEVER REUSED inside one scenario, which is what makes it an identity
        // rather than a label. `e1` is the aimed body's and is not ours to give.
        // STAMPED WITH THE UNIT, like every other placement — see `placeAt`.
        // A blank one means "whatever the aimed body is", which is what a
        // scenario written before this means and is not what a body being
        // placed now means.
        s.formation.push({ at, enemy: s.enemy, level: null, id: nextFoeId(s) });
        return true;
      }
    }
  }
  return false;
}

/// KEEP A DRAGGED BODY OUT OF EVERYONE ELSE, not just out of the player.
/// Circles do not overlap, and that rule was written for two bodies
/// (`engine::rules::space::CONTACT_RANGE_M`); with fifty it has to hold pairwise.
/// WHERE A DRAGGED BODY MAY ACTUALLY GO — or `null`, meaning it does not move.
///
/// A body is pushed out of the ONE body it is entering, which is what makes two
/// circles touch at contact and slide along each other instead of passing
/// through. It is not iterated: NOTHING ELSE MOVES, and a body that has nowhere
/// legal to go simply stays where it was.
///
/// The old version projected repeatedly, four passes over every body. With two
/// on the floor that is the same answer; in a CROWD it is not — it squeezed the
/// dragged body through gaps until it found somewhere to sit, so a drag toward
/// a packed rank ended somewhere the finger never went. Refusing is the honest
/// answer to "there is no room here", and it makes a wedged body immovable,
/// which is what a wedged body is.
function arenaSettle(s, at, skip) {
  const others = [s.player_at, ...arenaBodies(s).filter((_, i) => i !== skip)];
  const clash = (p) => others.filter((q) => Math.hypot(p[0] - q[0], p[1] - q[1]) < CONTACT_M - 1e-9);
  const hit = clash(at);
  if (!hit.length) return at;
  // THE NEAREST ONE decides, so the slide is along the surface being pressed.
  const q = hit.reduce((a, b) =>
    Math.hypot(at[0] - a[0], at[1] - a[1]) <= Math.hypot(at[0] - b[0], at[1] - b[1]) ? a : b);
  const d = Math.hypot(at[0] - q[0], at[1] - q[1]);
  // Dead centre has no direction to be pushed in; take the one away from the
  // shooter, which is the only axis the scene always has.
  const p = d < 1e-9
    ? [q[0], q[1] + CONTACT_M]
    : [q[0] + (at[0] - q[0]) * (CONTACT_M / d), q[1] + (at[1] - q[1]) * (CONTACT_M / d)];
  // …AND IF THAT LANDS IN SOMEBODY ELSE, THERE IS NO ROOM. Refused rather than
  // projected again: a second push is how the body ends up somewhere nobody
  // dragged it to.
  return clash(p).length ? null : p;
}

/// IS THE SCENE ACCEPTING DRAGS FROM A FINGER?
///
/// OFF BY DEFAULT, and the whole reason is that a browser decides who owns a
/// gesture at `pointerdown` and never gives it back. A body that drags on touch
/// means the finger that started on it can no longer SCROLL — and a 19x19
/// formation covers the canvas in bodies, so on a phone almost every scroll
/// past the arena dragged an enemy instead. The fight moved silently and the
/// result it had just produced was for a fight nobody was in any more.
///
/// A LONG PRESS CANNOT FIX IT: once the browser has given the gesture to
/// scrolling it is gone, so claiming it later is not something the page can do.
/// The honest answer is a MODE the reader turns on, which is also what makes
/// "why can I not move anything" answerable — the button is right there.
///
/// A MOUSE IS UNAFFECTED: it has no scroll to lose, so it drags either way.
let arenaTouchDrag = false;

