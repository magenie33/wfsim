# The desktop client

WFSim as a Windows app — the same `site/` the browser gets, served from a
directory the app can replace, so it updates itself and never sends anyone back
to a download link.

## Why it exists

The web build's art and its 5.43 MB wasm module come from Cloudflare, and
Cloudflare from mainland China is the least reliable thing on the page — which
is where the players are. MEASURED from Shanghai on 2026-08-26: the wasm module
downloads at **2.11 MB/s** from wfsim.app and at **9.73 MB/s** from a Tencent
COS bucket, and locally it is not a download at all — the module instantiates
in **~200 ms**. Multiply by the compute lanes, each of which loads its own copy,
and the difference is the whole first impression.

Three more things fall out of being local, and one of them is a capability
rather than a speed:

- **Offline.** Every simulation already runs on the reader's own CPU; the
  network was only ever delivering the files.
- **Presets survive.** Builds, scenarios and rivens live in `localStorage`;
  "clear browsing data" takes them all. The app has its own profile.
- **The 5 MB origin quota is gone.** `stripReplays` and `shedOtherResults`
  exist because a replay is 65 KB against a 1.6 KB summary and seventy weapons
  fill an origin. A desktop build can keep them. *(Not done yet — the shell
  ships first.)*

**It is not faster to compute with, and that is deliberate.** The engine could
be called natively instead of through wasm, worth perhaps 20–50%, but then the
shell would depend on `engine/` and every formula change would mean
re-downloading the whole 34 MB executable instead of swapping a file. This
project releases too often for that trade: see "two layers" below.

## Shape

```
desktop/                     an INDEPENDENT cargo workspace — it depends on
  build.rs                   nothing in engine/, so the main CI never compiles
  src/                       Tauri and the shell has no reason to change when
    main.rs                  the engine does
    layout.rs
    payload.rs
    protocol.rs
    update.rs
%LOCALAPPDATA%\WFSim\
  current\                   what the webview serves — the only directory read
  next\                      what the updater is assembling
  prev\                      the version `current` replaced; the way back
  boot.json                  consecutive launches that never rendered
```

The window is a Tauri 2 webview pointed at a **custom protocol** that reads
`current/`. Not a localhost server: a server listens on a port, and a port is a
firewall prompt on every install. `protocol.rs` serves files with
`Cache-Control: no-store` — the filenames are fixed, so after a swap the webview
would happily serve its own cached copy of the old `app.js`.

## Two layers, and why the frequent one is quiet

| | changes | how it ships | what the reader sees |
|---|---|---|---|
| **content** — `app.js`, `pkg/*.wasm`, `img/`, `board.json` | every push | files, swapped by two renames | a notice, then a restart |
| **shell** — the `.exe` | rarely | one file, downloaded once | a download |

Everything that moves a number is content. So the path that runs weekly is a
directory swap, and the path that means downloading 34 MB again runs almost
never. **Keeping the shell thin is the update strategy**, not tidiness.

**There is no installer**. The whole app is an
`include_bytes!` inside the executable, so the file cargo produces is the
product: double-click to run, delete to uninstall, no registry, no
administrator prompt. An NSIS bundle was worth 9 MB of compression and a Start
menu shortcut, and cost the **bundler** — the source of both of this project's
"builds cleanly, ships the wrong thing" failures, below. Portability changes
nothing about updates: the updater's target is `%LOCALAPPDATA%`, which has
nothing to do with where the executable sits.

The page-side updater lives in `app.js` (`mountDesktopUpdater`), not in the
shell's injected script, for the same reason: an updater compiled into the shell
cannot fix its own bugs, and every reader would be frozen on the broken version.
`app.js` is the thing updates replace.

## The channel

A signed manifest lists every file with its SHA-256. The client compares it with
the manifest describing its own `current/`, fetches what differs, copies the
rest out of `current/`, and hands the finished directory to `Layout::promote`.

- **Content-addressed** — files live at `blob/<sha256>`. Each distinct file is
  stored once for ever, a release uploads only what is new, and a reader who
  skipped ten versions still downloads only what they are missing. MEASURED: a
  one-file release is **0.8 KB over the wire, 1 of 764 files**.
- **Signed, ed25519** — the bucket is public-read over a network that is
  routinely interfered with. Without a signature the channel is a way to run
  arbitrary code on every reader's machine.
- **Sources are a list, and the manifest may replace it.** Moving to another
  provider costs a manifest, not a new installer. Hard-coding one origin would
  make that origin's bad day every reader's reinstall.
- **The manifest is published last.** Until it changes no client is looking at
  the new blobs, so a half-finished upload is invisible rather than broken.

### The private key is the one unrecoverable thing

`private/wfsim_update_key` (gitignored). Lose it and no installed client can
ever be updated again — every reader frozen, the only way out a manual
download, which is the outcome this whole design exists to avoid. **Back it up
somewhere that is not this machine.** The public half is compiled into
`update.rs`.

## Rollback

Two launches that never report a rendered page make the shell move `prev/` back
into place. `HEALTH_PROBE` polls for up to a minute rather than sampling once —
an earlier version checked at a fixed four seconds, which on a slow machine
would have rolled back a perfectly good update, on exactly the machines least
able to re-download it.

This is the one piece of recovery that cannot live in JavaScript: it runs before
any of it does.

## Working on it

```
cargo build --manifest-path desktop/Cargo.toml     # payload is rebuilt from site/
desktop/target/debug/wfsim-desktop.exe             # run it
desktop/target/debug/wfsim-desktop.exe --selftest  # 12 assertions, exits non-zero
desktop/target/debug/wfsim-desktop.exe --reset     # forget current/ and unpack again
```

`site/` must be built first (`python scripts/build_site_app.py`); `build.rs`
refuses otherwise. The payload is the client's slice of `site/` — 764 files,
29.2 MB — and it drops `og/` and `weapons/`, which are 38 MB of link-preview
cards and prerendered crawler pages that a desktop app has no use for.

**The file list is declared once**, in `build.rs`, which writes what it built to
`desktop/target/payload-manifest.json`; the release script reads that back
rather than keeping a second copy.

### Checks

- `--selftest` — wasm engine, page render, SPA fallback, storage, compute
  lanes, external links (both directions), IPC, the AGPL source offer, and the
  update channel. Run it on the RELEASE binary before shipping: that is the one
  being downloaded, and it is built differently from the debug one.
- `python scripts/check_desktop_update.py` — publishes a baseline, changes one
  file, publishes again, and drives a real client through seeing, fetching and
  applying it, then verifies the change by reading it out of `current/`. Ends
  with the two refusals: a blob that fails its checksum and a manifest whose
  signature does not verify must both leave `current/` untouched.
- `--measure-lanes N` — what a compute lane costs in memory. MEASURED at
  **48 MB**, linear from 1 to 16, which is what removed the lane ceiling.
- `--test-open <url>` — the real browser handoff, no window involved.

## Releasing

```
python scripts/ship.py                    # the whole of it, and it verifies
python scripts/ship.py --verify           # do the channel and site/ agree now?
```

**THE CLIENT IS THE SITE — same files, same bugs — and one command is what makes
that true.** `ship.py` builds `site/`, rebuilds the payload that declares what
the client gets, publishes the channel, commits, pushes, and then fetches the
manifest it just published and hashes every file it names against `site/`. A
mirror that is only promised drifts: pushing `main` deploys `site/` on its own,
so a channel published by hand is a channel published when somebody remembers.
It refuses a tree that is dirty outside `site/`, because what ships has to be a
tree that can be checked out again.

The three steps it runs, for when one of them has to be run alone:

```
python scripts/build_site_app.py          # site/
cargo build --manifest-path desktop/Cargo.toml
python scripts/release_desktop.py         # blobs + source.zip + signed manifest
```

~2.6 s when little changed. Only a change to the SHELL needs a new download:

```
python scripts/build_desktop.py           # dist/WFSim.exe + source.zip + notes
```

~52 s, and it is plain `cargo build --release` — no bundler.

No version number is ever chosen, and none appears in the filename. Windows
needs a version field for its own bookkeeping, so it comes from the build date;
what identifies a build for a bug report is the **commit**, which the page
footer already shows.

**The download is always `WFSim.exe`, and the notes carry no date.** A share
link is tied to the filename, so renaming invalidates every link already
posted. And a date would say something untrue: a file stamped August, read in
December, looks stale, when in fact any copy updates itself to current on its
first run. The SHA-256 tells two builds apart, and does it precisely.

### The network drive

`dist/` holds `WFSim.exe`, `source.zip` (AGPL requires the source to be offered
wherever the binary is) and `使用说明.txt`, which says which of the two to
download — **specifically that the source archive is not something a player
needs** — plus the SHA-256 and what to do about SmartScreen.

Upload once. It is a **download link, not an update channel** — a network drive
has no stable direct URL, and automated downloads would not count as real
traffic anyway.

### The offer: a pointer, and a page

`/download` is the offer. It is a page of the SHELL like `/support` and
`/benchmark` — it belongs to no weapon, and it is **not a fourth module**,
because it produces nothing the builder, the simulator or the optimizer
consume.

It is a page rather than a button because what a downloader asks is a page:
what SmartScreen does on first run, why the program is unsigned, what updating
costs, what uninstalling means, where the source is. The SmartScreen section is
the **owner's own wording**, transcribed from the notes file that ships beside
the binary — the notes answer the warning after the download, and the page
answers it before.

The URL is also the half that can be said out loud: a reader who saw this in a
video types `wfsim.app/download`.

The home hero carries **one line** pointing at it. That page is read by someone
who has not yet seen the tool work, which is the worst moment to ask them to run
an unsigned executable; the people who want the client are the ones already
using the site.

**Windows only.** The release workflow still cuts `WFSim.AppImage` and the
GitHub release page still lists it; the offer is not about it.

**The source is named beside the button.** This site does not host the
executable — it is on a Quark network drive — and a reader about to run a
downloaded binary is entitled to see whose drive it is on before they click.
The badge links to the same place, so "where does this come from" and "take me
there to look first" are one click apart. Its mark is a **generic drive glyph,
not Quark's own logo**: the badge has to say which service this is, which the
name does, and reproducing somebody else's trademark would borrow their mark
for a claim they have not made about this file.

**Inside the client**, `/download` says there is nothing here to install. The
URL is typed by hand and read off a video, so it has to answer there too.

`scripts/check_downloads.mjs` holds all of it. The NAME is what it asserts
rather than the element — an icon alone identifies nothing, which is the
failure the badge exists to prevent — and it asserts that the page ANSWERS each
question, since a page carrying the button and none of them is the button with
more scrolling. Its negative control is the other two desktops: a Mac or Linux
reader must be told, and handed no download link. It also reads the built
`site/download/index.html`, because the prerendered head is the half no browser
assertion can see.

## Three ways this builds cleanly and ships the wrong thing

All three were found in one afternoon, all three produced a zero exit code, and
none of them is visible in a log. They are why `build_desktop.py` refuses an
executable under 20 MB and why `check_desktop_probes.mjs` exists. Two of the
three were the BUNDLER, which is no longer used — dropping the installer
dropped them with it.

**The bundler picks a binary, and picks the wrong one.** With two `[[bin]]`
targets in the crate, Tauri bundled `updatekit` — the signing tool — renamed it
to `wfsim-desktop`, and produced a 288 KB installer that installs cleanly and
runs something that is not this app. `mainBinaryName` renames the choice; it
does not make it. The fix is to have one binary: `updatekit` is an
`examples/` target, invisible to `cargo build --bins`.

**LTO deletes the payload.** The app is a 29.2 MB `include_bytes!` static, and
`lto = true` drops it as unreachable: the same source builds to 268 KB with LTO
and 33.7 MB without. Both link, both run, and the small one unpacks nothing.
LTO is off, and costs nothing here — 87% of this binary is data.

**Chrome renders a blank icon and reports success.** `--screenshot` at sizes
between about 96 and 160 px writes a well-formed, fully transparent PNG and
exits 0. `icon.ico` shipped with an empty 128 px layer — the size Windows uses
for large-icon views. `make_icons.py` now counts opaque pixels in every layer
and fails on any that is empty.

The shared shape: **a generator that does not check its own output**. Each one
was caught by an assertion about the artifact, never by the tool that made it.

## Not signed

No code-signing certificate. SmartScreen shows its blue warning on first run and
the notes file says so with the steps. An OV certificate (~¥2000–5000/yr) would
not remove the warning immediately either — SmartScreen goes on file reputation,
so a fresh certificate still warns, just with a name attached. Revisit if there
is ever a reason to.
