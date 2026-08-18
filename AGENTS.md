# WFSim — agent guide

The ultimate Warframe **calculator** — builder, simulator, optimizer.
Core promise: **matches in-game measurements**. Rust workspace + YAML game
data + a dependency-free web UI, deployed as WASM on Cloudflare
(wfsim.app).

Those three are `docs/CORE.md`'s own sentence — "given weapon + mods +
target + scenario, output damage matching in-game measurements item by
item, and search backwards for the optimal build" — as three verbs: BUILD,
SIMULATE, SOLVE. Nothing else is a peer of them; anything new either feeds
one or reports from one. "fight simulator" and "Monte-Carlo optimizer"
named the implementation, which is not what the product is organised
around (decision 2026-07-31).

## Map

- `engine/` — all game mechanics. A fight has TWO actors and
  `engine::arena::Arena` is both of them (a `Tenno` from `data/tenno/`, a
  target with its hitboxes, a duration): the web api and the optimizer each
  build one from the same scenario and hand it to the same constructor, which
  is what makes a search's winner scored under the fight the replay runs.
  Every `condition:` on a mod card and every `kind: tenno_scaled` arcane is a
  question about that Tenno — see MECHANICS §8. Every formula carries a comment citing
  its source (wiki page / datamine / measurement). The engine knows NO
  weapon names: weapons/mods/etc. are loaded from `data/` YAML.
- `optimizer/` — build search (successive-halving funnel). It only ever
  calls the engine — never add a simplified damage formula here.
- `web/` — the native dev server (`cargo run -p wfsim-web`, port 8787).
  `web/src/static/` holds the UI (vanilla JS/CSS, no framework, no deps).
- `webapi/` — endpoint logic shared by `web/` (native) and `wasm/`.
- `wasm/` + `site/` — the static deployment. `site/` is **generated** by
  `scripts/build_site_app.py` — never hand-edit it.
- `data/` — versioned game data. `data/README.md` explains the reference
  graph; `docs/DATA_SOURCES.md` the sourcing rules. **THE WIKI WINS. Use it
  wherever it can answer** (owner, 2026-08-14) — WFCD's export (`vendor/`) is
  the CROSS-CHECK and the fallback, not a peer. It stopped being a peer on
  evidence: its Arch-Gun entries carry the ARCHWING column of a two-column
  infobox, which is the wrong column for an arena on the ground, and it agrees
  with the wiki on every OTHER field of those weapons — so a cross-check that
  did not name the damage row passed, and the Larkspur Prime posted 112 board
  rows at half its damage. An export cannot say "there are two of these and you
  want the other one"; a page can, and does.
  **The ONE standing exception is `base_drain`/`max_rank` on MODS**, where the
  wiki is wrong for ~20 of them and WFCD is right — an exception held up by its
  own evidence, not by symmetry.
  Still cross-check, and still join the two by `internal_name` == `uniqueName`,
  **never by name** (WFCD has stale duplicates sharing a display name).
- `data/abilities/` — WARFRAME ABILITY BUFFS, and the one data family that
  describes neither a weapon nor a build: a thing done TO this weapon for a
  while. It rides on the `Arena`, so `parse_fight` alone carries it into both
  the simulator and the search, and no board ruler sends one. Early access by
  the owner's own framing (2026-08-08) — the strength and duration are typed in
  today and come from the frame later, which is why `resolve` takes both as
  arguments. Four effect kinds, and the split is THREE MULTIPLIERS AND ONE
  INSTANCE: Roar/Eclipse/Nourish scale a number someone else computes, while
  `extra_hit` (Xata's Whisper) FIRES a second damage instance and therefore has
  to be told what triggered it — MECHANICS §7 §"Extra Hit" is the formula and
  MEASUREMENTS M40 the capture it decodes. See BUFFS.md §"A WARFRAME ABILITY".
- `docs/EXTRA_HIT.md` — ONE LAW behind four things built separately: a second
  damage instance beside a hit, worth a percentage of it, rolling its own
  status. Primary Debilitate's split, Cyte-09's Resupply, Xata's Whisper and
  Toxic Lash are members, and each supplies only a percentage and an element.
  It is where the `f³` triple dip comes from, and where the 0% rule lives —
  an Extra Hit REPLACES the base its status burns off, so one that deals
  nothing leaves the level above standing (owner, 2026-08-09).
- `docs/CATALOGS.md` — THE PER-WEAPON TABLES, in one place. Some mechanics are a
  formula plus a published table with one ROW PER WEAPON, and the row says what
  the weapon's own stats never would — this one multiplies where everyone else
  adds, this attack part is exempt, this one does not work at all. Condition
  Overload and Primary Compression are both that shape. The rule is the CO
  rule generalised (owner, 2026-07-30): **the catalog is authoritative and
  absence means ORDINARY, not unknown**, and a row is transcribed for the entry
  it names rather than generalised to a class. Each section carries the columns
  verbatim, the rows the roster holds, and where it has already gone wrong —
  the Shedu was filed as `Adding` on the reasoning that it had no row, and it
  has one saying Multiplying (2026-08-10).
- `docs/UNMODELLED.md` — the EDGES, by reason rather than by perk: one target,
  no distance, no movement, no holster, infinite ammo, nobody shoots back, and
  the Warframe layer. Written so "why is this perk worth nothing" is a lookup
  (owner, 2026-08-09). It also holds the OPEN DECISIONS — things the engine
  could do today and does not because doing them means inventing a play pattern,
  which is the owner's call and not the model's (reload interruption is the live
  one). `python scripts/intake_report.py --full` prints the per-weapon list; this
  is the six reasons behind it.
- `docs/` — CORE (design), MECHANICS (formulas), MEASUREMENTS (protocol +
  baselines), BUFFS, BOARD (the official leaderboard), OPTIMIZER, UI, WASM,
  GLOSSARY, DEVELOPMENT (setup), INVESTMENT (capacity/Forma), WEAPON_INTAKE
  (which weapons next, and what each costs), INCARNON (the Incarnon gun
  roster — every adapter, what it covers, and what is done).
- `tests/golden/` — golden tests calibrated against in-game measurements.
- `private/` — gitignored (devlogs, drafts, local assets, the `data/`
  verification scripts). **`git add -A` silently skips it**, so never report a
  change under `private/` as shipped, and never let something the repo needs
  live only there — put it in `docs/` (verification tooling is catalogued in
  `docs/DATA_SOURCES.md`). Single-machine development (2026-07-30): local-only
  is fine, invisible is not.

## Build, test, verify

- Toolchain via mise (`mise install`); on this repo plain `cargo` works
  once installed. CI = `cargo clippy --workspace --all-targets --
  -D warnings` + `cargo test --workspace` — run both before pushing.
- **Static files are `include_str!`'d into `wfsim-web.exe`**: after ANY
  edit under `web/src/static/`, stop the running server (it holds the
  exe), `cargo build -p wfsim-web`, restart. `cargo test` does NOT
  refresh the exe.
- **`data/` (weapons, mods, i18n, …) is embedded at COMPILE TIME too**
  (`engine::data::files_under`): a YAML edit — including
  `data/i18n/zh/` translations — needs the same rebuild + restart,
  and a site regeneration to reach wfsim.app. **This catches TESTS too**: cargo
  does not treat a yaml as a source dependency, so a test run right after a
  yaml edit reads the data compiled into the previous binary. It matters most
  when PROVING a check bites — revert the data, `touch` any `.rs` in that
  crate, then run. Without the touch the test passes and the check looks
  useless when it is fine (2026-08-07).
- After frontend or engine changes, regenerate the static site:
  `python scripts/build_site_app.py` (wasm-bindgen-cli version must match
  Cargo.lock). Commit the regenerated `site/`. It also PRERENDERS one
  `site/weapons/<Wiki_Name>/index.html` per roster weapon (own
  title/description/canonical/OG + a crawler-visible summary the app
  removes on boot), plus `sitemap.xml` and `robots.txt` — without them
  every URL answered with the same contentless shell, which is a soft 404
  to a crawler and an empty preview to a chat app.
- **Images are SAME-ORIGIN, and the art ships with the site** (rule
  2026-07-31, replacing "DE art stays out of the repo"). `site/img/` holds
  every file `data/assets.yaml` references (`scripts/fetch_images.py` fills
  `web/cache/img/`, `build_site_app.py` copies it and FAILS the build on a
  missing one). Why it changed: the static build used to hotlink
  `cdn.warframestat.us/img/…`, which answers **301 → raw.githubusercontent.com**
  — unreliable to blocked from mainland China, i.e. precisely where the
  players are, so the app's own art was the least reliable thing on the page.
  Same-origin ends the question: if wfsim.app loads, its art loads. The cost
  is ~4.3 MB write-once, against a 2 MB wasm this build rewrites every time.
  DE permits this: their Content Policy requires only that use of Warframe
  assets be non-commercial, and the wiki hosts the same files on the same
  basis — what it forbids is their LOGOS, so the only mark here stays ours.
  A `wiki:` prefix in `assets.yaml` means the CDN lacks that file and the
  FETCHER takes it from the wiki; the cached name and the page's URL are the
  bare name either way.
- Deploy = push to `main`: Cloudflare picks up `site/` automatically
  (takes ~1–2 min). There is no deploy step in CI.
- **NEVER RESCORE THE BOARD LOCALLY.** `.github/workflows/board.yml` already
  rescores every stored row on any push touching `engine/`, `data/`, `webapi/`
  or the scorer — which is precisely every change that moves a score — and the
  bot commits the result. Running `scripts/rescore_board.py --write` by hand
  buys nothing the push already bought, and it costs: it holds the board yaml
  TRUNCATED while it runs, so the board tests go red and `site/` cannot be
  regenerated until it finishes. At the rulers' 1000 runs that is an hour of
  blocking on work a runner was doing anyway (owner, 2026-08-11). Use it
  WITHOUT `--write` when you want to know whether a change moved anything;
  let the workflow write.
- **Engine COST: `cargo run --release --bin one_fight`**, and `-- save` first.
  The accuracy half is graded (`wfsim-truth` below); this is the other half, and
  without both "it feels faster" and "it got dumber" are the same sentence. It
  diffs a saved baseline and says whether the ANSWER moved — a moved answer is a
  non-zero exit, because an optimisation that changes a number is a bug. Read
  its table ACROSS: the default is three shapes, and a change to the inner loop
  rarely moves them together (`target-cpu=native` is −23% / −36% / **+31%**
  across them). docs/DEVELOPMENT.md §5 lists what has already been tried and
  what it was worth, so nobody spends a day on it twice.
- **Optimizer verification: `cargo run --release --bin wfsim-truth -- pool=<ids>
  …`**. A search cannot vouch for itself, so it is GRADED: the tool exhausts the
  scope, evaluates every job flat, and reports where the production search
  landed in that reference ranking (rank / regret / recall / cost, and whether
  the reference reproduces itself under a second seed). It goes through
  `parse_optimize`, so it grades the app's own fight, and it REFUSES a scope it
  cannot exhaust. Run it after ANY change to enumeration, scheduling or
  scoring. The cheap CI form is `optimizer/tests/search_accuracy.rs`. See
  docs/OPTIMIZER.md §Accuracy.
- **A CHECK CLEANS UP AFTER ITSELF** (2026-08-10). Each `openApp` runs Chrome in
  its own throwaway profile under `%TEMP%`, and for a long time none of them
  were ever removed: `finish()` called `proc.kill()` and `rmSync` on the next
  line, which on Windows always failed because Chrome's CHILD processes still
  held the directory — `kill()` reaches only the node that was spawned. 644
  directories and 17 GB of C: later (owner), `finish` kills the whole tree
  (`taskkill /T` on win32), waits for it, and retries the removal — and
  `sweepStaleProfiles` deletes any `wfsim-*` older than an hour ON THE WAY
  IN, which is the only cleanup a run that throws, is
  interrupted, or never calls `finish()` can still get.
- UI verification: drive headless Chrome over CDP (Node ≥22 has a global
  WebSocket; Chrome is at the default install path). Assert real DOM
  state; screenshots for layout review. `node scripts/check_parity.mjs`
  is the committed one — the builder and the optimizer must offer the
  same options and the same visibility on every axis, and it exits
  non-zero when they do not. Run it after adding a weapon or anything a
  weapon can carry. `node scripts/check_mobile.mjs` is the FIFTEENTH and the only
  one that looks at GEOMETRY rather than at what the DOM says: the page must fit
  the screen it is on, at 360-1280px, with nothing past the viewport and no
  sideways scroll. It exists because horizontal overflow is invisible on the
  machine it is written on — the mod grid was `repeat(2,1fr)` at every width, a
  bare `1fr` floors its track at MIN-CONTENT (198px a slot), and on a phone the
  right-hand slots hung 55px off-screen with their ⋯ button unreachable (owner,
  2026-08-05). It also asserts a mod NAME keeps room to be one, because the
  cheapest way to stop an overflow is to squeeze a column to nothing.
  `node scripts/check_equip_rules.mjs` is the TWELFTH — what a
  mod's CARD says the weapon may do, in both directions. An
  equip rule is asked of EVERY firing mode, and installing a form ADDS one — so
  Dual Toxocyst wears Semi-Pistol Cannonade until tier 1 goes in and not after
  (wiki: "must have Semi-Auto trigger type for both firing modes"). The engine
  decides (`pool_for_build`) and `/api/meta` states the CONSEQUENCE per
  evolution (`evo_forbids`); the check asserts the page acts on it — the picker
  stops offering it, installing the form UNEQUIPS it and says so, the Form
  control greys the Incarnon options with the reason on screen without moving
  the scenario's own selection, and the sim refuses the pair. It also covers the
  LOCK the same families carry ("set to its default ignoring other bonuses, even
  negative effects"): the panel pins the stat and NAMES what pinned it, and a
  buff whose only grant is that stat is not offered — a lock reaches the
  evolution, arcane and passive layers too, not just the mod bucket
  (MEASUREMENTS M30).
  `node scripts/check_board_link.mjs` is the SIXTEENTH: a board
  row opens THAT row — the build it names AND the ruler it is on. The link
  carried the weapon and the mode and not the ruler, and both boards call their
  leader "#1 · Incarnon cycle", so the no-aim leader opened the aimed board's
  leader under the aimed board's fight and re-running it matched no line on
  either board (owner, 2026-08-08). It walks every ruler, because the bug was
  that one of them was reachable and the rest resolved to it, and asserts
  against `BOARD` itself so it keeps holding as the board moves. It also holds a
  case the LIVE board has never had: ONE WEAPON, TWO MODES. The board has always
  LISTED every weapon in every mode it can be played in (`benchEntries` walks
  `w.modes`, and only the SUSTAINABLE ones reach the page), and the scorer has
  always kept its quota per weapon AND mode — but no submission has ever named a
  second mode, so the half of a row's identity that says HOW it was played had
  never been told apart from the same weapon's other row. The check injects a
  synthetic second-mode row and asserts both are listed, both measured, and that
  the second one's link opens ITS mode, ITS ruler and ITS build (2026-08-09).
  `node scripts/check_disclosure.mjs` is the EIGHTEENTH: what the app does NOT
  model is ON THE PAGE, in every family that has one — a weapon banner, an
  evolution chip, a mod line, an arcane line, an enemy caveat. The owner debugs
  by reading the card, so a gap that lives only in a yaml comment or a report
  script is a gap nobody can act on (2026-08-08). Each surface has gone silent
  at least once: an arcane effect the loader had no arm for went to `Inert`,
  which printed NOTHING, so both Deadheads promised a recoil reduction they did
  not apply. It also covers the FOURTH kind of admission, which is the only one
  that is not a shortfall: a LIVE BUG (`live_bugs:` on an arcane) says the
  number is RIGHT, the game is wrong, and a hotfix changes it — Primary
  Debilitate's split leaks its zero-damage instance's multipliers into the DoT
  it leaves (MEASUREMENTS M37), so a player building around x441 is told what it
  rests on (owner, 2026-08-08). It reaches EVOLUTIONS as of 2026-08-16, where a
  live bug is declared beside the effect it kills rather than on the perk —
  Carnage Reign's +60 base damage works and its "+33% per Status Type" pays
  nothing (MEASUREMENTS M49), so a card that condemned the whole option would be
  as wrong as one that stayed quiet. It carries a NEGATIVE CONTROL — a weapon with
  nothing to admit shows no banner — because a check that only asserts presence
  passes just as well on a page that shouts "not modelled" at everything, and
  it runs the whole pass in BOTH languages,
  since the banner's lines were rendered raw for a day and a Chinese page
  carried its one important paragraph in English. It also walks the BOARD,
  which is where weapons are compared and therefore the one place a weapon
  with unmodelled parts must not look like one without them.
  `node scripts/check_wf_buffs.mjs` is the NINETEENTH: a WARFRAME ABILITY buff
  is the FIGHT's, and it reaches the number. Roar, Eclipse, Nourish and the four
  elemental augments (`data/abilities/`) belong to neither the build nor the
  weapon — they ride on the Arena, which is what gives the optimizer them for
  free and keeps them off the board. It asserts the section draws in both
  languages under DE's OWN names (战吼, 黯然失色 — transcribed, never
  translated), that the card's value follows Ability Strength, that ticking one
  moves a real `/api/simulate` in the shipping wasm build, that two of a FAMILY
  do not stack AND the page says which one lost (owner, 2026-08-08 — the
  difference between
  +50% and +80% is a number you have to be told), that the optimizer shows the
  same buffs read-only, and — the negative control — that no RULER carries one.
  `node scripts/check_pace_and_hits.mjs` is the TWENTY-FIFTH: what a ROOM-CLEAR
  is paced by, where an IMPOSSIBLE NUMBER hides, and the fact that every block
  FOLDS. `dps` is the whole engagement with its reloads in it — the honest
  number for a long fight and the wrong one for a room — so burst DPS is the
  same damage over the time the trigger was actually down, and the check
  RECOMPUTES it rather than trusting it. Beside it: time to the first kill with
  its spread (a mean alone reads as a promise), the opening magazine, the
  biggest single instance, damage per shot and per pellet. The HISTOGRAM is the
  other half — every hit sorted by crit tier and body part, because the same
  damage spread over "one in twelve hits did 40x" and "every hit did 3.3x" reads
  identically as an average and is two different weapons, only one of them a
  bug; its counts have to add up to the pellets that were fired. And every block
  folds and REMEMBERS across a re-render and a reload (owner, 2026-08-11) — a
  panel that re-opens everything on every Run Sim is a panel you re-close on
  every Run Sim, so the state lives outside the markup. It caught a real one on
  the way in: the opening window never closed on a weapon that TRANSMUTES
  instead of reloading, because it was recorded at the
  reload rather than at the refill.
  `node scripts/check_hit_account.mjs` is the TWENTY-FOURTH: THE ACCOUNT OF ONE
  HIT HAS TO MULTIPLY OUT. Every other number the sim reports is an aggregate,
  and an aggregate hides an error inside an average — a factor applied twice, or
  in the wrong bracket, moves a mean by a few per cent and reads as "this build
  is good". The account is the one output that can be FALSIFIED (owner,
  2026-08-11): one damage instance per attack part from the median engagement,
  every factor listed with its value in the order the engine applies them, and
  the product is the number that went into the damage meter. The check does the
  arithmetic a reader would do, so a factor
  applied and not listed — or listed and not applied — fails it. That is why the
  account is written at the ONE site where every factor exists at the same time
  rather than reconstructed afterwards. Verified to bite: dropping the crit line
  gives 1,510 against a claimed 13,292. It also asserts the panel draws it, since
  a ledger nobody can see is a ledger nobody checks.
  `node scripts/check_debuff_coverage.mjs` is the TWENTY-THIRD: the DEBUFF table
  is the BUFF table, read from the other side. The replay had always shown what
  the BUILD had up — live stacks, uptime, dead bands, the ramp — and said nothing
  about what was on the TARGET, which is the other half of the same fight and
  the half that explains the number (owner, 2026-08-11). It is one component
  fed from both sides: `DEBUFF_ROSTER` is the mirror of `buff_roster`,
  `Frame.debuffs` of `Frame.stacks`, and the page draws the second table with
  the same renderer. The check asserts the SYMMETRY rather than the numbers —
  same roster shape, one series per entry, each as long as
  the clock, the cursor reading its own side — plus the one thing that is not
  symmetric and has to be: A RESPAWN IS THE SAME TARGET, so its stacks drop to
  zero and climb again INSIDE one series rather than starting a new row, and
  that gap counts against uptime. Rows the run never touched are dropped, since
  thirteen flat charts would bury the three that moved. Verified to bite —
  short-circuiting the second table away fails five of them.
  `node scripts/check_custom_enemies.mjs` is the TWENTY-SECOND: a target you
  MADE is a target like any other. It is the second custom, and the test of the
  claim above — if a custom enemy really is an `EnemySpec` in the scenario's
  list, then the simulator, the optimizer and the target card need no code of
  their own for it. Two of its assertions are the sharp ones. The IMMUNITY is
  MEASURED rather than read back off the card: a Toxin-immune target must take
  literally nothing from a Torid and the same target at x1 must take something,
  because a column that is shown and not applied looks exactly like one that
  works. And DELETING a custom must repoint the fight — a custom is the kind of
  collection whose deletion breaks references elsewhere, which is the whole
  difference between it and a preset. Verified to bite: dropping the inline
  travel gives `unknown enemy: custom:target 1`, because the server has never
  heard of it and never will.
  `node scripts/check_opt_modes.mjs` is the TWENTY-FIRST: HOW A WEAPON IS PLAYED
  is the BUILDER's control and the OPTIMIZER's dimension. The report was one
  screen doing neither (owner, 2026-08-11): the Phantasma's charged mode picked
  on the optimizer tab searched its base form, because the control there was the
  BUILDER's Mode block — drawn on that tab only because nothing hid it — and the
  optimize request carried no mode at all. So the page offered a choice it never
  sent. Both halves moved: the block is the builder's alone (mode is part of a
  build and saves in a build preset — 2026-08-07), and the optimizer got a real
  AXIS with pool/req marks, because it binds a SET where the builder binds a
  value. Server-side a VARIANT is now a (mode, evolution set) pair, which is why
  nothing downstream had to learn about modes — every consumer already read a
  variant as "the forms this candidate fires". The check pins the sharp case:
  PINNING a mode makes every ranked row come back in it, pooling both DOUBLES
  the candidate count, and each row carries the mode it was scored in into the
  build it becomes. Verified to bite — dropping `modes`/`mode` from the request
  reproduces the report exactly, every row `base`.
  `node scripts/check_run_counts.mjs` is the TWENTIETH: HOW HARD YOU MEASURE is
  a number someone can set, in all three modules, and it walks all three because
  the answer differs in each. The simulator defaults to the rulers' 1000 (owner,
  2026-08-11) so a first number is comparable with the board without touching a
  box — measured at 1.3 s a run in the shipping wasm build, against 0.14 s at
  100. The quick calc takes its own count with a FLOOR of 10, which is where a
  status mod stops being a coin flip (M24: one run swings it ±39 points), and a
  number under it is raised rather than obeyed. The optimizer's final round
  takes its own too, where it used to take the fight's by rule — and BLANK still
  means the fight's, which is the case the check exists for: a blank box that
  silently means something either reads as broken and works, or reads as fine
  and sends 0. It asserts blank FOLLOWS the fight rather than having copied it
  once, that a count of its own does not edit the fight, and that clearing it
  goes back.
  `node scripts/check_arena.mjs` is the TWENTY-EIGHTH: THE ARENA IS A PLACE YOU
  CAN DRAG, and what you drag is what gets simulated. A fight is two bodies on a
  floor, so the panel draws two bodies on a floor and you move them with your
  finger (owner, 2026-08-15) — and the picture is not a decoration, which is the
  whole reason the check exists: a scene that looked right and did not reach
  `/api/simulate` would be the most convincing wrong thing on the page. The
  bodies are drawn at their REAL radius (`space::BODY_RADIUS_M`, 0.25 m), so
  "as close as they go" is visible rather than a rule you are told: they touch
  at CONTACT (0.5 m) and will not pass through each other, which the engine
  clamps to as well. It caught two real faults on the way in, both invisible to
  a reader: the scene laid itself out in the host's pixel width, which is ZERO
  while the panel is on another tab, so one drag wrote `[null, null]` into the
  fight (fixed with a viewBox — a fixed coordinate space has no such moment);
  and `paint()` replaces the markup on every move, so listeners bound to the
  circles died with the first repaint and the scene was draggable exactly once.
  A SHOT LEAVES THE MUZZLE and a distance is the GAP (owner, 2026-08-16). The
  shooter fires from a point on its own circumference facing the target — drawn,
  with the arrow that says which way it faces — and hitting the circle is a hit,
  which makes the test ray-versus-circle (`range · sin θ ≤ r`, the range being
  muzzle to the target's CENTRE) rather than the `centre · tan θ` it was. That
  range is NOT a flight, and calling it one was worth an inconsistency the owner
  caught on sight: a bullet vanishes at the SURFACE it hits, so what it flies is
  the GAP between the two bodies — one radius shorter, ZERO AT CONTACT, the
  number a reader is shown, and what damage falloff reads. One quantity wearing
  three hats rather than three that have to be argued into agreement. CONTACT IS
  THEN UNMISSABLE AT ANY CONE WIDTH twice over — a flight of zero leaves a cone
  no distance to widen over, and the ray-circle test agrees from the other side
  — where the old formula dropped more than half a 60 degree cone's pellets
  pressed against an enemy. MECHANICS §11 is the whole geometry.
  THE CANVAS IS THE ONLY PLACE A POSITION IS SET (owner, 2026-08-16). The typed
  Distance box is gone: two controls for one fact is how one of them silently
  undoes the other's other axis, and the scene is the SOURCE — the target's
  place, and every enemy's place when there is more than one. The shortcuts
  that replaced the box live INSIDE it (contact / 5 / 10 / 20 / 40 m), each one
  moving the target ALONG the line it already stands on, which is the same rule
  the drag obeys because they move the same body; the chip for the distance you
  are at is marked. A BENCHMARK'S FIGHT IS NOT DRAGGABLE — the rulers pin their
  distance, and the scene refuses the gesture ITSELF because
  `lockOfficialScenario` sweeps `input,select,button,textarea` and these bodies
  are SVG circles that sweep never reached. Adding that assertion exposed that
  the check had been testing the wrong thing all along: the app lands a
  first-time visitor ON the official ruler, so every drag assertion here had
  been running against a fight that should never have been draggable, and the
  check now opens a scenario of its own first and asserts that it did. The
  OPTIMIZER draws the same scene read-only, because a fight is edited in one
  place.
  `node scripts/check_formation.mjs` is the THIRTY-SECOND: A FORMATION IS
  SOMETHING YOU BUILD ON THE FLOOR, and what you build is what gets simulated.
  The arena has drawn two bodies since 2026-08-15; this is the same claim for
  fifty (owner, 2026-08-17). It would catch the most convincing possible bug —
  a scene that looks like a formation and sends one target — so it asserts the
  whole chain: bodies draw without standing on each other, any one drags, the
  payload matches the scene body for body, and a real `/api/simulate` in the
  shipping wasm build answers HIGHER for a crowd than for one body, because the
  chain has somewhere to go. AIM IS A PLACE rather than a target: the marker
  rides the target until dragged, and once dragged the beam is on whichever body
  the LINE crosses — asserted with two bodies on one line where the nearest to
  the cursor is the FAR one. Two negative controls: a formation of one is the
  fight this app has always run (zero sent, aim null), and an official ruler
  refuses a crowd both by disabling the control and by not moving when it is
  clicked anyway.
  `node scripts/check_gunco_stated.mjs` is the TWENTY-NINTH: EVERY WEAPON SAYS
  WHICH CONDITION OVERLOAD RULE IT IS COMPUTED UNDER, with nothing equipped.
  The rules are PER WEAPON and hand-transcribed from a catalog — Adding or
  Multiplying, which attack parts take it, what fraction of the base the term
  reads — and the Burston Prime's fraction was wrong for months, caught only
  because a player measured it (MEASUREMENTS M48). The row used to appear only
  once a CO card was on the build, so the one thing a reader could check was
  invisible until they had already committed to the mod; it is unconditional
  now and says "no source equipped" plus how one WOULD be computed (owner,
  2026-08-16). It is a STATEMENT OF METHOD rather than an admission — the
  disclosure banner is for what the sim cannot do, this is what it does, said
  out loud so it can be argued with. The check walks all three behaviours from
  three weapons the catalog classifies differently and asserts they are three
  different sentences, so a page printing one of them for everything fails.
  `node scripts/check_opt_replay.mjs` is the THIRTIETH, and the only one written
  so that it CANNOT GO STALE. Every other check about a build names the axes it
  is about; this one asserts the ANSWER — it runs a real search, applies the
  winner through the button's own path, runs the simulator, and asserts the two
  numbers agree inside 4σ of their two standard errors. It does not know what an
  axis is, so a fifth one is covered on the day it is added, by nobody. Its
  rotation of NEGATIVE CONTROLS is discovered from the row's own `replay` keys:
  each is deleted in turn, the ones the engine notices are named in the
  assertion's own title (a check that quietly exercised one axis of five reads
  exactly like one that exercised all five), and a degenerate axis is REPORTED
  rather than failed — the Kuva Nukor's single firing mode is not a wiring
  fault. The sharp one is last: a build assembled from a replay with a LIVE axis
  removed must fail the very assertion that otherwise passes, which is what
  proves the assertion can fail at all. Two weapons, because no single one has
  every axis live — the Nukor for the progenitor element, the Torid for modes
  and evolution tiers. Verified to bite: reinstating the "+ add" bug takes the
  Nukor from 0.6514 to 0.2118.
  `node scripts/check_build_axes.mjs` is the THIRTY-FIRST and the cheap half of
  that pair: `engine::builds::BUILD_AXES` is the one declaration, served at
  `/api/meta.build_axes`, and the three JS surfaces that carry their own
  spellings of it — the page's build state, the share tuple, the worker's board
  record — each declare which axis their fields cover. It asserts the coverage
  both ways (an id the engine never heard of is a rename that happened on one
  side, which reads as coverage and is not) and that the worker's record and
  identity key are still DERIVED from its table rather than re-grown as hand
  lists. Plain node against the served meta and two source files, so it costs no
  browser beyond the meta fetch. It exists for the surfaces an answer cannot
  reach — a share link nobody has clicked, a board record nobody has submitted —
  and says in its own text that it is the weaker half. Verified to bite: a fake
  axis added in Rust reddens all three surfaces, each naming it.
  `node scripts/check_riven_pool.mjs` is the SEVENTEENTH: the riven editor
  offers the stats that weapon's rivens actually roll, in BOTH slots. What a
  riven can roll is DE's per-weapon table, published nowhere, and the wiki's
  25%-of-a-physical-type rule disclaims itself. THE RULES DECIDE AND THE SURVEY
  CHECKS (owner, 2026-08-08): `rivens_data::derived_for` is the model,
  `data/rivens/exceptions.yaml` overrides it per riven FAMILY with the evidence
  written into each entry, and `data/rivens/pools.yaml` (from
  `scripts/survey_riven_pools.py`) is read by a TEST and by nothing else. It
  was the other way round for a day and a re-run of
  the scrape came back "nothing rolls anything" for all 26 families, wrote itself
  to disk, and was caught by two unrelated tests — see DATA_SOURCES §"Riven
  pools" (MEASUREMENTS M35). It walks the NEGATIVE slot too, because
  that is where the report came from — a player's Furis riven carries Projectile
  Speed and the editor would not offer it (owner, 2026-08-08).
  `node scripts/check_enemies.mjs` is the ELEVENTH: every
  TARGET in the roster shows a picture that loads, a wiki link built from its
  ENGLISH name (it runs the whole pass twice, in both languages — a localized
  name in a wiki URL lands on garbage), its VULNERABILITY COLUMN (the Thrax's
  Void ×1.5 reaches the card only through `faction_damage_override:`, which
  serde was discarding until the column was implemented), and a statement of
  what the sim does not model about it. Enemy art is declared in the enemy's own YAML
  (`image:`, wiki-hosted), NOT in `data/assets.yaml` — WFCD has no usable
  enemy art — so it reaches `site/img/` by a different path than a mod card
  and needs its own check. `node scripts/check_search.mjs` is the TENTH and the
  end-to-end one: it runs a real optimize in the shipping wasm build and
  asserts the claims the search makes — a scope it finished reports
  `exhaustive` and says so on screen, a budgeted one reports its COVERAGE and
  does not pretend, and the WORKER FLEET covers more ground than one worker
  would (the browser shards the shuffled index range across Web Workers). `node scripts/check_gain_band.mjs` is the TWENTY-SIXTH: a
  quick-calc chip says HOW WELL IT KNOWS its own number, and never prints a
  zero. "≈0%" was one string for two different findings — a mod that does
  nothing, and a mod nobody measured hard enough — and only the difference
  between them is actionable: the first says pick something else, the second
  says raise the runs (owner, 2026-08-12). Both halves of the machinery behind
  it were wrong, and either one alone brings the symptom back. The scan read
  `score`/`dps`, which are the MEDIAN RUN — one engagement however many were
  paid for, moving 9.8% between seeds at 10 runs
  where the mean of the same runs moves 5.9%, and not even the statistic the
  optimizer ranks (`mean_kill_progress`). And it estimated its own resolution by
  running the reference a SECOND time at another seed: one sample of a spread,
  which on identical inputs answered anywhere from 0.7% to 11.2% — so the same
  scan censored every chip or none of them, at random. The server has all N
  runs and now reports what it already computed (`score_mean`/`score_se`,
  `dps_mean`/`dps_se`), which is one fewer simulation per scan and an answer
  that does not depend on a coin flip. THE WIDTH IS THE COMPARISON'S OWN, and
  it is DERIVED: `/api/simulate` returns the per-run series when the caller says
  it will pair with it (`run_series`), and the chip's band is the spread of
  `c_i - ratio*b_i` over those runs. It used to be a PROXY — had the MEDIAN
  run's proc count changed? — because two `mean ± sigma/sqrt(n)` summaries
  describe each build alone and cannot answer it; the proxy fails in the
  direction that matters, saying "same fight" whenever the count coincides. All
  seven of the Kuva Nukor's progenitor elements report 6079 while their fights
  differ by up to 30%, so seven chips claimed an exactness none had (owner,
  2026-08-14). A chip therefore has three shapes and the check asserts all three
  OCCUR, so no branch is dead: an exact one (`+165%` — every paired difference
  zero, so the candidate scaled the same engagement run for run), a banded one
  (`≈+3.1% ±7.2%`), and a measured zero, which says "no effect here" in words
  and points at the row's own disclosure line — a third of a rifle pool lands
  there against one standing target (ammo and magazine mods, Firestorm,
  punch-through, recoil and zoom, Cautious Shot, a Bane of the wrong faction),
  and printing "+0.00%" 38 times reads as a broken scan rather than as
  UNMODELLED.md. AND A COIN FLIP LOOKS LIKE ONE: the chip answers "what is this
  worth", the LIST answers "which do I pick" by sorting, and a sort produces an
  order where there is none — so an option not SEPARATED from the leader (the
  gap under the two bands combined) is marked `tied`, on the leader too, since
  neither is above the other. Its NEGATIVE CONTROL is the pair the bug came in
  on: Serration and Amalgam Serration differ only in base damage, so run for run
  they scale this fight by a constant, both band to exactly zero, and the order
  is the one the cards state — measured 0.9623 = 2.55/2.65 at every build
  strength and run count. The 3.8% gap between them is far inside the ±13% raw
  spread at 10 runs, so that ordering survives ONLY because the two are paired
  against the same luck, which is the thing that must not silently regress.
  `node scripts/check_board_submit.mjs` is the TWENTY-SEVENTH, and the only one
  that tests the WORKER: a build reaches the board through the page, the worker
  and the scorer, and the middle hop had no test at all — which is where builds
  were being lost, TWICE, the same way. `mode` was sent and never written down,
  so every Incarnon weapon's row said `cycle` (2026-08-09); then `valence`, and
  seven Kuva Nukor submissions were refused on every scoring run since they
  arrived while the panel had told each submitter "sent" (owner, 2026-08-14).
  The second one is the sharper case: `/api/board/check` had already approved a
  payload CARRYING the element, and the field was dropped after the verdict, in
  the one hop neither the engine nor the page can inspect. So the check asserts
  the PROPERTY rather than the two fields — every key the page sends survives
  into storage, and two builds differing in any one axis are two records rather
  than a silent overwrite. It runs in plain node against a KV stub, so it costs
  no browser — which is why it is the second check in CI beside the parity one,
  rather than something to remember to run. Its FIRST assertion is a CROSS-FILE
  one and is what would have caught both losses on the day they happened: the
  axes `boardPayload()` actually emits, read out of `app.js`, must be exactly
  the axes the worker's own `AXES` table knows how to keep. That table is the
  other half — validation, storage and the identity key are all DERIVED from it
  now, so an axis can no longer be added to two of the three, which is what
  happened both times. Verified to bite: removing `valence` from the table
  fails four assertions, naming the axis.
  `node scripts/check_mode_def.mjs` is the TWENTY-EIGHTH: a MODE is EXPLAINED,
  not just named, and its name is DERIVED. The Mode control was a dropdown of
  names, which is enough while every weapon's second mode is the same mechanic
  and stopped being enough the day two weapons earned a form by KILLING rather
  than by hitting (owner, 2026-08-15) — "cycle" does not say what fills the
  gauge, how many it takes, or what the earned form gets to fire, and those are
  the numbers that decide whether to pick it: a Torid pays 5 direct hits for 170
  rounds, a Mausolon pays 5 KILLS for one. Each sentence is a TEMPLATE with
  `{named}` holes filled from `/api/meta`'s forms, so a weapon that arrives
  tomorrow explains itself and costs no translation. The other half is the NAME,
  and the check carries a MATCHED PAIR because neither direction passes alone:
  the Mausolon and the Cortege must not be told they have an Incarnon anything,
  and the Torid and the Lex, which do, must still say so — a check asserting
  only the first passes just as well on a page that dropped the word entirely.
  It runs in BOTH languages, since a hole filled into an untranslated template
  is invisible in English and is half an English sentence on a Chinese page.
  Verified to bite: restoring the hardcoded `tr("Incarnon cycle")` fails four
  assertions, naming the weapon and the language.
  `node scripts/check_gain_freshness.mjs` is the ninth: a
  scenario edit reaches the quick calc immediately, including a field nobody
  has invented yet — the scan's cache key is DERIVED from the fight it will
  run, never a hand-listed copy of it. `node scripts/check_build_size.mjs` is the eighth: how full
  a searched build must be is a RANGE (`build_min`–`build_size`), so "exactly 8
  mods" is a setting rather than something the scope cannot express — both ends
  push each other, both ride the search preset, and both reach the request.
  `node scripts/check_buff_cards.mjs` is the seventh: buff
  cards are named in the display language (an EVOLUTION's buff was the last one
  left in English), open at the stack count the rule says, and report a
  coverage that is never rounded up to a flat 100%. It also walks the one buff
  that is a WEAPON PASSIVE — the Ocucor's tendrils, which its only augment
  scales with — because a stack count nobody can set is a mod nobody can
  measure: a tendril costs a kill, so against a target that dies slowly the
  card is the whole measurement (player report, 2026-08-08). See BUFFS.md
  §"A buff whose end is an EVENT".
  `node scripts/check_gain_axes.mjs` is the sixth: the
  quick-calc gain scan obeys the evolution TIER LADDER, so it never ranks a
  perk the builder will not let you click. `node scripts/check_replay.mjs` is the fifth: the
  median engagement plays back on screen — the buff curves draw, scrubbing
  drains the pools, and play advances the clock at the chosen multiplier.
  `node scripts/check_preset_independence.mjs` is the
  fourth: it asserts no collection's state is written from outside it —
  switching a build must not move the fight, and editing the fight must not
  touch a build. `node scripts/check_share.mjs` is the second: it opens a
  share link in a browser that has never seen the build and asserts what is on
  SCREEN, not what is in the variables — that distinction is the whole reason
  it exists, since the path has twice landed the data correctly and shown an
  empty page. `node scripts/check_tenno.mjs` is the third: the fight's PLAYER
  reaches the panel, the sim and a share link, so an arcane that scales off a
  Warframe is worth nothing with no frame and +500% with one.

## Hard rules

- **THE SIMULATOR IS THE TRUTH; THE OPTIMIZER OBEYS IT** (user, 2026-08-04).
  A search's winner is replayed under the simulator's fight, so any rule the
  optimizer applies that the simulator does not — or omits that the simulator
  applies — scores builds nobody can reproduce. The two must not be two
  implementations that agree;
  the optimizer must CALL the simulator's, and add only its own scope and
  budget.
  DONE 2026-08-04 (`parse_fight`): the fight is parsed once and the optimizer
  calls it. `simulate_json` reads `replay` and nothing else; `parse_optimize`
  reads `build_size`, `build_min`, `finalists`, `final_runs`, `deployment` and
  nothing else. Neither builds a second Tenno. Anything that is a property of
  the FIGHT goes in `parse_fight` — adding a scenario field to one module and
  not the other is no longer possible, because there is only one module that
  reads them.
  Measured before that: the two parsers read **9 of the same request fields**
  and call **10 of the same 11 helpers**. The optimizer's extra five are all
  scope (`build_size`, `build_min`, `finalists`, `final_runs`, `deployment`) —
  and the ONE helper it did not share, `chosen_evolutions`, is where the
  divergence bit. Three times, all the same shape: the form-unlock fallback
  (2026-08-04), a caller that omitted `evolutions` getting the Incarnon cycle
  free while the optimizer scored the base form (2026-08-03), and the
  optimizer keeping a buff config of its own (2026-08-02). A shared helper is
  not enough — the DECISIONS around it have to be shared too.
- **A FINGER SCROLLS; IT DOES NOT DRAG THE FIGHT** (owner, 2026-08-18).
  A browser decides who owns a gesture at `pointerdown` and never gives it back,
  so a body that drags on touch means the finger that started on it can no
  longer SCROLL. A 19x19 formation covers the canvas in bodies, so on a phone
  almost every scroll past the arena dragged an enemy instead — the fight moved
  silently and the result it had just produced was for a fight nobody was in any
  more, which reads as "I tapped and it made me simulate again".
  A LONG PRESS CANNOT FIX IT: once the gesture is the browser's it is gone, so
  claiming it later is not something a page can do. The answer is a MODE the
  reader turns on — a ✥ chip in the scene's own control row, off by default,
  drawn only where `navigator.maxTouchPoints > 0` (a touchscreen laptop reports
  a FINE pointer and has the same problem, so a `pointer: coarse` query is the
  wrong test). `touch-action` follows it: `pan-y` off, `none` on. A MOUSE is
  unaffected — it has no scroll to lose.
  `check_mobile.mjs` asserts it at every phone width, and it had to be taught to
  have a finger first: `mobile: true` on `setDeviceMetricsOverride` leaves
  `maxTouchPoints` at 0, so every touch-only behaviour was going untested on the
  one check that is about phones.
- **A SIMULATION RUNS ON A WORKER FLEET** (owner, 2026-08-18). The runs are
  INDEPENDENT given their index, so the page shards them across one worker per
  core (capped at eight, the quick calc's rule) and the shards merge back into
  exactly what one worker would have produced. Measured on the group-clear
  ruler with the board's own #1 Phantasma Prime build: **85.7 s -> 18.3 s**.
  THE ENABLER IS THE SEED. Each run's dice are now a pure function of
  `(seed, index)` rather than one Rng chain threaded through every run — which
  is what made a run impossible to start without replaying everything before it.
  Still reproducible; what moved ONCE is the SAMPLE, measured at 0.001% to
  0.09% across `one_fight`'s three shapes against a Monte-Carlo standard error
  of about 0.3% at a thousand runs.
  THE MERGE IS IN RUST, so there is one implementation of the arithmetic: the
  page schedules and collects, `simulate_merged` computes every field. A
  `Shard` carries SUMS rather than runs — 24 KB at a thousand runs against 8 MB
  — plus one `(effective, rng_state)` per run, because the MEDIAN engagement is
  what the panel shows and finding it means ranking every run. The merge ranks
  those and REPLAYS the winner.
  **A JSON NUMBER IN JAVASCRIPT IS A DOUBLE**, and that cost an evening: the
  64-bit RNG state came back ROUNDED across the wasm boundary, so the merge
  replayed a fight that never happened. Every mean matched to the last bit and
  only `score` disagreed, because `score` is the one figure taken from the
  median run. It travels as two `u32` halves now (`RunKey`).
  Asserted three times: on the summary (`eight_shards_are_one_run`, 23 fields
  plus the median), on the whole response (`a_fleet_of_shards_reports_what_one
  _worker_reports`), and ON THE WIRE in `check_run_counts` — which is the only
  one that could have caught the rounding.
  A COMPARISON IS TO A PART IN 10^12, not bit for bit: adding in eight groups
  and combining differs from one sequence in the last bit, because
  floating-point addition is not associative.
- **A LONG SIM SAYS HOW FAR IT HAS GOT** (owner, 2026-08-18). The run count is
  unbounded and so is the cost per run: a single-target fight is about a
  millisecond, a 361-body one is ~28 ms, so the rulers' 1000 runs is half a
  minute. It has always run on a WORKER, so the page was never frozen — but a
  button reading "Simulating…" for half a minute is reported as a hang, and it
  should be. `simulate_progress` is the wasm entry (its own, not a flag on
  `api`, because `/api/simulate` is the one endpoint whose cost is unbounded),
  the worker forwards `{done, total}`, and the panel draws a bar, THE COUNT and
  a time remaining. The count because "412 / 1000" is a number a reader can act
  on where a bar alone is only a feeling; the time because that is the number
  they actually want. THE ANSWER IS UNCHANGED — the callback observes and never
  steers — and the throttle is in the WASM layer at one message per percent,
  because a postMessage per run would cost more than the fight.
  The remaining time is hidden below a second and before 5%: an estimate off
  one or two runs extrapolated a hundredfold reads as a wild guess and is one.
- **EVERY ENEMY HAS A NAME, AND THE PAGE CAN ASK ABOUT ONE** (owner,
  2026-08-17). Debuffs have been per enemy since the formation landed —
  `SpreadFoe` is `{state, debuffs}` per body, with its own pools, armour, stack
  counts and DoT list. What no part of the model could do was say WHOSE: a body
  was identified by its INDEX in a request, so deleting the body in front
  renumbered everything behind it. `formation::FoeSpec::id` is that name, stable
  across edits because it travels in the scenario and filled in BY POSITION when
  blank, which is what every scenario written before ids existed means. The
  aimed body is `e1` and lives on the `Arena` — it is not in the formation list,
  it is the fight's own target — so the crowd reads as one list however it was
  assembled.
  `/api/simulate` returns a ROLL CALL (`bodies: [{id, aimed, at, damage}]`) of
  the ones that took something, because a 19x19 ruler is 361 bodies and a
  chaining beam reaches thirteen — and because a per-BODY figure is the only
  thing that can say a crowd was REACHED rather than a big number produced.
  SETTING UP A FIGHT AND READING ONE ARE TWO THINGS (owner, 2026-08-17), so
  the RESULT panel draws its OWN copy of the scene. The scenario's canvas is
  where a body is PLACED — draggable, with its distance shortcuts and its
  +1/+8; the result's is read-only, shaded by what each body TOOK, marks the one
  being examined and PICKS rather than drags. Neither is the other's control and
  neither can be mistaken for it. `mountArena` takes `heat`/`selected`/`onPick`
  for the second kind and draws no editing controls at all for it.
  A BODY IS NAMED WHERE IT IS CREATED (`nextFoeId`), one past the highest ever
  used rather than one past the count — so deleting a body never hands its name
  to the next one, and a roll call, a heat map and a debuff table cannot end up
  about different enemies.
  THE DEBUFF TABLE HAS A SUBJECT. `Replay::tracked` names the bodies it
  followed, the panel draws a chip per body and picking one redraws the table
  from the stored result at no simulation cost. It follows the aimed body plus
  the hardest-hit few (`REPLAY_TRACKED = 8`) because a series is 600 frames x 15
  debuffs = 18 KB a body, so a 19x19 would be 6.5 MB — larger than the whole
  wasm — and the cap is SAID ON SCREEN ("+N more took damage and are not
  followed"), never applied silently: an absence would read as "that is
  everyone". One body draws no chips at all, so the fight this app ran until now
  looks exactly as it did.
- **THERE IS ONE FIGHT, AND EVERY MODULE SENDS IT** (owner, 2026-08-17). The
  PAGE's half of the rule above. The server's half has held since `parse_fight`;
  the page had none, and grew FIVE spellings of "the fight" — Run Sim's, the
  share card's, the quick calc's, the optimizer gain scan's, and the
  optimizer's. Each was right when written and none was right by the end,
  because a fight keeps GAINING fields (`custom_enemies`, a formation, an aim
  point) and each one reached whichever spellings somebody remembered.
  `theFight()` is now the only one, and THE LIVE `sim` IS THE FIGHT — not the
  preset behind it, which is a saved COPY that `applyScenario` seeds `sim` from
  and the auto-save writes back, so reading it over the top can only hand back
  something staler.
  The quick calc had a SECOND SCENARIO POINTER of its own, persisted and sticky
  across weapons, scenarios and sessions. Build a nine-body Ocucor fight, switch
  the simulator to it, and every mod was still ranked under whatever that
  popover was last left on — an official single-target ruler, most likely, since
  that is where a first-time visitor lands. The mods that only pay in a crowd
  read as worth nothing and nothing on screen said why. Two controls for one
  fact, which is the arena's own rule (2026-08-16) in another module: the
  control that replaced it STATES the fight and cannot be picked from.
  THE ONLY THING A CALLER OWNS is `replay`, a `seed`, `run_series` and the quick
  calc's RUN COUNT — the reader's precision, not an edit to the fight, and the
  one axis deliberately decoupled. It lands LAST in the spread, which is the
  whole of that decoupling: it used to be written into the scenario object
  BEFORE the page's own count was spread over it, so the box silently did
  nothing while every chip's tooltip quoted it.
  THE BUFF MAP TRAVELS WHOLE, because buff settings are the FIGHT's and it is
  the BUILD that decides which have a source (the server's `BuffCfg` is a
  lookup, so an entry nothing grants is never read). Pruning it to the current
  build made the quick calc a different fight the moment a candidate granted a
  buff the current build lacked — which is every candidate worth ranking.
  A SWITCH IS THE OTHER WAY A FIGHT MOVES, and it has to RE-ASK rather than
  repaint. An EDIT re-ran the scan through `markScenarioDirty`'s debounce; a
  switch is a REPLACEMENT and goes nowhere near it, so the box was redrawn under
  the new fight's name while every chip beside it still answered the old one's
  question. `scenariosChanged` calls `refreshGains()` — that hook rather than
  the call sites, because it is already "the only thing every scenario mutation
  goes through", which makes it the one place a mutation added later cannot
  forget. `check_gain_freshness.mjs` asserts it on the EVOLUTION axis (the one
  that ranks with no picker open, so it tests the re-ask and not a repaint) and
  probes the scan's own BASELINE rather than a candidate's gain: a perk worth
  nothing under both fights is worth nothing under both fights, which is true
  and is no evidence anything was re-measured. Verified to bite — the baseline
  comes back byte-identical.
  `node scripts/check_storage.mjs` is the THIRTY-FOURTH and the only one about
  how much room the app takes on the READER's machine — see the hard rule
  "A MEASUREMENT COSTS ITS SUMMARY, NOT ITS REPLAY". It measures the ratio
  rather than asserting a constant, fills the disk from OTHER weapons' keys to
  prove the shed sweeps the origin, and plants a replay written under the old
  rule to prove the boot takes it back. Its second assertion is the one that
  keeps the fix honest: the panel must STILL DRAW a replay, because removing
  the feature would pass the first assertion perfectly.
  `scripts/check_one_fight.mjs` is the THIRTY-THIRD check and HOLDS NO LIST OF
  FIELDS: it asserts every module's outgoing request against `theFight()`
  ITSELF, so a field invented tomorrow is covered by nobody. Verified to bite —
  reinstating either old bug reddens it naming the field. Its weaker partner is
  `check_run_counts.mjs`, which reads the box; this one reads the wire.
- **A MOD POOL A WEAPON CLAIMS MUST HOLD SOMETHING** (2026-08-18). DE tags a mod
  PRIMARY, Rifle, or narrower still — Assault Rifle, Bow, Sniper — and a weapon
  draws every tag that applies to it, which is why `mod_pools:` is a LIST. The
  failure mode is silent in a way a missing mod is not: a pool a weapon
  DECLARES and no `data/mods/<pool>/` holds resolves to an empty list, with no
  error anywhere. Nine bows carried `[primary, rifle, bow]` from the day the
  roster began and `data/mods/bow/` did not exist, so Split Flights — the only
  multishot mod a bow can hold — was unreachable; fifteen snipers carried no
  `sniper` tag at all, so both Chambers were. No earlier sweep could see it,
  because every one of them asked about mods we HAVE.
  `scripts/survey_pool_mods.py` is the answer and it works from the ROSTER: every
  tag any weapon claims must map to an export `compatName`, and the script
  REFUSES to run when one does not. Its `data/surveys/pool_mods.yaml` is read by
  a ratchet test that also asserts, per weapon, that each claimed pool holds at
  least one mod — verified to bite. It is the sibling of
  `survey_weapon_mods.py`, which joins the same field against WEAPON NAMES;
  between them the export's compatibility column is fully swept. Adding a mod
  starts there: run the survey, take a row, transcribe it from the WIKI, lower
  the ceiling.
- **AIM IS DRAGGED, AND A PICK READS NOTHING** (owner, 2026-08-18). Two
  gestures on the result and the scene that were doing more than they were
  asked to.
  A BARE CLICK NO LONGER AIMS. It did, on the reasoning that a body is dragged
  and a place has nothing to grab — the cost was that every mis-click while
  selecting silently re-aimed the weapon, and a fight that moves on a mis-click
  makes the result on screen a result for a fight nobody was in. It clears the
  selection now. That leaves a fight whose aim has never moved with nothing to
  pick up, since the marker rides the target and the body wins the grab, so the
  rail got an AIM TOOL: an explicit verb, after which the marker drags in Select
  like anything else with a position.
  AND PICKING AN ENEMY IN THE RESULT READS NO STORAGE. It called
  `renderStoredSimResult`, which looks the run up in a preset collection — so a
  pick was a bet that the SAVE had worked, and every way it could not have took
  the result off screen. Fixing storage fixed one of those and the report came
  back unchanged: an active preset that is not in the list is another, and
  `saveSimResult` returns early on it having stored nothing anywhere. `renderResults`
  records `shownResult` and a pick redraws THAT. Storage is what survives a
  reload or a preset switch, and nothing else asks it a question.
  HARDEST HIT FIRST, in both views. `tracked` came back in the engine's
  slot order, which answers "who got followed" rather than the reader's "who
  took the most". The display order is sorted; `data-rpfoe` stays the index into
  `tracked`, because `dstacks[k]` is that body's series. Each chip carries the
  number it is sorted by, since an order nobody can check is an order nobody
  trusts.
  A UNIT IS A COLOUR, derived and never declared: `unitHue` hashes the id (FNV-1a)
  into a hue, so a formation of several units reads without clicking through it,
  a unit that arrives tomorrow already has one, and it is the same colour on
  every machine. The same hue is a swatch wherever a unit is NAMED — a key with
  nothing to read it against is decoration.
- **A BODY IS THE UNIT IT WAS PLACED WITH** (owner, 2026-08-18). The card on
  the left of the arena says what you are ABOUT to place; it is not a control
  over the floor. It was one, silently: a placed body carried no unit of its
  own and the server reads a blank one as "the aimed body's" — the right
  default for a scenario written before formations and the wrong one for a
  formation being built unit by unit — so reaching for a second enemy to place
  turned every enemy already down into it. Placing a Gunner line and then
  picking a Thrax destroyed the Gunner line, on the wire, with nothing on
  screen saying so.
  The unit is STAMPED at the moment of placement (`placeAt`, `arenaAddFoe`) and
  nothing afterwards moves it. `FoeSpec` has carried a per-body `enemy`,
  `level` and `eximus` since the formation landed; the page simply never filled
  them. The LEVEL deliberately stays the FIGHT's — it is one dial for every
  body on the floor and a ruler pins one number — so a body leaves it blank and
  follows.
  The AIMED body still follows the card, because that card IS the fight's
  target and the aimed body is the thing a placement copies. That split is
  invisible until somebody switches and watches nothing change, so the panel
  states it and counts what is holding a different unit.
  `check_formation.mjs` asserts all four halves ON THE WIRE rather than on page
  state, and bites: reinstating the blank reddens three of them, one of them
  reading `kept false`.
- **A MEASUREMENT COSTS ITS SUMMARY, NOT ITS REPLAY** (owner, 2026-08-18).
  Storage is the reader's machine and the app had no budget for it: a REPLAY is
  600 frames of debuff series per followed body plus a hit account per attack
  part — **65 KB against the 1.6 KB summary of every number a card, a share or
  the board ever reads, 42x** — and one was stored per WEAPON. About seventy-five
  weapons fill a 5 MB origin and the roster is 136.
  Past that the failure is not "storage is full". `setItem` THROWS, the throw
  lands in the save path of the run that just finished, and the reader is told
  **"sim failed: QuotaExceededError"** for a simulation that worked perfectly —
  or, from the other side, picks an enemy in the result and watches the result
  vanish, because the panel re-reads a collection the save never wrote.
  So a replay NEVER reaches the disk. Not "is shed under pressure" — never
  reaches it: `stripReplays` takes it out on the way to `localStorage` and
  `resultMem` (keyed by weapon AND preset) keeps it for the session, which is
  what makes the stored cost of a measurement bounded by its summary rather
  than by how hard it was measured.
  A SHED SWEEPS THE ORIGIN, not the list. A quota belongs to the origin and the
  first shed belonged to the collection being written, so a save for one weapon
  failed on space held by ANOTHER weapon's key, shed its own list to nothing,
  and still failed — while the room it needed sat in a key nothing would ever
  look at again. `shedOtherResults` walks every `wfsim-presets-*`, oldest result
  first, and the list being written is the LAST thing it may touch.
  AND WHAT IS ALREADY THERE COMES BACK: growth stopping is not space returning,
  and a reader at their quota fails the NEXT write rather than the one that
  filled it, so `reclaimStoredReplays` strips every replay written under the old
  rule on the way in. `scripts/check_storage.mjs` holds all four and bites —
  making `stripReplays` a no-op reddens two of them.
- **Golden values only change with an in-game measurement** justifying
  it. New mechanics need golden tests; a faithful-looking implementation
  without a measurement is not correct.
- **A RANKED ROW IS A BUILD YOU CAN RE-RUN, AND THE NUMBER ON IT IS THE
  SIMULATOR'S** (owner, 2026-08-16). The corollary of the rule above, and the
  half it never covered.
  "The simulator is the truth" was a statement about the ENGINE, and the engine
  holds: `parse_fight` sees to it, and a winner replayed under the fight it was
  scored in matches its row to 0.1%. The PAGE was not covered. It kept its own
  hand-written translation of a ranked row into a build, and dropped an axis out
  of it — a search won on Magnetic became a build fired on Impact, because
  `defaultValence` opens on the spec's first element. A player measured it: 26
  KPM on the ranking, 15 in the simulator, told it was the same build. For a
  product whose promise is "matches in-game measurements", a search that cannot
  reproduce its own answer is worse than a slow one.
  So the row stops DESCRIBING a build and starts CARRYING one. `entry()` emits
  `replay`: a complete simulate request, written by the same code that built the
  candidate, from the optimize request itself — so every field that reaches the
  optimizer rides along, including ones nobody has invented yet, and only the
  ranged axes are overwritten. POST it and you get the row's number, with no
  assembly anywhere. "+ add" applies it through `stateFromBuild`, the inverse of
  `buildPayload` and now the ONLY translation between a request and the page;
  the pair round-trips, which is a property one check asserts over every axis at
  once.
  AND THE RANKING REPORTS THE SIMULATOR. Each row is re-run through
  `/api/simulate` and the KPM on screen is what came back, with a ✓. The search's
  own figure keeps one job — ORDERING the list, since re-measuring cannot reorder
  a ranking without making the ranking meaningless — and the two are compared:
  4σ of the two standard errors combined, both of which the server reports, so
  "they disagree" is arithmetic rather than a tolerance somebody picked. A row
  that fails it is marked `≠` on screen. Any axis lost anywhere on the chain
  moves the number and trips it, which is the point of checking the ANSWER
  instead of counting the fields: it cannot go stale when an axis is added.
- **A BUILD'S AXES ARE DECLARED ONCE — IN THE ENGINE** (2026-08-16).
  `engine::builds::BUILD_AXES` is the list, served at `/api/meta.build_axes`.
  The SPELLINGS stay per-protocol (`arcane` on a request, `arcanes` on a board
  record, `arcaneRank` in page state) because renaming them would migrate every
  stored preset; what is shared is the list, and each surface declares which
  axis its own fields carry — `BUILD_STATE_KEYS` and `SHARE_AXES` in `app.js`,
  `axis:` per row in the worker's `AXES`. `check_build_axes.mjs` asserts the
  coverage in both directions, and bites: adding a fake axis in Rust reddens all
  three surfaces by name.
  Beside it, `buildState()` REQUIRES a value for every state key, so the five
  producers of a build state — the live page, "+ new", a board row, a share
  link, an optimizer result — must each name every axis. `undefined` stays a
  legal value meaning "the weapon's own default", because a blank build and a
  preset written before an axis existed both mean exactly that; what is no
  longer legal is not MENTIONING one.
  That distinction is the whole reason the bug was invisible. `restoreState`
  fills a missing axis with the weapon's default, which is RIGHT — and is why a
  producer that meant the default and one that never heard of the axis hand over
  the same object, with no consumer able to tell them apart. It happened four
  times, patched four times where it was found: `mode` missing from the board
  submission (2026-08-09), `valence` from the worker's table (2026-08-14), both
  from the share tuple (2026-08-15), `valence` from the optimizer's "+ add"
  (2026-08-16).
  A LIST IS THE WEAKER HALF and the file says so. It covers the surfaces an
  answer cannot reach — a share link nobody has clicked, a board record nobody
  has submitted — while the guarantee rests on `check_opt_replay.mjs`, which
  holds no list at all.
- **PUNCH THROUGH IS METRES OF MATERIAL, NOT FREE FLIGHT** (2026-08-17).
  *"The total distance of material (object or enemy) that a weapon's projectile,
  bullet or beam can pass through before dissipating"* — so a body costs a fixed
  `space::BODY_MATERIAL_M` and `space::struck_along` walks the aim ray spending
  it. THAT CONSTANT IS NOT TWICE THE RADIUS, and keeping the two apart is the
  decision: `BODY_RADIUS_M = 0.2` is MEASURED (M46, walking into an enemy stops
  at 0.4 m centre to centre) and governs spacing, the hit test and blast reach;
  `BODY_MATERIAL_M = 0.5` is PUBLISHED, by the wiki's "Minimum Mod Ranks for
  Penetration" table, whose thirteen humanoid cells are reproduced by that one
  threshold and which brackets it from both sides (0.4 fails on three
  independent mods, 0.5 works on Vigilante Offense). Raising the radius instead
  would overwrite an in-game measurement with a table whose own note says
  *"Average data, result will differ due to width variances"*, and move every
  distance-dependent number on the board for the privilege — while the property
  that motivates the question holds either way: 0.5 m reaches the SECOND of two
  adjacent enemies.
  A PUNCHED BODY IS A DIRECT HIT — full damage, multishot, and it may HEADSHOT
  (owner, 2026-08-17) — and on a chaining weapon it STARTS ITS OWN CHAIN, which
  is the wiki's own rule: *"Each enemy hit by the main beam from Punch Through
  can generate a new set of 3 chains"*, independently, and *"the chain from the
  target hit after the Punch Through can deal damage to the first target, and
  vice versa"*. `chain::resolve` takes the struck bodies as its seeds and each
  keeps its own `seen`, so that falls out rather than being arranged.
  AN AoE ATTACK TAKES NONE OF IT, from its weapon or from a mod — both halves
  are on the page, and it means a Shred on a grenade launcher is worth literally
  nothing. "An area of effect component" is BOTH shapes the engine models,
  `radial` and `lingering`: the Torid is the second and carries no `radial:`, so
  a rule naming only radials would have let it take Primed Shred. THE SHAPE IS
  ONLY THE FALLBACK, though — `punch_through_mods:` on an attack overrules it,
  and the Torid's INCARNON form is why the field exists: a beam with a damage
  radius, so neither `radial:` nor `lingering:`, whose own page says *"Punch
  Through mods have no effect on the behavior of the beam"*. The family does not
  settle it either, which is the sharp part — the wiki sentence that groups this
  weapon with the IGNIS for Primary Compression groups it with a weapon on the
  punch-through page's EXCEPTION list. Two weapons in one group for one
  mechanic, opposite sides of another; so it is transcribed per ENTRY.
  `punch_through_m` had sat in all 224 entries unread since the roster began —
  the honest place for it while the arena had one body — and the 22 weapons that
  admitted it as a gap admit nothing now. MECHANICS §13 is the whole of it.
- **A GAP THAT REPEATS IS A REASON, NOT A SENTENCE** (2026-08-15).
  `data/unmodelled/reasons.yaml` holds each one once, with `{named}` holes, and
  a weapon references it: `- reason: innate_punch_through` / `m: 1.2`. The
  audit that produced this counted 116 distinct admission sentences over 248
  uses with thirteen families of near-duplicates inside — SIXTEEN spellings of
  the damage-falloff line differing only in three numbers, ten of the
  punch-through line differing in one. The cost was not the bytes: every new
  spelling was a new string somebody had to translate, so the zh overlay grew
  with the ROSTER rather than with the ideas in it. Eleven reasons now cover
  155 of the 248 uses; a weapon whose falloff starts at a new distance costs
  ZERO translation. PROSE IS STILL RIGHT for a gap that happens once and needs
  a paragraph — 61 of them still are — and a free-text parameter is not allowed
  (it would carry English into every translation; the Cortege's "a grenade"
  line stayed prose for exactly that reason). The i18n counter asks for the
  TEMPLATE, and `trGap` in the page fills the same holes into whichever
  language the reader is in.
- **A FORM INHERITS ITS WEAPON** (2026-08-15). 88 of the roster's entries are
  form siblings rather than weapons, and a form states its ATTACK plus only the
  weapon-level fields that actually DIFFER — `inherits: <parent_id>` fills in
  the rest (`weapons_data::INHERITED`). It is not tidiness: before it, 313
  identical values were written twice and a real error was hiding in the noise
  — the ordinary Larkspur's alt-fire carried its BASE form's accuracy while its
  Prime's carried the alt-fire's, and nothing could catch it because nothing
  knew the two entries were one gun. Two guards hold it: a form may not restate
  a value identical to its weapon's (a restatement carries no information and is
  the only way the two can drift), and a form that copies six or more of its
  weapon's fields must declare the inheritance instead. The ATTACK is never
  inherited, and neither is `co_behavior` (the catalog gives it per ATTACK — the
  Mandonel's two forms take different classes from two different rows) or
  `unmodeled:` (a form's gaps are its own; the Lanka's "the partial charge is a
  separate entry" is nonsense printed on the partial charge).
- **Data discipline** (`data/`): define once, reference by `id` (stable
  English slugs, never translated). YAML fields are consumed data;
  narrative/prose belongs in comments. Perks: define-once /
  reference-anywhere (see `data/README.md`); violations fail the build.
- **i18n is an overlay**: English is the source everywhere (code,
  comments, data, UI strings). A locale is a DIRECTORY of merged files
  (`data/i18n/<locale>/`: hand-written `names.yaml` + `ui.yaml`, generated
  `descriptions.yaml`); ids are never translated. Mod and arcane CARD TEXT
  is DE's own localized sentence per rank, never a phrase-substituted
  English line — substitution is the fallback for what DE never wrote.
  **A STRING IS TRANSCRIBED, NEVER TRANSLATED** (user, 2026-08-03). DE's
  Chinese is routinely non-literal — Commodore's Fortune is 准将沐福 — so a name
  derived from the English is wrong more often than not (five Boar Prime
  evolution names were translated this way and four were wrong). If a source
  cannot be reached, LEAVE IT
  EMPTY AND SAY SO. `python scripts/wfcd_i18n.py check` reports every
  unnamed id in every family and where its name comes from; `fill` only ever
  ADDS, so a deliberate divergence and the comment explaining it survive.
  Wiki URLs are ALWAYS built from the English name (`x.name_en || x.name`)
  — a localized name in a wiki URL lands on garbage.
- **No native dialogs in the UI** — `prompt`/`alert`/`confirm` are
  blocked in the owner's browser. Use inline inputs/feedback.
- **Absolute asset paths in the UI** (`/img/…`, `/pol/…`, `/logo.svg`):
  the SPA also loads at `/weapons/<Wiki_Name>`, where relative paths
  resolve into the SPA fallback's HTML.
- The page is THREE MODULES — Builder | Simulator | Optimizer — with one
  tab/view each, plus EDITORS that feed them. An editor is not a fourth
  module: it produces something the three consume, and it earns a tab only
  because it is too big to live inside one of them. Rivens is the first
  (`/weapons/<Name>/rivens`, decision 2026-07-31) — what it produces is a
  MOD, which is why a riven equips, searches, and gets optimized through
  the ordinary pool with no riven-specific code in any of the three. A new
  tab has to pass that test: name what the three do with its output, or it
  belongs inside one of them. Preset collections are domain-named
  `<owner>-<collection>`, where the owner is a module — or an editor, and an
  editor whose ENTIRE content is one collection is its own domain (`rivens`),
  because there is no second collection to tell it apart from. Every durable
  name (localStorage key, DOM id, label) derives from the domain. A preset
  belongs to ONE WEAPON, so the storage key also carries it
  (`wfsim-presets-<weapon>-<domain>`) — DOM ids and labels stay weapon-free,
  and copying a preset across weapons is the explicit "⇤ import" action, which
  drops per axis what the target cannot hold. URLs mirror English wiki page
  names (spaces → `_`); internal ids never appear in URLs.
- **PRESETS vs CUSTOMS** — two kinds of collection, and the difference is who
  CONSUMES them (2026-08-02). "Preset" is the CATEGORY and never the name of a
  collection or of an item in one: a build is a build, a scenario a scenario, a
  search a search, a riven a riven. Each bar declares its `noun`, which names
  new items ("build 2") and every tooltip that refers to one. A **preset** is a saved state of something that
  always exists, read only by its own module: `builder-builds` (a build),
  `simulator-scenarios` (a fight, buff settings included), `optimizer` (a
  search: the SCOPE and HOW to run it — finalists, final-round runs and CPU
  threads (never buffs: those are the fight's). The
  optimizer tab is two halves split at its two preset bars, with nothing on it
  belonging to neither: everything above the fight's bar is the search preset,
  everything below is the simulator's, read-only. The final round is
  `finalists × runs`, and the RUNS ARE THE SEARCH'S with the fight's as their
  default — blank means "the count the replay will use", a number means "search
  at this instead" (owner, 2026-08-11). The default is the fight's because a
  winner crowned at a precision the replay never used is a winner nobody can
  reproduce; the override exists because the simulator defaults to the rulers'
  1000 runs, so a wide scope's last round is `finalists × 1000` on top of
  everything before it, and "search cheaply, then measure the winner properly in
  the simulator" is a real way to work.).
  There is always ≥1,
  "active" means the state you are in, and the key is
  `wfsim-presets-<weapon>-<domain>`. A **custom** is a thing you MADE that the
  OTHER modules consume — `rivens` becomes a mod in the pool, `enemies` becomes
  an entry in the scenario's target list (owner, 2026-08-11; it was written here
  as a promise before it existed). A custom enemy is the SAME TYPE as a
  published unit (`EnemySpec`), which is what keeps the rest of the app
  ignorant of it: level scaling, the vulnerability column, body parts, Eximus
  legality and the target card all read the one shape they already read. Three
  things are its own: an inline `damage_modifiers` column, because a target
  nobody published may want a vulnerability no faction has; a
  `status_immunities` list, which is a DIFFERENT MECHANIC and not that column
  reading 0 (owner, 2026-08-11 — I had conflated them); and the fact that it is
  NOT weapon-scoped, for the same reason a fight is not.
  **DAMAGE IMMUNITY AND STATUS IMMUNITY ARE TWO MECHANICS**, and the wiki puts
  both halves in one paragraph (`Status_Effect` §Status Immunity Interactions):
  *"Proc type chances are not altered by enemy resistances or weaknesses to the
  damage components used in their computation; however, they are modified by
  enemy status immunities. When an attack procs a status effect on an enemy
  which is immune to a particular proc type, the respective damage type is
  excluded from proc type chance calculations for that enemy"* — and they are
  independent, "regardless of whether that enemy is also immune to Corrosive
  damage". So a x0 column changes what a hit DEALS and leaves the proc draw
  alone; a status immunity changes what it PROCS, by leaving the denominator so
  the other types RENORMALIZE onto the roll (the wiki's own example moves the
  other four from 18/5/9/23% to 33/8/17/42% when Corrosive drops out). The
  engine has done the renormalisation since `status::draw_proc_type` was
  written and cites the same section; what it had no way to hear was an enemy
  DECLARING one. Owning none is ordinary,
  each carries its own identity rather than a label you invented, and deleting
  one breaks references elsewhere (a riven delete clears the slot that equipped
  it — a preset delete can never do that). The mental model is a FILE: a list
  you pick from, one open at a time, none open being a real state — so the UI
  is a list + editor, NOT the preset chip bar, and the key is
  `wfsim-customs-<weapon>-<domain>` / `wfsim-custom-open-…`. Everything below
  the key is shared: storage, undo, per-weapon scoping, ⇤ import. The optimizer owns no scenario — it RUNS the
  simulator's, drawn by the same renderer over the same state, so the winner
  is scored under the fight the replay will run — READ-ONLY there, with a link
  to the simulator: a preset is edited in exactly one place, because two
  editors over one document is how it gets edited twice and saved once. That
  includes the BUFFS (user, 2026-08-02): the optimizer kept a scope-wide buff
  config of its own, so the two modules scored the same fight under different
  buffs and "add this winner, then Run Sim" only agreed because adding a winner
  silently copied the search's config into your scenario. The chain is
  builder → simulator → optimizer, each reading upstream and writing nothing. Its three old collections
  (`optimizer-mods` / `-arcanes` / `-evolutions`) merged into one: they were
  split for cross-weapon reuse, which is the import's job.
  **NOTHING CROSSES BETWEEN WEAPONS — EXCEPT THE FIGHT** (user, 2026-08-02;
  amended 2026-08-09). A BUILD, a SEARCH and a RIVEN are statements about ONE
  weapon and are never born from each other: a weapon opened for the first time
  gets a blank build, the search's `finalists`/`threads` reset, and the
  previous weapon's optimizer RANKING is cleared rather than left on screen
  under the new weapon's name.
  A SCENARIO is not a statement about a weapon, so it is SHARED across the
  roster — one list, key `wfsim-presets-simulator-scenarios` with no weapon in
  it (`SHARED_DOMAINS`), and switching weapons keeps the fight you are measuring
  under. The amendment narrows the rule rather than weakening it, and it became
  true rather than being decided: the last weapon-shaped thing a scenario
  carried was `mode`, and mode left the fight and joined the build on
  2026-08-07. The OFFICIAL rulers were always shared — one `single_target`
  applies to every weapon on the board, which is the point of a ruler — so a
  player wanting to measure their own roster under their OWN fight was the only
  one made to re-create it per weapon, which is the opposite of what a scenario
  is for (owner, 2026-08-09). The one weapon-scoped knob it still holds is
  headshot %, handled the way the rulers handle it: the SERVER forces 0 on a
  weapon that cannot headshot. A shared bar offers no "⇤ import" — there is no
  other weapon to import from.

  **NOTHING OUTSIDE A COLLECTION WRITES ITS STATE** (user, 2026-08-02). A build
  used to carry a `sim` snapshot that loading it then APPLIED, so picking a
  build silently rewrote the fight you were working in — and the scenario bar,
  whose whole job is to be the one place a fight is edited, moved under you.
  The field is gone: a build is a build, and the live scenario is seeded from
  the active `simulator-scenarios` entry and from nowhere else. Nothing is
  lost — "what this build was last measured under" was never that field's job;
  `lastResult.key` is that record, it lives outside `state`, and it is what
  makes a stale result show as stale. `scripts/check_preset_independence.mjs`
  asserts it in both directions.
  Every collection writes through `storePresetList`, which is what makes one
  Ctrl+Z stack cover all four — presets auto-save, so the way back is not
  optional.
  Customs are OPTIONAL by nature: nothing is auto-created, the last one can be
  deleted, and the editor stands down instead of showing a document that is not
  there. Presets are not — the modules behind them always have a state, and
  "no build" is not something the builder can show.
- **A SHARE LINK reproduces the whole thing** (2026-08-02):
  `/weapons/<Wiki_Name>?b=<code>` carries the build, the RIVENS it equips
  (a custom exists only on the machine that made it, so it must travel
  inline), the scenario it was measured in, and the measurement itself as the
  sharer's claim. Opening one creates a NEW copy of each — never a merge, never
  an overwrite — repoints the build's riven ids at the copies, strips the query
  so a refresh cannot import twice, and says what it dropped. The payload is
  JSON → `deflate-raw` → base64url behind a one-character version. The payload
  is POSITIONAL and omits everything derivable (defaults, max ranks, a buff
  left at its own default, the shape drafts a riven regenerates), which took a
  full share from ~865 characters to ~425. The card carries a QR of the same
  link — `qrMatrix` is a from-scratch encoder (byte mode, ECC L, mask 0),
  VERIFIED against a reference encoder's matrices and decoded back out of the
  rendered PNG by an independent decoder; three bugs in it (a reversed
  generator polynomial, transposed format bits, alignment patterns skipped
  where they cross the timing line) only showed up under that check. It is
  drawn at a FIXED 8 device pixels per module — measured, not chosen: at 4 the
  card only scans at full size, at 6 it survives a 0.66x shrink, at 8 it still
  reads at 1080px wide after JPEG 60, which is what a chat app hands back.
  The code's size is therefore an input to the layout, not an output of it. IDs travel as their own stable slugs, never as indices into a
  table: a table would have to stay append-only forever or silently reinterpret
  every link already posted. It rides the QUERY, not the fragment — a fragment
  never reaches a crawler and these links are meant to be posted. The card
  (`drawShareCard`, a canvas PNG to paste into chat) always carries the
  wordmark and the site's host.
- `api()` transport: `/api/meta` and `/api/i18n` are GET, everything
  else is POST — the native server matches on exact (method, path).

## Style

- English everywhere in the repo; all-lowercase commit subjects in the
  form `area: what changed and why it is right`, no AI attribution, no
  marketing copy — **with ONE exception: the home hero** (`.hero-h` /
  `.hero-sub`), which is allowed to make a bold claim (decision
  2026-07-31). It stays a CHECKABLE one: "true to in-game numbers, down
  to the last proc" and "Theorycrafting, solved" are the golden tests and
  the optimizer, stated. Adjectives that nothing backs are still out —
  bold here means specific, not loud. Everywhere else, still no
  marketing copy.
- The product name is **WFSim** (repo slug stays `wfsim`). The wordmark
  is two-tone: "WF" + gold "Sim". Logo: `web/src/static/logo.svg`. In
  Chinese PROSE the product is **WF模拟** (user, 2026-07-31) — the
  wordmark itself is never translated, so the topbar and `<title>` stay
  WFSim while the zh footer says WF模拟.
- Match the surrounding code's comment density and idiom; comments state
  constraints/sources, not narration.
- **A COMMENT NEVER QUOTES THE OWNER** (owner, 2026-08-13). A decision is
  recorded as the RULE plus who decided it and when — `(owner, 2026-08-11)` —
  never as the sentence he typed, and never in Chinese. The comment has to be
  readable by someone who was not in the conversation, and a pasted chat line
  is not. What a quote is FOR still has a home: an in-game report is a
  MEASUREMENT and goes verbatim into `docs/MEASUREMENTS.md`, which is the one
  file where the original words are the record rather than a remark about it.
  A Chinese string that survives anywhere else is a SOURCE — DE's own card
  text, a CN wiki line, a name — and those stay, transcribed and attributed.
