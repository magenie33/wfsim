/// WHO ELSE IS FIRING — the fight's roster of seats beside the open build.
///
/// A FIGHT IS n AGAINST m. The enemy half has been a list since the formation
/// existed (`sim.formation`, edited on the arena); this is the other half, and
/// it is drawn the same way for the same reason: a squadmate, a companion and
/// a summon are one thing to the engine — something with a build that acts on
/// its own clock — so the page offers ONE list rather than a control per kind.
///
/// A SEAT IS A LINK, NEVER A COPY. `{weapon, preset}` is the same shape the
/// wielder's is, and for the same reason: the preset is the build, so editing
/// it moves this fight too. A copy stored here would be a second answer to
/// "what is that build" with no rule for which wins.
///
/// THE OPEN BUILD IS THE FIRST SEAT and is not in this list, so an entry `i`
/// here is seat `i + 2` to a reader and `wielder`'s neighbour on the wire. It
/// is the one the page is about, the one the panel resolves and the only one a
/// board submission can carry — so it cannot be removed or pointed elsewhere.

/// The roster as stored, repaired: a list of `{weapon, preset}`.
const alsoActing = () => (Array.isArray(sim.also_acting) ? sim.also_acting : [])
  .filter((x) => x && weaponExists(x.weapon))
  .map((x) => ({ weapon: x.weapon, preset: x.preset || DEFAULT_PRESET_ID }));

/// HOW MANY SEATS A FIGHT MAY HOLD BESIDE THE OPEN BUILD — the engine's own
/// ceiling (`fight::MAX_COMBATANTS`, served at `/api/meta`), less the one the
/// page is about. Read rather than written down: a number copied here is two
/// declarations of one fact, and the day one moves the other is silently
/// wrong — the same rule `ARENA_MAX_BODIES` follows for the floor.
///
/// EIGHT IS A SQUAD WITH ITS COMPANIONS, which is what the ceiling is for.
const ROSTER_MAX = () => ((META && META.max_combatants) || 8) - 1;

/// ONE SEAT'S BUILD STATE, from the preset it names. The blank (`default`) is a
/// state of its own rather than a missing one: an unmodded weapon is a build,
/// and it is what a seat means before anybody picks a preset for it.
function seatState(ref) {
  const ps = presetListWithIds(BUILDS, ref.weapon);
  const p = ps.find((x) => x.id === ref.preset || x.name === ref.preset);
  return { ...((p && p.state) || {}), weapon: ref.weapon };
}

/// THE ROSTER, AS REQUEST FIELDS — one whole seat per entry, resolved through
/// the same `seatPayload` the open build goes through. `also_acting` on the
/// wire; `seat_from` reads each one exactly as it reads the reported build.
const alsoActingPayload = () => alsoActing().map((ref) => seatPayload(seatState(ref)));

/// The presets a seat can point at, the blank last — the same list and the
/// same ordering the wielder's preset control offers.
const seatPresetItems = (weapon) => [
  ...presetListWithIds(BUILDS, weapon).map((p) => ({ value: p.id, label: p.name })),
  { value: DEFAULT_PRESET_ID, label: `${tr("Default")} · ${tr("read-only")}`,
    hint: tr("no build — unmodded") },
];

function setRoster(list) {
  setScenarioFields({ also_acting: list.length ? list : null });
}

/// …AND THE OPTIMIZER'S COPY, read-only. Every candidate is scored beside
/// this roster (`Scenario::also_acting`), so a tab that did not draw it would
/// rank builds under a fight the reader cannot see they asked for — the same
/// reason that half already mirrors the buffs, the arena and the limits.
function renderRosterRef() {
  renderOptFightBrief();
  const host = $("opt-roster");
  if (!host || !META) return;
  const list = alsoActing();
  const name = (ref) => escHtml(weaponExists(ref.weapon) ? tf(weaponInfo(ref.weapon).name) : ref.weapon);
  const preset = (ref) => escHtml((presetListWithIds(BUILDS, ref.weapon)
    .find((p) => p.id === ref.preset) || {}).name || tr("Default"));
  host.innerHTML = `<div class="rs">
    <div class="rs-row rs-you"><span class="rs-n">1</span>
      <span class="rs-me">${escHtml(tr("this build"))}</span>
      <span class="rs-hint">${escHtml(tr("the one being searched"))}</span></div>
    ${list.map((ref, i) => `<div class="rs-row"><span class="rs-n">${i + 2}</span>
      <span class="rs-me">${name(ref)}</span>
      <span class="rs-hint">${preset(ref)}</span></div>`).join("")}
    ${list.length ? "" : `<div class="rs-add"><span class="rs-hint">${
    escHtml(tr("nobody else — one gun against the formation"))}</span></div>`}
  </div>`;
}

function renderRoster() {
  renderRosterRef();
  const host = $("sim-roster");
  if (!host || !META) return;
  const list = alsoActing();
  const weapons = oneCardPerChamber(META.weapons || [])
    .map((w) => ({ value: w.id, label: w.name, image: w.image }));
  const rows = list.map((ref, i) => `<div class="rs-row" data-seat="${i}">
      <span class="rs-n">${i + 2}</span>
      ${ddButton(`dd-seat-${i}`, {
    value: ref.weapon, search: true, items: weapons,
    onPick: (id) => {
      // A NEW WEAPON IS A NEW BUILD. Its presets are its own, so the one named
      // here cannot survive the change — it lands on the blank, which is the
      // only build every weapon has.
      const next = alsoActing();
      next[i] = { weapon: id, preset: DEFAULT_PRESET_ID };
      setRoster(next);
    },
  })}
      ${ddButton(`dd-seat-preset-${i}`, {
    value: ref.preset, items: seatPresetItems(ref.weapon),
    onPick: (id) => {
      const next = alsoActing();
      next[i] = { weapon: next[i].weapon, preset: id };
      setRoster(next);
    },
  })}
      <button class="ghost-btn small rs-x" data-drop="${i}"
        title="${escHtml(tr("take this one out of the fight"))}">×</button>
    </div>`).join("");
  const full = list.length >= ROSTER_MAX();
  host.innerHTML = `<div class="rs">
    <div class="rs-row rs-you">
      <span class="rs-n">1</span>
      <span class="rs-me">${escHtml(tr("this build"))}</span>
      <span class="rs-hint">${escHtml(tr("the one this page is about, and the only one a board takes"))}</span>
    </div>
    ${rows}
    <div class="rs-add">
      <button class="ghost-btn small" id="rs-add"${full ? " disabled" : ""}
        >+ ${escHtml(tr("another gun"))}</button>
      <span class="rs-hint">${escHtml(list.length
    ? tr("everyone fires on their own clock, and the result reports each of them")
    : tr("a squadmate, a companion or a summon — anything with a build of its own"))}</span>
    </div>
  </div>`;
  const add = $("rs-add");
  if (add) {
    add.onclick = () => {
      // THE WEAPON IN FRONT OF YOU is what a new seat starts on, unmodded. It
      // is the one guess that is never wrong about intent: a reader adding a
      // seat is going to point it somewhere, and any other default would be
      // this page's opinion about who they play with.
      setRoster([...alsoActing(), { weapon: $("weapon").value, preset: DEFAULT_PRESET_ID }]);
    };
  }
  host.querySelectorAll("[data-drop]").forEach((b) => {
    b.onclick = () => {
      const next = alsoActing();
      next.splice(Number(b.dataset.drop), 1);
      setRoster(next);
    };
  });
}

// ---- the agent door's three actions, spread into `AGENT_ACTIONS` ----------
const ROSTER_ACTIONS = [
  {
    id: "simulator.roster.read",
    query: true,
    what: "Read who is firing: the open build as seat 1, and every other seat the fight holds as a weapon and the preset of it that is acting.",
    anchor: "#sim-roster",
    needs_weapon: true,
    args: {},
    run() { return agentRoster(); },
  },
  {
    id: "simulator.roster.add",
    writes: "scenario",
    what: "Put another gun in the fight — a squadmate, a companion or a summon. It acts on its own clock with its own build, and the result reports each seat separately. Omit preset for the weapon unmodded.",
    anchor: "#sim-roster",
    needs_weapon: true,
    args: {
      weapon: { kind: "string", required: true, what: "weapon id" },
      preset: { kind: "string", what: "a saved build of that weapon, by name" },
    },
    run({ weapon, preset }) {
      if (!weaponExists(weapon)) return agentNo("bad_argument", { argument: "weapon", because: "no weapon with that id" });
      const seats = alsoActing();
      if (seats.length >= ROSTER_MAX()) return agentNo("roster_full", { because: `a fight holds ${ROSTER_MAX() + 1} seats`, max_seats: ROSTER_MAX() + 1 });
      const own = presetListWithIds(BUILDS, weapon);
      const p = preset ? own.find((x) => x.id === preset || x.name === preset) : null;
      if (preset && !p) return agentNo("bad_argument", { argument: "preset", alternatives: own.map((x) => x.name).slice(0, 20) });
      return agentArena(() => {
        setRoster([...seats, { weapon, preset: p ? p.id : DEFAULT_PRESET_ID }]);
        return agentRoster();
      });
    },
  },
  {
    id: "simulator.roster.remove",
    writes: "scenario",
    what: "Take one seat out of the fight, by its number. Seat 1 is the open build and cannot be removed.",
    anchor: "#sim-roster",
    needs_weapon: true,
    args: { seat: { kind: "number", required: true, min: 2, max: 9, what: "seat number, as roster.read reports it" } },
    run({ seat }) {
      const seats = alsoActing();
      const i = Math.round(seat) - 2;
      if (i < 0 || i >= seats.length) return agentNo("bad_argument", { argument: "seat", alternatives: seats.map((_, n) => n + 2) });
      seats.splice(i, 1);
      return agentArena(() => { setRoster(seats); return agentRoster(); });
    },
  },
];
