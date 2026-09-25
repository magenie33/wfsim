// ---- Stats panel: merged buckets, each explained by source ----

/// WHAT THE PANEL ON SCREEN WAS BUILT FROM. Null until the first one lands.
let panelKey = null;

/// The build, as a value. Same idea as `simKey`: a key DERIVED from the state,
/// never a hand-listed set of things that ought to trigger a refresh.
const buildKey = () =>
  JSON.stringify([$("weapon").value, slots, arcanes, arcaneRanks, evoSel, rivenPayload(),
                  assembly]);

/// THE PANEL REFRESHES BECAUSE THE BUILD CHANGED, not because a control
/// remembered to say so.
///
/// Making every mutation responsible for calling `refreshPanel` is N places to
/// get right: the picker redraws its own slots while the panel — and the sim's
/// buff bar — keeps showing the previous arcane until an unrelated edit happens
/// to
/// refresh them (reported 2026-08-05). Fixing that one site would have left the
/// same trap for the next control someone adds.
///
/// So the trigger is derived instead. After any interaction anywhere, if the
/// build no longer matches what the panel was built from, the panel is rebuilt.
/// A control cannot forget, because it is never asked: this is the same rule
/// `check_gain_freshness` already asserts for the gain scan — the cache key is
/// DERIVED from the thing it describes, never a copy of it maintained by hand.
///
/// The explicit calls that remain are not redundant belt-and-braces; they make
/// the refresh IMMEDIATE rather than waiting for the next event. This is what
/// makes it CORRECT.
function panelWatchdog() {
  // After the handler that caused it, not during — a click that mutates state
  // runs its own listener first.
  setTimeout(() => {
    if (panelKey !== null && buildKey() !== panelKey) refreshPanel();
  }, 0);
}
for (const ev of ["click", "change", "input", "keyup"]) {
  document.addEventListener(ev, panelWatchdog, true);
}

let panelTimer = null;
function refreshPanel() {
  markPresetDirty(); // every build change funnels through here
  clearTimeout(panelTimer);
  panelTimer = setTimeout(async () => {
    const body = buildPayload();
    // Recorded where the payload is actually BUILT, so the key always
    // describes what was sent — recording it at call time would go stale
    // inside the debounce window.
    panelKey = buildKey();
    try {
      const r = await api("/api/panel", body);
      renderPanel(r);
    } catch (e) {
      $("stats-rows").innerHTML = `<div class="error">panel failed: ${e}</div>`;
    }
  }, 120);
}

function renderPanel(r) {
  if (!r || r.ok === false) {
    $("stats-rows").innerHTML = `<div class="error">${r ? r.error : "no data"}</div>`;
    return;
  }
  $("stats-sub").textContent = `max-rank values · ${r.policy}`;
  // A PASSIVE WE DO NOT MODEL makes every number below a FLOOR, and the reader
  // has no way to know that from the numbers themselves. Gotva Prime is the
  // first: its "15% chance to set the next hit's crit chance to 300%" is absent, so it scores like a weapon without a passive and looks simply
  // weaker rather than partly unmodelled.
  const wInfo = weaponInfo($("weapon").value) || {};
  const passiveNote = $("stats-passive");
  if (passiveNote) {
    // WHAT THIS WEAPON'S ENTRY DOES NOT MODEL, said in words, above the numbers
    // that omit it. Two sources, one banner: · the
    // PASSIVE flag, for a weapon whose prose passive has no rule yet; ·
    // `unmodeled`, the weapon file's own list — the bulk Incarnon intake
    // writes one line here per base attack part it could not carry, and a
    //     bow's uncharged shot or the Angstrum's explosion is exactly the kind
    //     of gap that makes a complete-looking number wrong.
    // A yaml comment is honest to whoever opens the file and invisible to
    // everyone else, which is not the same as honest.
    const gaps = gapsOf(wInfo).slice();
    if (wInfo.passive_unmodeled) {
      gaps.unshift(tr("this weapon's passive is not modelled yet"));
    }
    passiveNote.hidden = !gaps.length;
    passiveNote.innerHTML = gaps.length
      ? `<div class="unmod-h">◈ ${escHtml(tr("not modelled on this weapon — the numbers below are a floor, not its full output"))}</div>`
        // THROUGH `tr`, like the enemy card's gaps. These are OUR sentences,
        // not DE's, so they translate rather than being transcribed — and a
        // line with no entry falls through to the English it was written in,
        // which is the overlay's whole contract. Rendering them raw left a
        // Chinese page with the one paragraph that matters in English.
        // Through `trGap`, which knows a REASON from a sentence: a repeating
        // gap carries its template and parameters, so the overlay translates
        // the template once instead of once per set of numbers.
        + gaps.map((g) => `<div class="unmod-l">${escHtml(trGap(g))}</div>`).join("")
      : "";
    // …AND THE OPPOSITE ADMISSION, in its own block. A LIVE BUG says the number
    // IS right — measured, reproduced — and that nobody can explain it, so a
    // hotfix could take it away. Filing it under "not modelled" would tell the
    // reader the number is a floor, which is the one thing it is not.
    //
    // The case it exists for: the Laetum's Incarnon form pays Secondary
    // Irradiate's echo at 3.6x the hit while the arcane's own card, on the same
    // screen, prints DE's 180%. A number that departs from the card beside it
    // and says so nowhere reads as a bug, and it was reported as one. VERBATIM, not through `trGap`: these are OUR sentences and
    // they translate, but they carry no template holes.
    const bugs = (wInfo.live_bugs || []).slice();
    if ($("stats-livebug")) {
      $("stats-livebug").hidden = !bugs.length;
      $("stats-livebug").innerHTML = bugs.length
        ? `<div class="unmod-h">⚠ ${escHtml(
            tr("measured on this weapon and unexplained — the engine reproduces what the game does, and a hotfix could change it"))}</div>`
          + bugs.map((b) => `<div class="unmod-l">${escHtml(tr(b))}</div>`).join("")
        : "";
    }
  }
  // A source the row's LOCK is ignoring still lists, struck through and said
  // out loud: "Fire Rate cannot be modified" means this mod's bonus is not in
  // the number above, and a line that looks like every other line claims the
  // opposite.
  const srcLine = (s) =>
    `<div class="ssrc${s.ignored ? " sdead" : ""}"${s.ignored ? ` title="${escHtml(tr("ignored — this stat is locked at the weapon's default"))}"` : ""}>${s.value} — ${s.mod}${s.note ? ` <span class="snote">(${s.note})</span>` : ""}${s.ignored ? ` <span class="snote">(${escHtml(tr("ignored"))})</span>` : ""}</div>`;

  // THE MULTIPLICATIVE BUCKET, DRAWN (community request, 2026-08-05: the app
  // does the hard arithmetic and then shows only its answer, so the mechanics
  // stay as murky as they were).
  //
  // Warframe damage is a product of buckets: bonuses inside one bucket ADD,
  // buckets MULTIPLY. Which bucket a mod lands in is the single most useful
  // thing to know about it — adding to a bucket already at +200% is worth far
  // less than opening a new one — and the panel already had every number
  // needed to show it, arranged so you could only infer it.
  //
  // So draw the expression: `40.0 × (1 + 1.65 + 0.60) = 130`. Everything inside
  // one bracket is one bucket, and that shape needs no sentence to explain it.
  // It is the reader's OWN build's arithmetic, which is the thing a wiki cannot
  // give them.
  //
  // ONLY WHEN EVERY TERM IS A FRACTION. Evolution sources send no `frac`
  // because several are flat additions rather than percentages, and a line that
  // rendered them as `+ 0.5` would be asserting arithmetic the engine did not
  // do. Fewer than two terms is left alone as well — `40 × (1 + 1.65)` teaches
  // nothing that `40 → 106` did not.
  const bucketLine = (row) => {
    const src = row.sources || [];
    // A LOCKED row has no arithmetic to draw: its bucket was emptied, so
    // `3.3 × (1 - 0.20) = 3.3` would be a false equation printed in the one
    // place that exists to show the real one.
    if (row.locked_by) return "";
    if (src.length < 2 || !src.every((x) => typeof x.fraction === "number")) return "";
    const base = String(row.base || "").replace(/[^0-9.\-]/g, "");
    if (!base || row.base === "—") return "";
    const terms = src
      .map((x) => `<span class="bterm" title="${escHtml(x.mod)}">${x.fraction.toFixed(2)}</span>`)
      .join(" + ");
    return `<div class="sbucket">${escHtml(base)} × ( 1 + ${terms} ) = <b>${escHtml(row.final)}</b>` +
      ` <span class="bhint" title="${escHtml(
        tr("everything inside the bracket is ONE multiplicative bucket: these add together, and the bucket multiplies against the others"),
      )}">?</span></div>`;
  };

  const rowHtml = (row) => `
    <div class="srow">
      <div class="shead"><span class="sk">${tr(row.label)}</span>
        <span class="sv">${row.base !== "—" && row.base !== row.final ? `<span class="sbase">${tr(row.base)}</span> → ` : ""}<b>${tr(row.final)}</b></span></div>
      ${row.rule ? `<div class="srowrule" title="${escHtml(tr("the rule this weapon is computed under: how the bonus combines, what base it reads, and which attack parts take it"))}">▸ ${escHtml(row.rule)}</div>` : ""}
      ${row.note ? `<div class="srownote">⚙ ${row.note}</div>` : ""}
      ${row.locked_by ? `<div class="srownote">🔒 ${escHtml(tr("locked at the weapon's default by"))} ${escHtml(row.locked_by)}</div>` : ""}
      ${bucketLine(row)}
      ${(row.sources || []).map(srcLine).join("")}
    </div>`;
  const dmgHtml = (p) => (p.damage && p.damage.length)
    ? `<div class="sdmg-title">${tr("Damage (combined)")} — ${p.damage_total} ${tr("total")}</div>` +
      p.damage.map((d) => `<div class="sdmg"><span class="sk">${DT(d.type)}</span><span class="sv"><b>${d.amount}</b> <span class="snote">${d.share}</span></span></div>`).join("")
    : "";
  // A weapon is the GUN plus the PROJECTILE(s) it launches: the gun block
  // carries cadence and capacity, each projectile block its own damage,
  // crit and status — and a radial its blast geometry too. An Incarnon
  // Laetum shot is two instances, so it renders two projectile blocks.
  const partHtml = (p) => `
    <div class="fpart" data-part="${p.id}">
      <div class="fparth">${tr(p.label)}<span class="fmeta">${tr(p.meta)}</span></div>
      ${(p.stats || []).map(rowHtml).join("")}
      ${dmgHtml(p)}
    </div>`;
  // EVERY available form renders as its own section (base + Incarnon side
  // by side — no switching), headed by the form name + trigger mechanics.
  // Indirect stats (recoil, accuracy, ammo…) render like any bucket — they
  // are outside theoretical DPS but real in practice, so the panel states them.
  const section = (f) => `
    <div class="fsec">
      <div class="fhead">${tr(f.label)}<span class="fmeta">${f.meta}</span></div>
      ${[...(f.stats || []), ...(f.elements || []), ...(f.indirect || [])].map(rowHtml).join("")}
      ${(f.parts || []).map(partHtml).join("")}
    </div>`;
  $("stats-rows").innerHTML = (r.forms || []).map(section).join("");
  $("stats-damage").innerHTML = "";

  // WHO IS HOLDING THE GUN, stated before the clauses that read it. Several perks and mods are answered by the PLAYER — a gate at
  // 200 max energy, a bonus per 75 health, Primary Bulwark past 1,000 armor —
  // and the panel listed those clauses while saying nothing about the numbers
  // deciding them. The fields above read "0 = no frame", which is what they
  // MEAN and not what the fight resolved to once a frame, an aura or an archon
  // shard had moved them.
  //
  // The numbers are the SERVER's, from the same `tenno_from` every gate is
  // asked against, so this cannot drift from what was actually simulated.
  const wf = r.tenno;
  // THE WIELDER BEFORE THE OVERRIDES moves the fight panel's floor line and the
  // number a ticked override starts at, so a new one redraws the fight panel.
  const wielderNow = r.wielder || null;
  if (JSON.stringify(wielderNow) !== JSON.stringify(panelWielder)) {
    panelWielder = wielderNow;
    if (typeof renderSim === "function") renderSim();
  }
  $("stats-conditionals").innerHTML = (wf
    ? `<div class="sdmg-title">${escHtml(tr("Who is holding it"))}${wielderNow ? " · " + escHtml(tr(wielderNow.name)) : ""}</div>` +
      // ITS OWN CLASS, not `.scond`: that class IS the list of conditional
      // clauses, and `#stats-conditionals .scond` is how they are counted.
      `<div class="swielder">` + [
        [tr("Health"), wf.health], [tr("Shield"), wf.shield], [tr("Armor"), wf.armor],
        [tr("Max energy"), wf.energy], [tr("Sprint"), wf.sprint],
      ].map(([l, v]) => `<span><b>${escHtml(l)}</b> ${sig2(v || 0)}</span>`).join("")
      + `</div>`
    : "") + ((r.conditionals && r.conditionals.length)
    ? `<div class="sdmg-title">Conditional / not merged</div>` +
      r.conditionals.map((c) => `<div class="scond ${c.active ? "" : "off"}"><b>${c.mod}</b>: ${c.desc} <span class="snote">${c.why}</span></div>`).join("")
    : "");
  // The build's configurable buffs (weapon-scoped) drive the Sim section 2.
  buffList = r.buffs || [];
  renderSimBuffs();
}

// Wiki page for an item, from its display name (the data files' source
// urls follow the same Name_With_Underscores convention).
const wikiUrl = (name) => "https://wiki.warframe.com/w/" + encodeURIComponent(name.replace(/ /g, "_"));
// EVERY item name (mod / arcane / evolution) opens its wiki page in a new
// tab; the click never triggers the card's own handlers.
// ALWAYS pass an explicit url built from the ENGLISH name (x.name_en ||
// x.name) — `text` is the displayed (possibly localized) name, and the
// wiki only has English page names. The no-url fallback exists for plain
// English literals only.
const wl = (text, url) => `<a class="wl" href="${url || wikiUrl(text)}" target="_blank" rel="noopener" onclick="event.stopPropagation()">${text}</a>`;

/// THE WIKI PAGE BEHIND A MOD CARD — and a RIVEN'S IS THE MECHANIC'S.
///
/// Every other mod is one published card with one page. A riven is generated:
/// "Argon Critacan" is a name a drop invented, the wiki has no page by it, and
/// building the URL from the name the way the rest do gives a link that always
/// 404s. What a reader clicking a riven wants is the mechanic — disposition,
/// rolling, the stat pools — which is `Riven Mods`.
const modWikiUrl = (m) => (m.riven ? wikiUrl("Riven Mods") : wikiUrl(m.name_en || m.name));

/// THE ONE NUMBER THIS APP DOES NOT COMPUTE — what a card costs to buy.
///
/// warframe.market is the authority there and we are not competing with it, so
/// a tradeable card carries a mark that opens its page. The slug comes from
/// `/api/meta` (`engine::data::market`), never from the display name: a name
/// join pairs "Blaze" the mod with "Blaze" the arcane, and a localized page
/// has no English name to join on at all.
///
/// NO PRICE IS FETCHED. A price needs a refresh rule and a staleness rule, and
/// the app would then hold a number it cannot stand behind next to numbers it
/// can.
const MARKET = "https://warframe.market";
const marketItemUrl = (slug) => `${MARKET}/items/${encodeURIComponent(slug)}`;

/// THE AUCTION SEARCH FOR A RIVEN — filtered to the stats it actually rolled.
///
/// Not the weapon's whole auction list: unfiltered, that page returns its own
/// 500-result ceiling sorted by nothing a holder cares about. Naming the
/// stats turns it into the rivens that COMPETE with the one on screen, which
/// is the comparison the card's own number was asking for.
///
/// A stat we cannot translate is not silently dropped — see `rivenMarketUrl`.
const marketAuctionUrl = (weaponSlug, positives, negatives) => {
  const q = [["type", "riven"], ["weapon_url_name", weaponSlug]];
  if (positives.length) q.push(["positive_stats", positives.join(",")]);
  if (negatives.length) q.push(["negative_stats", negatives.join(",")]);
  return `${MARKET}/auctions/search?` +
    q.map(([k, v]) => `${k}=${encodeURIComponent(v)}`).join("&");
};

/// The auction link for one riven card, or null if it cannot be built HONESTLY.
///
/// A missing stat slug would produce a NARROWER search than the riven —
/// different rivens, presented as the comparable ones — so a stat that does
/// not translate cancels the link rather than shrinking the query.
function rivenMarketUrl(spec, weapon) {
  const slug = (weapon || {}).market_riven_slug;
  if (!slug || !spec) return null;
  const table = META.market_riven_stats || {};
  // An UNFILLED slot is a card still being described, not a stat that failed
  // to translate — it narrows nothing, so it is skipped and the rest still
  // search.
  const named = (xs) => xs.filter((s) => s && s.id).map((s) => table[s.id]);
  const pos = named(spec.bonuses || []), neg = named(spec.malus ? [spec.malus] : []);
  if (!pos.length || pos.concat(neg).some((s) => !s)) return null;
  return marketAuctionUrl(slug, pos, neg);
}

/// The mark itself. `marketPrefs.on` is the reader's switch (topbar ⋯), and
/// the link is skipped entirely when it is off — not hidden with CSS, so the
/// page does not ship a row of dead anchors to a reader who said no.
const marketLink = (url, title) => (!url || marketPrefs.on === false ? "" :
  `<a class="wm" href="${url}" target="_blank" rel="noopener" title="${escHtml(title)}"
      onclick="event.stopPropagation()" aria-label="${escHtml(title)}"><svg viewBox="0 0 24 24" width="12" height="12"
      aria-hidden="true" focusable="false"><path fill="currentColor" d="M7 4h10a1 1 0 0 1 .97.76l2 8A1 1 0 0 1 19 14H5a1 1 0 0 1-.97-1.24l2-8A1 1 0 0 1 7 4zm0 12h10a1 1 0 0 1 0 2H7a1 1 0 0 1 0-2zm1.5 3a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zm7 0a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3z"/></svg></a>`);

/// THE MARK ON A MOD CARD, wherever a card is drawn: the builder's picker and
/// the optimizer's scope (both through `modRow`) and the EQUIPPED slot, which
/// is where a reader decides they want one and is the arcane slot's own rule.
///
/// A RIVEN'S MARK IS ITS AUCTION, not an item page: warframe.market has no
/// item to sell for "Argon Critacan", and the same generated-name problem that
/// sends a riven's wiki link to `Riven Mods` sends its price question to the
/// auctions for the weapon it is on, filtered to the stats it rolled.
const modMarketLink = (m) => marketLink(
  m.riven ? m.market_url : (m.market_slug && marketItemUrl(m.market_slug)),
  tr(m.riven ? "find rivens with these stats on warframe.market"
    : "price on warframe.market"));

/// THE MARK ON AN ARCANE CARD. Three lists draw an arcane — the equipped slot,
/// the picker and the optimizer's scope — and they are three copies of the
/// same markup rather than one `modRow`, so this is called three times.
const arcaneMarketLink = (a) =>
  marketLink(a.market_slug && marketItemUrl(a.market_slug), tr("price on warframe.market"));

/// THE MARK ON THE WEAPON ITSELF, and the weapon is SOLD or AUCTIONED.
///
/// An ordinary weapon that trades at all trades as an item — a Prime as its
/// SET, which is what the slug names. An adversary weapon has no item: the
/// VALENCE a copy rolled is part of what changes hands, so it is auctioned
/// like a riven, and the link carries the element this build is using. Without
/// that the link lands on every Kuva Bramma ever listed, which is the 500-row
/// ceiling again and not the weapon the reader has configured.
///
/// OUR ELEMENT IDS ARE THEIR FILTER VALUES, all seven checked against the live
/// search: impact, heat, cold, electricity, toxin, magnetic, radiation. It
/// rejects a value it does not know with a 400, and it rejects "electric" —
/// so this is a join and not a coincidence of spelling.
function weaponMarketLink(w) {
  if (!w) return "";
  const auction = w.market_auction;
  if (auction) {
    const el = (valenceSpec(w.id) && valence && valence.element) || "";
    return marketLink(
      `${MARKET}/auctions/search?type=${encodeURIComponent(auction.type)}` +
      `&weapon_url_name=${encodeURIComponent(auction.slug)}` +
      (el ? `&element=${encodeURIComponent(el)}` : ""),
      tr("find this weapon on warframe.market"));
  }
  return marketLink(w.market_slug && marketItemUrl(w.market_slug),
    tr("price on warframe.market"));
}

/// ON BY DEFAULT: the mark is a convenience a reader cannot ask for if they
/// never see it, and it costs one glyph beside a name.
let marketPrefs = { on: true };
try { const s = JSON.parse(localStorage.getItem("wfsim-market")); if (s) marketPrefs = { ...marketPrefs, ...s }; } catch (_) { /* private mode */ }

/// The switch, in the topbar's overflow beside the other page-wide settings.
/// EVERY LIST IS REDRAWN, not just the open one: the mark sits in the picker,
/// in the optimizer's scope and on the equipped slots, and a switch that only
/// reached whichever was on screen would read as broken on the next tab.
(function () {
  const box = $("market-on");
  if (!box) return;
  box.checked = marketPrefs.on !== false;
  box.addEventListener("change", (e) => {
    e.stopPropagation();
    marketPrefs = { ...marketPrefs, on: box.checked };
    try { localStorage.setItem("wfsim-market", JSON.stringify(marketPrefs)); } catch (_) { /* private mode */ }
    if ($("mod-slots")) { renderMods(); renderArcanes(); }
    // The weapon's own mark lives in the title, which nothing above redraws.
    const w = weaponInfo($("weapon") && $("weapon").value);
    if (w) renderWeaponName();
    const pop = $("mod-popover");
    if (pop && !pop.hidden) renderMenu(pickerSlot, $("mod-search").value);
  });
})();

// Description lines at a rank: the verbatim in-game text with the
// rank-varying numbers filled server-side (mods and arcanes alike). Null
// when the pool has no yaml description (hardcoded rifle pool) — callers
// fall back to the model's effect lines.
const descAt = (o, r) => o.desc_ranks
  ? o.desc_ranks[Math.max(0, Math.min(o.desc_ranks.length - 1, r))].split("\n")
  : null;

// The SAME card, in DE's own words, when the active locale has them
// (data/i18n/<locale>/descriptions.yaml — mods and arcanes, one entry per
// rank). Preferred over translating our English line, because a card is not
// a bag of terms: "(x2 for Bows)" is "（弓类武器效果加倍）", which no phrase
// table reaches. Ids never collide across the two tables (engine test).
const officialDesc = (o, r) => {
  if (!I18N || !o || !o.id) return null;
  const t = (I18N.mod_descriptions || {})[o.id] || (I18N.arcane_descriptions || {})[o.id];
  return t && t.length ? t[Math.max(0, Math.min(t.length - 1, r))].split("\n") : null;
};

// Card lines at a rank, ALREADY in the display language: DE's sentence when
// there is one, otherwise our English line with the phrase table applied.
// Every caller renders these verbatim — nothing runs `tf` over a line that
// was already written in the target language.
// An EVOLUTION's card lines. Same rule as a mod's, with one difference that
// matters: when the locale has no transcription, the English falls through
// UNTOUCHED. Running the phrase table over prose is what produced
// "Increase Base 伤害 by +60." — half-swapped is worse than either language,
// and evolutions are the only card written as prose rather than as terms.
// `o.effects` (our own model statement) IS term-shaped, so it still gets tf.
const evoLines = (o) => {
  const zh = I18N && (I18N.evolution_descriptions || {})[o.id];
  if (zh) return zh.split("\n");
  if (o.desc && o.desc.length) return o.desc;      // English, as written
  return (o.effects || []).map(tf);
};

// WHAT WE DO NOT MODEL, appended to whatever the card already says.
//
// Not a replacement: DE's own text is what makes a player expect the thing to
// work, so hiding our gap behind it is the worst of both. `cardLines` prefers
// `officialDesc`, which every one of these has — so a "not modeled" line the
// model produced is suppressed on precisely the items that need it, which is
// how 5 arcanes and 12 mods carried a knowingly skipped effect in silence.
// TWO DIFFERENT ADMISSIONS, and saying "not modelled" for both is what made the
// whole app look unfinished. One is a todo; the other is the edge
// of what a single-target damage simulator IS.
/// AN EVOLUTION'S THREE ADMISSIONS, as chips. A player deciding what to equip
/// needs to know which they are looking at: a todo may be gone next week, an
/// EDGE is what a single-target damage simulator is.
///
/// AND THE THIRD IS NOT A SHORTFALL AT ALL: a clause the GAME does not pay out
/// (M49). It is the chip whose reader ACTION differs most — the other two say
/// wait for us, this one says do not pick the perk for that half, because
/// nobody is going to implement what DE has not shipped.
const evoGapChips = (o, tag) => {
  const todo = o.unmodeled || [];
  const edge = o.out_of_scope || [];
  const dead = o.live_bugs || [];
  const out = [];
  if (todo.length) {
    out.push(`<${tag} class="exchip unmod" title="${escHtml(
      (o.fully_unmodeled && !edge.length
        ? tr("this perk does nothing in the simulation — the model has no rule for it yet")
        : tr("part of this perk is not modelled — what it does here is less than the card says")
      ) + ": " + todo.join(", "))}">${
      escHtml(o.fully_unmodeled && !edge.length ? tr("not modelled yet") : tr("partly modelled"))}</${tag}>`);
  }
  if (edge.length) {
    out.push(`<${tag} class="exchip scope" title="${escHtml(
      tr("this cannot pay out in a one-target fight — it is an edge of the model, not a gap in it")
      + ": " + edge.map((x) => tr(x)).join(" · "))}">${escHtml(tr("nothing to earn here"))}</${tag}>`);
  }
  if (dead.length) {
    out.push(`<${tag} class="exchip livebug" title="${escHtml(
      tr("the card states this and the game does not do it — measured, and the simulation matches the game rather than the card")
      + ": " + dead.map((x) => tr(x)).join(" · "))}">${escHtml(tr("does not work in game"))}</${tag}>`);
  }
  // A FIFTH CHIP, and the second that is not a shortfall of ours — the OPPOSITE
  // advice to the one above it. A live bug says the card is right and the game
  // is broken, so do not pick the perk; a MISPRINT says the effect works and
  // the card is wrong ABOUT it, so pick it for a reason the card does not
  // state. Owner, 2026-08-18: anything that differs from what the game displays
  // is to be noted, and a note nobody can see on the card is not one.
  const wrong = o.misprints || [];
  if (wrong.length) {
    out.push(`<${tag} class="exchip misprint" title="${escHtml(
      tr("this works, and its own card describes it wrongly — the simulation follows the game")
      + ": " + wrong.map((x) => tr(x)).join(" · "))}">${escHtml(tr("card is wrong"))}</${tag}>`);
  }
  return out.length ? " " + out.join(" ") : "";
};

const notModeledLines = (o) => {
  const out = [];
  if (o.not_modeled) {
    out.push(`<span class="unmodeled" title="${escHtml(
      tr("real damage the simulator does not compute yet"),
    )}">⊘ ${escHtml(tr("not modelled yet"))}</span>`);
  }
  // PARTLY modelled: the mod works, and one named effect on it does not. A
  // third line rather than a third flag, because "not modelled yet" over a card
  // that lands 1,000 damage a blast is as wrong as saying nothing.
  const partial = o.unmodeled_effects || [];
  if (partial.length && !o.not_modeled) {
    out.push(`<span class="unmodeled part" title="${escHtml(
      // Each line TRANSLATED, not the label only. An arcane's are short derived
      // tokens with no zh entry and `tr` hands those straight back, so this
      // costs them nothing — but an ABILITY writes whole sentences here, and
      // one of them went out raw the first time (the same way the disclosure
      // banner's paragraph did).
      tr("everything else on this card is modelled; this is not:") + " " +
        partial.map((x) => tr(x)).join(", "),
    )}">⊘ ${escHtml(tr("partly modelled"))}</span>`);
  }
  if (o.out_of_scope) {
    out.push(`<span class="unmodeled oos" title="${escHtml(
      tr("this acts on something a weapon-damage simulator has none of — Warframe energy, enemy behaviour, movement — so it would change no number here"),
    )}">◇ ${escHtml(tr("outside the sim"))}</span>`);
  }
  // A FOURTH ADMISSION, and the only one that is not a shortfall: this IS
  // modelled, it matches the live game, and DE did not mean it to work this way. The other three say the number is lower than the
  // card; this one says the number is right today and a hotfix takes it away,
  // which is a different thing for a player to know before building around
  // it.
  for (const why of o.live_bugs || []) {
    out.push(`<span class="livebug" title="${escHtml(
      tr("this matches the live game and is a bug — DE may patch it, and the number here changes when they do") + ": " + tr(why),
    )}">⚑ ${escHtml(tr("unintended, modelled as it plays"))}</span>`);
  }
  return out;
};

const cardLines = (o, r, fallback) =>
  (officialDesc(o, r) || (descAt(o, r) || fallback || o.effects || []).map(tf))
    .concat(notModeledLines(o));

// One slot card (regular or exilus) with its polarity / rank / menu wiring.
function buildSlot(i) {
  // AN INDEX WITH NO ENTRY IS AN EMPTY SLOT, and this has to be total over the
  // nine of them. `slots` starts empty and is filled while a weapon is applied,
  // so anything that redraws the build during that window — the riven names
  // arriving, say — asked for a slot that did not exist yet and took the whole
  // page down with it, at boot, for everyone.
  const s = slots[i] || {};
  const el = document.createElement("div");
  const m = s.mod ? modById(s.mod) : null;
  // THE SLOT'S NUMBER, because the picker already speaks it and the grid did
  // not answer. A placed mod's chip in the picker reads "slot 5", and the eight
  // slots are drawn two to a row — so nothing on screen said whether 5 was the
  // third row's left cell or the first column's fifth (player report via the
  // owner, 2026-08-10).
  //
  // It is not decoration: the same mod is worth something different in another
  // slot, because ELEMENTS COMBINE IN SLOT ORDER. A player rearranging for
  // Corrosive instead of Radiation is reading exactly this number.
  //
  // The exilus slot is left unnumbered: there is one of it, its block is
  // labelled, and the picker calls it "exilus" rather than a number.
  if (m) {
    el.className = "slot filled" + (m.rarity ? " rar-" + m.rarity : "");
    const r = s.rank == null ? m.max_rank : s.rank;
    const base = modDrain(m, r);
    const eff = slotDrain(base, m.polarity, s.pol);
    const lowered = r < m.max_rank;
    const rank = m.max_rank > 0
      ? `<span class="rank ${lowered ? "lowered" : ""}"><button class="rk" data-d="-1">−</button><b>R${r}${lowered ? "/" + m.max_rank : ""}</b><button class="rk" data-d="1">+</button></span>`
      : "";
    // The configured mod shows its CURRENT description (values at the
    // slot's rank), exactly like the in-game card.
    //
    // ASKED THE WAY THE PICKER ASKS IT. This required a `desc_ranks` or an
    // official description, which a RIVEN has neither of — its card text is its
    // rolled stats, printed by the engine and carried on `effects`. So the same
    // riven listed its lines in the picker and lost them the moment it was
    // equipped, which is the one place a player reads them while building.
    // `cardLines` already falls back to `effects`; the gate was the only thing
    // in the way, and dropping it makes the slot agree with the list it came
    // from rather than adding a second rule about what a card may say.
    const desc = cardLines(m, r);
    // WHAT THE SLOT'S COLOUR DID TO THIS MOD, said the way the arsenal says it:
    // the mod's OWN polarity next to the number, and the number coloured for
    // the three answers — matched halves it, a different colour adds 25%, an
    // unpolarized slot is neither. The card carried the SLOT's colour and not
    // the mod's, so there was nothing on screen to compare it against and a
    // +25% surcharge read as an ordinary drain.
    const matchedPol = s.pol === m.polarity || (s.pol === "Omni" && m.polarity !== "Umbra");
    const fit = !s.pol ? "" : matchedPol ? " matched" : " mismatched";
    el.innerHTML = polBtn(s.pol, i) + imgTag(IMG(m.image), "mod") +
      `<div class="info"><div class="mn">${wl(m.name, modWikiUrl(m))}${modMarketLink(m)}</div>${desc.length ? `<div class="me">${desc.map((x) => `<div>${x}</div>`).join("")}</div>` : ""}<div class="drow"><div class="dr${fit}"><span class="mpol" title="${escHtml(polCap(m.polarity))}">${polGlyph(m.polarity)}</span>${eff} drain${eff !== base ? ` (base ${base})` : ""}</div>${rank}</div></div>` +
      `<button class="dots" title="options">⋯</button>`;
    el.querySelector(".dots").addEventListener("click", (e) => { e.stopPropagation(); openModSlotMenu(i, e.currentTarget); });
    el.querySelectorAll(".rk").forEach((b) => b.addEventListener("click", (e) => {
      e.stopPropagation();
      const nr = Math.max(0, Math.min(m.max_rank, r + Number(b.dataset.d)));
      equipMod(i, slots[i].mod, nr); renderMods();
    }));
  } else {
    el.className = "slot empty";
    el.innerHTML = polBtn(s.pol, i) + `<span class="plus">${i === EXILUS ? "+ add exilus mod" : "+ add mod"}</span>`;
    // the WHOLE empty slot opens the picker (the pol-btn stops propagation)
    el.addEventListener("click", (e) => { e.stopPropagation(); openPicker(i, el); });
  }
  // …AND NEITHER IS THE STANCE SLOT, for the same reason the exilus one is
  // unnumbered: there is one of it, its block is labelled, and the picker calls
  // it "stance" rather than a number. Numbered, it read as a tenth main slot.
  if (i !== EXILUS && i !== STANCE) {
    const no = document.createElement("span");
    no.className = "slotno";
    no.textContent = String(i + 1);
    // The same words the picker's chip uses, so the two are findable as one
    // thing rather than as a number and a coincidence.
    no.title = tr("slot") + " " + (i + 1);
    el.appendChild(no);
  }
  // A FIXED STANCE HAS NO MENU AND NO POLARITY TO CHANGE: it cannot be removed
  // and its slot takes no Forma (MEASUREMENTS M94).
  if (i === STANCE && fixedStanceOf($("weapon").value)) {
    const dots = el.querySelector(".dots");
    if (dots) dots.remove();
    el.classList.add("fixed");
    el.title = tr("fixed on this weapon — it cannot be removed, and its slot takes no Forma");
    return el;
  }
  // polarity is decoupled: clickable on every slot (mod or empty, incl. innate)
  el.querySelector(".pol-btn").addEventListener("click", (e) => { e.stopPropagation(); openPolMenu(i); });
  return el;
}

// The DOM node for slot i (popover anchoring).
const slotEl = (i) =>
  i === EXILUS ? $("exilus").firstElementChild
  : i === STANCE ? $("stance").firstElementChild
  : $("mod-slots").children[i];

