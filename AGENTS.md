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
- `docs/` — CORE (design), MECHANICS (formulas), MEASUREMENTS (protocol +
  baselines), BUFFS, OPTIMIZER, UI, WASM, GLOSSARY, DEVELOPMENT (setup).
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
  and a site regeneration to reach wfsim.app.
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
- **Optimizer verification: `cargo run --release --bin wfsim-truth -- pool=<ids>
  …`**. A search cannot vouch for itself, so it is GRADED: the tool exhausts the
  scope, evaluates every job flat, and reports where the production search
  landed in that reference ranking (rank / regret / recall / cost, and whether
  the reference reproduces itself under a second seed). It goes through
  `parse_optimize`, so it grades the app's own fight, and it REFUSES a scope it
  cannot exhaust. Run it after ANY change to enumeration, scheduling or
  scoring. The cheap CI form is `optimizer/tests/search_accuracy.rs`. See
  docs/OPTIMIZER.md §Accuracy.
- UI verification: drive headless Chrome over CDP (Node ≥22 has a global
  WebSocket; Chrome is at the default install path). Assert real DOM
  state; screenshots for layout review. `node scripts/check_parity.mjs`
  is the committed one — the builder and the optimizer must offer the
  same options and the same visibility on every axis, and it exits
  non-zero when they do not. Run it after adding a weapon or anything a
  weapon can carry. `node scripts/check_search.mjs` is the TENTH and the
  end-to-end one: it runs a real optimize in the shipping wasm build and
  asserts the two claims the search makes — a scope it finished reports
  `exhaustive` and says so on screen, a budgeted one reports its COVERAGE and
  does not pretend. `node scripts/check_gain_freshness.mjs` is the ninth: a
  scenario edit reaches the quick calc immediately, including a field nobody
  has invented yet — the scan's cache key is DERIVED from the fight it will
  run, never a hand-listed copy of it. `node scripts/check_build_size.mjs` is the eighth: how full
  a searched build must be is a RANGE (`build_min`–`build_size`), so "exactly 8
  mods" is a setting rather than something the scope cannot express — both ends
  push each other, both ride the search preset, and both reach the request.
  `node scripts/check_buff_cards.mjs` is the seventh: buff
  cards are named in the display language (an EVOLUTION's buff was the last one
  left in English), open at the stack count the rule says, and report a
  coverage that is never rounded up to a flat 100%.
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
  search: the SCOPE and HOW to run it — finalists and CPU threads (never
  buffs: those are the fight's). The
  optimizer tab is two halves split at its two preset bars, with nothing on it
  belonging to neither: everything above the fight's bar is the search preset,
  everything below is the simulator's, read-only. The final round is
  `finalists × the SCENARIO's runs` — how hard you measure belongs to the
  fight, so the optimizer offers no control for it (user, 2026-08-02)).
  There is always ≥1,
  "active" means the state you are in, and the key is
  `wfsim-presets-<weapon>-<domain>`. A **custom** is a thing you MADE that the
  OTHER modules consume — `rivens` becomes a mod in the pool, custom enemies
  will become entries in the scenario's enemy list. Owning none is ordinary,
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
  **NOTHING CROSSES BETWEEN WEAPONS** (user, 2026-08-02: "绝对不能串"). Two
  weapons' fights may LOOK alike; they are never born from each other. A weapon
  opened for the first time gets `defaultScenario()` — the server's defaults and
  nothing else — because the live `sim` at that moment still belongs to the
  weapon you just left. The same applies to the search's `finalists`/`threads`,
  and the previous weapon's optimizer RANKING is cleared rather than left on
  screen under the new weapon's name.

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
