// ---- THE SHARE CARD — a picture of the build, to paste into a chat ---------
//
// BUILT FROM BLOCKS, ONE PER BUILDER STEP, IN THE BUILDER'S ORDER. The order
// is read from the page (`builderSteps()`), so a step moved or added there
// moves or adds a block here; a step with no entry in `CARD_BLOCKS` fails
// `check_share_card` rather than vanishing from the card. docs/UI.md §The
// share card.
//
// Drawn here rather than server-side: it has to work on a static site and in
// the desktop client with no backend, from the same-origin art the page shows.

// The site's own name, not wherever this page happens to be served from: the
// card travels, and a localhost in its corner would be wrong everywhere.
const SITE_HOST = "wfsim.app";
const CARD_W = 1080, CARD_DPR = 2, CARD_PAD = 48;

/// A THEME IS TOKENS, never layout: the blocks and their order are the build's,
/// and a theme may only say how they look, and whether a frame is drawn round them. The default is the only one this
/// repo ships; anything else is registered from outside it.
const CARD_THEMES = {
  default: {
    bg: "#0e1014", panel: "#151820", line: "#262b36", text: "#f2f4f8", muted: "#8b93a3",
    gold: "#e8c37a", mark: "rgba(232,195,122,.22)", markWords: "rgba(232,195,122,.26)", glow: "rgba(232,195,122,.10)", matched: "#58d68d", mismatched: "#e06c75", riven: "#a58bd6", negative: "#e06c75",
    rarity: { common: "#c98a4b", uncommon: "#c3c9d4", rare: "#d8b25a", legendary: "#ececf4" },
    display: '"Bahnschrift", "DIN Alternate", "Segoe UI", system-ui, "Microsoft YaHei", sans-serif',
    body: 'system-ui, -apple-system, "Segoe UI", "Microsoft YaHei", "PingFang SC", sans-serif',
  },
};
const cardThemeRegister = (id, tokens) => { CARD_THEMES[id] = { ...CARD_THEMES.default, ...tokens }; };

const loadImg = (src) => new Promise((res) => {
  if (!src) return res(null);
  const im = new Image();
  im.onload = () => res(im);
  im.onerror = () => res(null);      // same-origin art, but never block on it
  im.src = src;
});

// Draw `im` to fit a box, centred, aspect kept.
function drawFit(g, im, x, y, w, h) {
  if (!im) return;
  const s = Math.min(w / im.width, h / im.height);
  const dw = im.width * s, dh = im.height * s;
  g.drawImage(im, x + (w - dw) / 2, y + (h - dh) / 2, dw, dh);
}

const roundRect = (g, x, y, w, h, r) => {
  g.beginPath();
  g.moveTo(x + r, y); g.arcTo(x + w, y, x + w, y + h, r); g.arcTo(x + w, y + h, x, y + h, r);
  g.arcTo(x, y + h, x, y, r); g.arcTo(x, y, x + w, y, r); g.closePath();
};

/// `s` cut with an ellipsis to `max` pixels in the current font.
function fitText(g, s, max) {
  s = String(s || "");
  if (g.measureText(s).width <= max) return s;
  while (s.length > 1 && g.measureText(s + "…").width > max) s = s.slice(0, -1);
  return s + "…";
}

/// `s` broken into at most `lines` lines of `max` pixels; words where there are
/// spaces, characters where there are none (Chinese has none).
function wrapText(g, s, max, lines) {
  const out = [];
  let rest = String(s || "");
  while (rest && out.length < lines) {
    if (g.measureText(rest).width <= max) { out.push(rest); rest = ""; break; }
    let cut = rest.length;
    while (cut > 1 && g.measureText(rest.slice(0, cut)).width > max) cut--;
    const sp = rest.lastIndexOf(" ", cut);
    if (sp > 0) cut = sp;
    out.push(rest.slice(0, cut).trim());
    rest = rest.slice(cut).trim();
  }
  if (rest && out.length) out[out.length - 1] = fitText(g, out[out.length - 1] + " " + rest, max);
  return out;
}

/// THE WORD THAT NAMES THE WEAPON: of the words in its English name, the one
/// fewest names in the roster share — "Prime", "Kuva" and "Dual" are in dozens,
/// "Plasmor" in two — the longest on a tie. Counted from the roster, so a new
/// prefix DE adds needs no list kept.
function cardKeyword(nameEn) {
  const words = String(nameEn || "").toUpperCase().split(/\s+/).filter(Boolean);
  if (words.length < 2) return { before: "", key: words[0] || "", after: "" };
  const names = new Set((META.weapons || []).map((w) => String(w.name_en || w.name).toUpperCase()));
  const count = (word) => [...names].filter((n) => n.split(/\s+/).includes(word)).length;
  let k = 0;
  words.forEach((wd, i) => {
    const a = count(wd), b = count(words[k]);
    if (a < b || (a === b && wd.length > words[k].length)) k = i;
  });
  return { before: words.slice(0, k).join(" "), key: words[k], after: words.slice(k + 1).join(" ") };
}

// ---- the blocks -------------------------------------------------------------
// Each block answers `null` (nothing to say for this build, so nothing drawn) or
// `{ half, height(w), draw(g, x, y, w) }`. `half` blocks pair up on one row.

const ROMAN_CARD = ["", "I", "II", "III", "IV", "V", "VI"];

const cardLabel = (c, t, s, x, y) => {
  c.g.fillStyle = t.muted; c.g.font = `600 13px ${t.body}`;
  c.g.fillText(s, x, y);
};

/// THE MODS, the way the arsenal lays them out: the stance above, the eight in
/// two rows of four, the exilus below — and the riven's own rolls beside them,
/// the one card a reader cannot look up. An empty slot still draws its
/// polarity: the Forma is part of whether the build is legal.
function cardMods(c) {
  const ids = [...Array(8).keys()];
  const any = slots.some((s) => s && s.mod) || slots.some((s) => s && s.pol);
  if (!any) return null;
  const hasStance = weaponAxes().hasStance, hasExilus = weaponAxes().hasExilus;
  const CW = 136, CH = 184, GAP = 10;
  const gridW = CW * 4 + GAP * 3;
  const rid = slots.map((s) => s && s.mod).find((m) => m && isRivenId(m));
  const riven = rid ? modById(rid) : null;
  const rivenLines = riven ? cardLines(riven, slots.find((s) => s.mod === rid).rank) : [];
  const extra = (hasStance ? CH + 26 : 0) + (hasExilus ? CH + 26 : 0);
  const height = () => 30 + 34 + CH * 2 + GAP + extra;
  const modCard = (g, t, i, x, y) => {
    const s = slots[i] || {};
    const m = s.mod ? modById(s.mod) : null;
    const col = !m ? t.line : m.riven || isRivenId(s.mod) ? t.riven : t.rarity[m.rarity] || t.muted;
    g.fillStyle = t.panel; roundRect(g, x, y, CW, CH, 8); g.fill();
    g.lineWidth = m ? 2 : 1; g.strokeStyle = col;
    if (!m) g.setLineDash([5, 4]);
    roundRect(g, x + 1, y + 1, CW - 2, CH - 2, 8); g.stroke();
    g.setLineDash([]);
    const pol = s.pol && c.img.get(POL(polCap(s.pol)));
    if (!m) {
      if (pol) drawPol(g, pol, x + CW / 2 - 10, y + CH / 2 - 10, 20);
      return;
    }
    const r = s.rank == null ? m.max_rank : s.rank;
    // THE DRAIN AND THE SLOT'S COLOUR, top right, as the arsenal shows them:
    // green where they match, red where the slot's colour costs extra. The
    // stance GRANTS, so it shows a plus.
    const base = modDrain(m, r);
    const eff = i === STANCE ? stanceGrant() : slotDrain(base, m.polarity, s.pol);
    const matched = s.pol && (s.pol === m.polarity || (s.pol === "Omni" && m.polarity !== "Umbra"));
    g.font = `600 15px ${t.display}`; g.textAlign = "right";
    g.fillStyle = !s.pol ? t.text : matched ? t.matched : t.mismatched;
    g.fillText(i === STANCE ? `+${eff}` : String(eff), x + CW - (pol ? 30 : 10), y + 22);
    g.textAlign = "left";
    if (pol) drawPol(g, pol, x + CW - 26, y + 9, 16);
    const art = c.img.get(IMG(m.image));
    if (art) { g.save(); roundRect(g, x + 22, y + 32, 92, 92, 5); g.clip(); drawFit(g, art, x + 22, y + 32, 92, 92); g.restore(); }
    g.fillStyle = t.text; g.font = `600 12px ${t.body}`; g.textAlign = "center";
    wrapText(g, m.name, CW - 12, 2).forEach((ln, k, all) =>
      g.fillText(ln, x + CW / 2, y + 142 + k * 15 + (all.length === 1 ? 7 : 0)));
    g.textAlign = "left";
    const pips = Math.min(m.max_rank || 0, 10);
    const pw = pips * 7 - 2;
    for (let k = 0; k < pips; k++) {
      g.fillStyle = k < r ? col : t.line;
      g.beginPath(); g.arc(x + (CW - pw) / 2 + k * 7 + 2.5, y + CH - 12, 2.5, 0, Math.PI * 2); g.fill();
    }
  };
  return {
    half: false, height,
    draw(g, t, x, y, w) {
      cardLabel(c, t, tr("Mods").toUpperCase(), x, y + 14);
      // THE PRICE OF THE BUILD, which is half of what a reader is judging.
      const f = formaCount(), used = capacityUsed(), cap = builderCap();
      g.font = `600 19px ${t.display}`;
      const capS = `${used} / ${cap}`;
      const formaS = [`${f.regular} Forma`, f.umbra ? `${f.umbra} Umbra` : null, f.omni ? `${f.omni} Omni` : null].filter(Boolean).join(" · ");
      g.fillStyle = used > cap ? t.mismatched : t.text;
      g.fillText(capS, x, y + 50);
      g.fillStyle = t.muted;
      g.fillText(formaS, x + g.measureText(capS).width + 24, y + 50);
      let gy = y + 64;
      // NOTHING BESIDE THE GRID WITHOUT A RIVEN, so the grid stands centred.
      if (!riven) x += Math.floor((w - gridW) / 2);
      const labelled = (s, yy) => { g.fillStyle = t.muted; g.font = `12px ${t.body}`; g.fillText(s, x, yy + 12); };
      if (hasStance) { labelled(tr("Stance"), gy); modCard(g, t, STANCE, x, gy + 18); gy += CH + 26; }
      ids.forEach((i) => modCard(g, t, i, x + (i % 4) * (CW + GAP), gy + Math.floor(i / 4) * (CH + GAP)));
      gy += CH * 2 + GAP;
      if (hasExilus) { labelled(tr("Exilus"), gy + 8); modCard(g, t, EXILUS, x, gy + 26); }
      if (riven) {
        const rx = x + gridW + 24, rw = w - gridW - 24, ry = y + 64 + (hasStance ? CH + 26 : 0);
        const rh = 74 + rivenLines.length * 30;
        g.strokeStyle = t.riven; g.lineWidth = 1;
        roundRect(g, rx + .5, ry + .5, rw - 1, rh, 10); g.stroke();
        const art = c.img.get(IMG(riven.image));
        if (art) drawFit(g, art, rx + 16, ry + 16, 44, 44);
        g.fillStyle = t.riven; g.font = `600 17px ${t.display}`;
        g.fillText(fitText(g, riven.name, rw - 90), rx + 72, ry + 36);
        g.fillStyle = t.muted; g.font = `12px ${t.body}`;
        g.fillText(tr("Riven"), rx + 72, ry + 55);
        g.font = `14px ${t.body}`;
        rivenLines.forEach((ln, k) => {
          const txt = String(ln).replace(/<[^>]*>/g, "");
          g.fillStyle = /^\s*[-−]/.test(txt) ? t.negative : t.text;
          g.fillText(fitText(g, txt, rw - 32), rx + 16, ry + 92 + k * 30);
        });
      }
    },
  };
}

function drawPol(g, im, x, y, s) {
  // The polarity art is black line work; a light card needs it as it is and a
  // dark one needs it inverted, so it is drawn as a mask in the text colour.
  const off = document.createElement("canvas");
  off.width = off.height = s * 2;
  const o = off.getContext("2d");
  drawFit(o, im, 0, 0, s * 2, s * 2);
  o.globalCompositeOperation = "source-in";
  o.fillStyle = "#f2f4f8"; o.fillRect(0, 0, s * 2, s * 2);
  g.drawImage(off, x, y, s, s);
}

/// A row of named things with their art — arcanes, evolutions, parts.
function cardItems(c, title, items, half) {
  if (!items.length) return null;
  const tile = !half;          // a full-width block lays its items side by side
  return {
    half,
    height: () => 26 + (tile ? 150 : items.length * 84 - 8),
    draw(g, t, x, y, w) {
      cardLabel(c, t, title.toUpperCase(), x, y + 14);
      const y0 = y + 26;
      if (tile) {
        const n = items.length, gap = 12, tw = (w - gap * (n - 1)) / n;
        items.forEach((it, k) => {
          const tx = x + k * (tw + gap);
          g.fillStyle = t.panel; roundRect(g, tx, y0, tw, 150, 10); g.fill();
          g.strokeStyle = t.line; g.lineWidth = 1; roundRect(g, tx + .5, y0 + .5, tw - 1, 149, 10); g.stroke();
          g.textAlign = "center";
          if (it.tag) { g.fillStyle = t.gold; g.font = `600 13px ${t.display}`; g.fillText(it.tag, tx + tw / 2, y0 + 22); }
          const art = c.img.get(it.img);
          if (art) drawFit(g, art, tx + tw / 2 - 30, y0 + 32, 60, 60);
          g.fillStyle = t.text; g.font = `600 13px ${t.body}`;
          wrapText(g, it.name, tw - 16, 2).forEach((ln, j) => g.fillText(ln, tx + tw / 2, y0 + 112 + j * 16));
          g.textAlign = "left";
        });
        return;
      }
      items.forEach((it, k) => {
        const iy = y0 + k * 84;
        g.fillStyle = t.panel; roundRect(g, x, iy, w, 76, 10); g.fill();
        g.strokeStyle = t.line; g.lineWidth = 1; roundRect(g, x + .5, iy + .5, w - 1, 75, 10); g.stroke();
        const art = c.img.get(it.img);
        const tx = art ? x + 80 : x + 16;
        if (art) drawFit(g, art, x + 10, iy + 8, 60, 60);
        g.fillStyle = t.text; g.font = `600 17px ${t.body}`;
        g.fillText(fitText(g, it.name, w - (tx - x) - 14), tx, iy + 33);
        if (it.note) { g.fillStyle = t.muted; g.font = `13px ${t.body}`; g.fillText(fitText(g, it.note, w - (tx - x) - 14), tx, iy + 55); }
      });
    },
  };
}

const cardArcanes = (c) => cardItems(c, tr("Arcane"), (arcanes || []).map((id, i) => {
  const a = id && id !== "none" ? arcaneById(id) : null;
  if (!a) return null;
  const r = arcaneRanks[i] == null ? a.max_rank : arcaneRanks[i];
  return { name: a.name, img: IMG(a.image), note: a.max_rank ? `R${r}${r < a.max_rank ? "/" + a.max_rank : ""}` : "" };
}).filter(Boolean), true);

const cardValence = (c) => {
  const s = valenceSpec($("weapon").value);
  if (!s || !valence.element) return null;
  return cardItems(c, tr("Valence"), [{ name: DT(valence.element), note: `+${Math.round(valence.bonus * 1000) / 10}%` }], true);
};

const cardEvolutions = (c) => cardItems(c, tr("Evolution"), weaponEvos().map((tier) => {
  const o = (tier.options || []).find((x) => x.id === evoSel[tier.tier]);
  return o ? { tag: ROMAN_CARD[tier.tier] || String(tier.tier), name: o.name, img: o.icon ? IMG(o.icon) : null } : null;
}).filter(Boolean), false);

const cardParts = (c) => {
  const s = assemblySpec($("weapon").value);
  if (!s || !assembly) return null;
  const grip = (s.grips || []).find((x) => x.id === assembly.grip);
  const loader = (s.loaders || []).find((x) => x.id === assembly.loader);
  return cardItems(c, tr("Parts"), [
    grip && { name: grip.name, note: tr("Grip"), img: grip.image ? IMG(grip.image) : null },
    loader && { name: loader.name, note: tr("Loader"), img: loader.image ? IMG(loader.image) : null },
  ].filter(Boolean), true);
};

const cardWielder = (c) => {
  const f = (META.warframes || []).find((x) => x.id === buildWielder.frame)
    || (typeof compHosts === "function" ? compHosts() : []).find((x) => x.id === buildWielder.frame);
  if (!f) return null;
  const own = wielderList(buildWielder.frame).find((p) => p.id === wielderIdOf(buildWielder));
  return cardItems(c, tr("Wielder"), [{ name: f.name, img: f.image ? IMG(f.image) : null,
    note: own ? own.name : `${tr("Default")}` }], true);
};

/// EVERY BUILDER STEP, keyed by its block id. The mode is a step and is drawn
/// in the head, under the weapon's name: a mode is an action list that may
/// grow long, so the card names it and the link carries it whole.
const CARD_BLOCKS = {
  "mode-block": "head",
  "assembly-block": cardParts,
  "mod-block": cardMods,
  "arcane-block": cardArcanes,
  "element-block": cardValence,
  "evo-block": cardEvolutions,
  "wielder-block": cardWielder,
};

/// THE HEAD IS A POSTER: the weapon's keyword in outlined capitals fills the
/// back (English in every locale — outlined Chinese reads as noise), the art
/// stands on it lifted by its shadow alone, and the name, its kind and the
/// mode sit bottom left in the reader's language.
function cardHead(c, t, w) {
  const g = c.g, H = 320, W = CARD_W;
  // An ellipse squashed to end inside the head, or its edge draws a box.
  g.save(); g.translate(W * 0.76, H * 0.55); g.scale(1, 0.33);
  const glow = g.createRadialGradient(0, 0, 10, 0, 0, 420);
  glow.addColorStop(0, t.glow); glow.addColorStop(1, "rgba(0,0,0,0)");
  g.fillStyle = glow; g.fillRect(-W, -H * 3, W * 2, H * 6);
  g.restore();
  const kw = cardKeyword(w.name_en || w.name);
  g.font = `700 100px ${t.display}`;
  const size = Math.min(250, Math.floor(100 * 1000 / Math.max(1, g.measureText(kw.key).width)));
  g.font = `700 ${size}px ${t.display}`;
  const top = (H - size * 0.72) / 2 + size * 0.72;
  g.strokeStyle = t.mark; g.lineWidth = 1.5;
  g.strokeText(kw.key, 30, top);
  const small = Math.max(20, Math.round(size * 0.17));
  g.font = `700 ${small}px ${t.display}`;
  g.fillStyle = t.markWords;
  const capTop = top - size * 0.72 - 10;
  if (kw.before) g.fillText(kw.before, 36, capTop);
  if (kw.after) {
    g.font = `700 ${size}px ${t.display}`;
    const end = 30 + g.measureText(kw.key).width;
    g.font = `700 ${small}px ${t.display}`;
    g.textAlign = "right"; g.fillText(kw.after, end - 6, capTop); g.textAlign = "left";
  }
  const art = c.img.get(IMG(w.image));
  if (art) {
    g.save();
    g.shadowColor = "rgba(0,0,0,.9)"; g.shadowBlur = 30; g.shadowOffsetY = 22;
    drawFit(g, art, W - 60 - 280, 24, 280, 280);
    g.restore();
  }
  // name, kind, mode
  const x = CARD_PAD, maxW = 560;
  g.font = `600 13px ${t.body}`; g.fillStyle = t.gold;
  const kind = [tr(cap1(w.slot || "")), tr(w.subtype || w.mod_class || "")].filter(Boolean).join(" · ");
  let ns = 64;
  g.font = `700 ${ns}px ${t.display}`;
  while (ns > 40 && g.measureText(w.name).width > maxW) { ns -= 2; g.font = `700 ${ns}px ${t.display}`; }
  const nameLines = wrapText(g, w.name, maxW, 2);
  const modeY = H - 40, nameY = modeY - 44 - (nameLines.length - 1) * ns * 1.05;
  g.save(); g.shadowColor = "rgba(0,0,0,.6)"; g.shadowBlur = 24; g.fillStyle = t.text;
  nameLines.forEach((ln, k) => g.fillText(ln, x, nameY + k * ns * 1.05));
  g.restore();
  g.font = `600 13px ${t.body}`; g.fillStyle = t.gold;
  g.fillText(kind.toUpperCase(), x, nameY - ns * 0.95 - 4);
  g.font = `600 13px ${t.body}`; g.fillStyle = t.muted;
  const label = tr("Mode");
  g.fillText(label, x, modeY);
  const lx = x + g.measureText(label).width + 12;
  g.font = `700 22px ${t.display}`; g.fillStyle = t.gold;
  g.fillText(fitText(g, modeLabel(w, mode), maxW - (lx - x)), lx, modeY + 1);
  return H;
}

/// THE SHARER'S RESULT, when they chose to send one — the number, its unit and
/// the fight it came from, in the app's own spelling.
function cardResult(c, t, m, y) {
  const g = c.g, x = CARD_PAD;
  g.strokeStyle = t.line; g.beginPath(); g.moveTo(x, y + .5); g.lineTo(CARD_W - x, y + .5); g.stroke();
  g.font = `700 46px ${t.display}`; g.fillStyle = t.gold;
  g.fillText(m.value, x, y + 62);
  const vw = g.measureText(m.value).width;
  g.font = `600 18px ${t.body}`; g.fillStyle = t.text;
  g.fillText(m.unit, x + vw + 14, y + 62);
  g.font = `14px ${t.body}`; g.fillStyle = t.muted;
  g.fillText(fitText(g, m.scene, CARD_W - 2 * x), x, y + 90);
  return 112;
}

/// The blocks this build has, in the builder's order.
function cardBlocks(c) {
  return builderSteps().filter((b) => !b.hidden).map((b) => {
    const make = CARD_BLOCKS[b.id];
    const block = typeof make === "function" ? make(c) : null;
    return block && { ...block, id: b.id };
  }).filter(Boolean);
}

/// Every picture the card may draw, fetched before anything is laid out.
function cardImages(w) {
  const want = new Set([IMG(w.image)]);
  slots.forEach((s) => {
    const m = s && s.mod ? modById(s.mod) : null;
    if (m && m.image) want.add(IMG(m.image));
    if (s && s.pol) want.add(POL(polCap(s.pol)));
  });
  (arcanes || []).forEach((id) => { const a = id && id !== "none" && arcaneById(id); if (a && a.image) want.add(IMG(a.image)); });
  weaponEvos().forEach((tier) => {
    const o = (tier.options || []).find((x) => x.id === evoSel[tier.tier]);
    if (o && o.icon) want.add(IMG(o.icon));
  });
  const f = (META.warframes || []).find((x) => x.id === buildWielder.frame);
  if (f && f.image) want.add(IMG(f.image));
  const list = [...want].filter(Boolean);
  return Promise.all(list.map(loadImg)).then((ims) => new Map(list.map((k, i) => [k, ims[i]])));
}

/// THE SHOWCASE, under the head: who shared it and what they gave the board —
/// `s.name`, `s.volunteer`, the signed link's `s.mark`, and the figures the
/// sharer picked, `s.stats` as `[value, label]`, and `s.rank`, their
/// contributor rank or null. The rank and the honour are filled and a paid
/// mark only outlined, so what was computed reads first.
const SHOWCASE_H = 150;
function cardShowcase(c, t, s, y0) {
  const g = c.g, x = CARD_PAD, y = y0 + 8, w = CARD_W - 2 * CARD_PAD, h = SHOWCASE_H - 24;
  g.fillStyle = t.panel; roundRect(g, x, y, w, h, 14); g.fill();
  g.strokeStyle = t.line; g.lineWidth = 1; roundRect(g, x + .5, y + .5, w - 1, h - 1, 14); g.stroke();
  g.fillStyle = t.gold; roundRect(g, x, y + 18, 4, h - 36, 2); g.fill();
  const lx = x + 32;
  g.font = `13px ${t.body}`; g.fillStyle = t.muted; g.fillText(tr("Shared by"), lx, y + 34);
  g.font = `700 32px ${t.display}`; g.fillStyle = t.text; g.fillText(s.name, lx, y + 74);
  let bx = lx;
  const pill = (text, filled, ink) => {
    g.font = `600 13px ${t.body}`;
    const pw = g.measureText(text).width + 22;
    if (ink) { g.fillStyle = t.line; roundRect(g, bx, y + 88, pw, 24, 12); g.fill(); g.fillStyle = ink; }
    else if (filled) { g.fillStyle = t.gold; roundRect(g, bx, y + 88, pw, 24, 12); g.fill(); g.fillStyle = t.bg; }
    else { g.strokeStyle = t.gold; roundRect(g, bx + .5, y + 88.5, pw - 1, 23, 12); g.stroke(); g.fillStyle = t.gold; }
    g.fillText(text, bx + 11, y + 105);
    bx += pw + 8;
  };
  if (s.rank != null) pill(tr("Contributor rank {n}").replace("{n}", s.rank), true, t.text);
  if (s.volunteer) pill(tr("WFSim Volunteer"), true);
  if (s.mark) pill(s.mark, false);
  let rx = x + w - 32;
  g.textAlign = "right";
  for (let i = s.stats.length - 1; i >= 0; i--) {
    const [v, label] = s.stats[i];
    g.font = `700 34px ${t.display}`; const vw = g.measureText(v).width;
    g.font = `13px ${t.body}`; const lw = g.measureText(label).width;
    g.font = `700 34px ${t.display}`; g.fillStyle = i === 0 ? t.gold : t.text; g.fillText(v, rx, y + 74);
    g.font = `13px ${t.body}`; g.fillStyle = t.muted; g.fillText(label, rx, y + 100);
    rx -= Math.max(vw, lw) + 44;
    if (i > 0) { g.strokeStyle = t.line; g.beginPath(); g.moveTo(rx + 22.5, y + 40); g.lineTo(rx + 22.5, y + 104); g.stroke(); }
  }
  g.textAlign = "left";
  return SHOWCASE_H;
}

/// THE CARD. `opts.measured` is the sharer's result or null; `opts.by` the
/// signer's name and mark when the link is signed; `opts.showcase` what the
/// sharer chose to show of themselves (`cardShowcase`), which then names them
/// and the foot does not.
async function drawShareCard(canvas, url, opts = {}) {
  const t = CARD_THEMES[opts.theme] || CARD_THEMES.default;
  const w = weaponInfo($("weapon").value);
  const c = { g: null, img: await cardImages(w) };
  // Measured on a scratch context first: the height is the sum of the blocks.
  const scratch = document.createElement("canvas").getContext("2d");
  c.g = scratch;
  const blocks = cardBlocks(c);
  const inner = CARD_W - 2 * CARD_PAD, GAP_X = 28, GAP_Y = 36;
  const rows = [];
  for (let i = 0; i < blocks.length; i++) {
    const b = blocks[i];
    if (b.half && blocks[i + 1] && blocks[i + 1].half) { rows.push([b, blocks[i + 1]]); i++; }
    else rows.push([b]);
  }
  const colW = (row) => (row.length === 2 || row[0].half ? (inner - GAP_X) / 2 : inner);
  const rowH = rows.map((row) => Math.max(...row.map((b) => b.height(colW(row)))));
  const qc = qrCanvas(url);
  const QR = 120;
  const H = 320 + (opts.showcase ? SHOWCASE_H : 0) + (opts.measured ? 112 : 0) + 24 + rowH.reduce((a, h) => a + h + GAP_Y, 0) + QR + 56;

  canvas.width = CARD_W * CARD_DPR; canvas.height = H * CARD_DPR;
  const g = canvas.getContext("2d");
  g.scale(CARD_DPR, CARD_DPR);
  c.g = g;
  g.textBaseline = "alphabetic";
  g.fillStyle = t.bg; g.fillRect(0, 0, CARD_W, H);
  let y = cardHead(c, t, w);
  if (opts.showcase) y += cardShowcase(c, t, opts.showcase, y);
  if (opts.measured) y += cardResult(c, t, opts.measured, y);
  y += 24;
  rows.forEach((row, k) => {
    const cw = colW(row);
    row.forEach((b, j) => b.draw(g, t, CARD_PAD + j * (cw + GAP_X), y, cw));
    y += rowH[k] + GAP_Y;
  });

  // ---- the foot: the wordmark, who shared it, and the way back ----------
  // A card is pasted into a chat and read on a PHONE, which cannot click a
  // picture — without the code the link and the image are two things to send.
  const fy = H - QR - 32;
  g.strokeStyle = t.line; g.beginPath(); g.moveTo(CARD_PAD, fy - 18.5); g.lineTo(CARD_W - CARD_PAD, fy - 18.5); g.stroke();
  g.font = `700 26px ${t.display}`;
  g.fillStyle = t.text; g.fillText("WF", CARD_PAD, fy + 40);
  g.fillStyle = t.gold; g.fillText("Sim", CARD_PAD + g.measureText("WF").width, fy + 40);
  g.font = `14px ${t.body}`; g.fillStyle = t.muted;
  g.fillText(tr("Every number here is the WFSim engine's — open the link to run it yourself."), CARD_PAD, fy + 70);
  if (opts.by && !opts.showcase) {
    g.font = `14px ${t.body}`; g.fillStyle = t.muted;
    const lead = tr("Shared by") + " ";
    g.fillText(lead, CARD_PAD, fy + 98);
    let bx = CARD_PAD + g.measureText(lead).width;
    g.font = `700 15px ${t.body}`; g.fillStyle = t.text;
    g.fillText(opts.by.name, bx, fy + 98);
    bx += g.measureText(opts.by.name).width + 10;
    if (opts.by.mark) {
      g.font = `600 12px ${t.body}`;
      const mw = g.measureText(opts.by.mark).width + 18;
      g.strokeStyle = t.gold; roundRect(g, bx, fy + 84, mw, 20, 10); g.stroke();
      g.fillStyle = t.gold; g.fillText(opts.by.mark, bx + 9, fy + 98);
      bx += mw + 10;
    }
    g.font = `12px ${t.body}`; g.fillStyle = t.muted;
    g.fillText(tr("checked with wfsim.app"), bx, fy + 98);
  }
  if (qc) {
    g.imageSmoothingEnabled = false;
    g.drawImage(qc, CARD_W - CARD_PAD - QR, fy, QR, QR);
    g.imageSmoothingEnabled = true;
    g.textAlign = "right"; g.font = `13px ${t.body}`; g.fillStyle = t.muted;
    g.fillText(SITE_HOST, CARD_W - CARD_PAD - QR - 16, fy + QR - 4);
    g.textAlign = "left";
  }
  // A FRAME, where a theme names one: drawn last, over every block's edge.
  if (t.frame) {
    const fw = t.frameWidth || 2;
    g.strokeStyle = t.frame; g.lineWidth = fw;
    roundRect(g, fw / 2 + 6, fw / 2 + 6, CARD_W - fw - 12, H - fw - 12, 14); g.stroke();
  }
}
