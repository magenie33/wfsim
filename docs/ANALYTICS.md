# Analytics plan: how many people use WFSim, and how well

**Status (2026-08-02): NOT implemented. Phase 0 is zero-code and available
today; phases 1–4 are unwritten.** Nothing here blocks the UI rework or the
mechanics work — it is scheduled around them, not against them.

**Goal.** Answer two questions with numbers instead of guesses: *how many
people use wfsim.app*, and *how well do they use it*. Not marketing
attribution — where visitors come from is explicitly out of scope (owner,
2026-08-02). The output is roadmap fuel: which module and which weapons are
actually exercised, and whether a visit produces a RESULT or nothing.

## The problem this exists to solve

The deployed site makes **zero server requests after boot**. `api()`
(`web/src/static/app.js:282`) takes the wasm branch on the static build
(`WASM = !!window.WFSIM_WASM`, line 10) and dispatches to worker RPC —
`/api/simulate`, `/api/optimize` and the rest never touch the network. The
same property that makes the site fast, offline-capable, and unblockable
also makes it invisible to server-side analytics: Cloudflare cannot tell a
bounce from a 40-minute optimizer session.

So the edge can answer "how many people" but is structurally incapable of
answering "how well". The second half requires a first-party beacon. There
is no zero-cost option for it.

## Rules this plan is bound by

- **Same-origin, first-party.** The beacon posts to wfsim.app itself, for
  the same reason the art moved in-repo (rule 2026-07-31): a third-party
  script host that mainland China blocks would under-count precisely the
  players WFSim is for. This rules out Google Analytics
  (`googletagmanager.com`) and makes Cloudflare Web Analytics
  (`static.cloudflareinsights.com`) unreliable for the core audience.
- **Event names derive from the DOMAIN, never the UI.** The three modules
  and the editors are the namespace — `builder.*`, `simulator.*`,
  `optimizer.*`, `rivens.*` — exactly as preset collections are named
  `<owner>-<collection>`. The UI is expected to change; the modules are
  not. An event named after a button dies with the button, and **history
  cannot be backfilled**, so a renamed event is a permanently broken time
  series. This is the one decision that is expensive to get wrong later.
- **No PII, no cookies, no accounts.** Retention needs a stable identifier
  and nothing more: one random `wfsim-cid` in localStorage, alongside the
  existing `wfsim-lang` / `wfsim-picker` / `wfsim-theme` keys. Clearable by
  the visitor like any other site data.
- **English ids only.** Weapons report their English `name_en`-derived id,
  never a localized name — same rule as wiki URLs.

---

## Phase 0 — read what Cloudflare already recorded (no code)

Cloudflare has been counting at the edge since the domain went up, and the
data is retrospective. Open the `wfsim.app` zone → Analytics & Logs →
Traffic for unique visitors, requests, and geography.

One usable behavioral signal exists today for free: `site/img/` is
same-origin, so opening a weapon page pulls its art, and Top Paths shows
**which weapons get opened**. Caching swallows repeat visits, so treat it
as relative heat, never as a count.

**Done when:** a baseline (weekly uniques, top weapon images) is written
down somewhere durable, so later numbers have something to be compared to.

---

## Phase 1 — the collection channel, three events

`wrangler.jsonc` is assets-only (no worker script). Add a minimal worker
that accepts `POST /api/e` and writes to **Workers Analytics Engine**;
everything else keeps falling through to the static assets exactly as now.

> **Open question:** confirm Analytics Engine's availability and quota on
> the current Workers plan before building on it. If it is not available,
> the fallback is a counter in KV or D1 — the frontend contract below does
> not change either way.

Frontend: one `track(event, props)` helper and a `wfsim-cid`. Three events
only — the minimum that answers "how many, and did they get anything":

| event | fires when | why |
|---|---|---|
| `app.boot` | once per session, after the wasm engine is ready | denominator; also separates "loaded" from "loaded and worked" |
| `builder.result` | a panel is computed with ≥1 mod equipped | ACTIVATION — distinguishes "opened the default weapon" from "actually built something" |
| `optimizer.run` | an optimize run REACHES completion | the flagship feature, measured on finish (a start can be cancelled) |

Common props on every event: `cid`, `locale`, `weapon` (English id), and a
schema version so later changes stay readable.

**Done when:** the three events land in production, `check_parity.mjs` still
passes, and no beacon failure can break a page (fire-and-forget, never
awaited, never blocking a render).

---

## Phase 2 — coverage

Same events, richer props: module in view, weapon class, whether a riven is
equipped. This is what turns analytics into a roadmap — which weapons and
which module are exercised, so the next build decision has evidence behind
it instead of intuition.

**Done when:** a weapon can be ranked by real usage.

---

## Phase 3 — read it back

Data nobody looks at is data nobody collected. A small query script (or an
internal page) producing, on demand:

- weekly actives (unique `cid`)
- **activation rate** — sessions with a `builder.result` or
  `optimizer.run` ÷ sessions with `app.boot`
- **7-day / 30-day return rate**
- top weapons, module split, locale split

Activation and return are the two that matter. Pageviews are not a measure
of a calculator; producing a result is.

**Done when:** the four numbers can be pulled in one command.

---

## Phase 4 — depth and sharing (decide AFTER phase 3 has data)

Deliberately unspecified. Candidates: optimizer runs per session, weapon
switches per session, riven editor engagement, and — once shareable build
links exist — *shared* and *opened-from-share*. A shareable build is the
closest thing a calculator has to a network effect, and both halves of it
are usage, not attribution, so they belong here.

Do not pre-build this. Let phase 3's numbers say which axis is worth
splitting.

---

## Sequencing note

Implementation is deferred while the UI changes and correctness work
continues; phase 1 is roughly half a day.

One consideration for *when*: judging whether a new UI is better requires
data from the old one, and old-UI data can only be collected while the old
UI is live. Every week without a beacon is a week of history that cannot be
recovered. That argues for phase 1 landing before the rework rather than
after — but it is a call about what the numbers are worth, not a technical
constraint. The event vocabulary above is deliberately UI-independent so
that the rework cannot invalidate it either way.

## Explicitly not doing

Referrer and campaign attribution (out of scope by decision), Google
Analytics (blocked host, wrong audience), user accounts (roadmap has them
behind OAuth much later), and any third-party script on the critical path.
