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
/// THE CARD IS OFF. A card states a MEASUREMENT, and what may be shared today
/// is a build and nothing else — so the entry to it is not drawn. The code
/// below it stands unreached on purpose: the question it answers (how a number
/// travels without landing in the reader's app) is still open.
const SHARE_CARD_ENABLED = false;
const SHARE_V_DEFLATE = "1";
const SHARE_V_PLAIN = "0";
/// **v3: THE IDS TRAVEL AS INDICES, AND THE PAYLOAD IS PLAIN TEXT**.
///
/// Spelling the ids out and then deflating them costs ~280 characters for a
/// build: the payload is mostly identifiers, and deflate cannot know the ones
/// the payload does NOT contain. The same Laetum is 76 characters
/// as indices — and at that length deflate makes it BIGGER (its own header
/// outweighs what it finds), so v3 goes into the URL as it is.
///
/// EVERY SEPARATOR IS URL-SAFE, which is what lets the text travel raw: `~`,
/// `-`, `.` and `_` are unreserved in RFC 3986 and `:`, `;`, `,` and `!` are
/// sub-delims a query accepts without escaping. Anything that is not — a riven
/// somebody named in Chinese — falls back to the deflate+base64 path, which is
/// still there and still reads every link ever posted.
const SHARE_V_TEXT = "3";
/// **v4: EVERY ID IS TWO CHARACTERS, SO NOTHING SEPARATES THEM.**
///
/// v3 spells an index in decimal and pays a `.` to say where it ends — about
/// 4.4 characters per id. The manifest holds 1749 entries and 62² is 3844, so
/// the same index is TWO characters of base62 and a run of them needs no
/// separator at all. That is the whole of v4: the same indices, spelled denser.
///
/// IT IS A RESPELLING, NEVER A RENUMBERING. `data/share_order.yaml` stays
/// append-only and every v3 link still reads — the ratchet in
/// `engine::data::share_order` guards the same thing it always did.
///
/// Headroom is 3844, about double what is used. Crossing it is a THIRD width,
/// which is another version character; that is what the version character is
/// for.
const SHARE_V_B62 = "4";
/// The alphabet, every character of it unreserved in RFC 3986. `-` and `.` are
/// unreserved too and deliberately NOT in it, which is what lets them mark a
/// slot that no id can be confused with.
const B62 = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const B62_MAX = 62 * 62;
const b62 = (n) => B62[Math.floor(n / 62)] + B62[n % 62];
const b62Back = (t) => {
  const a = B62.indexOf(String(t)[0]), b = B62.indexOf(String(t)[1]);
  return a < 0 || b < 0 ? -1 : a * 62 + b;
};
/// An id as its two characters, or `null` for one v4 cannot spell — a mod added
/// since the last `gen_share_order.py`, or a manifest past its headroom. The
/// encoder measures every form and takes the shortest, so `null` here is a link
/// that goes out as v3: longer, and correct.
const si62 = (id) => {
  const n = shareIndex().to.get(id);
  return n == null || n >= B62_MAX ? null : b62(n);
};
const si62Back = (t) => {
  const n = b62Back(t);
  return n < 0 ? "" : (shareIndex().from.get(n) || "");
};
/// A RIVEN ROLL IS 200 STEPS, NOT FIVE CHARACTERS. Every stat rolls in
/// 0.9–1.1 (`engine::build::rivens::{ROLL_MIN, ROLL_MAX}`) and the payload
/// already rounds to three decimals, so a roll is one of 201 values — two
/// characters, losslessly, where `1.052` cost five.
///
/// THE BAND IS FROZEN HERE, not read from the engine: a wire format cannot
/// follow a constant that moves, or every link already posted decodes to a
/// different riven. A roll outside it is a payload v4 declines to write.
/// A RIVEN'S SHAPE IS ONE CHARACTER. There are four of them (`RIVEN_SHAPES`)
/// plus the absent one a board riven carries, and "" is a different value from
/// "3+1" to everything downstream — so it is CODED here rather than derived
/// from the bonus count, and coded rather than carried raw, because the `+` in
/// it is the one character a query turns into a space.
///
/// A shape not in this table is one v4 declines to write, and the encoder falls
/// back to a form that spells it out.
const SHAPE_CODE = { "": "-", "2": "2", "3": "3", "2+1": "a", "3+1": "b" };
const SHAPE_BACK = Object.fromEntries(
  Object.entries(SHAPE_CODE).map(([k, v]) => [v, k]));
const ROLL_FLOOR = 0.9, ROLL_CEIL = 1.1, ROLL_STEP = 0.001;
const roll62 = (x) => {
  const n = Math.round((Number(x) - ROLL_FLOOR) / ROLL_STEP);
  return Number(x) < ROLL_FLOOR - 1e-9 || Number(x) > ROLL_CEIL + 1e-9 ? null : b62(n);
};
const roll62Back = (t) => {
  const n = b62Back(t);
  return n < 0 ? 1 : r3(ROLL_FLOOR + n * ROLL_STEP);
};
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
/// An id as a number, or `!<id>` for one the manifest has never been told
/// about — a mod added since the last `gen_share_order.py`. A link that carries
/// one is longer and still correct, which is the only direction this may fail.
const siOf = (id) => {
  const n = shareIndex().to.get(id);
  return n == null ? "!" + id : String(n);
};
const siBack = (tok) => {
  if (tok === "" || tok == null) return "";
  if (tok[0] === "!") return tok.slice(1);
  const id = shareIndex().from.get(Number(tok));
  return id == null ? "" : id;
};
/// The characters the compact forms may travel raw in. Everything a format
/// itself uses, plus what an id or a rank can contain — and `%`, which is how
/// the one field a PERSON types gets into that alphabet.
const SHARE_TEXT_OK = /^[A-Za-z0-9~.:;,!_%-]*$/;

/// A NAME IS THE ONE FIELD SOMEBODY TYPES, so it is the one that can hold
/// anything: a space, a quote, Chinese. It is ESCAPED INTO the alphabet rather
/// than allowed to veto it: a space in it would otherwise send the WHOLE
/// payload back to base64, 2.5x for one field the reader can do without.
///
/// Everything outside `[A-Za-z0-9_-]` is escaped, not just what a URL would
/// demand: the format's own separators (`~ . : ; , !`) are legal in a query and
/// would pass `encodeURIComponent` untouched, straight into the grammar.
const encName = (s) => String(s || "").replace(/[^A-Za-z0-9_-]/g, (c) =>
  Array.from(new TextEncoder().encode(c))
    .map((b) => "%" + b.toString(16).toUpperCase().padStart(2, "0")).join(""));
/// Its inverse, and identity on every name posted before it existed — those
/// could not contain a `%`, because `%` was not in the alphabet above.
const decName = (s) => {
  try { return decodeURIComponent(String(s || "")); } catch (_) { return String(s || ""); }
};

/// A weapon's evolution id prefix, WITHOUT reading the page — `evoPrefix` asks
/// the DOM and the decoder runs before the weapon has been switched.
const evoPrefixFor = (weaponId) => {
  const w = (META.weapons || []).find((x) => x.id === weaponId);
  const any = ((w && w.evolutions) || [])[0];
  const first = ((any || {}).options || [])[0];
  return first && first.id.startsWith(weaponId + "_") ? weaponId + "_" : "";
};

/// The v2 array as v3 text. One place, and its inverse is directly below it —
/// the pair round-trips, which is what `check_share` asserts over every axis.
// v4's layout, read it beside `unpackV4`. The RARE fields are last, so the
// list's trailing trim removes them from an ordinary link:
//   0  weapon                2 chars
//   1  slots                 2 chars each: an id, "--" empty, "-<n>" the nth riven
//   2  arcanes               2 chars each
//   3  evolutions by tier    2 chars each, "--" for a tier left unset
//   4  rivens                "!" apart, each ";" apart: name;shape;rank;pol;
//                            stats;malus — shape is ONE character, see SHAPE_CODE
//   5  build name            escaped into the alphabet, "" when it is not the
//                            sharer's (see `sharePayload`)
//   6  mode      7  valence      8  assembly (two ids, no separator)
//   9  the sparse extras: "s<slot><pol><rank>" and "a<arcane><rank>", "." apart.
//      A polarity or a below-max rank is rare, and inline they would cost every
//      OTHER slot the separator that says where its modifier stopped.
//
function packV4(a) {
  const [, weapon, name, slots9, arcs, evos, rivens, sc, m, md, val, asm] = a;
  if (sc || m) return null;
  let bad = false;
  const id = (x) => { const t = si62(x); if (t === null) bad = true; return t || "00"; };
  const pre = evoPrefixFor(weapon);
  const extra = [];

  const slots = (slots9 || []).map((s, i) => {
    if (!s) return "--";
    const [sid, pol, rank] = typeof s === "string" ? [s] : s;
    // THE MODIFIER IS RECORDED BEFORE THE SLOT IS SPELLED, because a RIVEN slot
    // is spelled differently and would otherwise leave its polarity behind.
    if (pol || rank != null) extra.push(`s${B62[i]}${pol || "-"}${rank == null ? "" : rank}`);
    return String(sid)[0] === "~" ? "-" + String(sid).slice(1) : id(sid);
  });
  const arcanes = (arcs || []).map((x, i) => {
    if (!Array.isArray(x)) return id(x);
    extra.push(`a${B62[i]}${x[1]}`);
    return id(x[0]);
  });
  const riven = (r) => {
    const [rn, shape, rank, pol, bonuses, malus] = r;
    const sc2 = SHAPE_CODE[String(shape ?? "")];
    if (sc2 === undefined) bad = true;
    const stat = ([sid, roll]) => {
      const t = roll62(roll);
      if (t === null) bad = true;
      return id(sid) + (t || "00");
    };
    const derived = boardRivenName({
      bonuses: (bonuses || []).map(([x]) => x),
      malus: malus ? malus[0] : null,
    });
    return [rn === derived ? "" : encName(rn), sc2 || "-", rank, pol,
      (bonuses || []).map(stat).join(""), malus ? stat(malus) : ""].join(";");
  };

  const out = [
    id(weapon),
    slots.join(""),
    arcanes.join(""),
    (evos || []).map((e) => (e ? id(pre + e) : "--")).join(""),
    (rivens || []).map(riven).join("!"),
    encName(name),
    md || "",
    val ? `${val[0]}:${val[1]}` : "",
    asm ? id(asm[0]) + id(asm[1]) : "",
    extra.join("."),
  ];
  if (bad) return null;
  while (out.length > 2 && out[out.length - 1] === "") out.pop();
  const text = out.join("~");
  return SHARE_TEXT_OK.test(text) ? text : null;
}

function unpackV4(text) {
  const f = String(text).split("~");
  const weapon = si62Back(f[0] || "");
  if (!weapon) return null;
  const pre = evoPrefixFor(weapon);
  // FIXED WIDTH IS THE GRAMMAR: a run of ids is read two characters at a time,
  // which is why nothing separates them.
  const pairs = (str) => {
    const out = [];
    const t = String(str || "");
    for (let i = 0; i + 1 < t.length; i += 2) out.push(t.slice(i, i + 2));
    return out;
  };
  const sExtra = new Map(), aExtra = new Map();
  String(f[9] || "").split(".").filter(Boolean).forEach((t) => {
    // THE INDEX IS ONE base62 CHARACTER, not one decimal digit: a decimal one
    // stops being one character at ten slots, and a melee already has more
    // than nine things a modifier could name.
    const i = B62.indexOf(t[1]);
    if (t[0] === "s") sExtra.set(i, { pol: t[2] === "-" ? "" : t[2], rank: t.slice(3) });
    else if (t[0] === "a") aExtra.set(i, Number(t.slice(2)));
  });
  const slots9 = pairs(f[1]).map((t, i) => {
    if (t === "--") return 0;
    // ONE TAIL FOR BOTH KINDS: a riven slot takes a polarity exactly as a mod
    // slot does, and reading it anywhere but here is how it goes missing.
    const sid = t[0] === "-" ? "~" + t.slice(1) : si62Back(t);
    if (!sid) return 0;
    const x = sExtra.get(i);
    if (!x) return sid;
    return x.rank === "" ? [sid, x.pol] : [sid, x.pol, Number(x.rank)];
  });
  const arcs = pairs(f[2]).map((t, i) => {
    const aid = si62Back(t);
    return aExtra.has(i) ? [aid, aExtra.get(i)] : aid;
  });
  const evos = pairs(f[3]).map((t) => {
    if (t === "--") return "";
    const eid = si62Back(t);
    return eid && pre && eid.startsWith(pre) ? eid.slice(pre.length) : eid;
  });
  const rivens = String(f[4] || "").split("!").filter(Boolean).map((r) => {
    const [rn, shape, rank, pol, stats, mal] = r.split(";");
    const quad = (t) => [si62Back(t.slice(0, 2)), roll62Back(t.slice(2, 4))];
    const bonuses = [];
    const st = String(stats || "");
    for (let i = 0; i + 3 < st.length; i += 4) bonuses.push(quad(st.slice(i, i + 4)));
    const malus = mal ? quad(mal) : 0;
    const name = decName(rn) || boardRivenName({
      bonuses: bonuses.map(([x]) => x),
      malus: malus ? malus[0] : null,
    });
    return [name, SHAPE_BACK[shape] ?? "", Number(rank), pol, bonuses, malus];
  });
  const val = f[7] ? f[7].split(":") : null;
  return [2, weapon, decName(f[5]) || 0, slots9, arcs, evos, rivens, 0, 0,
    f[6] || 0, val ? [val[0], Number(val[1])] : 0,
    f[8] ? [si62Back(f[8].slice(0, 2)), si62Back(f[8].slice(2, 4))] : 0];
}

function packV3(a) {
  const [, weapon, name, slots9, arcs, evos, rivens, sc, m, md, val, asm] = a;
  // A LINK THAT SOMEHOW CARRIED A FIGHT COULD NOT BE WRITTEN IN v3 — and one
  // can no longer be built at all, so this is the format's own limit restated
  // rather than a branch anything reaches.
  if (sc || m) return null;
  const pre = evoPrefixFor(weapon);
  const slot = (s) => {
    if (!s) return "";
    const [id, pol, rank] = typeof s === "string" ? [s] : s;
    // A RIVEN SLOT names the riven by its place in field 5, as v2 does with
    // "~0" — `r0` here, because `~` is the field separator.
    const head = String(id)[0] === "~" ? "r" + String(id).slice(1) : siOf(id);
    if (rank != null) return `${head}:${pol || ""}:${rank}`;
    return pol ? `${head}:${pol}` : head;
  };
  const riven = (r) => {
    const [rn, shape, rank, pol, bonuses, malus] = r;
    const stats = (bonuses || []).map(([id, roll]) => `${siOf(id)}:${roll}`).join(",");
    const mal = malus ? `${siOf(malus[0])}:${malus[1]}` : "";
    // A NAME THE SHAPE ALREADY IMPLIES DOES NOT TRAVEL — the same rule every
    // other field here follows. A board riven's local name is
    // `boardRivenName(shape)` and nothing else, so sending it is pure length —
    // and it is the one string in this payload that is NOT url-safe (it is
    // localized: "榜单 · critical_chance / …"), which would have sent every
    // board-riven link back to base64 for a name the reader can compute.
    //
    // AND THE READER NAMES IT IN THEIR OWN LANGUAGE, which is better than
    // carrying the sharer's.
    const derived = boardRivenName({
      bonuses: (bonuses || []).map(([id]) => id),
      malus: malus ? malus[0] : null,
    });
    return [rn === derived ? "" : encName(rn), shape, rank, pol, stats, mal].join(";");
  };
  const out = [
    siOf(weapon),
    encName(name),
    (slots9 || []).map(slot).join("."),
    (arcs || []).map((x) => (Array.isArray(x) ? `${siOf(x[0])}:${x[1]}` : siOf(x))).join("."),
    (evos || []).map((e) => (e ? siOf(pre + e) : "")).join("."),
    (rivens || []).map(riven).join("!"),
    md || "",
    val ? `${val[0]}:${val[1]}` : "",
    asm ? `${siOf(asm[0])}:${siOf(asm[1])}` : "",
  ];
  while (out.length > 3 && out[out.length - 1] === "") out.pop();
  const text = out.join("~");
  return SHARE_TEXT_OK.test(text) ? text : null;
}

/// v3 text back into the v2 array, so everything downstream is unchanged.
function unpackV3(text) {
  const f = String(text).split("~");
  const weapon = siBack(f[0]);
  if (!weapon) return null;
  const pre = evoPrefixFor(weapon);
  const list = (i, sep) => (f[i] ? String(f[i]).split(sep || ".") : []);
  const slots9 = list(2).map((t) => {
    if (!t) return 0;
    const [head, pol, rank] = t.split(":");
    const id = head[0] === "r" ? "~" + head.slice(1) : siBack(head);
    if (!id) return 0;
    if (rank != null && rank !== "") return [id, pol || "", Number(rank)];
    return pol ? [id, pol] : id;
  });
  const arcs = list(3).map((t) => {
    const [i, rank] = t.split(":");
    return rank == null ? siBack(i) : [siBack(i), Number(rank)];
  });
  const evos = list(4).map((t) => {
    const id = siBack(t);
    return id && pre && id.startsWith(pre) ? id.slice(pre.length) : id;
  });
  const rivens = list(5, "!").filter(Boolean).map((r) => {
    const [rn, shape, rank, pol, stats, mal] = r.split(";");
    const pair = (t) => { const [i, roll] = t.split(":"); return [siBack(i), Number(roll)]; };
    const bonuses = stats ? stats.split(",").map(pair) : [];
    const malus = mal ? pair(mal) : 0;
    // AN EMPTY NAME MEANS "the one the shape implies" — see `packV3`.
    const name = decName(rn) || boardRivenName({
      bonuses: bonuses.map(([id]) => id),
      malus: malus ? malus[0] : null,
    });
    return [name, shape, Number(rank), pol, bonuses, malus];
  });
  const val = f[7] ? f[7].split(":") : null;
  const asm = f[8] ? f[8].split(":") : null;
  return [2, weapon, decName(f[1]) || 0, slots9, arcs, evos, rivens, 0, 0,
    f[6] || 0, val ? [val[0], Number(val[1])] : 0,
    asm ? [siBack(asm[0]), siBack(asm[1])] : 0];
}

const b64urlEnc = (u8) => {
  let s = "";
  u8.forEach((b) => { s += String.fromCharCode(b); });
  return btoa(s).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
};
const b64urlDec = (s) => {
  const b = atob(s.replace(/-/g, "+").replace(/_/g, "/"));
  return Uint8Array.from(b, (c) => c.charCodeAt(0));
};

async function deflate(u8) {
  if (typeof CompressionStream === "undefined") return null;
  const cs = new CompressionStream("deflate-raw");
  const w = cs.writable.getWriter();
  w.write(u8); w.close();
  return new Uint8Array(await new Response(cs.readable).arrayBuffer());
}
async function inflate(u8) {
  const ds = new DecompressionStream("deflate-raw");
  const w = ds.writable.getWriter();
  w.write(u8); w.close();
  return new Uint8Array(await new Response(ds.readable).arrayBuffer());
}

// v2 — POSITIONAL, and nothing that can be derived travels.
//
// v1 was 2.5 kB of JSON before compression and most of it was waste: a
// 914-byte freshness key (a serialised copy of the build, inside the payload
// describing that build), 401 bytes of riven shape DRAFTS the recipient
// regenerates blank anyway, and a JSON key beside every value. Links are
// posted into chat windows and printed into QR codes, so length is a feature.
//
// The layout, by index — read it beside `decodeShare`:
//   0  version (2)
//   1  weapon id
//   2  build name
//   3  slots: 9 entries, each null | [modId] | [modId, pol] | [modId, pol, rank]
//      pol is one letter, rank omitted when it is the mod's max
//   4  arcanes: [] | [id] | [[id, rank], …]      rank omitted when max
//   5  evolutions by tier: ["evo1_incarnon_form", "", …] — the weapon prefix
//      is stripped, since a tier's options belong to the weapon in field 1
//   6  rivens: [[name, shape, rank, pol, [[statId, roll], …], [malusId, roll]|0], …]
//   7, 8  FROZEN AT 0 — were the scenario and the measurement. Neither
//      travels; older links carry them and `decodeShare` does not look.
const POL_LETTER = { Madurai: "M", Naramon: "N", Vazarin: "V", Umbra: "U", Omni: "O" };
const LETTER_POL = Object.fromEntries(Object.entries(POL_LETTER).map(([k, x]) => [x, k]));

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

function sharePayload() {
  const st = snapshotState();
  const p = loadPresetList(BUILDS).find((x) => x.name === activePreset);
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
    const id = evoSel[x.tier];
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
  const r = p && p.lastResult && p.lastResult.r;
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
  // `single_target#cycle#p#1`. Neither means anything to the person opening the
  // link, `importShare` names an unnamed build anyway, and `build 1` was the
  // more expensive of the two: its SPACE is outside the compact form's
  // alphabet, so every ordinary link paid 2.5x for a name nobody chose.
  const nm = officialBuildActive() || isGeneratedName(activePreset)
    ? 0 : activePreset;

  const out = [2, st.weapon, nm, slots9, arcs, evos, rivens, 0, 0, md, val, asm];
  while (out.length > 9 && !out[out.length - 1]) out.pop();
  return out;
}

const r3 = (x) => Math.round((Number(x) || 0) * 1000) / 1000;
const cap1 = (s) => String(s || "").replace(/^./, (c) => c.toUpperCase());

async function shareUrl() {
  const payload = sharePayload();
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
  const code = forms.concat(zipped).reduce((a, b) => (b.length < a.length ? b : a), zipped);
  const w = weaponInfo($("weapon").value);
  return `${location.origin}${weaponPath(w.id)}?${SHARE_PARAM}=${code}`;
}

async function decodeShare(code) {
  if (!code) return null;
  // THE COMPACT FORMS ARE TEXT, so they never go near base64 or inflate.
  // Dispatched on the version character the code has always carried, which is
  // why adding a form costs every older one nothing.
  let data;
  if (code[0] === SHARE_V_B62) {
    data = unpackV4(code.slice(1));
    if (!data) return null;
  } else if (code[0] === SHARE_V_TEXT) {
    data = unpackV3(code.slice(1));
    if (!data) return null;
  } else {
    const bytes = b64urlDec(code.slice(1));
    const json = code[0] === SHARE_V_DEFLATE ? await inflate(bytes) : bytes;
    data = JSON.parse(new TextDecoder().decode(json));
  }
  if (!Array.isArray(data)) return v1Share(data);      // links posted before v2
  // FIELDS 7 AND 8 ARE NOT DESTRUCTURED, and that is the whole of "a link
  // carries a build and nothing else". A v1 or v2 link posted while the fight
  // and the measurement travelled still HAS them on the wire; refusing to read
  // them here is what stops one landing, and it is one place rather than a
  // guard at every use.
  const [, weapon, name, slots9, arcs, evos, rivens, , , md, val, asm] = data;
  return {
    w: weapon,
    n: name,
    // Absent means "whatever this weapon's default is", which is exactly what
    // `defaultMode`/`defaultValence` answer when handed nothing — so a link
    // posted before these two travelled still lands where it always did.
    mode: md || undefined,
    valence: val ? { element: val[0], bonus: val[1] } : undefined,
    // Absent means this weapon's default, the same as the two above — so a link
    // posted before parts travelled still lands where it always did, and an
    // ordinary weapon (which has none) is unaffected either way.
    assembly: asm ? { grip: asm[0], loader: asm[1] } : undefined,
    slots: (slots9 || []).map((s) => {
      if (!s) return { mod: null, pol: null, rank: null };
      const [id, pol, rank] = typeof s === "string" ? [s] : s;
      return { mod: id, pol: LETTER_POL[pol] || null, rank: rank ?? null };
    }),
    arcane: (arcs || []).map((a) => (Array.isArray(a) ? a[0] : a)),
    arcaneRank: (arcs || []).map((a) => (Array.isArray(a) ? a[1] : null)),
    evos: evos || [],
    rivens: (rivens || []).map(([rn, shape, rank, pol, bonuses, malus]) => ({
      n: rn,
      s: {
        shape, rank, polarity: (LETTER_POL[pol] || "Madurai").toLowerCase(),
        bonuses: (bonuses || []).map(([id, roll]) => ({ id, roll })),
        malus: malus ? { id: malus[0], roll: malus[1] } : null,
      },
    })),
    // ALWAYS NULL, for every link this decoder will ever be handed. A fight is
    // not a build's to move and a number measured in a fight nobody has is not
    // a claim, so neither reaches a reader — the fields stay here, spelled out,
    // because `importShare` reads this object and not the wire.
    sc: null,
    m: null,
  };
}

// A v1 link (the first shape this shipped in) read into the v2 structure.
// Its fight and its measurement are dropped on the same terms as v2's.
function v1Share(d) {
  if (!d || !d.b) return null;
  return {
    w: d.w, n: d.n,
    slots: d.b.slots || [],
    arcane: d.b.arcane || [], arcaneRank: d.b.arcaneRank || [],
    evos: Object.values(d.b.evoSel || {}).map((x) => x || ""),
    rivens: d.r || [],
    sc: null,
    m: null,
  };
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
  const name = freeName(builds, (n) => base + (n > 1 ? " " + n : ""));
  activePreset = name;
  localStorage.setItem(presetActiveKey(BUILDS), name);
  // THE BUILD, AND NOTHING ELSE TOUCHES THE FIGHT. `applyScenario` is the only
  // door a scenario is set through, so not opening it IS the guarantee — the
  // same rule `check_preset_independence.mjs` asserts for a build being LOADED.
  whileApplying(() => restoreState(state, w.id));
  // NO `lastResult`: a measurement is a build plus the fight it was made in
  // plus the number, and the fight did not travel. A number without one is not
  // a claim a reader could check, so the build lands unmeasured and the first
  // run here is the first number it has ever had.
  builds.push({ name, savedAt: Date.now(), state: snapshotState() });
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

// The share panel: the link, and a CARD to paste into a chat. Both carry the
// site's own address — an image that travels without one is a screenshot of
// nowhere.
async function openSharePanel(bar) {
  const panel = bar.querySelector(".pshare");
  if (!panel) return;
  if (!panel.hidden) { panel.hidden = true; return; }
  // ONE THING IS SHAREABLE HERE, AND IT IS A BUILD. A build is a statement
  // about a weapon; a RESULT is that build plus the fight it was measured in
  // plus the number, and a fight is not a build's to move — so the panel
  // offers the one link there is and needs no simulation to open.
  panel.hidden = false;
  const bUrl = await shareUrl();
  panel.innerHTML =
    `<div class="sh-row"><input class="sh-url" type="text" readonly value="${escHtml(bUrl)}">` +
    `<button class="cu-btn sh-copy">${escHtml(tr("copy link"))}</button></div>` +
    `<div class="sh-note">${escHtml(tr("the build and its rivens, and nothing else: no fight, no measurement, so opening it leaves the reader's own scenario untouched"))}</div>` +
    (SHARE_CARD_ENABLED
      ? `<div class="sh-more"><button class="cu-btn sh-full">${escHtml(tr("…as a card →"))}</button></div>`
      : "");
  const bBox = panel.querySelector(".sh-url");
  bBox.onclick = () => bBox.select();
  panel.querySelector(".sh-copy").onclick = async () => {
    try { await navigator.clipboard.writeText(bUrl); presetToast(tr("link copied")); }
    catch (_) { bBox.select(); presetToast(tr("press Ctrl+C to copy the selected link")); }
  };
  const full = panel.querySelector(".sh-full");
  if (full) full.onclick = () => openShareClaim(panel, bUrl);
}

/// THE CARD: a picture of this build's run, to paste into a chat window. Split
/// out of the panel above so it costs a simulation only when somebody asks.
///
/// THE LINK IS THE SAME BUILD LINK, never a second longer one carrying the
/// fight and the measurement — a CLAIM: a link may plant a build and never a
/// scenario. The measurement still
/// travels, as the picture, which is a thing a reader looks at rather than
/// something that lands in their app.
async function openShareClaim(panel, url) {
  const more = panel.querySelector(".sh-more");
  if (!more) return;
  // Measure BEFORE building the link, so both the card and the payload carry
  // a number produced by exactly this build in exactly this fight.
  more.innerHTML = `<div class="sh-note">${escHtml(tr("simulating this build in the current scenario…"))}</div>`;
  await resultForShare();
  more.innerHTML =
    `<div class="sh-row"><input class="sh-url" type="text" readonly value="${escHtml(url)}">` +
    `<button class="cu-btn sh-copy">${escHtml(tr("copy link"))}</button>` +
    `<button class="cu-btn sh-img">${escHtml(tr("copy image"))}</button>` +
    `<button class="cu-btn sh-dl">${escHtml(tr("download image"))}</button></div>` +
    `<div class="sh-note">${escHtml(tr("the CARD shows the measurement; the link beside it is the same build-only link — opening it never touches the reader's own scenario"))}</div>` +
    `<canvas class="sh-canvas" width="900" height="640"></canvas>`;
  const urlBox = more.querySelector(".sh-url");
  urlBox.onclick = () => urlBox.select();
  const canvas = more.querySelector(".sh-canvas");
  await drawShareCard(canvas, url);

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

