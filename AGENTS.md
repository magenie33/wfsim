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
  graph; `docs/DATA_SOURCES.md` the sourcing rules. **Two sources, always
  cross-checked**: the wiki Lua modules and WFCD's export (`vendor/`). The wiki
  is NOT authoritative for `base_drain`/`max_rank` — it is wrong for ~20 mods.
  Join the two by `internal_name` == `uniqueName`, **never by name** (WFCD has
  stale duplicates sharing a display name).
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
  blocking on work a runner was doing anyway (owner, 2026-08-11: "榜单那个你是
  不是完完全全服务器自己跑就可以了啊…因为还有很多功能需要做"). Use it WITHOUT
  `--write` when you want to know whether a change moved anything; let the
  workflow write.
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
  directories and 17 GB of C: later (owner: "我有一堆临时文件，在C盘都快满了"),
  `finish` kills the whole tree (`taskkill /T` on win32), waits for it, and
  retries the removal — and `sweepStaleProfiles` deletes any `wfsim-*` older
  than an hour ON THE WAY IN, which is the only cleanup a run that throws, is
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
  script is a gap nobody can act on (2026-08-08: "我需要用户也能看见，因为我也是
  这样排查的"). Each surface has gone silent at least once: an arcane effect the
  loader had no arm for went to `Inert`, which printed NOTHING, so both
  Deadheads promised a recoil reduction they did not apply. It also covers the FOURTH kind of admission, which is the only one
  that is not a shortfall: a LIVE BUG (`live_bugs:` on an arcane) says the
  number is RIGHT, the game is wrong, and a hotfix changes it — Primary
  Debilitate's split leaks its zero-damage instance's multipliers into the DoT
  it leaves (MEASUREMENTS M37), so a player building around x441 is told what it
  rests on (owner, 2026-08-08: "我要建立啊，但是标记可能非本意，我要忠实原本游
  戏，如果修了那我就改"). It carries a
  NEGATIVE CONTROL — a weapon with nothing to admit shows no banner — because a
  check that only asserts presence passes just as well on a page that shouts
  "not modelled" at everything, and it runs the whole pass in BOTH languages,
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
  do not stack AND the page says which one lost (owner, 2026-08-08: "同时选了
  roar 和 roar（helminth），那就选择生效当前最强的" — the difference between
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
  folds and REMEMBERS across a re-render and a reload (owner, 2026-08-11: "每个
  小块都应该支持可伸缩") — a panel that re-opens everything on every Run Sim is
  a panel you re-close on every Run Sim, so the state lives outside the markup.
  It caught a real one on the way in: the opening window never closed on a
  weapon that TRANSMUTES instead of reloading, because it was recorded at the
  reload rather than at the refill.
  `node scripts/check_hit_account.mjs` is the TWENTY-FOURTH: THE ACCOUNT OF ONE
  HIT HAS TO MULTIPLY OUT. Every other number the sim reports is an aggregate,
  and an aggregate hides an error inside an average — a factor applied twice, or
  in the wrong bracket, moves a mean by a few per cent and reads as "this build
  is good". The account is the one output that can be FALSIFIED (owner,
  2026-08-11: "方便我可以根据数据里找出计算瑕疵"): one damage instance per attack
  part from the median engagement, every factor listed with its value in the
  order the engine applies them, and the product is the number that went into
  the damage meter. The check does the arithmetic a reader would do, so a factor
  applied and not listed — or listed and not applied — fails it. That is why the
  account is written at the ONE site where every factor exists at the same time
  rather than reconstructed afterwards. Verified to bite: dropping the crit line
  gives 1,510 against a claimed 13,292. It also asserts the panel draws it, since
  a ledger nobody can see is a ledger nobody checks.
  `node scripts/check_debuff_coverage.mjs` is the TWENTY-THIRD: the DEBUFF table
  is the BUFF table, read from the other side. The replay had always shown what
  the BUILD had up — live stacks, uptime, dead bands, the ramp — and said nothing
  about what was on the TARGET, which is the other half of the same fight and
  the half that explains the number (owner, 2026-08-11: "你就和我们现在的buff列
  表对称"). It is one component fed from both sides: `DEBUFF_ROSTER` is the
  mirror of `buff_roster`, `Frame.debuffs` of `Frame.stacks`, and the page draws
  the second table with the same renderer. The check asserts the SYMMETRY rather
  than the numbers — same roster shape, one series per entry, each as long as
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
  `node scripts/check_riven_pool.mjs` is the SEVENTEENTH: the riven editor
  offers the stats that weapon's rivens actually roll, in BOTH slots. What a
  riven can roll is DE's per-weapon table, published nowhere, and the wiki's
  25%-of-a-physical-type rule disclaims itself. THE RULES DECIDE AND THE SURVEY
  CHECKS (owner, 2026-08-08: "紫卡不应该是按照规则自动生成的吗？抓取只是来当验证
  才对"): `rivens_data::derived_for` is the model, `data/rivens/exceptions.yaml`
  overrides it per riven FAMILY with the evidence written into each entry, and
  `data/rivens/pools.yaml` (from `scripts/survey_riven_pools.py`) is read by a
  TEST and by nothing else. It was the other way round for a day and a re-run of
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
  says raise the runs (owner, 2026-08-12: "就不要出现约等于0的情况"). Both
  halves of the machinery behind it were wrong, and either one alone brings the
  symptom back. The scan read `score`/`dps`, which are the MEDIAN RUN — one
  engagement however many were paid for, moving 9.8% between seeds at 10 runs
  where the mean of the same runs moves 5.9%, and not even the statistic the
  optimizer ranks (`mean_kill_progress`). And it estimated its own resolution by
  running the reference a SECOND time at another seed: one sample of a spread,
  which on identical inputs answered anywhere from 0.7% to 11.2% — so the same
  scan censored every chip or none of them, at random. The server has all N
  runs and now reports what it already computed (`score_mean`/`score_se`,
  `dps_mean`/`dps_se`), which is one fewer simulation per scan and an answer
  that does not depend on a coin flip. A chip therefore has three shapes and the
  check asserts all three OCCUR, so no branch is dead: an exact one (`+165%` —
  the fight did not re-roll, same procs), a banded one (`≈+3.1% ±7.2%`), and a
  measured zero, which says "no effect here" in words and points at the row's
  own disclosure line — a third of a rifle pool lands there against one standing
  target (ammo and magazine mods, Firestorm, punch-through, recoil and zoom,
  Cautious Shot, a Bane of the wrong faction), and printing "+0.00%" 38 times
  reads as a broken scan rather than as UNMODELLED.md. Its NEGATIVE CONTROL is
  the pair the bug came in on: Serration and Amalgam Serration differ only in
  base damage, so neither re-rolls the fight, both compare exactly, and the
  order is the one the cards state — measured 0.9623 = 2.55/2.65 at every build
  strength and run count. The 3.8% gap between them is far inside the ±13% raw
  spread at 10 runs, so that ordering survives ONLY because the two are paired
  against the same luck, which is the thing that must not silently regress.
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

- **THE SIMULATOR IS THE TRUTH; THE OPTIMIZER OBEYS IT** (user, 2026-08-04:
  "我希望 optimizer 执行的，是 simulator 的规矩"). A search's winner is replayed
  under the simulator's fight, so any rule the optimizer applies that the
  simulator does not — or omits that the simulator applies — scores builds
  nobody can reproduce. The two must not be two implementations that agree;
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
- **Golden values only change with an in-game measurement** justifying
  it. New mechanics need golden tests; a faithful-looking implementation
  without a measurement is not correct.
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
  **A STRING IS TRANSCRIBED, NEVER TRANSLATED** (user, 2026-08-03: "为啥要你
  自己翻译啊，不是有官方文本吗"). DE's Chinese is routinely non-literal —
  Commodore's Fortune is 准将沐福 — so a name derived from the English is
  wrong more often than not (five Boar Prime evolution names were translated
  this way and four were wrong). If a source cannot be reached, LEAVE IT
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
  **NOTHING CROSSES BETWEEN WEAPONS — EXCEPT THE FIGHT** (user, 2026-08-02:
  "绝对不能串"; amended 2026-08-09). A BUILD, a SEARCH and a RIVEN are statements
  about ONE weapon and are never born from each other: a weapon opened for the
  first time gets a blank build, the search's `finalists`/`threads` reset, and
  the previous weapon's optimizer RANKING is cleared rather than left on screen
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
  is for (owner, 2026-08-09: "要是玩家自己想批量测试白富美…现在这样子太不方便
  了"). The one weapon-scoped knob it still holds is headshot %, handled the way
  the rulers handle it: the SERVER forces 0 on a weapon that cannot headshot. A
  shared bar offers no "⇤ import" — there is no other weapon to import from.

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
