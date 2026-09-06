# Distribution: one channel, many mirrors, many shells

**Status: phases 1–4 implemented. Phase 5 is written and unconfigured** — the
stage job needs COS credentials in the repository's secrets and is silent and
green without them. See §Phases.

## What this answers

Every simulation runs on the reader's own CPU, through the same engine compiled
to wasm. So "the browser and the desktop client compute the same number" is not
a question about the engine at all — it is a question about **which build of it
the reader is running**. Distribution is the whole of it, and this file is where
that is decided.

## Three planes

A file's change rate and its trust requirement decide which plane it belongs to.
There are three, and **a file may not travel in a plane that is not its own.**

| plane | what | changes | signed | may lag |
| --- | --- | --- | --- | --- |
| **release** | `index.html`, `app.js`, `style.css`, `worker.js`, `pkg/` | on a code change | **yes** — it is code | **no**: two channels on different releases give different answers |
| **assets** | `img/`, `pol/`, `logo.svg` | rarely, and only by addition | no — the hash is the check | yes: an asset is present and correct, or absent |
| **data** | the board | every hour | no — it is reproducible | yes, briefly, and visibly |

The cost of ignoring this is not theoretical. `site/board.json` is 4.3 MB and is
rewritten every hour; the blob store is never pruned. Carrying it
in the release would bank about 38 GB of immortal blobs a year in the one place
this project pays for, and hand every client a 4.3 MB download for a file it
replaces within the hour. **So the planes are separated before the publish is
automated, not after.**

## Three words

- **Channel** — the stream of releases. There is **exactly one**, and it serves
  the browser and every installed client alike.
- **Mirror** — somewhere bytes can be fetched. There are several and they are
  interchangeable, ranked by speed alone.
- **Shell** — a program that hosts the page: a browser, the Windows client,
  whatever comes next.

Three rules fall out of the vocabulary, and most decisions answer themselves
once they are stated:

- **A shell has no version of its own.** It has a build number for its own
  binary; the release it runs is everyone's release.
- **A mirror may lag. A mirror may not be wrong.** Every byte fetched is checked
  against a hash the manifest already named, so a mirror is either correct or
  short — never a third answer.
- **The channel may not fork.** Nothing is ever built twice for two consumers.
  Two builds of one tree are not guaranteed to be one artefact, and a guarantee
  that rests on "they should be the same" is not one.

## The web is a client, not a second publisher

wfsim.app is a **mirror**, and the page it serves is a **client**. This is what
makes "every channel agrees with the web" a property rather than a hope: if the
web published on its own path, there would again be two publishers of one truth,
and two publishers drift — that is the whole of it.

So the page carries the same release identity every other client carries, and
its assets are named by content:

- `pkg/` is written under content-addressed names and served `immutable`.
  Without this a browser holding a cached `app.js` can pair it with a freshly
  fetched wasm module, which is the same "same seed, different answer" bug one
  layer down and invisible from both ends.
- `index.html` is the only mutable document of a release, and it is replaced
  atomically with it.

## Identity, and where it is shown

Three identifiers, printed by every client. The footer carries the first, on
every page; `/support` carries all three:

```
release <digest> · commit <sha>
board <digest> · <scored at> · <n> rows
shell <build>                     the only per-platform version there is
```

A reader who says "our numbers disagree" is answered by reading two strings.
Without them the only way to tell a version difference from a real defect is to
hash several hundred files by hand, which is not a thing anyone does in a chat.

A line is **omitted rather than guessed**: the dev server has no release, a
browser has no shell, and `dev` printed beside two real digests is a string a
reader would quote to no purpose.

## The release plane

A **release** is the immutable set of files a client runs, listed in a manifest
with each file's SHA-256. Two identifiers name it, and they answer different
questions:

- **`release`** — a digest over the release plane alone (`scripts/build_site_app.py`,
  `release_id`), published as `site/release.json` and stamped into the page.
  *Is this the same code?* It is the identifier that survives a shell shipping
  different art, so it is the one clients print and readers quote.
- **`manifest`** — the digest of the manifest's own bytes. *Is this the same set
  of files on disk?* It is what the updater fetches and checks against.

The file list is declared once, in `desktop/payload.lst`, and read by both
`desktop/build.rs` (which packs it into the binary) and
`scripts/payload_manifest.py` (which describes it to the channel without a Tauri
toolchain). `ship.py` holds the two outputs against each other on every run.

Publishing is two acts, and they are kept apart because their requirements are:

1. **Stage.** Upload every blob, the manifest at `manifest/<digest>.json`, and
   last a marker at `release/<release>.json` naming it. Needs a toolchain, a
   network and bucket credentials; needs **no key**, and changes nothing any
   client reads.
2. **Promote.** Resolve the marker, check it, sign a pointer and put it up.
   Needs the key, takes a second, and **builds nothing** — so a promotion cannot
   produce bytes that differ from the ones already staged and deployed.

**The gate is the release, not the manifest.** A manifest names the commit it
was built from, so its digest moves on every push whether or not a served byte
did; the release digest moves when the code does. That is what makes staging on
every push cost one request an hour instead of a publish.

It is also why promotion resolves through the marker rather than recomputing:
recomputing would produce a different digest on any later commit, and then
promotion would refuse a release that is staged and correct.

The failure mode of a forgotten promotion is that readers stay on the last good
release. **Late, not divergent** — which is the property the split is for.

### The pointer

`channel.json` is one object, and the signature travels inside it:

```json
{ "sig": "<hex>", "signed": "{\"manifest\":\"…\",\"release\":\"…\",\"sources\":[…],\"version\":\"…\"}" }
```

`sig` covers the UTF-8 bytes of `signed`, verbatim. **One object, because two
objects fetched separately can be torn**: a client that reads a new manifest
beside an old detached signature is told its update failed verification, which
reads as an attack and is a publish landing between two GETs.

A client verifies the signature, fetches `manifest/<digest>.json`, and checks
that what came back hashes to the digest it just verified. The manifest is then
content-addressed and cacheable for ever.

### Keys

**The shell accepts a LIST of public keys, never one.** The accepted set is
compiled into the binary, so a single key makes rotation a reinstall for every
reader — the one outcome this client exists to avoid. To rotate: ship a shell
that accepts both, wait, then publish under the new key.

### Compatibility

**`manifest.json` and `manifest.json.sig` keep being published, unchanged and
indefinitely.** A shell only changes with an installer, so shells that predate
the pointer stay in the field; taking their path away leaves them with no way to
update and no way to be told so. The cost is two small files per publish.

## The asset plane

Assets are content-addressed and shared across releases: the store holds each
distinct file once, a publish uploads only what is new, and a client that
skipped ten releases fetches only what it is actually missing.

**Delivery policy is per-shell; asset identity is global.** The Windows client
ships all of the art because "installed means offline" is why it exists; another
shell may stream it and cache. They are running the same release.

## The data plane

**The board is fetched, not released.** It is the output of a pipeline that runs
on a schedule, and it moves at its own rate.

- `board.json` is served at a stable URL and fetched at runtime.
- `board.meta.json` beside it is small and carries the stamp: the payload's
  digest, when each board was scored, and how many rows. A client reads the
  stamp to learn whether the copy it holds is current **without fetching 4.3 MB
  to find out** — which is the whole economy of the plane.
- The payload is served mutably rather than by digest **because there is exactly
  one live board and no client ever wants an older one.** The stamp is what
  makes staleness detectable; content-addressing it would buy caching this plane
  does not need.
- `scripts/board_meta.py` writes the stamp, and **both writers of the board call
  it** — the scoring job every hour, and the site build keeping a local
  tree in step. A stamp only one of them maintains lies for the other.

**A shell serves live data through its own protocol.** The page asks for
`/board.json` and gets the freshest copy the shell has, which the shell refreshes
in the background and keeps on disk beside the release rather than inside it. The
page is identical on every shell and no cross-origin configuration is involved.

**A client that has not fetched one yet asks the site.** `fetchJson` tries the
same origin and then `wfsim.app`, which is why `board.json` carries
`Access-Control-Allow-Origin`. On the web the first answer always wins; on a
shell the local copy is the offline one, and the fallback is what a fresh
install reads before its first refresh. A shell that has neither answers **404,
never its SPA fallback** — `index.html` with a 200 is what `res.ok` reads as
success.

There is deliberately **no seed in the payload**. A seed is stale on arrival and
it puts a file that moves every hour inside an artefact that must not,
which is the whole failure this plane exists to prevent.

**What a board still cannot say is which engine scored it.** `fp` is per row and
is dropped on the way to the page, so a client comparing its own run against a
row cannot yet tell whether the same code produced both. The stamp is where that
belongs when the engine exports one fingerprint for the whole of its data.

## The shell contract

A new platform implements this and nothing else. It is what keeps "we should
have an app" from meaning "write the updater again".

```
manifest()   which release this client is running
check()      resolve the pointer, verify it, diff against the local manifest
download()   fetch blobs, verify each hash, assemble the next directory
promote()    swap the directory
data(name)   serve the freshest live data this shell holds, else the seed
```

**Verification lives in the shell, never in the page.** The page is the thing
being replaced; asking it to verify its own replacement is circular. Policy and
interface stay in the page — when to check, what to show — because those are the
parts that should look the same everywhere.

**A miss on live data is a 404, never the SPA fallback.** `index.html` returned
with a 200 is what `res.ok` reads as success, and the page would then parse
markup as a board; the honest 404 is the empty board it already draws.

## Alignment is checked, not assumed

`scripts/check_mirrors.py` asks every mirror which pointer it serves and reports
any that disagree, with how far behind it is. Read-only, no credentials, so it
runs on a schedule and on demand. **An invariant nothing checks is a wish**, and
this one is invisible by construction: a stale mirror serves a page that works.

## Phases

1. **Identity.** *Done.* `release_id` over the release plane, `site/release.json`,
   `RELEASE_ID` stamped into the page, the footer and `/support` printing it,
   `board.meta.json` beside the board.
2. **Keys and contract.** *Done.* `PUBLIC_KEYS` is a list; the contract above is
   written down while there is still one shell to fit it to.
3. **The data plane.** *Done.* The shell keeps `live/` beside `current/`, asks
   the stamp every hour, and fetches the board only when the digest it
   holds is not the one being served.
4. **The pointer.** *Done.* `channel.json` published and preferred;
   `manifest.json` and its detached signature kept for older shells.
5. **Automation.** *Written; the CI half is unconfigured.* The stage → promote
   path is proved end to end from a workstation: staged, resolved through the
   marker, 866 files verified against the tree, pointer signed and read back,
   and a second stage answering "already staged". `stage.yml` runs on every push
   and computes the same manifest a Tauri toolchain does, then stops — it needs
   `COS_SECRET_ID`, `COS_SECRET_KEY`, `COS_BUCKET` and `COS_REGION` in the
   repository's secrets to reach the bucket, and is silent and green without
   them. `scripts/promote.py` and `.github/workflows/mirrors.yml` need nothing.

   **Adding those four secrets is what turns this on**, and it is the last act
   of the plan: after it, a push stages itself and the only thing left for a
   person is signing a pointer that builds nothing.

Two things are deliberately NOT on this list. **`site/` leaving git**: Cloudflare
deploys from the repository and the browser checks run against the committed
`site/`, so it costs two changes at once and buys little once the rest is in
place. **`channel.json` on wfsim.app**: a blob now falls back to
`<mirror>/<path>`, so the site is a real second source for the PAYLOAD — but it
does not serve the pointer, so resolving which release to fetch still has one
host. Publishing a signed pointer into `site/` would mean signing inside the
site build, which is the key back in the automatic path. The mirror check is
what says whether that one host is answering.
