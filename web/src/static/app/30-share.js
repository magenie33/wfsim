// ---- SHARING — a link that carries a build -----------------------------
//
// THE PRINCIPLE: A LINK IS A STATEMENT ABOUT A WEAPON, NEVER ABOUT A FIGHT.
// So it carries a build and the RIVENS it equips — a custom riven exists only
// on the machine that made it, which is the one thing a reader cannot rebuild
// — and it carries nothing else. Opening it creates a fresh copy of each and
// overwrites nothing, and the reader's own fight is where they left it.
//
// What that rules out is the reason for it: a fight is `SHARED_DOMAINS`, one
// list for the whole roster, so a link that planted one would follow the
// reader onto every other weapon they own.
//
// It rides the QUERY, not the fragment: a fragment never reaches a
// crawler, and the link is meant to be posted, previewed and looked at.
//
// Encoding: three forms measured against each other and the shortest sent,
// behind a one-character version so the reader knows what it is holding —
// `shareUrl` picks, and `engine::data::share_order` holds the price of v3.
const SHARE_PARAM = "b";
/// THE KILL SWITCH FOR SHARING, and it is worth keeping rather than deleting:
/// with it false an incoming `?b=` is dropped, the query is stripped, and the
/// visitor lands on the weapon's own page with a line saying why — the worst
/// case a posted link should ever reach.
///
/// It has been used once, for a shared link opening blank that could not be
/// reproduced here, and an unreproducible failure in the one feature whose job
/// is to be pasted somewhere public is not one to leave running. Both halves of
/// that are now covered: `SHARE_AXES` exists so an axis cannot go missing from
/// the tuple without a check going red, and the boot GUARD in the document head
/// plus the footer's BUILD STAMP make a repeat report answerable — without the
/// stamp, "still broken" and "still holding the old file" are the same
/// sentence.
const SHARE_ENABLED = true;
/// THE CARD'S KILL SWITCH. The card is a picture of the build, with the
/// sharer's result only when they chose to send one (`31-share-card.js`).
const SHARE_CARD_ENABLED = true;
/// The frozen order both ends index into, from `/api/meta`'s `si` per entity.
/// Built once, from what is already travelling — the manifest itself never
/// crosses the wire.
let shareIx = null;
function shareIndex() {
  if (shareIx) return shareIx;
  const to = new Map();
  const from = new Map();
  const take = (x) => {
    if (!x || x.si == null) return;
    to.set(x.id, x.si);
    from.set(x.si, x.id);
  };
  (META.weapons || []).forEach((w) => {
    take(w);
    (w.evolutions || []).forEach((t) => (t.options || []).forEach(take));
  });
  // MODS ARRIVE BY POOL, not as one list — `mod_pools` is a map of pool name to
  // its mods, and a mod in two pools is the same object twice, which `take`
  // absorbs.
  Object.values(META.mod_pools || {}).forEach((list) => (list || []).forEach(take));
  (META.arcanes || []).forEach(take);
  Object.values(META.riven_stats || {}).forEach((list) => (list || []).forEach(take));
  shareIx = { to, from };
  return shareIx;
}

/// A weapon's evolution id prefix, WITHOUT reading the page — `evoPrefix` asks
/// the DOM and the decoder runs before the weapon has been switched.
const evoPrefixFor = (weaponId) => {
  const w = (META.weapons || []).find((x) => x.id === weaponId);
  const any = ((w && w.evolutions) || [])[0];
  const first = ((any || {}).options || [])[0];
  return first && first.id.startsWith(weaponId + "_") ? weaponId + "_" : "";
};


// The weapon's evolution group, so ids can travel without their prefix.
const evoPrefix = () => {
  const tiers = weaponEvos();
  const any = (tiers[0] || { options: [] }).options[0];
  if (!any) return "";
  const w = $("weapon").value;
  // Ids are "<group>_<name>" and the group is the transform group, which is
  // the weapon id for every weapon that has one today.
  return any.id.startsWith(w + "_") ? w + "_" : "";
};

/// WHICH BUILD AXES THE SHARE TUPLE CARRIES, by their engine ids.
///
/// The tuple is POSITIONAL and omits everything a recipient can derive, so it
/// cannot iterate a served list the way the state table can — appending a field
/// is a format decision, not a loop. What it can do is DECLARE, and
/// `scripts/check_build_axes.mjs` holds this against
/// `engine::board::builds::BUILD_AXES`: a new axis fails the check until somebody
/// writes its id here, and writing it means being in this function looking at
/// the tuple. That is the whole mechanism — a forced touch point, rather than a
/// hope that the next person remembers a file they have never opened.
///
/// It has been wrong once, and expensively: `mode` and `valence` were both in
/// `snapshotState` and neither was in the tuple, so a shared Kuva Nukor reopened
/// on the default progenitor element — a link claiming a number for a build it
/// did not carry.
const SHARE_AXES = ["mods", "evolutions", "arcanes", "arcane_ranks", "mode",
                    "valence", "rivens", "assembly"];
/// …AND THE AXES A LINK DOES NOT CARRY YET, named so leaving one out is said and
/// not done in silence — `check_build_axes` reads this list. `wielder`: a link
/// would have to bring the linked Warframe build with it, and links do not yet.
const SHARE_EXCLUDED_AXES = ["wielder"];

/// `build`, when given, is `{ state }` — a build that is not the open one (a
/// search's finalist), shared without opening it: opening resets the search.
function sharePayload(build) {
  const st = build ? build.state : snapshotState();
  const p = loadPresetList(BUILDS).find((x) => presetId(x) === activePreset);
  const pre = evoPrefix();

  // The RIVENS the build equips travel whole (field 6), so a slot names one
  // by its index there — "~0" instead of "riven:some long name" repeated.
  const used = st.slots.map((s) => s.mod).filter(isRivenId);
  const rivenOrder = [...new Set(used)].map((id) => String(id).slice(RIVEN_PREFIX.length));
  const slots9 = st.slots.map((s) => {
    if (!s.mod) return 0;
    const id = isRivenId(s.mod)
      ? "~" + rivenOrder.indexOf(String(s.mod).slice(RIVEN_PREFIX.length))
      : s.mod;
    const m = modById(s.mod);
    const pol = POL_LETTER[s.pol] || "";
    const rankOff = m && s.rank != null && s.rank !== m.max_rank;
    // A plain slot is the id alone. The array form only appears when there is
    // something else to say about it.
    if (!pol && !rankOff) return id;
    return rankOff ? [id, pol, s.rank] : [id, pol];
  });

  const arcs = (st.arcane || []).filter((a) => a && a !== "none").map((a, i) => {
    const def = arcaneById(a);
    const rank = (st.arcaneRank || [])[i];
    return (def && rank != null && rank !== def.max_rank) ? [a, rank] : a;
  });

  const tiers = weaponEvos();
  const evos = tiers.map((x) => {
    const id = (build ? st.evoSel || {} : evoSel)[x.tier];
    return id ? (pre && id.startsWith(pre) ? id.slice(pre.length) : id) : "";
  });
  while (evos.length && !evos[evos.length - 1]) evos.pop();

  // The RIVENS the build actually equips, by definition — the recipient has
  // no such item and never will unless it travels. This is the custom/preset
  // line showing up in the wire format: a preset is a state both sides can
  // hold, a custom is a thing only one of them made. Only the ACTIVE shape
  // goes: the other three drafts are scratch paper.
  // BY THE CARD, because that is what a slot names. The wire keeps carrying the
  // NAME as the entry's first field: a card's identity is local — the recipient
  // is being handed a copy and gives it one of their own — so a share format
  // that carried it would be exporting a private key for no reader.
  const byId = new Map(loadPresetList(RIVENS).map((x) => [x.id, x]));
  const rivens = rivenOrder.map((n) => byId.get(n)).filter(Boolean)
    .map((x) => {
      const s = x.state || {};
      return [x.name, s.shape || "", s.rank ?? 8, POL_LETTER[cap1(s.polarity)] || "M",
        (s.bonuses || []).map((b) => [b.id, r3(b.roll)]),
        s.malus ? [s.malus.id, r3(s.malus.roll)] : 0];
    });

  // Only what DIFFERS from the defaults both sides already have.
  const d = META.defaults || {};
  const sc = {};
  const live = snapshotScenario();
  Object.keys(live).forEach((k) => {
    const val = live[k];
    if (k === "buffs") {
      // A buff left at its own default is not a setting — both sides derive
      // the same default from the same mod, so sending it is pure length.
      const def = Object.fromEntries((buffList || []).map((b) => [b.id, b]));
      const out = {};
      Object.entries(val || {}).forEach(([id, c]) => {
        const b = def[id];
        if (b && c.stacks === b.default_stacks && !!c.locked === !!b.default_locked) return;
        out[id] = c.locked ? [c.stacks, 1] : [c.stacks];
      });
      if (Object.keys(out).length) sc.buffs = out;
      return;
    }
    // AN OBJECT NEVER COMPARES EQUAL to another object, so a plain `!==` would
    // send `extra_stats: {}` on every link. Compared by VALUE, which is what
    // "only what differs from the defaults both sides already have" meant all
    // along — it just had no object field to be wrong about until now.
    const same = (a, b) =>
      a && typeof a === "object" ? JSON.stringify(a) === JSON.stringify(b || {}) : a === b;
    if (!same(val, d[k])) sc[k] = val;
  });

  // The sharer's MEASUREMENT, kept as a claim rather than as a fact: the
  // recipient's own run is what decides. Three numbers, not the whole result
  // object — the card needs the headline and the fight, and the fight is
  // field 7.
  const r = p && (resultLatest(presetWeapon(), presetId(p)) || {}).r;
  const m = r ? [r3(r.score), r.duration, Math.round(r.dps || 0)] : 0;

  // THE TWO AXES THAT ARE THE BUILD AND ARE NOT A SLOT: how the weapon is
  // PLAYED, and what a Lich handed it. `snapshotState` has carried both for
  // some time and this tuple read neither, so a shared Kuva Nukor reopened on
  // the default progenitor element and a shared charged Phantasma reopened in
  // base form — a link that changes the build it is claiming a number for.
  //
  // Omitted when the recipient would derive the same value anyway, like every
  // other field here: `defaultMode`/`defaultValence` are what both ends ask,
  // so sending the answer they already have is pure length.
  const dv = defaultValence(st.weapon, null);
  const md = st.mode === defaultMode(st.weapon, null) ? 0 : st.mode;
  const val = (st.valence && st.valence.element !== dv.element) || (st.valence && st.valence.bonus !== dv.bonus)
    ? [st.valence.element, r3(st.valence.bonus)] : 0;

  // **A SHARE LINK IS A BUILD AND NOTHING ELSE**. Fields 7
  // and 8 — the fight and the measurement — are always `0` now, and `0` means
  // NO FIGHT TRAVELLED, which is a different statement from `{}`: an empty
  // object means "a fight travelled and it happens to equal the defaults". The
  // importer has always told the two apart, so every link posted before today
  // still lands exactly as it did; only new ones stop carrying a scenario.
  //
  // WHY IT NARROWED. A link that plants a scenario preset changes the fight the
  // reader is working in, which is the one thing a build link has no business
  // doing — and the measurement beside it is a claim ABOUT that scenario, so
  // without the fight it means nothing anyway. The measured result still
  // travels as the CARD, which is a picture of a run rather than something that
  // lands in the reader's app.
  //
  // It also ends a size argument that should never have existed: the rulers pin
  // every wielder field on purpose, so a ruler-derived link was carrying four
  // Warframe numbers the recipient computes anyway.
  // THE PARTS, on the same terms: omitted when the recipient derives the same
  // answer, since `defaultAssembly` is what both ends ask. Two ids, in the
  // order the control draws them.
  const da = defaultAssembly(st.weapon, null);
  const asm = st.assembly && da
      && (st.assembly.grip !== da.grip || st.assembly.loader !== da.loader)
    ? [st.assembly.grip, st.assembly.loader] : 0;

  // A NAME THE SHARER CHOSE TRAVELS; A MACHINE-GENERATED ONE DOES NOT — and
  // "+ new"'s `build 1` is as machine-generated as a board build's
  // `standard_single_target#cycle#p#1`. Neither means anything to the person opening the
  // link, `importShare` names an unnamed build anyway, and `build 1` was the
  // more expensive of the two: its SPACE is outside the compact form's
  // alphabet, so every ordinary link paid 2.5x for a name nobody chose.
  const nm = build || officialBuildActive() || isGeneratedName(activePreset)
    ? 0 : activePreset;

  const out = [2, st.weapon, nm, slots9, arcs, evos, rivens, 0, 0, md, val, asm];
  while (out.length > 9 && !out[out.length - 1]) out.pop();
  return out;
}

const cap1 = (s) => String(s || "").replace(/^./, (c) => c.toUpperCase());

/// WHERE SHORT LINKS LIVE, whatever origin made one: the desktop client and the
/// dev server have no store of their own, and a link must open for anybody.
const SHARE_ORIGIN = LIVE_ORIGIN;
const shareApi = () => (location.origin === SHARE_ORIGIN ? "" : SHARE_ORIGIN);
/// THE SITE AND THE DESKTOP SHELL make short links; a dev server or a check
/// (127.0.0.1) makes the long form, so no test ever writes the live store.
const SHARE_SHORT_HOSTS = LIVE_HOSTS;

/// THE LINK: short when the store answers, the full code when it does not —
/// offline, on a shell with no network, or a store that is down. The long form
/// opens exactly as it always has, so a failure here costs length and nothing
/// else.
async function shareUrl(claim, signer = null, build = null) {
  const w = weaponInfo($("weapon").value);
  const code = await shareCode(build);
  if (SHARE_SHORT_HOSTS.includes(location.hostname)) try {
    const ask = new AbortController();
    const timer = setTimeout(() => ask.abort(), 4000);
    const r = await fetch(`${shareApi()}/api/s`, {
      method: "POST", headers: { "content-type": "application/json" }, signal: ask.signal,
      body: JSON.stringify({ w: weaponPath(w.id).slice("/weapons/".length), c: code, ...(claim ? { m: claim } : {}) }),
    });
    clearTimeout(timer);
    const j = r.ok ? await r.json() : null;
    if (j && j.ok && /^[0-9A-Za-z]{10}$/.test(j.id)) {
      const signed = signer && await signer.sign(j.id, weaponPath(w.id).slice("/weapons/".length));
      return signed || `${SHARE_ORIGIN}${weaponPath(w.id)}/s/${j.id}`;
    }
  } catch (_) { /* the long form below */ }
  return `${location.origin}${weaponPath(w.id)}?${SHARE_PARAM}=${code}`;
}

/// WHO SHARED THE BUILD ON SCREEN, when an extension can say (`shareWho`):
/// drawn over the build bar while that build is open, and only for the link
/// that names it.
let shareBy = null;
async function showShareBy(id, sig) {
  const by = await extHook("shareWho", id, sig);
  if (!by) return;
  shareBy = { preset: activePreset, ...by };
  renderShareBy();
}
function renderShareBy() {
  const el = $("share-by");
  if (!el) return;
  const on = !!shareBy && activePreset === shareBy.preset;
  el.hidden = !on;
  if (!on) return;
  el.className = "share-by" + (shareBy.className ? ` ${shareBy.className}` : "");
  el.innerHTML = shareBy.html;
}

/// THE BUILD A SHORT LINK NAMES, or null. Its id is a hash of what it stores,
/// so the answer never changes and the browser may keep it.
async function shortShareCode(id) {
  try {
    const r = await fetch(`${shareApi()}/api/s/${id}`);
    const j = r.ok ? await r.json() : null;
    return j && j.ok && typeof j.c === "string" ? j.c : null;
  } catch (_) { return null; }
}

/// THE CODE, WITH NO NAMES IN IT. A name is the one field a person types, and
/// it was most of a link's length — a riven named in Chinese, a build called
/// "… copy copy". The reader gets the riven's generated name and a build named
/// for where it came from (`importShare`), which is all a name told them.
async function shareCode(build) {
  const payload = sharePayload(build);
  payload[2] = 0;
  payload[6] = (payload[6] || []).map(([, ...rest]) => [boardRivenName({
    bonuses: (rest[3] || []).map(([x]) => x),
    malus: rest[4] ? rest[4][0] : null,
  }), ...rest]);
  // THE SHORTEST OF FOUR, chosen by MEASURING rather than by rule. v4 wins on
  // every ordinary build; v3 catches what v4 declines to spell (an id the
  // manifest has not been told about); the two base64 forms catch what neither
  // can express, and still read every link ever posted. A form that loses here
  // costs nothing — it is not reached.
  const forms = [[SHARE_V_B62, packV4(payload)], [SHARE_V_TEXT, packV3(payload)]]
    .filter(([, t]) => t != null)
    .map(([v, t]) => v + t);
  const json = new TextEncoder().encode(JSON.stringify(payload));
  const z = await deflate(json);
  const zipped = z && z.length < json.length
    ? SHARE_V_DEFLATE + b64urlEnc(z)
    : SHARE_V_PLAIN + b64urlEnc(json);
  return forms.concat(zipped).reduce((a, b) => (b.length < a.length ? b : a), zipped);
}


// Land a shared link: a NEW copy of every part, never a merge into what is
// already there. A link is someone else's work — it may not overwrite yours,
// and it may not need you to rebuild half of it by hand.
async function importShare(code) {
  let data;
  try { data = await decodeShare(code); } catch (_) { data = null; }
  if (!data) { presetToast(tr("that share link could not be read")); return false; }
  const w = (META.weapons || []).find((x) => x.id === data.w);
  if (!w) { presetToast(tr("that share link is for a weapon this build of the site does not have")); return false; }

  switchWeapon(w.id);

  // 1. The RIVENS first: the slots point at them by the wire's own key, and the
  //    copy made here is a card of the recipient's with an identity of its
  //    own — so what the slots are rewritten with is the map from that key to
  //    the new id.
  const imported = {};
  if ((data.rivens || []).length) {
    const ps = loadPresetList(RIVENS);
    data.rivens.forEach((x) => {
      const name = freeName(ps, (n) => x.n + (n > 1 ? " " + n : ""));
      const id = newRivenId(loadPresetWhole(RIVENS).concat(ps));
      imported[x.n] = id;
      ps.push({ id, name, savedAt: Date.now(), state: withDrafts(x.s) });
    });
    storePresetList(RIVENS, ps);
    refreshRivenNames();
  }

  // 2. THERE IS NO SCENARIO STEP. A fight is not a build's to move: posting
  //    "here is my Torid" into a chat must not change the fight of everyone
  //    who clicks it, and a scenario is shared across the whole roster
  //    (`SHARED_DOMAINS`), so one planted here would follow the reader onto
  //    every other weapon. `decodeShare` returns no fight for any link, so
  //    this is a step that cannot be taken rather than one guarded by an `if`.

  // 3. The BUILD, with riven ids repointed at the copies just made and any id
  //    this build of the site does not know dropped — said out loud rather
  //    than silently, the same rule the cross-weapon import follows.
  const dropped = [];
  const slots2 = (data.slots || []).map((s) => {
    if (!s || !s.mod) return { mod: null, pol: s ? s.pol : null, rank: null };
    // "~<n>" names the nth riven in field 6 — resolve it to the copy just
    // imported, whatever that copy had to be renamed to.
    if (/^~\d+$/.test(s.mod)) {
      const src = (data.rivens || [])[Number(s.mod.slice(1))];
      const nm = src && (imported[src.n] || src.n);
      return nm ? { ...s, mod: RIVEN_PREFIX + nm } : { mod: null, pol: s.pol, rank: null };
    }
    if (isRivenId(s.mod)) {                       // v1 links
      const was = String(s.mod).slice(RIVEN_PREFIX.length);
      return { ...s, mod: RIVEN_PREFIX + (imported[was] || was) };
    }
    if (!modById(s.mod)) { dropped.push(s.mod); return { mod: null, pol: s.pol, rank: null }; }
    return s;
  });
  const pre = evoPrefix();
  const evoSel2 = {};
  (data.evos || []).forEach((id, i) => { if (id) evoSel2[i + 1] = pre && !id.startsWith(pre) ? pre + id : id; });

  const state = buildState(w.id, {
    slots: slots2,
    arcane: data.arcane,
    arcaneRank: data.arcaneRank,
    evoSel: evoSel2,
    // Handed to `restoreState` raw, because it already cleans both against the
    // weapon being opened: an element this spec does not offer is dropped, a
    // mode it cannot be played in falls back to the arsenal's.
    mode: data.mode,
    valence: data.valence,
    // Same terms: `restoreState` repairs a part this weapon cannot take, and
    // `undefined` on a link posted before parts travelled means its default.
    assembly: data.assembly,
    // A LINK DOES NOT CARRY THE WIELDER yet (`SHARE_EXCLUDED_AXES`), so a
    // shared build lands in the hands this weapon has by default.
    wielder: null,
  });
  const builds = loadPresetList(BUILDS);
  // Named for where it came from. Without it a link lands as "build 1 2",
  // which says nothing about being someone else's work.
  const base = `${data.n || "build"} (shared)`;
  const entry = presetEntry(freeName(builds, (n) => base + (n > 1 ? " " + n : "")), null);
  activePreset = entry.id;
  localStorage.setItem(presetActiveKey(BUILDS), entry.id);
  // THE BUILD, AND NOTHING ELSE TOUCHES THE FIGHT. `applyScenario` is the only
  // door a scenario is set through, so not opening it IS the guarantee — the
  // same rule `check_preset_independence.mjs` asserts for a build being LOADED.
  whileApplying(() => restoreState(state, w.id));
  track("share.open", w.id);
  // NO RESULT: a measurement is a build plus the fight it was made in
  // plus the number, and the fight did not travel. A number without one is not
  // a claim a reader could check, so the build lands unmeasured and the first
  // run here is the first number it has ever had.
  entry.state = snapshotState();
  builds.push(entry);
  storePresetList(BUILDS, builds);

  renderPresetBar(); renderScenarioBar(); renderMods(); renderSim(); refreshPanel();
  renderStoredSimResult();
  // WHAT LANDED, said out loud. A build and its rivens is the whole list there
  // can be, so nothing here is conditional on what the link happened to carry.
  const bits = [tr("build")];
  if ((data.rivens || []).length) bits.push(`${data.rivens.length} ${tr("riven")}`);
  presetToast(`${tr("imported")}: ${name} · ${bits.join(" + ")}` +
    (dropped.length ? ` · ${tr("dropped")} ${dropped.length}` : ""));
  return true;
}

/// THE BUILD AS LINES OF TEXT, for a chat that shows a pasted link as a bare
/// string: a heading, then mods, riven, arcanes, evolutions. Read back out of
/// the link's own code, so the text states what the link carries and nothing
/// the link does not — named in the sharer's language, for the sharer's chat.
async function shareText(build) {
  const d = await decodeShare(await shareCode(build));
  const w = weaponInfo(d.w);
  const name = (x) => (x && x.name) || "";
  const mods = d.slots.map((s) => s.mod).filter((m) => m && !String(m).startsWith("~"))
    .map((m) => name(modById(m)) || m);
  const rivens = d.rivens.map((r) => [
    ...(r.s.bonuses || []).map((b) => "+" + (rivenStat(b.id) ? rivenStatName(rivenStat(b.id)) : b.id)),
    ...(r.s.malus ? ["−" + (rivenStat(r.s.malus.id) ? rivenStatName(rivenStat(r.s.malus.id)) : r.s.malus.id)] : []),
  ].join(" "));
  const arcanes = d.arcane.filter((a) => a && a !== "none").map((a) => name(arcaneById(a)) || a);
  const pre = evoPrefix();
  const evos = d.evos.filter(Boolean).map((e) => {
    const o = weaponEvos().flatMap((t) => t.options || []).find((x) => x.id === pre + e || x.id === e);
    return name(o) || e;
  });
  return [
    `${w.name} ${tr("build")} — WFSim`,
    mods.length ? `${tr("Mods")}: ${mods.join(" · ")}` : "",
    rivens.length ? `${tr("Riven")}: ${rivens.join("; ")}` : "",
    arcanes.length ? `${tr("Arcane")}: ${arcanes.join(" · ")}` : "",
    evos.length ? `${tr("Evolutions")}: ${evos.join(" · ")}` : "",
  ].filter(Boolean);
}

/// THE SHARER'S RESULT in the scenario on screen, when they choose to send one:
/// the claim the worker stores beside the link (`shareClaim` in worker/index.js
/// names every field) and the line the copied text shows. The LINK still lands
/// the build and nothing else — the number travels beside it, as theirs.
/// Measured first when the stored result does not describe this build in this
/// fight (`resultForShare`); null when no run answers.
async function shareMeasurement() {
  const got = await resultForShare();
  if (!got || !got.r) return null;
  const met = metricOf(sim.metric);
  const v = fmtScore(metricValue(met, got.r));
  const sc = scenarioNamed(activeScenario) || {};
  // AN OFFICIAL SCENARIO IS NAMED, and the name is the whole fight; anything
  // else is stated by its terms, with a target the sharer built left unnamed.
  const claim = officialScenarioActive() ? { s: sc.builtin, k: met.id, v } : {
    k: met.id, v,
    ...(customEnemiesFor(sim.enemy).length ? {} : { e: sim.enemy }),
    l: Math.round(Number(sim.level) || 1),
    ...(sim.steel_path ? { sp: 1 } : {}),
    d: Math.round(Number(sim.duration) || 1),
  };
  return { claim, line: `${v} ${metricLabel(met)} — ${sc.name || tr("Scenarios")}`,
    card: { value: v, unit: metricLabel(met), scene: sc.name || tr("Scenarios") } };
}

/// Whether this browser last chose to send its result. A convenience, so it is
/// browser storage and may be lost.
const SHARE_RESULT = "wfsim-share-result";

/// WHERE ON THE PAGE THE PANEL WAS OPENED, as `share.entry`'s subject: the bar's
/// own button, or a share beside a build somewhere else (`openBuildShare`).
let shareFrom = "bar";

/// A SHARE BESIDE A BUILD — a finder row, a search's finalist, a result — is the
/// one panel, opened for that build. The caller makes it the open build first,
/// so the bar names what is shared, and the panel opens under the bar — or,
/// for a `build` that is not opened (a finalist), under `host`.
function openBuildShare(from, host, build) {
  const bar = $("preset-bar-builder-builds");
  const at = host || bar;
  if (!at) return;
  if (host && !host.querySelector(":scope > .pshare")) host.insertAdjacentHTML("beforeend", `<div class="pshare" hidden></div>`);
  const panel = at.querySelector(".pshare");
  if (!panel) return;
  panel.hidden = true;
  openSharePanel(at, from, build);
  at.scrollIntoView({ block: "nearest", behavior: "smooth" });
}

// THE RESULT'S SHARE: the simulated build is the open one already.
document.addEventListener("click", (e) => { if (e.target.closest("#sim-share")) openBuildShare("simulator"); });

// The share panel: the link, as a link, as text, or through the system's share
// sheet — with the sharer's result beside it when they ask for that.
/// A `build` that is not the open one leaves without the result and the card:
/// both are drawn from the open build, so they would describe another.
async function openSharePanel(bar, from = "bar", build = null) {
  const panel = bar.querySelector(".pshare");
  if (!panel) return;
  if (!panel.hidden) { panel.hidden = true; return; }
  panel.hidden = false;
  shareFrom = from;
  let withResult = false;
  try { withResult = !build && localStorage.getItem(SHARE_RESULT) === "1"; } catch (_) { /* off */ }
  // A LINK AN EXTENSION CAN SIGN offers it here (`shareSigner`); one it signs
  // only for some readers is shown `locked`, greyed, with the extension's words
  // on whose it is — never a prompt. Otherwise the link is plain.
  const signer = await extHook("shareSigner");
  // A GREYED OPTION IS A WAY IN, seen: counted against the ones taken (`door.open`).
  if (signer && signer.locked) track("door.seen", "share");
  const draw = async () => {
    if (withResult) panel.innerHTML = `<div class="sh-note">${escHtml(tr("simulating this build in the current scenario…"))}</div>`;
    const measured = withResult ? await shareMeasurement() : null;
    const signing = !!signer && !signer.locked && signer.on();
    const bUrl = await shareUrl(measured && measured.claim, signing ? signer : null, build);
    const lines = await shareText(build);
    const text = measured ? [lines[0], measured.line, ...lines.slice(1)] : lines;
    const native = typeof navigator.share === "function";
    panel.innerHTML =
      `<div class="sh-row"><input class="sh-url" type="text" readonly value="${escHtml(bUrl)}">` +
      `<button class="cu-btn sh-copy">${escHtml(tr("copy link"))}</button>` +
      `<button class="cu-btn sh-text">${escHtml(tr("copy as text"))}</button>` +
      (native ? `<button class="cu-btn sh-native">${escHtml(tr("share…"))}</button>` : "") +
      `</div>` +
      (build ? "" : `<label class="sh-opt"><input type="checkbox" class="sh-result"${withResult ? " checked" : ""}> ` +
      `${escHtml(tr("include my result in this scenario"))}` +
      (measured ? ` <b>${escHtml(measured.line)}</b>` : "") + `</label>`) +
      (signer ? (signer.locked
        ? `<label class="sh-opt sh-locked"><input type="checkbox" class="sh-sign" disabled> ${signer.option}</label>`
        : `<label class="sh-opt"><input type="checkbox" class="sh-sign"${signing ? " checked" : ""}> ${signer.option}</label>`) : "") +
      `<div class="sh-note">${escHtml(tr(measured
        ? "the link still opens the build alone; your result travels beside it, shown as yours"
        : "the build and its rivens, and nothing else: no fight, no measurement, so opening it leaves the reader's own scenario untouched"))}`
        + (signing ? ` ${signer.note}` : "") + `</div>` +
      (SHARE_CARD_ENABLED && !build
        ? `<div class="sh-more"><button class="cu-btn sh-full">${escHtml(tr("…as a card →"))}</button></div>`
        : "");
    const bBox = panel.querySelector(".sh-url");
    bBox.onclick = () => bBox.select();
    // `n` says HOW it left: 1 the link, 2 as text, 3 through the system's share sheet.
    panel.querySelector(".sh-copy").onclick = async () => {
      track("share.create", $("weapon").value, 1);
      track("share.entry", shareFrom);
      try { await navigator.clipboard.writeText(bUrl); presetToast(tr("link copied")); }
      catch (_) { bBox.select(); presetToast(tr("press Ctrl+C to copy the selected link")); }
    };
    panel.querySelector(".sh-text").onclick = async () => {
      track("share.create", $("weapon").value, 2);
      track("share.entry", shareFrom);
      try { await navigator.clipboard.writeText(`${text.join("\n")}\n${bUrl}`); presetToast(tr("text copied")); }
      catch (_) { bBox.select(); presetToast(tr("press Ctrl+C to copy the selected link")); }
    };
    const nat = panel.querySelector(".sh-native");
    if (nat) nat.onclick = async () => {
      track("share.create", $("weapon").value, 3);
      track("share.entry", shareFrom);
      // A CANCELLED SHEET REJECTS, and a reader closing it is not an error.
      try { await navigator.share({ title: text[0], text: text.slice(1).join("\n"), url: bUrl }); } catch (_) { /* closed */ }
    };
    const signBox = panel.querySelector(".sh-sign");
    if (signBox) signBox.onchange = (e) => {
      signer.set(e.target.checked);
      draw();
    };
    const resultBox = panel.querySelector(".sh-result");
    if (resultBox) resultBox.onchange = (e) => {
      withResult = e.target.checked;
      try { localStorage.setItem(SHARE_RESULT, withResult ? "1" : "0"); } catch (_) { /* this page only */ }
      draw();
    };
    const full = panel.querySelector(".sh-full");
    if (full) full.onclick = () => openShareClaim(panel, bUrl, measured);
  };
  await draw();
}

/// WHAT A SHARER MAY SHOW OF THEMSELVES on the card (`cardShowcase`): name,
/// honour and the figures they pick, read from the server for the account
/// signed in, and offered only to one named on the ranking with points — the
/// same consent the ranking asks. Which figures is remembered in this browser.
const SHOWCASE_PICK = "wfsim-card-showcase";
const SHOWCASE_ITEMS = [["rank", "Contributor rank"], ["points", "Points"], ["week", "Weekly ranking"], ["recent", "Monthly ranking"], ["all", "All-time ranking"]];
async function showcaseStanding() {
  if (!accountState.account) return null;
  await loadDevices();
  const d = devicesState;
  return d && d.named && d.points > 0
    ? { name: accountName(accountState.account), volunteer: !!d.volunteer, points: d.points, ranks: d.ranks || {},
      contributor_rank: d.contributor_rank || null } : null;
}
/// THE SHARER'S PICK, or the default: their rank, their points and their best
/// place. A pick kept from before the rank was offered gains it once (`ranked`).
function showcasePick(st) {
  try {
    const saved = JSON.parse(localStorage.getItem(SHOWCASE_PICK) || "null");
    if (saved && Array.isArray(saved.items)) {
      return { on: saved.on !== false, ranked: true, items: saved.ranked ? saved.items : ["rank", ...saved.items.filter((k) => k !== "rank")] };
    }
  } catch (_) { /* no pick kept */ }
  // A TIE GOES TO THE LONGER RANKING: all time outweighs a month, a month a week.
  const best = ["all", "recent", "week"].filter((k) => st.ranks[k]).sort((a, b) => st.ranks[a] - st.ranks[b])[0];
  return { on: true, ranked: true, items: ["rank", "points", ...(best ? [best] : [])] };
}
function showcaseOf(st, pick, mark) {
  if (!pick.on) return null;
  const stats = SHOWCASE_ITEMS.filter(([k]) => k !== "rank" && pick.items.includes(k))
    .map(([k, label]) => (k === "points" ? [st.points.toLocaleString(accountLocale()), tr(label)]
      : st.ranks[k] ? [`#${st.ranks[k]}`, tr(label)] : null))
    .filter(Boolean);
  const rank = pick.items.includes("rank") && st.contributor_rank ? st.contributor_rank.rank : null;
  return { name: st.name, volunteer: st.volunteer, mark: mark || "", rank, stats };
}

/// THE CARD: a picture of this build, to paste into a chat window — with the
/// sharer's result when the panel is sending one, their name when the link is
/// signed, drawn from the paid half's answer for that link and nothing else,
/// and the showcase the sharer chose.
async function openShareClaim(panel, url, measured) {
  const more = panel.querySelector(".sh-more");
  if (!more) return;
  more.innerHTML = `<div class="sh-note">${escHtml(tr("drawing the card…"))}</div>`;
  const signed = url.match(/\/s\/([0-9A-Za-z]{10})\/([^/?#]+)$/);
  const by = signed ? await extHook("shareWho", signed[1], signed[2]) : null;
  more.innerHTML =
    `<div class="sh-row"><input class="sh-url" type="text" readonly value="${escHtml(url)}">` +
    `<button class="cu-btn sh-copy">${escHtml(tr("copy link"))}</button>` +
    `<button class="cu-btn sh-img">${escHtml(tr("copy image"))}</button>` +
    `<button class="cu-btn sh-dl">${escHtml(tr("download image"))}</button></div>` +
    `<div class="sh-note">${escHtml(tr("the card shows the build; the link beside it opens the same build, and never touches the reader's own scenario"))}</div>` +
    `<canvas class="sh-canvas"></canvas>`;
  const urlBox = more.querySelector(".sh-url");
  urlBox.onclick = () => urlBox.select();
  const canvas = more.querySelector(".sh-canvas");
  const standing = await showcaseStanding();
  let pick = standing && showcasePick(standing);
  const draw = () => drawShareCard(canvas, url, {
    measured: measured && measured.card,
    by: by && { name: by.name, mark: by.mark || "" },
    theme: (by && by.theme) || "",
    showcase: standing && showcaseOf(standing, pick, by && by.mark),
  });
  if (standing) {
    const box = (id, label, on, off) => `<label class="sh-opt"><input type="checkbox" data-showcase="${id}"${
      on ? " checked" : ""}${off ? " disabled" : ""}> ${escHtml(tr(label))}</label>`;
    const row = document.createElement("div");
    row.className = "sh-row sh-showcase";
    const paint = () => {
      row.innerHTML = box("on", "Show me on the card", pick.on, false) + SHOWCASE_ITEMS
        .filter(([k]) => k === "points" || (k === "rank" ? standing.contributor_rank : standing.ranks[k]))
        .map(([k, label]) => box(k, label, pick.items.includes(k), !pick.on)).join("");
    };
    paint();
    row.onchange = async (e) => {
      const k = e.target.dataset.showcase;
      if (k === "on") pick = { ...pick, on: e.target.checked };
      else pick = { ...pick, items: e.target.checked ? [...pick.items, k] : pick.items.filter((x) => x !== k) };
      try { localStorage.setItem(SHOWCASE_PICK, JSON.stringify(pick)); } catch (_) { /* this page only */ }
      paint();
      await draw();
    };
    canvas.before(row);
  }
  await draw();

  // SCOPED TO `more`, NOT TO THE PANEL: the build link above has a `.sh-copy`
  // of its own, and a panel-wide lookup finds THAT one — which would leave the
  // claim's button dead and silently repoint the build's at the claim's url.
  const say = (msg) => presetToast(tr(msg));
  more.querySelector(".sh-copy").onclick = async () => {
    try { await navigator.clipboard.writeText(url); say("link copied"); }
    // Clipboard permission can be refused; selecting the text is the fallback
    // that always works, and no dialog is involved either way.
    catch (_) { urlBox.select(); say("press Ctrl+C to copy the selected link"); }
  };
  more.querySelector(".sh-img").onclick = async () => {
    try {
      const blob = await new Promise((res) => canvas.toBlob(res, "image/png"));
      await navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
      say("image copied");
    } catch (_) { say("this browser will not copy images — use download"); }
  };
  more.querySelector(".sh-dl").onclick = () => {
    const a = document.createElement("a");
    a.href = canvas.toDataURL("image/png");
    a.download = `wfsim-${$("weapon").value}-${presetLabel(buildNamed(activePreset))}.png`.replace(/[\s#]+/g, "-");
    a.click();
  };
}

