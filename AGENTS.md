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

- `engine/` — all game mechanics. Every formula carries a comment citing
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
- UI verification: drive headless Chrome over CDP (Node ≥22 has a global
  WebSocket; Chrome is at the default install path). Assert real DOM
  state; screenshots for layout review. `node scripts/check_parity.mjs`
  is the committed one — the builder and the optimizer must offer the
  same options and the same visibility on every axis, and it exits
  non-zero when they do not. Run it after adding a weapon or anything a
  weapon can carry.

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
- **FOUR collections, one per thing a module owns** (2026-08-02):
  `builder-builds` (a build), `simulator-scenarios` (a fight, buff settings
  included), `optimizer` (a search: scope + the funnel's final-round
  contract), `rivens` (a riven). The optimizer owns no scenario — it RUNS the
  simulator's, drawn by the same renderer over the same state, so the winner
  is scored under the fight the replay will run — READ-ONLY there, with a link
  to the simulator: a preset is edited in exactly one place, because two
  editors over one document is how it gets edited twice and saved once. Its three old collections
  (`optimizer-mods` / `-arcanes` / `-evolutions`) merged into one: they were
  split for cross-weapon reuse, which is the import's job.
  A build preset also carries a `sim` SNAPSHOT — "what this build was last
  tested with". That is a copy, not a pointer to a scenario preset, and it
  stays one (user, 2026-08-02): editing a scenario later must not rewrite
  what an old build says it was measured under.
  Every collection writes through `storePresetList`, which is what makes one
  Ctrl+Z stack cover all four — presets auto-save, so the way back is not
  optional.
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
