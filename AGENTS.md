# WFSim — agent guide

The ultimate Warframe **calculator** — builder, simulator, optimizer.
Core promise: **matches in-game measurements**. Rust workspace + YAML game data
+ a dependency-free web UI, deployed as WASM on Cloudflare (wfsim.app).

`docs/CORE.md`: "given weapon + mods + target + scenario, output damage matching
in-game measurements item by item, and search backwards for the optimal build" —
three verbs: **BUILD, SIMULATE, SOLVE**. Nothing else is a peer of them; anything
new either feeds one or reports from one.

## How this file is written

State the current rule and nothing else. Git holds what the rule used to be, who
asked for it and how it was found — a doc that retells that buries the one thing
a reader needs.

Four kinds of comment, and the test for any sentence is **"if I delete this,
what does the next agent get wrong?"**. Answer "nothing" → delete it.

| kind | contains | length |
|---|---|---|
| **constraint** | an imperative plus the consequence of breaking it | ≤3 lines |
| **evidence** | the value plus its source (wiki page, datamine, measurement id); verbatim quote when the wording is load-bearing | as long as it needs |
| **design** | the rule plus the one sentence that makes the wrong alternative visibly wrong | as long as it needs |
| **everything else** | — | delete |

Where each lives: **this file** carries constraints, one entry each, and points
at `docs/` for the rest. **`docs/`** carries evidence and design, organised by
topic. **Code comments** carry design at the decision and evidence beside the
constant. **Commit messages** carry history.

No attributions. A rule in a public repository has no author and no date; a
measurement has a source, never a person. `docs/MEASUREMENTS.md` is the one
exception — there the provenance is the data. `scripts/check_comment_style.mjs`
enforces this.

## Map

- `engine/` — all game mechanics. A fight has TWO actors and
  `engine::arena::Arena` is both of them (a `Tenno` from `data/tenno/`, a target
  with its hitboxes, a duration): the web api and the optimizer each build one
  from the same scenario and hand it to the same constructor, which is what
  makes a search's winner scored under the fight the replay runs. Every
  `condition:` on a mod card and every `kind: tenno_scaled` arcane is a question
  about that Tenno — MECHANICS §8. Every formula carries a comment citing its
  source. The engine knows NO weapon names: weapons/mods/etc. load from `data/`.
- `optimizer/` — build search (successive-halving funnel). It only ever calls
  the engine — never add a simplified damage formula here.
- `web/` — the native dev server (`cargo run -p wfsim-web`, port 8787).
  `web/src/static/` holds the UI (vanilla JS/CSS, no framework, no deps).
- `webapi/` — endpoint logic shared by `web/` (native) and `wasm/`.
- `wasm/` + `site/` — the static deployment. `site/` is **generated** by
  `scripts/build_site_app.py` — never hand-edit it.
- `desktop/` — WFSim as a Windows app, and an INDEPENDENT cargo workspace: it
  depends on nothing in `engine/`, because every simulation already runs in the
  wasm module the page carries. The main CI never compiles Tauri and the shell
  has no reason to change when the engine does, which IS the update strategy.
  Two layers with different costs: CONTENT (`app.js`, `pkg/*.wasm`, `img/`,
  `board.json`) changes every push and ships as FILES swapped by two renames —
  no installer, no UAC, no antivirus watching a program rewrite itself; the
  SHELL changes rarely and ships as an NSIS installer.
  It exists because Cloudflare from mainland China is the least reliable thing
  on the page, which is where the players are: measured from Shanghai, the
  5.43 MB wasm downloads at **2.11 MB/s** from wfsim.app and **9.73 MB/s** from
  a Tencent COS bucket; locally it is not a download at all — **~200 ms** to
  instantiate, times however many compute lanes.
  THE UPDATER LIVES IN `app.js`, not in the shell's injected script, and that is
  the one placement decision the whole thing rests on: an updater compiled into
  the shell cannot fix its own bugs, and every reader would be frozen on the
  broken version with no way out but a manual download. `app.js` is the thing
  updates replace.
  The channel is a SIGNED manifest over CONTENT-ADDRESSED blobs
  (`blob/<sha256>`), so a release uploads only what is new and a client fetches
  only what it lacks — measured at 0.8 KB, 1 of 764 files, for a one-file
  release. `private/wfsim_update_key` is the one unrecoverable thing in this
  project. See `docs/DESKTOP.md`.
- `data/` — versioned game data. `data/README.md` explains the reference graph;
  `docs/DATA_SOURCES.md` the sourcing rules. **THE WIKI WINS. Use it wherever it
  can answer** — WFCD's export (`vendor/`) is the CROSS-CHECK and the fallback,
  not a peer: its Arch-Gun entries carry the ARCHWING column of a two-column
  infobox, which is the wrong column for an arena on the ground, and it agrees
  with the wiki on every OTHER field of those weapons. An export cannot say
  "there are two of these and you want the other one"; a page can.
  **The ONE standing exception is `base_drain`/`max_rank` on MODS**, where the
  wiki is wrong for ~20 of them and WFCD is right.
  Still cross-check, and join the two by `internal_name` == `uniqueName`,
  **never by name** (WFCD has stale duplicates sharing a display name).
- `data/abilities/` — WARFRAME ABILITY BUFFS, the one data family that describes
  neither a weapon nor a build: a thing done TO this weapon for a while. It
  rides on the `Arena`, so `parse_fight` alone carries it into both the
  simulator and the search, and no board ruler sends one. The strength and
  duration are typed in today and come from the frame later, which is why
  `resolve` takes both as arguments. Four effect kinds, and the split is THREE
  MULTIPLIERS AND ONE INSTANCE: Roar/Eclipse/Nourish scale a number someone else
  computes, while `extra_hit` (Xata's Whisper) FIRES a second damage instance
  and therefore has to be told what triggered it — MECHANICS §7 §"Extra Hit" is
  the formula, MEASUREMENTS M40 the capture it decodes. See BUFFS.md.
- `docs/EXTRA_HIT.md` — ONE LAW behind four things built separately: a second
  damage instance beside a hit, worth a percentage of it, rolling its own
  status. Primary Debilitate's split, Cyte-09's Resupply, Xata's Whisper and
  Toxic Lash are members, each supplying only a percentage and an element. It is
  where the `f³` triple dip comes from, and where the 0% rule lives — an Extra
  Hit REPLACES the base its status burns off, so one that deals nothing leaves
  the level above standing.
- `docs/MELEE.md` — the first weapon family that is not a gun. **Each way of
  swinging a melee weapon is an independent BUILD**: a melee player picks one
  loop and runs it for the whole engagement, which is what
  `WeaponPlayMode::sustainable` already asks — so the four stance combos, the
  heavy attack, the slide and the heavy slam are seven `FormKind`s and seven
  MODES, and melee needs no new build axis.
  A MODE ID IS THE INPUT, NEVER THE COMBO'S NAME — a stance names its own combos
  and a different stance names them differently, so a name here would bake one
  stance into a durable id that every preset, share link and board row carries.
  A MODE'S NAME IS FIXED AND ITS STRENGTH IS NOT: the id is the input —
  `neutral`, `block_forward` — and so is what a reader sees; the stance changes
  what `neutral` is WORTH and never what it is called. The one question a stance
  slot exists to answer, which stance is best for the neutral combo, cannot be
  asked if the two builds call that mode different things.
  THE COUNTER DOES NOT MULTIPLY A NORMAL SWING, verbatim: *"Melee Combo
  Multiplier does not multiply the damage of your normal attacks. Instead, you
  can spend Melee Combo Count to perform Heavy Attacks, which deals between 2x
  and 12x damage."* So a combo mode's counter pays only through Blood Rush and
  Weeping Wounds — which read it and never spend it — and a heavy mode's is
  emptied by the swing that read it, leaving INITIAL COMBO (a floor refilling at
  40 points a second) as that build's whole engine.
  A TENNOKAI HEAVY BREAKS THE STANCE CHAIN, so the next light swing starts the
  combo over. It decides which swings ever happen: Raging Whirlwind is
  `400/200/300/500` and Discipline's Merit opens the window every FOUR hits,
  which is that combo's length — so under a restarting chain the 500% finisher
  is never reached.
  A STANCE IS THE FIRST MOD THAT CHANGES WHAT A WEAPON FIRES rather than what it
  fires with: it publishes a combo per FORM and installing one replaces the
  entry's own script, so the same Magistar in the same mode is a different
  sequence of swings under Crushing Ruin and under Shattering Storm (1,275
  against 1,162 DPS). IT NEEDS NO FIELD OF ITS OWN ON THE WIRE: a stance mod is
  legal in the stance slot and NOWHERE else, so a flat mod list can say which
  entry is the stance by looking at it. That is what the EXILUS slot cannot do —
  an exilus-eligible mod is legal in a main slot too — so that one travels in a
  field of its own and this one rides `mods`, appended.
  THE COMBOS COME FROM `Module:Stances/data`, the wiki's own Lua table: a swing
  that lands TWICE, a bonus to the Impact component ALONE (distinct from the
  forced Knockback proc several of the same swings carry), and the SLAM three of
  four combos end on. It confirms the DERIVED durations exactly — 3.00 / 2.60 /
  2.25 / 4.25 from the rendered table's two columns.
  MELEE'S CONDITION OVERLOAD IS THE ORIGINAL ONE AND IS UNCONDITIONAL — no kill,
  no stacks, no clock — so it carries `starts_full` and is NOT routed through
  the GALVANIZED family's path, which earns its payload on a kill and opens at
  zero. `starts_full` is NOT derived from `duration == NO_TIMEOUT`: LOCKING a
  buff card writes exactly that duration, and locking removes the expiry and
  nothing else — the count still starts where the card sets it. Two cards
  sharing a payload are still two cards.
  A CRIT CARD THAT SAYS `(x2 for Heavy Attacks)` CARRIES THE RULE ITSELF, not
  the bucket: True Steel, Sacrificial Steel and Galvanized Steel all print it
  and Blood Rush sits in the same bracket and does not. Same for the three cards
  that NAME an attack — Killing Blow on a heavy, Seismic Wave on a slam, Maiming
  Strike on a slide.
  TENNOKAI is the one melee mechanic that changes what the LOOP does rather than
  what a number is, and it is what makes the melee exilus slot a decision: when
  its window is open the next swing of a light combo becomes a HEAVY ATTACK —
  the class's multiplier in place of the stance's, times a combo multiplier it
  reads AND DOES NOT SPEND. A combo build climbs the counter to 12x with its
  swings and fires FREE 12x heavy attacks between them, which is why a 15%
  chance is worth nearly three times the build. Use the window the moment it
  fires. ALL SEVEN TENNOKAI CARDS ENABLE IT — every one opens with the same
  three words on its own card and only then says what else it does. The negative
  control is a build carrying none of them. The other four exilus cards are
  blocking and movement, which this arena has neither of.
- `docs/CATALOGS.md` — THE PER-WEAPON TABLES, in one place. Some mechanics are a
  formula plus a published table with one ROW PER WEAPON, and the row says what
  the weapon's own stats never would. Condition Overload and Primary Compression
  are both that shape. **The catalog is authoritative and absence means
  ORDINARY, not unknown**, and a row is transcribed for the entry it names
  rather than generalised to a class.
  **"ABSENCE" MEANS ABSENT FROM THE WIKI, NOT FROM THIS FILE.**
  `docs/CATALOGS.md` carries only the rows the roster already holds, so asking
  it about a NEW weapon can only ever answer no. Read the PAGE.
  `scripts/audit_condition_overload.py` is that check, executed; the compression
  half is `the_roster_reproduces_primary_compressions_published_column`, which
  re-derives the wiki's own bonus column from each entry's radius. The tool
  REPORTS a row it cannot place rather than skipping it, because the catalog
  names an attack the way that WEAPON's page does.
- `docs/UNMODELLED.md` — the EDGES, by reason rather than by perk: one target,
  no distance, no movement, no holster, infinite ammo, nobody shoots back, and
  the Warframe layer. It also holds the OPEN DECISIONS — things the engine could
  do today and does not because doing them means inventing a play pattern, which
  is the owner's call and not the model's (reload interruption is the live one).
  `python scripts/intake_report.py --full` prints the per-weapon list.
- `docs/` — CORE (design), MECHANICS (formulas), MEASUREMENTS (protocol +
  baselines), BUFFS, BOARD, OPTIMIZER, UI, WASM, GLOSSARY, DEVELOPMENT (setup),
  INVESTMENT (capacity/Forma), WEAPON_INTAKE, INCARNON, NAMING, DESKTOP.
- `tests/golden/` — golden tests calibrated against in-game measurements.
- `private/` — gitignored (devlogs, drafts, local assets, the `data/`
  verification scripts). **`git add -A` silently skips it**, so never report a
  change under `private/` as shipped, and never let something the repo needs
  live only there — put it in `docs/` (verification tooling is catalogued in
  `docs/DATA_SOURCES.md`).

## Build, test, verify

- Toolchain via mise (`mise install`); plain `cargo` works once installed.
  CI = `cargo clippy --workspace --all-targets -- -D warnings` +
  `cargo test --workspace` — run both before pushing.
- **Static files are `include_str!`'d into `wfsim-web.exe`**: after ANY edit
  under `web/src/static/`, stop the running server (it holds the exe),
  `cargo build -p wfsim-web`, restart. `cargo test` does NOT refresh the exe.
- **`data/` is embedded at COMPILE TIME too** (`engine::data::files_under`): a
  yaml edit — including `data/i18n/zh/` — needs the same rebuild + restart, and
  a site regeneration to reach wfsim.app. **This catches TESTS too**: cargo does
  not treat a yaml as a source dependency, so a test run right after a yaml edit
  reads the data compiled into the previous binary. When PROVING a check bites,
  revert the data, `touch` any `.rs` in that crate, then run.
- After frontend or engine changes, regenerate the static site:
  `python scripts/build_site_app.py` (wasm-bindgen-cli version must match
  Cargo.lock). Commit the regenerated `site/`. It also PRERENDERS one
  `site/weapons/<Wiki_Name>/index.html` per roster weapon (own
  title/description/canonical/OG + a crawler-visible summary the app removes on
  boot), plus `sitemap.xml` and `robots.txt` — without them every URL answers
  with the same contentless shell, which is a soft 404 to a crawler.
- **Images are SAME-ORIGIN, and the art ships with the site.** `site/img/` holds
  every file `data/assets.yaml` references (`scripts/fetch_images.py` fills
  `web/cache/img/`, `build_site_app.py` copies it and FAILS the build on a
  missing one). Hotlinking `cdn.warframestat.us/img/…` answers **301 →
  raw.githubusercontent.com**, which is unreliable to blocked from mainland
  China. If wfsim.app loads, its art loads.
  **A SIZE CLAIM IS MADE ON THE WIRE, NOT ON DISK.** Cloudflare answers `br`, so
  the raw byte count is not a number about any reader: a 6.7 MB wasm is
  **1,336 KB** downloaded. Judge a change by compressing both sides with the
  same brotli. `wasm-opt -Oz` takes 6.74 MB to 5.89 MB, which reads as 13% and
  is **-0.3% on the wire**, because it shrinks CODE and 59% of this binary is
  DATA. Not shipping the 43% of `data/` that is comments (`engine/build.rs`)
  moves it: 1,192 KB to 927 KB, **-22%**. wasm-opt runs anyway, for the 1.5 MB
  it takes off the blob this repo COMMITS every build.
  DE permits this: their Content Policy requires only that use of Warframe
  assets be non-commercial, and the wiki hosts the same files on the same basis.
  What it forbids is their LOGOS, so the only mark here stays ours.
  A `wiki:` prefix in `assets.yaml` means the CDN lacks that file and the FETCHER
  takes it from the wiki; the cached name and the page's URL are the bare name.
- Deploy = push to `main`: Cloudflare picks up `site/` automatically (~1–2 min).
  There is no deploy step in CI.
- **THE STORE IS A LIBRARY OF BUILDS, AND EVERY RULER CROSSES THE WHOLE OF IT.**
  A submission carries a BUILD and never a score; the number is produced by the
  scorer under the ruler's own pinned seed. So the ruler a build was measured
  under is provenance, not a gate. ANY FIGHT CAN UPLOAD, and the consent notice
  is ONE story everywhere: what leaves is the BUILD, not the fight, and nothing
  about you — the worker stores no IP, no token, and no time finer than the day,
  and a record expires after a year. From a fight of your own the page asks the
  door about EVERY ruler and reports "2 of 3 boards will take it"; it never
  predicts a SCORE. A new ruler costs no community effort — it is scored from
  the library the day it lands.
- **A RESCORE COSTS THE ROWS THAT READ WHAT CHANGED.**
  `engine::data_fingerprint` hashes what a row actually reads (its ruler, its
  weapon and every form it fires, each mod, arcane and evolution, plus
  everything no entity owns), the board stores it per row, and `--engine` is the
  CODE alone. Measured on 24 real rows: 26.0 s full, **0.075 s** when nothing
  changed, 2 of 24 for a Heavy Caliber edit, **0 of 24** for a whole new weapon.
  The one hand list (`AFFECTS_NO_NUMBER`) can only cost TIME — anything
  unclassified falls into the global bucket every row carries. Comments are
  free, since `build.rs` embeds each file with them stripped.
- **THE EXILUS SLOT IS OPTIONAL.** Beam range is exilus and IS modelled, but
  does not bind on the current rulers; `vile_precision` is −36% fire rate and
  takes an Ignis Wraith from 11.9694 to 9.3737 on the group ruler. NOT `full`:
  requiring one would publish whichever mod the dice favoured. IT TRAVELS IN A
  FIELD OF ITS OWN everywhere (wire, worker `AXES`, board row, fingerprint,
  `builds::identity`) because an exilus-eligible mod is legal in a MAIN slot, so
  a flat `mods` list cannot say which entry came out of the exilus slot.
- **THE BOARD STAYS A STATIC FILE, AND SAYS HOW FAR BEHIND IT IS.** Committed to
  the repo and served from the CDN, which is what makes it fast and unblockable.
  `GET /api/board/pending` answers the one fact the file cannot carry about
  itself: how many builds the library holds. A COUNT and nothing else. The
  scorer writes `submissions:` per board and the difference is a footnote,
  SILENT when the board is current.
- **NEVER RESCORE THE BOARD LOCALLY.** `.github/workflows/board.yml` rescores
  every stored row on any push touching `engine/`, `data/`, `webapi/` or the
  scorer, and the bot commits the result. `scripts/rescore_board.py --write` by
  hand holds the board yaml TRUNCATED while it runs, so the board tests go red
  and `site/` cannot be regenerated until it finishes — an hour of blocking at
  the rulers' 1000 runs. Use it WITHOUT `--write` to see whether a change moved
  anything; let the workflow write.
- **Engine COST: `cargo run --release --bin one_fight`**, and `-- save` first.
  It diffs a saved baseline and says whether the ANSWER moved — a moved answer
  is a non-zero exit, because an optimisation that changes a number is a bug.
  Read its table ACROSS: the default is four shapes and a change to the inner
  loop rarely moves them together (`target-cpu=native` is −23% / −36% / **+31%**).
  IT GRADES ITS OWN COVERAGE — the fourth shape is a Braton Prime, whose 60%
  SLASH is the one thing an elemental mod cannot combine away, and the tool
  FAILS when the whole suite burns nothing. `docs/DEVELOPMENT.md` §5 lists what
  has been tried and what it was worth.
- **`one_fight` COMPARES TWO BINARIES, NOT TWO MOMENTS.** Its baseline is a
  property of the machine on the day, and a day of driving headless browsers
  moves that machine. When a delta matters: `cargo build --release --bin
  one_fight`, copy the exe, `git stash`, build again, run them alternately
  against one baseline. The tool's noise column is measured in seconds and
  cannot see hours.
- **Optimizer verification: `cargo run --release --bin wfsim-truth -- pool=<ids>
  …`**. A search cannot vouch for itself: the tool exhausts the scope, evaluates
  every job flat, and reports where the production search landed in that
  reference ranking (rank / regret / recall / cost, and whether the reference
  reproduces itself under a second seed). It goes through `parse_optimize`, so
  it grades the app's own fight, and REFUSES a scope it cannot exhaust. Run it
  after ANY change to enumeration, scheduling or scoring. The cheap CI form is
  `optimizer/tests/search_accuracy.rs`. See `docs/OPTIMIZER.md` §Accuracy.
- **A CHECK CLEANS UP AFTER ITSELF.** Each `openApp` runs Chrome in its own
  throwaway profile under `%TEMP%`. `finish` kills the whole process tree
  (`taskkill /T` on win32), waits for it, and retries the removal; on Windows
  `kill()` reaches only the node that was spawned and Chrome's children hold the
  directory. `sweepStaleProfiles` deletes any `wfsim-*` older than an hour ON
  THE WAY IN, which is the only cleanup a run that throws, is interrupted, or
  never calls `finish()` can get.
- UI verification: drive headless Chrome over CDP (Node ≥22 has a global
  WebSocket; Chrome is at the default install path). Assert real DOM state;
  screenshots for layout review. `scripts/cdp.mjs` is the shared harness — a
  static server for `site/`, the Chrome launch, `evaluate`, `check`, `finish`.
  A check's page-side body is a TEMPLATE LITERAL: an unescaped backtick in it,
  including in a comment, ends the literal early.

### The checks

Each asserts a property of the shipping build. Run the ones a change touches.

- `check_page_bodies` — `node --check` over every check script. No browser; runs
  first in CI.
- `check_parity` — the builder and the optimizer offer the same options, the
  same visibility, the same ORDER, under the same numbers and names, on every
  axis. It asserts the property rather than a list (`orderOptScope` reads the
  order off the builder's own blocks) and SCRAMBLES the sections first, since
  markup authored in the right order would pass on a page where nothing orders
  anything. Run it after adding a weapon or anything a weapon can carry.
- `check_board_submit` — plain node against a KV stub, no browser. Every key
  `boardPayload()` emits, read out of `app.js`, is a key the worker's `AXES`
  table knows how to keep; every key survives into storage; two builds differing
  in any one axis are two records.
- `check_mobile` — GEOMETRY, not DOM: the page fits the screen at 360–1280px,
  nothing past the viewport, no sideways scroll, and a mod NAME keeps room to be
  one. It measures the page WITH A POPOVER OPEN, which is the one thing that can
  leave the viewport with no container noticing — `place` caps the width BEFORE
  the clamp, because a popover wider than the screen cannot be clamped into it.
  It sets `maxTouchPoints` itself, since `mobile: true` on
  `setDeviceMetricsOverride` leaves it at 0 and every touch-only behaviour would
  go untested.
- `check_equip_rules` — what a mod's CARD says the weapon may do, in both
  directions. An equip rule is asked of EVERY firing mode and installing a form
  ADDS one. The engine decides (`pool_for_build`), `/api/meta` states the
  consequence per evolution (`evo_forbids`), and the page acts on it: the picker
  stops offering it, installing the form unequips it and says so, the Form
  control greys the options with the reason on screen without moving the
  scenario's own selection, and the sim refuses the pair. It covers the LOCK the
  same families carry ("set to its default ignoring other bonuses, even negative
  effects"): the panel pins the stat and NAMES what pinned it, and a buff whose
  only grant is that stat is not offered — a lock reaches evolutions, arcanes
  and passives, not just mods (MEASUREMENTS M30).
- `check_board_link` — a board row opens THAT row: the build it names AND the
  ruler it is on. It walks every ruler and asserts against `BOARD` itself. It
  holds a case the live board has never had — ONE WEAPON, TWO MODES — by
  injecting a synthetic second-mode row. IT ALSO WATCHES THE ORDER one level
  down: the builder's picker groups a weapon's deeper ranks by mode and numbers
  each inside its group, asserted over EVERY weapon the board holds in more than
  one mode, picking the WORST-INTERLEAVED one for the DOM half. The rank
  assertion beside it says #1 is that mode's LEADER, because a position counter
  and its rank agree however the list is ordered.
- `check_disclosure` — what the app does NOT model is ON THE PAGE, in every
  family that has one: a weapon banner, an evolution chip, a mod line, an arcane
  line, an enemy caveat. It covers the fourth kind of admission, which is not a
  shortfall: a LIVE BUG (`live_bugs:` on an arcane, or beside the effect it
  kills on an evolution) says the number is RIGHT, the game is wrong, and a
  hotfix changes it. The live bug is INJECTED, with the flag's removal as the
  negative control, so the claim is that the machinery can SAY it rather than
  that some perk happens to be broken. It carries a NEGATIVE CONTROL — a weapon
  with nothing to admit shows no banner — runs in BOTH languages, and walks the
  BOARD, where weapons are compared and a weapon with unmodelled parts must not
  look like one without them.
- `check_wf_buffs` — a Warframe ability buff is the FIGHT's and reaches the
  number: the section draws in both languages under DE's OWN names (战吼,
  黯然失色), the card's value follows Ability Strength, ticking one moves a real
  `/api/simulate`, two of a FAMILY do not stack AND the page says which one
  lost, the optimizer shows the same buffs read-only, and — the negative control
  — no RULER carries one.
- `check_pace_and_hits` — what a room-clear is paced by, and where an impossible
  number hides. `dps` is the whole engagement with its reloads in it; burst DPS
  is the same damage over the time the trigger was down, RECOMPUTED rather than
  trusted. Beside it: time to the first kill with its spread, the opening
  magazine, the biggest single instance, damage per shot and per pellet. Every
  block folds and REMEMBERS across a re-render and a reload, so the state lives
  outside the markup.
- `check_combat_record` — a ledger has to multiply out, asked of EVERY row.
  `engine::record` is one ordered stream of everything that happened where **a
  row is one number the game POPS**, not one hit — the only output of this app
  that can be laid beside a recording and checked number for number. A pellet
  landing on a shielded body pops TWO numbers, because Toxin bypasses a shield
  and the rest does not. IT IS THE WRITE PATH, NOT A REPORT: the stream is
  filled by the same call that moves the target's pools, from the same numbers.
  What it is authoritative about is bounded and the bounds are in
  `engine/src/record.rs`. THE CHECK DOES THE ARITHMETIC OFF THE SCREEN — it
  reads the factors as DRAWN, multiplies them, and compares with the two totals
  the same row prints. It pins the KIND list so a fifth thing cannot arrive
  unnoticed, and MAKES a miss happen (the target pushed to 40 m) because every
  claim about misses passes perfectly on a fight that has none.
- `check_damage_pops` — every drawn number NAMES the record row it is (the id
  resolves, that row's damage is the text on screen, the row belongs to the
  frame being shown), and every row in that frame is drawn, up to the cap. The
  second half is not decoration: "every number on screen is a row" is satisfied
  perfectly by drawing ONE of them.
- `check_debuff_coverage` — the DEBUFF table is the BUFF table read from the
  other side, one component fed from both: `DEBUFF_ROSTER` mirrors
  `buff_roster`, `Frame.debuffs` mirrors `Frame.stacks`, one renderer. It
  asserts the SYMMETRY rather than the numbers, plus the one thing that is not
  symmetric: A RESPAWN IS THE SAME TARGET, so its stacks drop to zero and climb
  again INSIDE one series and that gap counts against uptime. Rows the run never
  touched are dropped.
- `check_custom_enemies` — a target you MADE is a target like any other, which
  is the test of the claim: a custom enemy is an `EnemySpec` in the scenario's
  list, so the simulator, the optimizer and the target card need no code for it.
  The IMMUNITY is MEASURED rather than read off the card (a Toxin-immune target
  takes literally nothing from a Torid; the same target at x1 takes something),
  and DELETING a custom must repoint the fight.
- `check_opt_modes` — mode is the BUILDER's control and the OPTIMIZER's
  dimension. Pinning a mode makes every ranked row come back in it, pooling both
  DOUBLES the candidate count, and each row carries the mode it was scored in
  into the build it becomes. Server-side a VARIANT is a (mode, evolution set)
  pair, which is why nothing downstream had to learn about modes.
- `check_run_counts` — how hard you measure is a number someone can set, in all
  three modules, and the answer differs in each. The simulator defaults to the
  rulers' 1000 so a first number is comparable with the board without touching a
  box (1.3 s a run in the shipping build, against 0.14 s at 100). The quick calc
  takes its own with a FLOOR of 10, where a status mod stops being a coin flip
  (M24: one run swings it ±39 points), and a number under it is raised rather
  than obeyed. The optimizer's final round takes its own too, and it is a
  PREFERENCE: typed, never blank, in NEITHER half of the tab and in NEITHER
  preset. It asserts the number on screen is the number sent, that the box is
  drawn outside both halves, that `snapshotOpt` does not carry it, that
  restoring a scope leaves it where the reader put it — and, its negative
  control, that the CPU threads box is gone and no `threads` reaches the request.
- `check_arena` — the arena is a place you can DRAG, and what you drag is what
  gets simulated. Bodies are drawn at their REAL radius (`space::BODY_RADIUS_M`,
  0.25 m), so "as close as they go" is visible: they touch at CONTACT (0.5 m)
  and will not pass through each other, which the engine clamps to as well. The
  scene uses a viewBox, because a host has zero pixel width while its panel is
  on another tab and one drag would write `[null, null]` into the fight;
  `paint()` replaces the markup, so listeners are delegated rather than bound to
  circles. A BENCHMARK'S FIGHT IS NOT DRAGGABLE and the scene refuses the
  gesture ITSELF, because `lockOfficialScenario` sweeps
  `input,select,button,textarea` and these bodies are SVG circles. The check
  opens a scenario of its own first and asserts that it did, since the app lands
  a first-time visitor ON the official ruler. The OPTIMIZER draws the same scene
  read-only.
- `check_formation` — a formation is something you build on the floor, and what
  you build is what gets simulated: bodies draw without standing on each other,
  any one drags, the payload matches the scene body for body, and a real
  `/api/simulate` answers HIGHER for a crowd than for one body. AIM IS A PLACE
  rather than a target: the marker rides the target until dragged, and once
  dragged the beam is on whichever body the LINE crosses — asserted with two
  bodies on one line where the nearest to the cursor is the FAR one. Two
  negative controls: a formation of one sends zero and a null aim, and an
  official ruler refuses a crowd both by disabling the control and by not moving
  when it is clicked anyway. It asserts the per-body unit stamp ON THE WIRE.
- `check_gunco_stated` — every weapon says which Condition Overload rule it is
  computed under, with nothing equipped. The rules are per weapon and
  hand-transcribed: Adding or Multiplying, which attack parts take it, what
  fraction of the base the term reads. It is unconditional and says "no source
  equipped" plus how one WOULD be computed. The check walks all three behaviours
  from three weapons the catalog classifies differently and asserts they are
  three different sentences.
- `check_opt_replay` — the only check about a build that CANNOT go stale: it
  runs a real search, applies the winner through the button's own path, runs the
  simulator, and asserts the two numbers agree inside 4σ of their two standard
  errors. It does not know what an axis is, so a fifth one is covered on the day
  it is added. Its rotation of NEGATIVE CONTROLS is discovered from the row's
  own `replay` keys: each is deleted in turn, the ones the engine notices are
  named in the assertion's own title, and a degenerate axis is REPORTED rather
  than failed. The sharp one is last: a build assembled from a replay with a
  LIVE axis removed must fail the assertion that otherwise passes. Two weapons,
  because no single one has every axis live.
- `check_build_axes` — the cheap half of that pair, and the file says so.
  `engine::builds::BUILD_AXES` is the one declaration, served at
  `/api/meta.build_axes`; the three JS surfaces that carry their own spellings —
  the page's build state, the share tuple, the worker's board record — each
  declare which axis their fields cover. It asserts coverage BOTH ways and that
  the worker's record and identity key are still DERIVED from its table. Plain
  node against the served meta and two source files.
- `check_melee_slots` — a melee weapon has TWO slots a gun does not, and one
  decides what it swings. Every assertion is on the WIRE or on a real
  `/api/simulate`. Its sharpest pair is the ROUND TRIP — `buildPayload` into
  `stateFromBuild` must put the stance back in the STANCE slot rather than in
  slot 9 — and the FALLBACK: an empty slot fires the entry's own script, which
  happens to be Crushing Ruin's, so a stance that failed to apply would read as
  a pass.
- `check_slot_ranges` — every axis says how many of its slots a candidate fills,
  in one shape: the mods axis is 8 slots and a number 0–8, every other axis is
  ONE slot and 0–0 / 0–1 / 1–1. It walks all three states on all four axes ON
  THE WIRE. The range is DERIVED first and adjusted second, so no existing scope
  grows; a PIN is not a range (a pinned candidate settles at 1–1 with the inputs
  disabled); and 0–0 KEEPS THE CANDIDATES, which is why the evolution ladder
  keys on the RANGE rather than the marks. Mode and valence carry the row
  read-only at 1–1. The empty choice is a mark like any other — `none`, or
  `none:<pool>` on an arcane seat — so the range is a VIEW over the option set
  and needs no field in the preset, the request or the round trip. An arcane
  costs no capacity and no Forma, so an empty seat can only tie the same build
  with the arcane in it: `an_arcane_seat_marked_none_is_not_a_default`.
- `check_build_size` — how full a searched build must be is a RANGE
  (`build_min`–`build_size`): both ends push each other, both ride the search
  preset, both reach the request. The floor starts at 0 and is drawn AFTER the
  mod list, because how full a build must be is a CONCLUSION and means nothing
  until the required and the pooled are chosen. "Nothing marked" is the empty
  option, as on every other axis; once anything is marked the DERIVED floor is
  at least 1 and wins. A 0 ceiling OUTRANKS the derived floor in three places
  that must agree — `min_slots`, the guard refusing pooled mods with no slot to
  reserve, and the page's `poolStarved` — or `SubsetSpace::new(1, 0)` reports a
  legal request as "no legal builds in this scope". The row says what the marks
  raised the floor to, stated only when the two DIFFER. The count is the product
  of all six axes.
- `check_riven_pool` — the riven editor offers the stats that weapon's rivens
  actually roll, in BOTH slots. THE RULES DECIDE AND THE SURVEY CHECKS:
  `rivens_data::derived_for` is the model, `data/rivens/exceptions.yaml`
  overrides it per riven FAMILY with the evidence in each entry, and
  `data/rivens/pools.yaml` (from `scripts/survey_riven_pools.py`) is read by a
  TEST and by nothing else. See DATA_SOURCES §"Riven pools" (MEASUREMENTS M35).
- `check_riven_family` — a riven is a card for a weapon FAMILY, not an entry:
  *"Riven mods can be used on variants of a particular weapon, including MK1,
  Prime, Vandal, Wraith, Dex, Prisma, Mara, and Syndicate variants"*. The scope
  is (FAMILY, RIVEN CLASS) rather than the family, because a KITGUN chamber
  built as a primary takes a RIFLE riven and as a secondary a PISTOL one. A
  saved riven holds ROLLS and the shown value is that roll against THIS weapon's
  disposition, recomputed by `/api/riven` on every render, so one card reads
  1.45's worth on a Burston and 1.35's on its Prime. It holds the shared list,
  the disposition RATIO (2.243 → 2.088 = 1.35/1.45), three negative controls,
  the migration, and the rename/delete sweep with a same-named card in another
  family as its control. A RENAME AND A DELETE REACH EVERY BUILD THAT NAMES THE
  CARD (`repointRivenInBuilds`, whose SCOPE IS PASSED IN because rename and
  delete pass the family's members while the migration passes one weapon). AN
  EDIT IS NOT A DELETE: editing a riven is the game's own reroll, so a build
  KEEPS it and picks the new values up — dropping the rank from 8 to 0 on the
  Burston takes the Burston Prime's build from 18 drain / +208.8% to 2 /
  +23.2%, slot intact.
- `check_enemies` — every TARGET shows a picture that loads, a wiki link built
  from its ENGLISH name (the whole pass runs in both languages, because a
  localized name in a wiki URL lands on garbage), its VULNERABILITY COLUMN, and
  a statement of what the sim does not model about it. Enemy art is declared in
  the enemy's own YAML (`image:`, wiki-hosted), NOT in `data/assets.yaml`.
- `check_search` — a real optimize in the shipping build: a scope it finished
  reports `exhaustive` and says so on screen, a budgeted one reports its
  COVERAGE and does not pretend, and the WORKER FLEET covers more ground than
  one worker would.
- `check_gain_band` — a quick-calc chip says HOW WELL IT KNOWS its own number
  and never prints a zero. The scan reads `score_mean`/`score_se` and
  `dps_mean`/`dps_se`, which the server already computes, rather than the MEDIAN
  run. THE WIDTH IS THE COMPARISON'S OWN and it is DERIVED: `/api/simulate`
  returns the per-run series when the caller says it will pair with it
  (`run_series`), and the chip's band is the spread of `c_i - ratio*b_i` over
  those runs. A chip therefore has three shapes and the check asserts all three
  OCCUR: exact (`+165%`), banded (`≈+3.1% ±7.2%`), and a measured zero that says
  "no effect here" in words and points at the row's own disclosure line. An
  option not SEPARATED from the leader is marked `tied`, on the leader too. Its
  NEGATIVE CONTROL is Serration against Amalgam Serration: they differ only in
  base damage, band to exactly zero, and order as the cards state — measured
  0.9623 = 2.55/2.65 at every build strength and run count, which survives only
  because the two are paired against the same luck.
- `check_mode_def` — a mode is EXPLAINED, not just named, and its name is
  DERIVED. Each sentence is a TEMPLATE with `{named}` holes filled from
  `/api/meta`'s forms, so a weapon that arrives tomorrow explains itself and
  costs no translation. It explains THE MODE YOU ARE IN and not the other six —
  exactly one entry, and it is the one you are in, the second clause carrying
  the meaning; comparison across modes is the BOARD's job, which ranks every
  mode of every weapon as its own row. The names appear in the DROPDOWN, read
  from `modeOpts`, and must TELL THE MODES APART (a mode id can be a form id).
  Every line either carries a NUMBER or names something this mode does and its
  neighbours do not: how many of its swings reach the whole room, whether it
  spends the combo counter, whether its damage is a slam the weapon's reach does
  not bound, what it forces on the target. THE THREE NUMBERS ARE IN ONE UNIT:
  `swing_share` and `radial_share` ride the FORM, because a combo script's
  multipliers are relative to the ENTRY they are written in and the explosion is
  not in the script at all. A NAME THAT ALREADY SAYS THE TRIGGER DOES NOT SAY IT
  TWICE, compared on the SOURCE strings with both sides checked non-empty.
  Carries a MATCHED PAIR (the Mausolon and Cortege must not be told they have an
  Incarnon anything; the Torid and Lex must still say so) and runs in both
  languages.
- `check_gain_freshness` — a scenario edit reaches the quick calc immediately,
  including a field nobody has invented yet: the scan's cache key is DERIVED
  from the fight it will run. It asserts the EVOLUTION axis (which ranks with no
  picker open, so it tests the re-ask and not a repaint) and probes the scan's
  own BASELINE rather than a candidate's gain.
- `check_buff_cards` — buff cards are named in the display language, open at the
  stack count the rule says, and report a coverage never rounded up to a flat
  100%. It walks the one buff that is a WEAPON PASSIVE — the Ocucor's tendrils —
  because a stack count nobody can set is a mod nobody can measure. See BUFFS.md.
- `check_gain_axes` — the quick-calc gain scan obeys the evolution TIER LADDER,
  so it never ranks a perk the builder will not let you click.
- `check_replay` — the median engagement plays back on screen: the buff curves
  draw, scrubbing drains the pools, and play advances the clock at the chosen
  multiplier.
- `check_preset_independence` — no collection's state is written from outside
  it: switching a build must not move the fight, and editing the fight must not
  touch a build.
- `check_share` — it opens a share link in a browser that has never seen the
  build and asserts what is on SCREEN, not what is in the variables.
- `check_tenno` — the fight's PLAYER reaches the panel, the sim and a share
  link, so an arcane that scales off a Warframe is worth nothing with no frame
  and +500% with one.
- `check_squad` — a squad AURA and an ARCHON SHARD ride on the fight's `Tenno`.
  Every assertion is on the wire or on a real `/api/simulate`. Its damage
  assertion needs a fight where ARMOUR is the binding constraint: at the default
  level an unmodded rifle never gets a target off its shields, the armour term
  is never read, and the two runs come back byte-identical. It measures kill
  PROGRESS, because dps is what the weapon puts out and armour decides what
  arrives.
- `check_storage` — how much room the app takes on the reader's machine. It
  measures the RATIO rather than asserting a constant, fills the disk from OTHER
  weapons' keys to prove the shed sweeps the origin, and plants a replay written
  under the old rule to prove the boot takes it back. Its second assertion keeps
  the fix honest: the panel must STILL DRAW a replay.
- `check_one_fight` — holds no list of fields: it asserts every module's
  outgoing request against `theFight()` ITSELF, so a field invented tomorrow is
  covered by nobody.
- `check_scan_progress` — a scan says how far along it is where the work is
  being READ, mounted in all five places a scan ranks something. AN AXIS ONLY
  SHOWS ITS OWN, since two lists can be open at once. It draws NOTHING when
  nothing runs and the check asserts the ABSENCE as well as the presence. Its
  evolution half needs a CROWD: a one-body Torid ranks its dozen evolutions
  faster than the 250 ms repaint throttle, so nothing is ever drawn.
- `check_board_dedup` — a build the board already holds is not sent to it again,
  and the page asks the ENGINE which (`/api/build/keys` → `builds::board_key`).
  A build is not its spelling: `canonical_mods` sorts the non-elementals by
  drain and leaves the elementals in the order that PAIRS them, evolutions are a
  set, a riven is a shape and not its rolls, and the mod POOL is what tells an
  elemental mod from any other — which only the engine has. A MATCH IS PROOF AND
  AN ABSENCE IS NOT: the board LISTS only builds scoring at least half their
  weapon's leading row, so the page only ever suppresses an upload it can prove
  is redundant. Its NEGATIVE CONTROL is the half that matters — a build the
  board does not hold must still be offered.
- `check_support` — the page that ASKS for something makes its case in numbers
  it COUNTED. A drawn figure and a counted one look identical, so each is
  compared against the source it claims: the weapons tile against
  `META.weapons`, the mods tile against the union of `META.mod_pools`, the built
  line against the injected `PROJECT_FACTS`. The other half is the one line
  about the READER — how much they have run here — asserted absent on a browser
  that has run nothing, correct after a real run at a run count the check chose,
  and absent from the request that run sent. Its negative controls are the
  channels (an entry with no url draws nothing) and the supporter line (silent
  while its store is unconfigured). It FORCES English rather than inheriting it,
  since the app boots into the browser's language.
- `check_comment_style` — no attribution and no dated decision survives in the
  repo's prose, and the narrative phrases that mark a history being retold are
  ratcheted: the count may fall and never rise. `docs/MEASUREMENTS.md` is exempt.

## Hard rules

- **THE SIMULATOR IS THE TRUTH; THE OPTIMIZER OBEYS IT.** A search's winner is
  replayed under the simulator's fight, so any rule the optimizer applies that
  the simulator does not — or omits that the simulator applies — scores builds
  nobody can reproduce. The optimizer must CALL the simulator's code and add
  only its own scope and budget. `parse_fight` is that shared parse:
  `simulate_json` reads `replay` and nothing else; `parse_optimize` reads
  `build_size`, `build_min`, `finalists`, `final_runs`, `deployment` and nothing
  else. Neither builds a second Tenno. Anything that is a property of the FIGHT
  goes in `parse_fight`. A shared helper is not enough — the DECISIONS around it
  have to be shared too.
- **THE OPTIMIZER IS THE BUILDER, IN BULK.** The same claim on the PAGE: every
  axis on the optimizer tab is a question the builder already asks, and the only
  difference is what gets bound — the builder binds a VALUE, the optimizer binds
  a SET. Same axes, same order, same numbers, same names, with the exilus slot
  INSIDE Mods because that is where the builder's exilus slot sits.
  NOTHING DECLARES THAT ORDER TWICE: `orderOptScope` walks the builder's own
  blocks in DOM order and stamps each heading from that block's `.n` and `<h2>`
  — already translated. `OPT_SCOPE_OF` is the only hand-written half and is
  touched only when an axis is added or removed.
  THE SAME ARGUMENT ONE LEVEL DOWN: the `.opt` row is ONE function (`modRow`)
  with the trailing control as its parameter, and those segs are one function
  (`oseg`) that six lists call. A copied row is a comment that stops being true
  in silence. Searching the stance SLOT is a real axis and is still missing; it
  wants the treatment the exilus slot has, in `optimizer/` as well as on the
  page (`docs/OPTIMIZER.md`).
- **A FINGER SCROLLS; IT DOES NOT DRAG THE FIGHT.** A browser decides who owns a
  gesture at `pointerdown` and never gives it back, so a body that drags on
  touch means the finger that started on it can no longer SCROLL — and a 19x19
  formation covers the canvas in bodies. A LONG PRESS CANNOT FIX IT: once the
  gesture is the browser's it is gone. The answer is a MODE the reader turns on
  — a ✥ chip in the scene's own control row, off by default, drawn only where
  `navigator.maxTouchPoints > 0` (a touchscreen laptop reports a FINE pointer,
  so a `pointer: coarse` query is the wrong test). `touch-action` follows it:
  `pan-y` off, `none` on. A mouse is unaffected.
- **A SIMULATION RUNS ON A WORKER FLEET.** The runs are INDEPENDENT given their
  index, so the page shards them across one worker per core (capped at eight)
  and the shards merge back into exactly what one worker would have produced.
  Measured on the group-clear ruler with the board's #1 Phantasma Prime build:
  **85.7 s → 18.3 s**. THE ENABLER IS THE SEED — each run's dice are a pure
  function of `(seed, index)`. THE MERGE IS IN RUST, so there is one
  implementation of the arithmetic: the page schedules and collects,
  `simulate_merged` computes every field. A `Shard` carries SUMS rather than
  runs — 24 KB at a thousand runs against 8 MB — plus one
  `(effective, rng_state)` per run, because the MEDIAN engagement is what the
  panel shows; the merge ranks those and REPLAYS the winner.
  **A JSON NUMBER IN JAVASCRIPT IS A DOUBLE**: the 64-bit RNG state travels as
  two `u32` halves (`RunKey`), or it comes back ROUNDED and the merge replays a
  fight that never happened — every mean matching to the last bit while `score`,
  the one figure taken from the median run, disagrees. Asserted three times: on
  the summary (`eight_shards_are_one_run`), on the whole response
  (`a_fleet_of_shards_reports_what_one_worker_reports`), and ON THE WIRE in
  `check_run_counts`, the only one that could catch the rounding.
  A COMPARISON IS TO A PART IN 10^12, not bit for bit: floating-point addition
  is not associative.
- **A LONG SIM SAYS HOW FAR IT HAS GOT.** The run count is unbounded and so is
  the cost per run: single-target is about a millisecond, a 361-body fight
  ~28 ms. `simulate_progress` is the wasm entry (its own, not a flag on `api`,
  because `/api/simulate` is the one endpoint whose cost is unbounded), the
  worker forwards `{done, total}`, and the panel draws a bar, THE COUNT and a
  time remaining. The count, because "412 / 1000" is a number a reader can act
  on. THE ANSWER IS UNCHANGED — the callback observes and never steers — and the
  throttle is in the WASM layer at one message per percent. The remaining time
  is hidden below a second and before 5%.
- **EVERY ENEMY HAS A NAME, AND THE PAGE CAN ASK ABOUT ONE.** `SpreadFoe` is
  `{state, debuffs}` per body; `formation::FoeSpec::id` names it, stable across
  edits because it travels in the scenario and is filled in BY POSITION when
  blank. The aimed body is `e1` and lives on the `Arena` — it is not in the
  formation list, it is the fight's own target — so the crowd reads as one list.
  `/api/simulate` returns a ROLL CALL (`bodies: [{id, aimed, at, damage}]`) of
  the ones that took something, because a per-BODY figure is the only thing that
  can say a crowd was REACHED rather than a big number produced.
  SETTING UP A FIGHT AND READING ONE ARE TWO THINGS, so the RESULT panel draws
  its OWN copy of the scene: the scenario's canvas is where a body is PLACED —
  draggable, with distance shortcuts and +1/+8; the result's is read-only,
  shaded by what each body TOOK, marks the one being examined and PICKS rather
  than drags. `mountArena` takes `heat`/`selected`/`onPick` for the second kind.
  A BODY IS NAMED WHERE IT IS CREATED (`nextFoeId`), one past the highest ever
  used rather than one past the count, so deleting a body never hands its name
  to the next one.
  THE DEBUFF TABLE HAS A SUBJECT: `Replay::tracked` names the bodies it
  followed, the panel draws a chip per body, and picking one redraws the table
  from the stored result at no simulation cost. It follows the aimed body plus
  the hardest-hit few (`REPLAY_TRACKED = 8`), because a series is 600 frames ×
  15 debuffs = 18 KB a body and a 19x19 would be 6.5 MB. The cap is SAID ON
  SCREEN ("+N more took damage and are not followed"), never applied silently.
  One body draws no chips at all.
- **A FIGHT IS ONE DOCUMENT, AND A SCENARIO'S OVERRIDES SIT BEHIND LEGALITY.** A
  scenario holds everything a measurement needs — the target, the buffs, the
  wielder — AND what it rules for each weapon CLASS, so any weapon can be tested
  against one file and the official rulers are written in the same language a
  player's own fight is.
  THE ENGINE DECIDES WHAT MAY BE RULED ON, derived rather than listed.
  `scenario::Capability::absence()` sorts every capability into two kinds and
  that is the whole guard: a GAME FACT is the game's own rule — a Sentinel
  cannot put a shot on a head — and a HOUSE RULE is ours. A scenario may say
  *"in my fight, Arch-Guns have infinite ammo"* and may not say *"in my fight,
  Sentinels land headshots"*. Exactly one of the four capabilities is a house
  rule today. `overridable_pairs()` derives the legal (class, axis) set from the
  two tables, `/api/meta` serves it, and the page draws exactly what is listed.
  It is pinned as an EXACT set by a test, because the failure to guard against
  is the list GROWING without anyone deciding it.
  The resupply rule lives in the capability, not in `reserve_is_infinite`, which
  takes the RESOLVED answer.
  THE DEFAULT IS THE WEAPON IN FRONT OF YOU: the scenario blocks show what
  applies here; the whole-fight panel is where the other classes are edited, and
  a rule that merely AGREES with the capability is pruned rather than stored. A
  RULER REFUSES ONE, like every other edit — `sim-whole-fight-body` is in
  `lockOfficialScenario`'s sweep.
- **THERE IS ONE FIGHT, AND EVERY MODULE SENDS IT.** `theFight()` is the only
  spelling, and THE LIVE `sim` IS THE FIGHT — not the preset behind it, which is
  a saved COPY that `applyScenario` seeds `sim` from and the auto-save writes
  back.
  THE ONLY THING A CALLER OWNS is `replay`, a `seed`, `run_series` and the quick
  calc's RUN COUNT — the reader's precision, not an edit to the fight. It lands
  LAST in the spread, or the box silently does nothing while every chip's
  tooltip quotes it.
  THE BUFF MAP TRAVELS WHOLE, because buff settings are the FIGHT's and it is
  the BUILD that decides which have a source (the server's `BuffCfg` is a
  lookup, so an entry nothing grants is never read). Pruning it to the current
  build makes the quick calc a different fight the moment a candidate grants a
  buff the current build lacks — which is every candidate worth ranking.
  A SWITCH IS THE OTHER WAY A FIGHT MOVES, and it has to RE-ASK rather than
  repaint: an EDIT re-runs the scan through `markScenarioDirty`'s debounce, a
  switch goes nowhere near it. `scenariosChanged` calls `refreshGains()` — that
  hook rather than the call sites, because it is already the only thing every
  scenario mutation goes through.
- **A MOD POOL A WEAPON CLAIMS MUST HOLD SOMETHING.** DE tags a mod PRIMARY,
  Rifle, or narrower — Assault Rifle, Bow, Sniper — and a weapon draws every tag
  that applies to it, which is why `mod_pools:` is a LIST. A pool a weapon
  DECLARES and no `data/mods/<pool>/` holds resolves to an empty list, with no
  error anywhere. `scripts/survey_pool_mods.py` works from the ROSTER: every tag
  any weapon claims must map to an export `compatName`, and the script REFUSES
  to run when one does not. Its `data/surveys/pool_mods.yaml` is read by a
  ratchet test that also asserts, per weapon, that each claimed pool holds at
  least one mod. It is the sibling of `survey_weapon_mods.py`, which joins the
  same field against WEAPON NAMES. Adding a mod starts there: run the survey,
  take a row, transcribe it from the WIKI, lower the ceiling.
- **A CONDITION ABOUT THE TARGET IS SIMULATED; ONE ABOUT THE TENNO IS ASSUMED.**
  `arc_condition` returns a TYPED condition and `Unknown` for anything it has
  not been taught, and a test walks every arcane yaml and refuses an unknown
  one. A data file stating a rule the engine does not apply is worse than one
  that omits it: to anyone auditing, it reads as if the rule were being applied.
  A CONDITION IS HONOURED AT RESOLVE OR AT THE HIT, and the test that says so is
  DERIVED from the yaml rather than naming arcanes. A Tenno state pays NOTHING
  under `Emergent`, the app's default policy — asserted against
  `ArcaneFx::none()`, not against another policy of the same code, because
  comparing `Emergent` to `BaseOnly` cannot see a guard removed from BOTH. A
  target state sets the gate the sim reads. Both arms are verified to bite.
  Secondary Kinship pays zero under BOTH policies, because a solo fight has no
  allies to buff — the honest answer rather than a broken gate.
- **AIM IS DRAGGED, AND A PICK READS NOTHING.** A bare click clears the
  selection and does not aim; the rail carries an explicit AIM TOOL, after which
  the marker drags in Select like anything else with a position. A fight that
  moves on a mis-click makes the result on screen a result for a fight nobody
  was in.
  PICKING AN ENEMY IN THE RESULT READS NO STORAGE: `renderResults` records
  `shownResult` and a pick redraws THAT. Storage is what survives a reload or a
  preset switch, and nothing else asks it a question.
  HARDEST HIT FIRST, in both views. `tracked` comes back in the engine's slot
  order, which answers "who got followed" rather than "who took the most", so
  the display order is sorted; `data-rpfoe` stays the index into `tracked`,
  because `dstacks[k]` is that body's series. Each chip carries the number it is
  sorted by.
  A UNIT IS A COLOUR, derived and never declared: `unitHue` hashes the id
  (FNV-1a) into a hue, so a unit that arrives tomorrow already has one and it is
  the same colour on every machine. The same hue is a swatch wherever a unit is
  NAMED.
- **A BODY IS THE UNIT IT WAS PLACED WITH.** The card on the left of the arena
  says what you are ABOUT to place; it is not a control over the floor. The unit
  is STAMPED at placement (`placeAt`, `arenaAddFoe`) and nothing afterwards
  moves it — the server reads a blank unit as "the aimed body's", so an
  unstamped body changes species when the card does. `FoeSpec` carries a
  per-body `enemy`, `level` and `eximus`; the LEVEL deliberately stays the
  FIGHT's, one dial for every body, so a body leaves it blank and follows.
  The AIMED body still follows the card, because that card IS the fight's target
  and the aimed body is what a placement copies. That split is invisible until
  somebody switches and watches nothing change, so the panel states it and
  counts what is holding a different unit.
  EVERY FORMATION SAVED BEFORE THE RULE PINS ITSELF ON LOAD: `applyScenario`
  fills the blank in from the scenario's own enemy. Growth stopping is not the
  same as what is already there being fixed.
- **THE PAGE SAYS WHICH BUILD IT IS.** A fix that is deployed and a fix that is
  on the reader's screen are two different things, and without a version on the
  page neither side of a bug report can tell "still broken" from "still holding
  the old file". `build_site_app.py` stamps the footer with the commit, a `+`
  for a dirty tree, and the UTC minute it generated `site/` — the commit alone
  is not enough, because `site/` is built from a WORKING TREE. The dev server
  ships the `dev` placeholder. The same stamp goes into `app.js` as `BUILD_ID`,
  so a browser holding an old page with a new script can say so
  (`checkBuildMatches`).
- **A MEASUREMENT COSTS ITS SUMMARY, NOT ITS REPLAY.** A REPLAY is 600 frames of
  debuff series per followed body plus a hit account per attack part — **65 KB
  against the 1.6 KB summary** of every number a card, a share or the board ever
  reads, 42x — and one would be stored per WEAPON. About seventy-five weapons
  fill a 5 MB origin. Past that `setItem` THROWS in the save path of the run
  that just finished, and the reader is told "sim failed: QuotaExceededError"
  for a simulation that worked.
  So a replay NEVER reaches the disk: `stripReplays` takes it out on the way to
  `localStorage` and `resultMem` (keyed by weapon AND preset) keeps it for the
  session. A SHED SWEEPS THE ORIGIN, not the list — a quota belongs to the
  origin, so `shedOtherResults` walks every `wfsim-presets-*`, oldest result
  first, and the list being written is the LAST thing it may touch. AND WHAT IS
  ALREADY THERE COMES BACK: `reclaimStoredReplays` strips every replay written
  under the old rule on the way in.
- **WHAT THE WARFRAME BRINGS IS THE FIGHT'S.** A squad AURA (`data/auras/`) and
  an ARCHON SHARD (`data/shards/`) belong to neither the weapon nor the build,
  so they ride on the fight's `Tenno` exactly as `data/abilities/` does —
  carried into both modules through `parse_fight` alone, and kept off the BOARD,
  scored under the neutral player.
  THEY ARE OFFERED, NEVER TYPED. The Extra stats grid accepts any number into
  any bucket; what it cannot do is say WHERE the number came from. A named shard
  has a source that can be checked against the wiki; a typed +45% has nothing
  behind it.
  THE AMP FAMILY DOES NOT SHARE ONE GATE, which is why `AuraDef::pays` is a
  function: Rifle Amp asks a MOD POOL — *"also affects bows, sniper rifles and
  launchers"* — while Dead Eye asks a CLASS and is narrower than any pool:
  *"only affects actual sniper rifles … even though bows and launchers draw from
  the sniper ammo pool, they are not affected"*. The ENGINE decides and
  `/api/meta` states the CONSEQUENCE per weapon (`auras: [id]`), which is
  `evo_forbids`' own pattern.
  THE AMPS LAND IN SERRATION'S BUCKET, where their own page puts them: *"adds to
  the base damage as Serration and Heavy Caliber do"*. So an amp is worth LESS
  the more Serration is already in the sum, and a reader wants to see it in
  there.
  AN EFFECT IS APPLIED OR IT SAYS WHY NOT, never neither and never both. Twenty
  of the twenty-seven shard effects pay nothing in this arena, and THREE of
  those are real weapon-damage quantities transcribed correctly and still not
  paid, because the bucket they need is narrower than any this engine has.
  `ShardEffect::unmodelled_reason` is the one answer and the page prints it; the
  test that holds it is DERIVED from the roster.
  A LOCALE'S TABLES ARE NO LONGER A HAND LIST: `LocaleSpec::merge` is tested by
  serializing the merged spec and asking the question of every field there is.
- **A STATUS HAS THREE MODELS, NOT TWO.** Slash and Toxin are PER INSTANCE —
  each stack its own clock, timer and number. Heat is a SINGLETON that every
  proc refreshes and that pays one consolidated tick. Electricity and Gas are
  the third: *"multiple procs on an enemy no longer deal their respective damage
  separately, like current Slash statuses, but once per second, similar to Heat
  status. However, they still maintain each own timer and will not refresh,
  unlike Heat"* (wiki `Damage/Electricity Damage`, **Update 33.6**); Gas is the
  same shape, confirmed in game where the wiki does not say. `push_dot_capped`
  moves a joining instance onto its family's clock and `Ev::Dot` pays every live
  one as ONE instance.
  THE CLOCK ALONE IS HALF THE RULE: an instance is the unit attenuation clamps,
  a shield gate multiplies by 5%, and overkill is measured against, so ten small
  ones and one large one differ on any target that has those. The merge is
  tick-count neutral by arithmetic — an instance with `k` ticks joining a clock
  `φ < 1` ahead fires `ceil(k − φ) = k` times.
  THE HEAP GOES STALE AND THE SCAN CANNOT: `process_ticks` picks its path by
  live DoT count, and advancing a whole family leaves heap keys for ticks
  already paid. Without the guard a stale key advances its Dot again and BURNS a
  tick, so the damage comes out LOW. A fixture proving it must be dense enough
  to leave the scan path (`TICK_QUEUE_MIN`).
- **A DoT'S WEAK-POINT RULE IS PER STATUS, AND A BLAST IS NOT A DoT**
  (MEASUREMENTS M54). A Toxin tick does NOT inherit the weak point of the hit
  that applied it — measured in game, against a wiki page that says it does — so
  `dot_takes_weakpoint` names Toxin and nothing else. Every other DoT is
  UNMEASURED and keeps the wiki's answer. A BLAST goes the other way and was
  measured exactly: ten stacks applied by BODY hits reach a neighbour for 1050,
  by HEAD hits for 3150, **3.000** with no remainder, and `1050 / 10.5 = 100`
  confirms the published 10× between the radial and single-target halves. Four
  mechanics that sound like one family do not share a rule; the only way to know
  is to measure each.
- **THE SPACING IS THE GROUP RULER'S ANSWER, NOT ITS ARRANGEMENT.** A 5 m Blast
  sphere holds `π·25/spacing²` bodies — 35 at 1.5 m, 5 at 4 m — so the grid's
  spacing decides the whole splash-versus-single-target ordering before a weapon
  is read. Measured on one weapon with one build per element and everything else
  pinned, Blast swings **71×** across 1.5–6 m while Heat is FLAT (58–72),
  because Heat is a DoT on one body. It stands at **3 m**, the near edge of the
  crossover band. IT COSTS SOMETHING REAL: 1.5 m was the only spacing that
  separated all three steps of a radius mod (6/9/13 bodies) and 3 m does not.
  AND 3 IS FITTED, NOT MEASURED — it was chosen to make the ORDERING match play,
  which is weaker evidence than measuring the parameter. The quantity to measure
  is not the spacing but what it sets: how many enemies one blast detonation
  actually reaches in a real fight (~9 at 3 m, ~20 at 2 m, ~5 at 4 m).
  A RULER'S PROSE QUOTES ITS OWN NUMBERS. The spacing is written three times —
  the field, the ruler's NAME, and the rule sentence — and a test reads the RAW
  yaml (the grid is expanded into 361 positions at load) and asserts the prose
  quotes the field.
- **A FIGHT POPS NUMBERS, THEY ARE EVENTS, AND THERE IS ONE LIST OF THEM.**
  Everything else a replay carries is a CURVE, and an aggregate cannot tell one
  hit for 400,000 from twenty for 20,000. IT IS THE COMBAT RECORD, REPLAYED —
  one stream, not two: two lists filled from one place still hold different
  SETS, so a number could float over a body with no row to explain it. For a
  panel whose claim is "this is what happened", ONE-TO-ONE IS THE CLAIM.
  THE CAP IS A DISPLAY DECISION, made in `popsDraw`: twelve a frame, biggest
  kept, the rest counted. DE caps its own display the same way ("a maximum of 10
  tick numbers are shown at once"), and it is unavoidable besides — a dense
  fight deals ~320,000 instances against 600 frames. The dropped count is on
  screen.
  THE TWO VIEWS MEET ON AN ID: a drawn number carries `data-rpevent` and a table
  line `data-recevent`, both the event's own place in the stream.
  ON THE PAGE it is a DOM overlay appended AFTER `mountArena`, never before —
  the mount takes the host over and rewrites it. The analysis mount publishes
  `host.__arena` and the overlay puts that viewBox through the svg's own fit:
  ONE geometry.
  IT IS FETCHED ON A GESTURE. Pressing play or scrubbing asks for the record; it
  does NOT ride along with the result.
- **A MEASUREMENT'S RECORD IS A QUERY, NOT A PAYLOAD.** `/api/log` is
  deliberately not a field on `/api/simulate`: an ordinary fight deals
  **2,000–5,000** damage instances over 180 s and the densest build measured
  deals **408,817**, so a log that rode along would be free on most builds and
  megabytes on exactly the ones a player is most likely to be arguing about.
  Asking separately costs ONE re-run — about a millisecond single-target — and
  keeps "a measurement costs its summary" intact. `/api/simulate` answers with
  the median run's RNG state as two u32 halves, which is the handle that makes
  the log the report's own fight.
- **A FACTOR IS A TYPE, AND THE WIRE SENDS ITS INDEX.** `record::Factor` replaces
  a `&'static str` written at each call site: nothing tied the word "critical" to
  the 4.4 beside it, a typo was a new factor nobody would notice, and TWO
  different things were both called "shield gate" (the 0.1 s window and the 5%
  leak past a broken shield). The table is sent once and a row names its factors
  by index; the weapon state and the two stack lists are omitted when unchanged
  from the row before and filled forward on arrival. Measured: **859 → 481 bytes
  an event, 17.2 → 9.6 MB, 1,811 → 546 ms** for 20,000 rows.
  `Record::wants(t)` is the other half: a row's arguments cost a `TargetAt`
  snapshot, three Vecs and a String, which on a dense build are built for the
  whole fight and thrown away by `push`.
  One page-side trap: `const F` declared beside the other helpers at the bottom
  of `recordRow` while the factor lookups sit above it is a temporal dead zone,
  and it throws from inside an async paint — which surfaces as the panel sitting
  on "reading…" for ever with nothing in the console.
- **A BUILD THE BOARD ALREADY HOLDS IS NOT SENT TO IT AGAIN, AND THE PAGE ASKS
  THE ENGINE WHICH.** `officialBuildActive()` answers whether the ACTIVE PRESET
  is a builtin, which is true of a board row opened from the picker and false of
  the same build reached any other way. `/api/build/keys` keys a LIST of builds
  through `builds::board_key`, so the build on screen and every row its weapon
  holds are keyed by one engine in one pass. `builds::board_key` is that one
  spelling — `format!("{}#{}", identity(&v), mode)`, defaulting a blank mode to
  `base` — and THE MODE IS PART OF THE KEY, because one build played two ways is
  two entrants. The one order that IS the identity is the elemental one: Torid
  Heat/Cold/Toxin/Electric is Blast+Corrosive at 12,424 DPS against
  Heat/Toxin/Cold/Electric's Gas+Magnetic at 46,583.
- **A LONG RECORD IS PAGED, AND IT CAN LEAVE THIS WINDOW.** The densest build
  measured is 24,652 events; a table of that many rows is ~250,000 cells, laid
  out again on every repaint of the result panel. THE FETCH AND THE VIEW ARE
  PAGED SEPARATELY: the stream in memory is the entire fight — `Copy as text`
  writes all of it and a floating number can name its row across a page boundary
  — and only what is on SCREEN is bounded (`REC_PAGE`, 500). The pager is drawn
  above the table AND below it.
  IT OPENS IN A WINDOW OF ITS OWN: the parent keeps the state and calls the same
  `recordBody`/`wireRecord` against the child's host, so there is ONE
  implementation and the window is only where it is drawn. Every control is
  found inside the HOST rather than in `document`, because the two are different
  documents. The child is WRITTEN rather than navigated to — a real navigation
  would boot a second copy of the whole SPA to display a table the parent
  already holds. A BLOCKED POPUP IS NOT A SILENCE, and closing the window hands
  it back. `recordMarkup` emits an EMPTY host and `paintRecord` fills it.
  The popup half of the check needs `evaluate(…, { userGesture: true })`.
- **DAMAGE COMES THROUGH ONE DOOR, AND THE COMPILER HOLDS IT.**
  `engine::dummy::ledger` owns the run's totals and the DPS curve in types whose
  fields are PRIVATE TO IT, so the only thing in this crate that can move them
  is `ledger::settle` — which books the number and writes the row that explains
  it in the same call. A damage site that moves every curve on the page and
  appears in no ledger does not compile.
  THE RAW TRAVELS ON `Settled`: `apply` carries it back, so a site cannot book
  one figure and settle another.
  THE ARGUMENTS ARE A CLOSURE, which is what lets the per-site gate go: an
  `Instance` is a dozen locals, three Vecs and a String, and a `TargetAt` re-runs
  the armour scaling curve — +4.0% on `one_fight` if the 999 runs nobody reads
  pay for it. `settle` takes `impl FnOnce() -> Instance` and calls it only when
  the record is on.
  THE GENERIC HALF IS TINY, measured: the whole body inside the closure-generic
  function is stamped out per damage site and costs **+2.7%**; splitting the
  cold half into a non-generic `write_row` puts it back. `Curve` is its own type
  rather than a third field on `Meter` for the same reason: it is a 600-slot
  array, `RunResult` is `Copy`, and grouping 4.8 KB with the two hot scalars
  costs **2.4%**.
  Verified to bite means the sabotage must fail to COMPILE: a tenth site writing
  `r.meter.raw += 1.0` is rejected with "field `raw` of struct `Meter` is
  private".
- **THE STREAM IS THE FOUR THINGS A FIGHT DOES, PLUS A MISS.** A shot, a
  reload's two ends, a transmute's two ends, and the numbers the enemy takes.
  There is no ARRIVAL row. Against a target at CONTACT, which is every official
  ruler, it is one row per pellet reading "it arrived, 0.00 m" — 6,192 arrivals
  against 12,404 damage rows on the board's leading Laetum, half the stream
  saying nothing. Which numbers are one arrival is still answerable from the
  shot they share.
  THE MISS IS REAL: three exits in the pellet loop produce no damage at all —
  outside the cone, out of the weapon's range, an explosion that reached nobody
  — so "why did a three-pellet shot pop two numbers" has an answer. It is NOT a
  per-pellet row: on every official ruler the target is at contact and it never
  fires.
- **A ROW'S STATE COLUMNS ARE THE INSTANCE'S, AND ITS LABELS ARE NOT ITS
  IDENTITY.** Every state column on a row belongs to that row: `set_stacks` is
  called in the stage loop beside `DebuffState::amps`, the roster is built once
  per run because `buff_roster` allocates, and it costs nothing measurable
  because it only runs while a record is being taken. A state column that does
  not match the number beside it is the panel telling a reader their own
  arithmetic is wrong.
  A LABEL IS TRANSLATED; A KEY IS NOT. Every factor chip carries `data-factor`,
  every pool `data-pool`, every origin chip `data-origin`, all in the ENGINE's
  own spelling, and the checks ask those: the DOM carries the identity, the text
  carries the language.
  THE RECORD IS A WINDOW THE PLAYHEAD SETS. The 20,000-event cap bites on
  exactly the builds people argue about — the board's leading Laetum deals
  ~230,000 damage instances over 180 s — so scrubbing or playing past the end of
  the window fetches the next one, the panel states the slice it is showing, and
  how many did not fit is counted in the chip beside them.
- **A VOLLEY HAS AN ORDER, AND EVERY INSTANCE RE-READS THE TARGET**
  (MEASUREMENTS M62). Pellets leave the muzzle at one instant and do NOT settle
  at one instant: a pellet resolves its own explosion before the next pellet's
  collision, and each of those four instances reads the target as the one before
  it left it. A Laetum forcing a Viral proc on both halves pops
  `200 / 1,200 / 450 / 1,500` — the Viral ladder read at 0 / 1 / 2 / 3 stacks —
  and no other assignment of those numbers to those instances survives the
  arithmetic. `DebuffState::amps` is read inside the STAGE loop; `prune` stays
  once per pellet, because the whole volley is at one instant `t`. AN INSTANCE
  DOES NOT AMPLIFY ITSELF: the first collision reads x1.00 because its own
  forced proc lands after it has been settled. The golden test
  `a_volley_settles_pellet_by_pellet_and_each_instance_re_reads_the_target` pins
  the four NUMBERS rather than the rule.
- **PROGRESS BELONGS WHERE THE WORK IS BEING READ.** A pool of ninety mods at a
  real run count is tens of seconds of a list that does not move, and a list
  that does not move is read as broken rather than as busy. The per-row "…" chip
  is a different claim: it says THIS row has no answer yet. `scanStrip` is one
  component — a bar, a count, sticky at the top of the list — fed from whichever
  scan state that list reads, mounted in all five places a scan ranks something.
- **Golden values only change with an in-game measurement** justifying it. New
  mechanics need golden tests; a faithful-looking implementation without a
  measurement is not correct.
- **A RANKED ROW IS A BUILD YOU CAN RE-RUN, AND THE NUMBER ON IT IS THE
  SIMULATOR'S.** The row CARRIES a build rather than describing one: `entry()`
  emits `replay`, a complete simulate request written by the same code that
  built the candidate, from the optimize request itself — so every field that
  reaches the optimizer rides along, including ones nobody has invented yet, and
  only the ranged axes are overwritten. POST it and you get the row's number.
  "+ add" applies it through `stateFromBuild`, the inverse of `buildPayload` and
  the ONLY translation between a request and the page; the pair round-trips.
  AND THE RANKING REPORTS THE SIMULATOR. Each row is re-run through
  `/api/simulate` and the KPM on screen is what came back, with a ✓. The
  search's own figure keeps one job — ORDERING the list — and the two are
  compared at 4σ of the two standard errors, both of which the server reports,
  so "they disagree" is arithmetic rather than a tolerance somebody picked. A
  row that fails it is marked `≠`.
- **A BUILD'S AXES ARE DECLARED ONCE — IN THE ENGINE.**
  `engine::builds::BUILD_AXES` is the list, served at `/api/meta.build_axes`.
  The SPELLINGS stay per-protocol (`arcane` on a request, `arcanes` on a board
  record, `arcaneRank` in page state) because renaming them would migrate every
  stored preset; what is shared is the list, and each surface declares which
  axis its own fields carry — `BUILD_STATE_KEYS` and `SHARE_AXES` in `app.js`,
  `axis:` per row in the worker's `AXES`.
  `buildState()` REQUIRES a value for every state key, so the five producers of
  a build state — the live page, "+ new", a board row, a share link, an
  optimizer result — must each name every axis. `undefined` stays a legal value
  meaning "the weapon's own default"; what is not legal is not MENTIONING one.
  `restoreState` fills a missing axis with the weapon's default, which is RIGHT
  — and is why a producer that meant the default and one that never heard of the
  axis hand over the same object.
- **PUNCH THROUGH IS METRES OF MATERIAL, NOT FREE FLIGHT.** *"The total distance
  of material (object or enemy) that a weapon's projectile, bullet or beam can
  pass through before dissipating"* — so `space::traverse` walks the aim ray
  spending it, body by body, and what a body costs is what the ray actually
  CROSSED: `space::material_at` scales the cost by the chord,
  `BODY_MATERIAL_M · sqrt(r² − perp²) / r` — the published figure at dead
  centre, nothing at the rim. Every scenario aims at a body's CENTRE and that
  case is the calibration; what changes is the body BEHIND, and it changes a lot
  — a Burston Incarnon with 2.1 m reaches 5 of six bodies down their centres and
  all 7 when it clips them at 0.9 of a radius, 53,619 DPS against 72,311.
  `traverse` is ONE walk with two readers (`struck_along` and
  `dissipation_point`).
  THERE IS ONE NUMBER, AND IT IS THE RADIUS. `BODY_RADIUS_M = 0.25` for a Tenno
  and an enemy alike, and `BODY_MATERIAL_M` is DERIVED from it — `2r`, because a
  body is a circle and the material a shot crosses through the middle of one IS
  its width. The wiki's "Minimum Mod Ranks for Penetration" brackets a humanoid
  to `(0.4, 0.5]` — 0.4 fails on three independent mods, 0.5 works on Vigilante
  Offense — thirteen cells with no exceptions, asserted by
  `a_body_costs_what_the_wiki_table_says`. A radius of 0.2 gives a diameter of
  0.4 and is EXCLUDED by that table; 0.25 gives exactly 0.5. This AMENDS M47,
  which derived 0.2 from walking into an enemy and stopping at 0.4 m centre to
  centre — a step that assumes the stop distance is exactly two radii with no
  push-out margin, which nothing measured. The hit test at contact is `r / 2r`
  for ANY radius, so nothing moved; what changes is past contact, where a 2
  degree cone reaches one body to about 7 m rather than 5.7 m.
  AND THE FORMULA TAKES A RADIUS: `space::material_through(r, perp)` is
  `2·sqrt(r² − perp²)` and nothing else.
  A PUNCHED BODY IS A DIRECT HIT — full damage, multishot, and it may HEADSHOT —
  and on a chaining weapon it STARTS ITS OWN CHAIN: *"Each enemy hit by the main
  beam from Punch Through can generate a new set of 3 chains"*, independently,
  and *"the chain from the target hit after the Punch Through can deal damage to
  the first target, and vice versa"*. `chain::resolve` takes the struck bodies
  as its seeds and each keeps its own `seen`.
  AN AoE ATTACK TAKES NONE OF IT, from its weapon or from a mod, and both halves
  are on the page. "An area of effect component" is BOTH shapes the engine
  models, `radial` and `lingering`. THE SHAPE IS ONLY THE FALLBACK:
  `punch_through_mods:` on an attack overrules it — a beam with a damage radius
  is neither, and its own page says *"Punch Through mods have no effect on the
  behavior of the beam"*. The family does not settle it either: the wiki
  sentence that groups the Torid with the IGNIS for Primary Compression groups
  it with a weapon on the punch-through page's EXCEPTION list. So it is
  transcribed per ENTRY. MECHANICS §13.
  AND "AN AoE ATTACK" IS TWO KINDS OF ATTACK. The class rule's own sentence
  opens *"With a very few exceptions"* and never says which.
  `weapons_data::BlastKind` is the type: a `contact` blast goes off on the first
  thing it touches and is the true area-of-effect attack the rule means; a
  `terminal` one goes off where the round DISSIPATES and takes punch-through
  mods normally (MEASUREMENTS M53). THE BUDGET BUYS MATERIAL: the round crosses
  `space::BODY_MATERIAL_M` per body and detonates in whichever one it cannot get
  out of, so in a crowd the blast lands DEEPER in the line — a Burston Incarnon
  with 2.1 m strikes `1 + floor(2.1/0.5)` = 5 bodies and detonates on the fifth,
  16,566 against 53,619 DPS on a line of seven. What is left over when it clears
  them all is spent as flight, because this arena has no wall; that is the one
  stand-in in the model and it is bounded by the weapon's own punch through.
  Against a LONE enemy the blast moves back and the damage drops (16,584 to
  16,358, about 4σ). With no budget the epicentre is the contact point.
- **A SHOT LEAVES THE MUZZLE AND A DISTANCE IS THE GAP.** The shooter fires from
  a point on its own circumference facing the target — drawn, with the arrow
  that says which way it faces — and hitting the circle is a hit, which makes
  the test ray-versus-circle (`range · sin θ ≤ r`, the range being muzzle to the
  target's CENTRE). That range is NOT a flight: a bullet vanishes at the SURFACE
  it hits, so what it flies is the GAP between the two bodies — one radius
  shorter, ZERO AT CONTACT, the number a reader is shown, and what damage
  falloff reads. One quantity wearing three hats rather than three that have to
  be argued into agreement. CONTACT IS UNMISSABLE AT ANY CONE WIDTH twice over —
  a flight of zero leaves a cone no distance to widen over, and the ray-circle
  test agrees from the other side. MECHANICS §11.
  THE CANVAS IS THE ONLY PLACE A POSITION IS SET. There is no typed Distance
  box: two controls for one fact is how one silently undoes the other's other
  axis, and the scene is the SOURCE. The shortcuts live INSIDE it (contact / 5 /
  10 / 20 / 40 m), each moving the target ALONG the line it already stands on,
  and the chip for the distance you are at is marked.
- **A GAP THAT REPEATS IS A REASON, NOT A SENTENCE.**
  `data/unmodelled/reasons.yaml` holds each one once, with `{named}` holes, and
  a weapon references it: `- reason: innate_punch_through` / `m: 1.2`. Eleven
  reasons cover 155 of 248 uses; a weapon whose falloff starts at a new distance
  costs ZERO translation. PROSE IS STILL RIGHT for a gap that happens once and
  needs a paragraph — 61 of them are — and a free-text parameter is not allowed,
  since it would carry English into every translation. The i18n counter asks for
  the TEMPLATE, and `trGap` fills the same holes into whichever language the
  reader is in.
- **A FORM INHERITS ITS WEAPON.** 88 of the roster's entries are form siblings
  rather than weapons, and a form states its ATTACK plus only the weapon-level
  fields that actually DIFFER — `inherits: <parent_id>` fills in the rest
  (`weapons_data::INHERITED`). Two guards hold it: a form may not restate a
  value identical to its weapon's (a restatement carries no information and is
  the only way the two can drift), and a form that copies six or more of its
  weapon's fields must declare the inheritance instead. The ATTACK is never
  inherited, and neither is `co_behavior` (the catalog gives it per ATTACK — the
  Mandonel's two forms take different classes from two different rows) or
  `unmodeled:` (a form's gaps are its own).
- **Data discipline** (`data/`): define once, reference by `id` (stable English
  slugs, never translated). YAML fields are consumed data; narrative belongs in
  comments. Perks: define-once / reference-anywhere (see `data/README.md`);
  violations fail the build.
- **i18n is an overlay.** English is the source everywhere (code, comments,
  data, UI strings). A locale is a DIRECTORY of merged files
  (`data/i18n/<locale>/`: hand-written `names.yaml` + `ui.yaml`, generated
  `descriptions.yaml`); ids are never translated. Mod and arcane CARD TEXT is
  DE's own localized sentence per rank, never a phrase-substituted English line
  — substitution is the fallback for what DE never wrote.
  **A STRING IS TRANSCRIBED, NEVER TRANSLATED.** DE's Chinese is routinely
  non-literal — Commodore's Fortune is 准将沐福 — so a name derived from the
  English is wrong more often than not. If a source cannot be reached, LEAVE IT
  EMPTY AND SAY SO. `python scripts/wfcd_i18n.py check` reports every unnamed id
  in every family and where its name comes from; `fill` only ever ADDS, so a
  deliberate divergence and the comment explaining it survive.
  Wiki URLs are ALWAYS built from the English name (`x.name_en || x.name`).
- **No native dialogs in the UI** — `prompt`/`alert`/`confirm` are blocked in
  the owner's browser. Use inline inputs/feedback.
- **Absolute asset paths in the UI** (`/img/…`, `/pol/…`, `/logo.svg`): the SPA
  also loads at `/weapons/<Wiki_Name>`, where relative paths resolve into the
  SPA fallback's HTML.
- **THE PAGE IS THREE MODULES — Builder | Simulator | Optimizer** — with one
  tab/view each, plus EDITORS that feed them. An editor is not a fourth module:
  it produces something the three consume, and it earns a tab only because it is
  too big to live inside one of them. Rivens is the first
  (`/weapons/<Name>/rivens`) — what it produces is a MOD, which is why a riven
  equips, searches and gets optimized through the ordinary pool with no
  riven-specific code in any of the three. A new tab has to pass that test: name
  what the three do with its output, or it belongs inside one of them.
  Preset collections are domain-named `<owner>-<collection>`, where the owner is
  a module — or an editor, and an editor whose ENTIRE content is one collection
  is its own domain (`rivens`). Every durable name (localStorage key, DOM id,
  label) derives from the domain. A preset belongs to ONE WEAPON, so the storage
  key also carries it (`wfsim-presets-<weapon>-<domain>`) — DOM ids and labels
  stay weapon-free, and copying a preset across weapons is the explicit "⇤
  import" action, which drops per axis what the target cannot hold. URLs mirror
  English wiki page names (spaces → `_`); internal ids never appear in URLs.
- **PRESETS vs CUSTOMS** — two kinds of collection, and the difference is who
  CONSUMES them. "Preset" is the CATEGORY and never the name of a collection or
  of an item in one: a build is a build, a scenario a scenario, a search a
  search, a riven a riven. Each bar declares its `noun`, which names new items
  ("build 2") and every tooltip that refers to one.
  A **preset** is a saved state of something that always exists, read only by
  its own module: `builder-builds` (a build), `simulator-scenarios` (a fight,
  buff settings included), `optimizer` (a search: the SCOPE and `finalists`, and
  nothing else — never buffs, never a run count, never a thread count). There is
  always ≥1, "active" means the state you are in.
  THE OPTIMIZER TAB IS TWO HALVES AND TWO BOXES: one box is the SEARCH and is
  exactly what a search preset saves; the next is the SIMULATOR's fight,
  read-only, edited there. What sits OUTSIDE both boxes is in neither preset —
  the final round's run count is that thing. It is a PREFERENCE with a key of
  its own, TYPED rather than defaulted from elsewhere, saved by no preset and
  pinned by no ruler. The cost is stated rather than hidden: the two counts can
  differ, so a winner may be crowned at a precision the replay will not use —
  and the ranking already reports it, marking a row `≠` when the two disagree by
  more than 4σ.
  CPU THREADS IS GONE: how much of this machine the page may use is ONE setting,
  in the TOPBAR (`compute-select`). `woptWorkerCount()` is `poolSize()`; an
  older preset's `threads` and `runs` are read by nothing and dropped on the
  next save.
  A **custom** is a thing you MADE that the OTHER modules consume — `rivens`
  becomes a mod in the pool, `enemies` becomes an entry in the scenario's target
  list. A custom enemy is the SAME TYPE as a published unit (`EnemySpec`), which
  is what keeps the rest of the app ignorant of it. Three things are its own: an
  inline `damage_modifiers` column, because a target nobody published may want a
  vulnerability no faction has; a `status_immunities` list, which is a DIFFERENT
  MECHANIC and not that column reading 0; and the fact that it is NOT
  weapon-scoped. Owning none is ordinary, each carries its own identity rather
  than a label you invented, and deleting one breaks references elsewhere (a
  riven delete clears the slot that equipped it — a preset delete can never do
  that). The mental model is a FILE: a list you pick from, one open at a time,
  none open being a real state — so the UI is a list + editor, NOT the preset
  chip bar, and the key is `wfsim-customs-<weapon>-<domain>` /
  `wfsim-custom-open-…`. Everything below the key is shared: storage, undo,
  per-weapon scoping, ⇤ import.
  **DAMAGE IMMUNITY AND STATUS IMMUNITY ARE TWO MECHANICS**, and the wiki puts
  both halves in one paragraph (`Status_Effect` §Status Immunity Interactions):
  *"Proc type chances are not altered by enemy resistances or weaknesses to the
  damage components used in their computation; however, they are modified by
  enemy status immunities. When an attack procs a status effect on an enemy
  which is immune to a particular proc type, the respective damage type is
  excluded from proc type chance calculations for that enemy"* — independently,
  "regardless of whether that enemy is also immune to Corrosive damage". So a x0
  column changes what a hit DEALS and leaves the proc draw alone; a status
  immunity changes what it PROCS, by leaving the denominator so the other types
  RENORMALIZE onto the roll (the wiki's own example moves the other four from
  18/5/9/23% to 33/8/17/42%).
  The optimizer owns no scenario — it RUNS the simulator's, drawn by the same
  renderer over the same state, READ-ONLY there, with a link to the simulator: a
  preset is edited in exactly one place. That includes the BUFFS. The chain is
  builder → simulator → optimizer, each reading upstream and writing nothing.
  **NOTHING CROSSES BETWEEN WEAPONS — EXCEPT THE FIGHT, AND A RIVEN WITHIN ITS
  FAMILY.** A BUILD and a SEARCH are statements about ONE weapon and are never
  born from each other: a weapon opened for the first time gets a blank build,
  the search's `finalists` resets, and the previous weapon's optimizer RANKING
  is cleared rather than left on screen under the new weapon's name.
  A SCENARIO is not a statement about a weapon, so it is SHARED across the
  roster — one list, key `wfsim-presets-simulator-scenarios` with no weapon in
  it (`SHARED_DOMAINS`), and switching weapons keeps the fight you are measuring
  under. The one weapon-scoped knob it holds is headshot %, handled the way the
  rulers handle it: the SERVER forces 0 on a weapon that cannot headshot. A
  shared bar offers no "⇤ import" — there is no other weapon to import from.
  **NOTHING OUTSIDE A COLLECTION WRITES ITS STATE.** A build carries no `sim`
  snapshot: a build is a build, and the live scenario is seeded from the active
  `simulator-scenarios` entry and from nowhere else. "What this build was last
  measured under" is `lastResult.key`, which lives outside `state` and is what
  makes a stale result show as stale. Every collection writes through
  `storePresetList`, which is what makes one Ctrl+Z stack cover all four.
  Customs are OPTIONAL by nature: nothing is auto-created, the last one can be
  deleted, and the editor stands down instead of showing a document that is not
  there. Presets are not — the modules behind them always have a state, and "no
  build" is not something the builder can show.
- **A SHARE LINK reproduces the whole thing**: `/weapons/<Wiki_Name>?b=<code>`
  carries the build, the RIVENS it equips (a custom exists only on the machine
  that made it, so it must travel inline), the scenario it was measured in, and
  the measurement itself as the sharer's claim. Opening one creates a NEW copy
  of each — never a merge, never an overwrite — repoints the build's riven ids
  at the copies, strips the query so a refresh cannot import twice, and says
  what it dropped. The payload is POSITIONAL and omits everything derivable
  (defaults, max ranks, a buff left at its own default, the shape drafts a riven
  regenerates).
  A v3 link names an id by its place in `data/share_order.yaml`, which is
  APPEND-ONLY and held there by a ratchet — `engine::share_order` recomputes the
  generator's digest over the whole list and fails on anything that is not an
  append, so a reorder is a red test rather than a link that quietly opens
  somebody else's build. It is worth 3.4x: the same Laetum is 279 characters as
  slugs and 79 as indices. The v2 array is still the one internal
  representation, so `importShare` and everything below it are untouched; v1 and
  v2 links still open.
  AND v3 IS PLAIN TEXT IN THE URL. At 79 characters deflate makes the payload
  BIGGER, so the text goes in raw; the separators are RFC 3986 unreserved
  characters and sub-delims a query accepts unescaped. A payload it cannot
  express — a CLAIM, or a name in a script the URL would escape — falls back to
  the deflate+base64 form, so the encoder measures all three and takes the
  shortest.
  A NAME THE SHAPE IMPLIES DOES NOT TRAVEL: a board riven's local name is
  `boardRivenName(shape)`, derived on arrival, which is shorter AND names it in
  the reader's own language.
  It rides the QUERY, not the fragment — a fragment never reaches a crawler and
  these links are meant to be posted. The card (`drawShareCard`, a canvas PNG to
  paste into chat) always carries the wordmark and the site's host, and a QR of
  the same link. `qrMatrix` is a from-scratch encoder (byte mode, ECC L, mask
  0), VERIFIED against a reference encoder's matrices and decoded back out of
  the rendered PNG by an independent decoder. It is drawn at a FIXED 8 device
  pixels per module — measured: at 4 the card only scans at full size, at 6 it
  survives a 0.66x shrink, at 8 it still reads at 1080px wide after JPEG 60,
  which is what a chat app hands back. The code's size is therefore an input to
  the layout, not an output.
- **THE PAGE THAT ASKS FOR SOMETHING ARGUES THE WAY THE REST OF THE SITE DOES.**
  Every figure on `/support` is COUNTED, never typed: `PROJECT_FACTS` is written
  into `app.js` by `build_site_app.py` (engine tests, browser checks, commits,
  the first commit's day) and everything the page can count for itself comes
  from `META` and `BOARD`.
  A FIGURE IS CLAIMED ONLY IF A READER CAN CHECK IT against the public
  repository. A count of in-game measurements cannot be, so it is not on the
  page.
  THE ORDER IS THE ARGUMENT: what this is and what it holds, why it can be
  checked, what the reader has already got out of it, then the door. The
  evidence behind that shape and behind the $3 floor is NextAfter's
  donation-page experiments (a stated value proposition; video loses),
  Wikimedia's banner testing (a facts appeal over a personal one, and a low
  suggested amount), Cialdini & Schroeder 1976 (legitimising a small gift raises
  participation without lowering the mean gift) and Adena/Huck/Rasul (a higher
  suggestion buys a higher mean at the cost of participation).
  TWO CHANNELS, TWO JOBS: Ko-fi is the one-off, Patreon the month. THE ACCOUNT
  IS THE PROJECT (`ko-fi.com/wfsim`) and the first-person half lives in the
  platform's own bio.
  A SUBSCRIPTION BUYS ORDER AND COMPANY, NEVER PRODUCT — a channel where the
  work is discussed, and reports read first. DE's Content Policy forbids
  charging for access to what is made with Warframe assets and permits passive
  advertising instead; this project's own promise is that nothing bought moves a
  result. NO RESPONSE TIME IS PROMISED IN ANY LANGUAGE: "read first" is an
  order, "within N hours" is an obligation that grows with every subscriber.
  THE SUPPORTER COUNT IS THE ONLY MONEY FIGURE PUBLISHED, and the store cannot
  hold more: `SUPPORT` is one empty KV key per Ko-fi message id with the DAY as
  metadata — no amount, no name, no email. A namespace of its OWN, because
  `/api/board/pending` counts every key in `SUBMISSIONS`.
  WHAT THE READER HAS RUN NEVER LEAVES THE BROWSER. `wfsim-use` is two integers
  written by `runSim` and read by `/support` alone; the page says so where it
  prints them.
- `api()` transport: `/api/meta` and `/api/i18n` are GET, everything else is
  POST — the native server matches on exact (method, path).

## Style

- **A NAME MAY BE LONG; IT MAY NOT BE VAGUE.** `docs/NAMING.md` is the
  convention and `engine::naming` enforces it. The shape is
  `[scope_]<subject>_<aspect>[_<unit>]` — `falloff_start_m` reads as "the
  falloff's start, in metres" to someone who has never opened the file. Never
  trade information away for brevity.
  A UNIT IS PART OF THE NAME and has ONE spelling: `_m`, `_seconds`, `_deg`,
  `_mps`, `_pct`. A dimensionless number declares its ROLE instead — `_chance`,
  `_multiplier`, `_bonus`, `_rate`. Words are not abbreviated (`damage` not
  `dmg`) except where DE abbreviates them on a card (`crit`, `co`, `aoe`, `dps`).
  WHAT IS FROZEN IS THE WIRE. A field inside a saved preset, a share link or a
  board record is a durable name and stays as it is — `wf_armor`,
  `wf_energy_pct`, `headshot_pct`, `no_resupply` — the same rule
  `builds::BUILD_AXES` states for axes. `naming::FROZEN` is that list and it may
  only SHRINK.
  The ratchet walks every yaml key and every engine field rather than a list of
  names. A ratchet that cannot fail is not a ratchet; prove it bites.
- English everywhere in the repo; all-lowercase commit subjects in the form
  `area: what changed and why it is right`, no AI attribution, no marketing copy
  — **with ONE exception: the home hero** (`.hero-h` / `.hero-sub`), which is
  allowed to make a bold claim. It stays a CHECKABLE one: "true to in-game
  numbers, down to the last proc" and "Theorycrafting, solved" are the golden
  tests and the optimizer, stated. Adjectives that nothing backs are still out —
  bold here means specific, not loud.
- The product name is **WFSim** (repo slug stays `wfsim`). The wordmark is
  two-tone: "WF" + gold "Sim". Logo: `web/src/static/logo.svg`. In Chinese PROSE
  the product is **WF模拟** — the wordmark itself is never translated, so the
  topbar and `<title>` stay WFSim while the zh footer says WF模拟. 沃肥模拟 —
  the phonetic reading of WF — is the COMMUNITY NICKNAME and stays out of the
  product entirely: it belongs in Bilibili titles and the QQ group, where being
  sayable and being a joke are both assets, and nowhere under `data/i18n/zh/`,
  where the claim being made is accuracy.
- Match the surrounding code's comment density and idiom; comments state
  constraints and sources, not narration.
