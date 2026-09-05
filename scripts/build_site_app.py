#!/usr/bin/env python3
"""Package the in-browser builder into site/ (docs/WASM.md phase 4).

Steps:
  1. cargo build --release -p wfsim-wasm --target wasm32-unknown-unknown
  2. wasm-bindgen --target no-modules  ->  site/app/pkg/
  3. wasm-opt -Oz (if available; optional)
  4. copy web/src/static/{index.html,app.js,style.css,worker.js,logo.svg,pol/} -> site/
  5. inject <script>window.WFSIM_WASM = true;</script> into the copied
     index.html — that flag flips app.js's api() from fetch to worker RPC.
  6. write site/_headers and site/_redirects (what the edge is told about each
     path — see ship_edge_config)
  7. derive site/img/ from the art cache (same-origin art, downscaled to
     webp — see ship_art)
  8. prerender one HTML file per weapon (its own title/description/OG plus a
     crawler-visible summary and its board standing), the /weapons roster that
     links to them, and sitemap.xml + robots.txt — see prerender() for why a
     single shell was not enough.

wrangler already serves site/ at wfsim.app, so after this script the builder
lives at wfsim.app/ and every simulation runs on the visitor's own CPU.

Prereqs: rustup target add wasm32-unknown-unknown;
         cargo install wasm-bindgen-cli --version <matching Cargo.lock>.
"""

import functools
import hashlib
import html as html_mod
import json
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

import yaml

# THE C LOADER WHERE THERE IS ONE — measured 9.3x on this tree's own yaml, and
# `data/` is parsed several times a build: every weapon for the roster, every
# file again for the parse check, the board on top of that. It is the same
# parser behind the identical safe subset, so what it refuses and what it
# produces are unchanged; a Python built without libyaml falls back and is
# merely slower.
YAML_LOADER = getattr(yaml, "CSafeLoader", yaml.SafeLoader)


def yload(text: str):
    """`yaml.safe_load`, through whichever loader this Python has."""
    return yaml.load(text, Loader=YAML_LOADER)


# Same question, one answer: the fetcher decides what counts as an image and
# this gate refuses anything it would have rejected.
from fetch_images import is_image

ROOT = Path(__file__).resolve().parent.parent
STATIC = ROOT / "web" / "src" / "static"
APP = ROOT / "site"
WASM = ROOT / "target" / "wasm32-unknown-unknown" / "release" / "wfsim_wasm.wasm"
SITE = "https://wfsim.app"


def past_the_scanner(do):
    """Do one file write, past a real-time scanner holding the file.

    THIS SCRIPT WRITES ~1,200 FILES IN TWO BURSTS — the art, then a page per
    weapon — and Windows real-time scanning takes a handle on what was just
    written. The collision surfaces as `EINVAL` out of `open`, at a DIFFERENT
    file every run, and the same path succeeds a moment later. Retrying is the
    whole fix; without it a build dies two thirds of the way through and the
    wasm rebuild goes with it.
    """
    for attempt in range(6):
        try:
            return do()
        except OSError:
            if attempt == 5:
                raise
            time.sleep(0.2 * (attempt + 1))


def roster() -> list[dict]:
    """The weapons that get a page: one per WEAPON, not per form.

    `default_form` is the arsenal's form and therefore the roster row
    (engine::weapons_data::roster does the same filter) — a bow's tapped shot
    and an Incarnon form are forms of a weapon, not separate pages.
    """
    # A FORM MAY BE THE DEFAULT, and a form states only what DIFFERS from its
    # weapon (`weapons_data::INHERITED`). The Nataruk is the case: its arsenal
    # shows the PERFECT shot, so `default_form` sits on an entry that inherits
    # its class, slot and mastery rank — and this loader read the yaml raw and
    # died on the missing `class`. The engine merges before it
    # reads; so does this now.
    inherited = (
        "slot", "class", "mod_pools", "mastery_rank", "max_rank", "accuracy",
        "disposition", "polarities", "exilus_polarity", "riven_family",
        "internal_name", "noise", "magazine", "reload_seconds", "ammo_type",
        "ammo_max", "ammo_pickup", "traits", "deployment", "no_resupply",
    )
    specs = {}
    for f in sorted((ROOT / "data" / "weapons").rglob("*.yaml")):
        spec = yload(f.read_text(encoding="utf-8"))
        specs[spec["id"]] = spec
    out = []
    for spec in specs.values():
        if not spec.get("default_form"):
            continue
        parent = specs.get(spec.get("inherits"))
        if parent:
            spec = dict(spec)
            for k in inherited:
                if k not in spec and k in parent:
                    spec[k] = parent[k]
        out.append(spec)
    return sorted(out, key=lambda s: s["name"])


def wiki_name(name: str) -> str:
    """The WIKI PAGE name behind a display name.

    A display name may carry a parenthesised qualifier that is OURS, not part
    of the page: "Larkspur Prime (Atmosphere)" is one weapon on the wiki with
    two stat columns, and we ship the ground one. The wiki has no page by that
    name, so every URL — the page path, the OG card, the outbound link — is
    built from the bare name. `wikiSlug` in app.js splits on the same " (".
    """
    return name.split(" (")[0]


def wiki_slug(name: str) -> str:
    """The wiki page name as a path segment."""
    return wiki_name(name).replace(" ", "_")


@functools.lru_cache(maxsize=1)
def _slug_owner() -> dict:
    """For each wiki slug, the LOWEST id that answers to it."""
    own: dict = {}
    for s in roster():
        g = wiki_slug(s["name"])
        if g not in own or s["id"] < own[g]:
            own[g] = s["id"]
    return own


def url_slug(spec: dict) -> str:
    """THE PATH SEGMENT A WEAPON LIVES AT — the wiki page name, and the ID
    where that name is not this weapon's alone.

    URLs mirror wiki page names (AGENTS.md) and ids never appear, which holds
    for every weapon whose display name is its own. Two Kitgun slots are ONE
    wiki page and two roster entries, so the rule maps them onto one address
    and the loser of that collision has NO URL AT ALL — no prerendered page,
    no sitemap row, nothing to link. An id is uglier than a wiki name and it
    is reachable, which beats correct-looking and absent.

    THE LOWEST ID KEEPS THE WIKI NAME, so `/weapons/Tombfinger` is a stable
    address rather than one that follows roster order — and it stays the one
    the Slot control swaps INSIDE, which is what makes a Kitgun one page.
    `urlSlug` in app.js is the same rule and has to stay it.
    """
    g = wiki_slug(spec["name"])
    return g if _slug_owner().get(g) == spec["id"] else spec["id"]


def wiki_path(spec: dict) -> str:
    """The URL a weapon lives at."""
    return "/weapons/" + url_slug(spec)


# The app's dark palette (style.css `prefers-color-scheme: dark`), so a card
# pasted into a chat looks like the page it opens.
CARD_BG, CARD_TEXT, CARD_MUTED, CARD_GOLD = "#0e1014", "#f2f4f8", "#a6adbb", "#e8c37a"
# CJK-capable, in preference order; the last entry is the graceful give-up.
CARD_FONTS = ("C:/Windows/Fonts/msyhbd.ttc", "C:/Windows/Fonts/msyh.ttc",
              "C:/Windows/Fonts/simhei.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf")


def card_font(size: int):
    from PIL import ImageFont

    for path in CARD_FONTS:
        if Path(path).exists():
            try:
                return ImageFont.truetype(path, size)
            except OSError:
                pass
    return ImageFont.load_default(size)


# WHAT DRAWING A CARD DEPENDS ON BESIDES ITS ARGUMENTS. Bumped by hand when
# `og_card` changes what it paints: the digest below cannot see the code.
CARD_SIG = "card-v1"


def og_card(out: Path, name: str, cn: str | None, facts: str, stats: str) -> bool:
    """Draw the link-preview image for one weapon — OUR art, not DE's.

    The site now ships the weapon renders too (`ship_art`), so this is no
    longer about what we may host. It is about what a card has to SAY: a
    preview in a chat window is read in one glance, and a render on a
    transparent background says only "a gun". The card names the weapon in
    both languages, states the numbers the reader came for, and carries our
    wordmark — and being ours, no policy question touches it.
    """
    try:
        from PIL import Image, ImageDraw
    except ImportError:
        return False
    img = Image.new("RGB", (1200, 630), CARD_BG)
    d = ImageDraw.Draw(img)
    d.rectangle([0, 0, 1200, 6], fill=CARD_GOLD)
    mark = card_font(34)
    d.text((72, 64), "WF", font=mark, fill=CARD_TEXT)
    d.text((72 + d.textlength("WF", font=mark), 64), "Sim", font=mark, fill=CARD_GOLD)

    y = 190
    d.text((72, y), name, font=card_font(76), fill=CARD_TEXT)
    y += 96
    if cn:
        d.text((72, y), cn, font=card_font(40), fill=CARD_GOLD)
        y += 62
    d.text((72, y), facts, font=card_font(28), fill=CARD_MUTED)
    y += 52
    # The stat line is long; wrap it rather than let it run off the card.
    line, words, f = "", stats.split(" "), card_font(26)
    for w in words:
        probe = f"{line} {w}".strip()
        if d.textlength(probe, font=f) > 1056:
            d.text((72, y), line, font=f, fill=CARD_TEXT)
            y, line = y + 38, w
        else:
            line = probe
    if line:
        d.text((72, y), line, font=f, fill=CARD_TEXT)
    d.text((72, 548), "wfsim.app · builder · simulator · optimizer",
           font=card_font(26), fill=CARD_MUTED)
    out.parent.mkdir(parents=True, exist_ok=True)
    # DRAWN ONCE PER SET OF INPUTS. `img.save(..., optimize=True)` is 36 ms and
    # there are 387 of them — FOURTEEN SECONDS of a build, spent redrawing
    # pictures whose every input was what it was last time. The card is a pure
    # function of the arguments above plus the code that paints it, so a digest
    # of those answers "would this come out identical" exactly.
    #
    # THE CODE IS IN THE DIGEST TOO, as `CARD_SIG`: a card redrawn only when its
    # text moves is a card that never picks up a new layout.
    key = hashlib.sha256(
        "\x00".join([CARD_SIG, name, cn or "", facts, stats]).encode("utf-8")
    ).hexdigest()[:16]
    # THE SIGNATURE LIVES UNDER `target/`, not beside the card. `site/` is
    # committed, and 387 sidecar files nobody reads is noise in every diff; a
    # fresh clone has no signatures and redraws, which is the safe direction.
    sig = ROOT / "target" / "og-sigs" / (out.stem + ".sig")
    sig.parent.mkdir(parents=True, exist_ok=True)
    if out.exists() and sig.exists() and sig.read_text(encoding="utf-8").strip() == key:
        return True
    past_the_scanner(lambda: img.save(out, "PNG", optimize=True))
    sig.write_text(key, encoding="utf-8")
    return True


# WHAT THE EDGE IS TOLD ABOUT EACH PATH. Cloudflare's default for a static
# asset is `max-age=0, must-revalidate`, which buys a round trip per file per
# session — on the 2.11 MB/s the desktop updater measured to Shanghai, a picker
# opening forty icons pays forty of them before it can draw.
#
# ART ONLY, AND DELIBERATELY. `app.js`, `style.css` and the wasm are served
# under names that carry no content hash, so a reader holding a cached app.js
# beside a fresh wasm is one long `max-age` away — and that pair fails at
# runtime, far from anything that names the cache. Art is addressed by DE's own
# asset name, which changes when the picture does; a week bounds the case where
# it does not.
EDGE_HEADERS = """\
/img/*
  Cache-Control: public, max-age=604800
/og/*
  Cache-Control: public, max-age=604800
/pol/*
  Cache-Control: public, max-age=604800
/logo.svg
  Cache-Control: public, max-age=604800
"""

# Legacy URLs from the /app/-era layout, plus the one weapon page that shipped
# with an extension.
EDGE_REDIRECTS = """\
/app / 301
/app/ / 301
/app/* /:splat 301
/weapons/Dual_Toxocyst.html /weapons/Dual_Toxocyst 301
"""


def ship_edge_config() -> None:
    """Write `site/_headers` and `site/_redirects`, the two files the Workers
    asset layer reads (`wrangler.jsonc`).

    GENERATED RATHER THAN COMMITTED BY HAND. `site/` is this script's output and
    a hand-placed file inside it survives only because nothing here clears the
    directory — which makes it invisible to anyone reading the build to find out
    what gets deployed, and silently absent from a build into a fresh directory.
    """
    (APP / "_headers").write_text(EDGE_HEADERS, encoding="utf-8", newline="\n")
    (APP / "_redirects").write_text(EDGE_REDIRECTS, encoding="utf-8", newline="\n")


# WHAT THE PAGE IS SENT, AND WHY IT IS NOT WHAT WAS DOWNLOADED.
#
# `ART_MAX` is the largest size the CSS ever draws this art (`.wcard .wc-img`,
# 84px) doubled for a 2x display. Above that the pixels cannot be seen. If a
# surface ever draws art larger, raise this — a stale cap is a blurry page.
#
# WEBP, AND LOSSY. Measured over the cache: the downloads are already tight
# (448 of 855 are palettized PNG), so re-encoding at full size wins 1.2x and
# downscaling to PNG LOSES — resampling turns flat fills into gradients that
# PNG cannot pack. Only the pair wins: 27.2 KiB -> ~8 KiB, 3x. Lossless webp at
# this size is a wash, which is what makes q80 the choice and not the default.
ART_MAX = 168
ART_QUALITY = 80
ART_SIG = "art-v1"


def derive_art(src: Path, dst: Path) -> Path:
    """Downscale one cached image into the form the page asks for.

    DERIVED HERE AND NOT IN THE CACHE. `web/cache/img/` holds what the CDN and
    the wiki served, byte for byte — it is the record of what was fetched, and
    the native server answers `.webp` out of it directly (`img_response`). This
    is a DEPLOYMENT step, so it belongs to the thing that builds the deployment.
    """
    from PIL import Image

    # Same bargain as the OG cards: 855 encodes is most of a minute, and the
    # output is a pure function of the bytes in and the settings above.
    stat = src.stat()
    key = hashlib.sha256(
        f"{ART_SIG}\x00{ART_MAX}\x00{ART_QUALITY}\x00{stat.st_size}\x00{stat.st_mtime_ns}"
        .encode("utf-8")).hexdigest()[:16]
    sig = ROOT / "target" / "art-sigs" / (dst.stem + ".sig")
    sig.parent.mkdir(parents=True, exist_ok=True)
    if dst.exists() and sig.exists() and sig.read_text(encoding="utf-8").strip() == key:
        return dst
    img = Image.open(src)
    img.load()
    # RGBA THROUGHOUT: 448 of these are palettized and 66 already carry alpha,
    # and webp cannot take a P-mode image. Converting a palette that holds
    # transparency straight to RGB would fill it black.
    if img.mode != "RGBA":
        img = img.convert("RGBA")
    img.thumbnail((ART_MAX, ART_MAX), Image.LANCZOS)
    past_the_scanner(lambda: img.save(dst, "WEBP", quality=ART_QUALITY, method=6))
    sig.write_text(key, encoding="utf-8")
    return dst


def ship_art() -> None:
    """Derive `site/img/` from the art cache, so the static deployment serves
    game art from ITS OWN ORIGIN.

    Loading straight from the CDN goes through a redirector:
    `cdn.warframestat.us/img/X.png` answers 301 to raw.githubusercontent.com.
    GitHub is unreliable-to-blocked from mainland China, which is where the
    players are — so the app's own images would be the slowest, least reliable
    thing on the page for its actual audience. Same-origin removes the
    question entirely: if wfsim.app loads, its art loads.

    DE's art may be redistributed non-commercially — their Content Policy
    ("Use of Warframe assets must be non-commercial. You cannot profit from
    the direct sale of Warframe's IP"), and the wiki hosts the same files on
    the same basis. What the policy does forbid is their LOGOS, which is why
    the only mark on this site is our own (decision 2026-07-31, superseding
    "DE art stays out of the repo": ~4.5 MB, write-once, against a 2 MB wasm
    this script rewrites on every build).
    """
    cache = ROOT / "web" / "cache" / "img"
    assets = yload((ROOT / "data" / "assets.yaml").read_text(encoding="utf-8"))
    # `wiki:` says where the BUILD fetches it from; the cached file is bare.
    want = {
        v[5:] if str(v).startswith("wiki:") else v
        for table in assets.values() if isinstance(table, dict)
        for v in table.values()
    }
    # Art a data file declares itself, because the CDN does not carry it:
    # evolution icons (`icon:`) and enemy portraits (`image:`), both from the
    # wiki. Same rule as everything else — it ships, or the build fails.
    want |= {
        spec[field]
        for rel, field in (("evolutions", "icon"), ("enemies", "image"))
        for f in (ROOT / "data" / rel).rglob("*.yaml")
        for spec in [yload(f.read_text(encoding="utf-8"))]
        if spec.get(field)
    }
    # EXISTS is not the question — IS AN IMAGE is. `Special:FilePath` answers a
    # name that does not exist with 200 and an HTML error page, so a mistyped
    # `icon:` cached as a 31 KB ".png", passed this gate, and shipped a picture
    # no browser could draw (Boar Prime's Incarnon form, 2026-08-03). A gate
    # that only ever says yes is not a gate.
    missing = sorted(n for n in want if not (cache / n).exists())
    corrupt = sorted(n for n in want if (cache / n).exists() and not is_image(cache / n))
    if missing or corrupt:
        bad = missing + corrupt
        sys.exit(
            f"{len(bad)} images are not usable "
            f"({', '.join(bad[:4])}{' …' if len(bad) > 4 else ''}) — "
            f"{len(missing)} not cached, {len(corrupt)} cached but not an image; "
            "run `python scripts/fetch_images.py`"
        )
    out = APP / "img"
    out.mkdir(parents=True, exist_ok=True)
    # APPENDED, NOT SUBSTITUTED. Six stems in this cache carry both extensions
    # for DIFFERENT pictures — `Sobek.png` is the shotgun, `Sobek.jpg` is
    # Shattering Justice — so `<stem>.webp` would silently draw one of each
    # pair on the other's card. `<name>.webp` also makes the reverse exact:
    # the native server strips the suffix and has the cached name back.
    shipped = [derive_art(cache / name, out / (name + ".webp")) for name in sorted(want)]
    # A RENAMED ASSET LEAVES A GHOST otherwise: nothing here clears `site/img/`,
    # so the file the last build shipped stays, gets committed, and is deployed
    # for as long as nobody looks. The art directory is generated in full.
    keep = {p.name for p in shipped}
    for stale in out.iterdir():
        if stale.name not in keep:
            stale.unlink()
    size = sum(p.stat().st_size for p in shipped)
    raw = sum((cache / n).stat().st_size for n in want)
    print(f"art: {len(want)} images -> site/img/ "
          f"({size / 1e6:.1f} MB, from {raw / 1e6:.1f} MB)")


# WHAT THE SCORER WRITES ABOUT ITS OWN RUN RATHER THAN ABOUT THE BUILD, and
# therefore the only fields this file may drop. `page_row` — the Rust writer of
# the same file — emits none of them, and the two have to agree byte for byte or
# every local build leaves board.json dirty.
#
#   `weapon`  the map KEY this row is filed under.
#   `fp`      the reuse key: which data files this row's score depends on.
#   `cost`    what the row took to simulate, the input to the shard packing.
#   `listed`  and `probe` decide whether the row belongs on this page at all,
#             and the ones that do not have already been dropped.
#
# Naming the set is what makes the assertion below able to hold: a field the
# scorer adds is kept unless it is declared here, with its reason.
BOOKKEEPING = ("weapon", "fp", "cost", "listed", "probe")


def write_board() -> None:
    """`site/board.json` — the board the PAGE fetches, from the canonical yaml.

    The board is the one piece of data that changes without a release, so it is
    not compiled into the wasm like the rest of `data/`: an hourly update must
    not cost a full rebuild. The scoring job writes this file directly beside
    the yaml; this function is what keeps a LOCAL build in step with it.
    """
    # `shown` IS NOT RECOMPUTED HERE, it is CARRIED. The scorer writes it with
    # `boards_data::format_score`, which is four SIGNIFICANT figures rather than
    # four decimals — so for a score under 1 it is not the same string the
    # page's `toFixed(4)` fallback would produce, and dropping it loses real
    # precision. Recomputing it in Python would be a second copy of a rounding
    # rule that already exists twice; carrying it keeps the scorer the only
    # thing that decides.
    #
    # It also avoids a churn: a local site build that rewrites board.json
    # WITHOUT this field leaves the file dirty after every run, and a careless
    # commit ships it over a fresh rescore.
    # A ROW IS FOUND BY ITS BUILD, not by its score. Keying on the number was a
    # proxy for identity and it broke on the thing a proxy always breaks on:
    # 10 of 118 rows had a score one ULP apart between the yaml and the json
    # (11.980987537508963 against ...64), so those rows matched nothing, lost
    # their `shown` string, and came back rewritten on every local build.
    #
    # The gap is real and not a formatting bug — Rust prints an f64 the same
    # shortest-round-trip way through `{}` and through serde_json, so one run
    # cannot spell one number two ways. It means the two files were written by
    # two RUNS, and a board score is not bit-reproducible across runs: a
    # hundred engagements are summed, and summation order decides the last bit.
    # Which is exactly why an identity key is the right one — the build is what
    # a row IS, and the score is a measurement of it.
    def _ident(r):
        """A row's identity, DERIVED from the row rather than from a list.

        It was six named arguments, and the caller had to remember to pass
        every axis. `riven` was never added, so two builds differing only in
        their riven shared an identity and a carried score.

        Everything but `score`, `shown` and `source` is identity: a row is a
        BUILD, and every field the scorer writes about the build is part of
        which build it is. That is the same rule `builds::BUILD_AXES` states in
        the engine, and deriving it here is what stops this list going stale
        the next time an axis is added.
        """
        skip = {"score", "shown", "source"}
        return json.dumps({k: v for k, v in r.items() if k not in skip},
                          sort_keys=True, separators=(",", ":"))

    # ...and then the score decides only whether the CARRIED figures still
    # describe this row. Equal to a part in 1e-12 is the same measurement (a
    # rescore that moves a number moves it in the fourth digit, not the
    # sixteenth); anything further apart is a new one, so both the number and
    # the string it prints come from the yaml and the page's own rounding.
    def _same_measurement(a, b):
        try:
            a, b = float(a), float(b)
        except (TypeError, ValueError):
            return False
        return abs(a - b) <= 1e-12 * max(abs(a), abs(b), 1.0)

    prior: dict = {}
    board_path = APP / "board.json"
    if board_path.exists():
        try:
            for weapon, rows in json.loads(board_path.read_text(encoding="utf-8")).items():
                for r in rows:
                    prior[(weapon, _ident(r))] = r
        except (ValueError, AttributeError):
            pass  # unreadable or an older shape: fall through and omit `shown`

    out: dict = {}
    for f in sorted((ROOT / "boards").glob("*.yaml")):
        b = yload(f.read_text(encoding="utf-8")) or {}
        for e in b.get("entries") or []:
            # THE YAML IS THE ARCHIVE AND THIS FILE IS THE BOARD. Every row the
            # run scored is in the yaml — listed, held under the floor, and
            # screened without being measured — while the Rust writer of this
            # same file emits `kept`, which is the listed ones alone. Taking
            # every entry made a local build write 22,695 rows over the 7,571
            # the scorer wrote: three times the page's fetch, and every row the
            # floor decided not to show, published by whoever ran the site
            # build last. `listed` defaults TRUE, the same as the scorer's.
            if not e.get("listed", True) or e.get("probe"):
                continue
            # **EVERY FIELD THE SCORER WROTE, not a list of the ones somebody
            # remembered.** This function's whole job is to be a NO-OP against
            # the scorer's own output, and it was a hand list of seven keys —
            # so a field the scorer added was silently dropped on the way to
            # the page, for as long as nobody looked.
            #
            # IT HAS NOW HAPPENED TWICE. `valence` went first, and a local site
            # build stripped it from all 118 rows; the comment written then said
            # what this function is for and left the list in place. `riven` went
            # second and cost three symptoms at once — the
            # benchmark's "riven only" view showed nothing, the builder could
            # not group riven rows, and TAKING one left an empty slot, because
            # `row.riven` never reached the page so the bare `riven` id resolved
            # to no mod.
            # …MINUS TWO THAT ARE NOT FACTS ABOUT THE BUILD. `fp` is the
            # SCORER'S REUSE KEY — which data files this row's score depends on,
            # so the next run can skip re-scoring it
            # (`engine::data_fingerprint`) — and `weapon` is the MAP KEY this
            # row is filed under. The Rust writer of this same file emits
            # neither, and the two have to agree byte for byte or every local
            # build leaves board.json dirty.
            #
            # `weapon` was leaking, and it cost more than a churned file. `_ident` is "every field but score/shown/source", so
            # a row carrying an extra key had a DIFFERENT identity from the same
            # row in the file already on disk — `prior` never matched, `shown`
            # was dropped from all 118 rows, and the page fell back to rounding
            # `score` itself. The `missing` assertion below has always named
            # `weapon` as legitimately absent, which is the intent this restores.
            row = {k: v for k, v in e.items() if k not in BOOKKEEPING}
            row["benchmark"] = b.get("benchmark")
            row["source"] = b.get("source", "")
            # FLOAT, always: the yaml writes a whole score as `10` and the
            # scorer emits `10.0` from an f64. Two spellings of one number is a
            # diff on every build.
            row["score"] = float(e["score"]) if e.get("score") is not None else None
            # …and the three the page reads by name are guaranteed present,
            # because an entry may legitimately omit an empty one and the page
            # would rather have `[]` than `undefined`.
            for k in ("mods", "evolutions", "arcanes"):
                row.setdefault(k, [])
            # A RIVEN'S MALUS IS ALWAYS A KEY, even when there is not one. The
            # yaml omits it (serde skips a `None`) and the Rust writer of this
            # file emits it as `null` unconditionally — `json!({"bonuses": …,
            # "malus": rv.malus, "rolls": …})` — so a riven row read back from
            # the yaml is a key short of the same row written by the scorer.
            # Eight rows churned on every local build because of it, and they
            # lost their `shown` string with it: `_ident` is the whole row bar
            # three fields, so a riven that is a key short is a DIFFERENT build
            # as far as the carry-over lookup is concerned.
            if isinstance(row.get("riven"), dict):
                row["riven"].setdefault("malus", None)
            # THE ELEMENT AN ADVERSARY WEAPON WAS SCORED ON, and part of the
            # row's identity for the same reason `mode` is: two Kuva Nukors on
            # different valences are two entrants.
            row.setdefault("valence", "")
            row.setdefault("mode", None)
            # THE PRIOR ROW WINS ON BOTH FIGURES OR ON NEITHER. Carrying the
            # string while re-deriving the number from the yaml would leave the
            # file dirty after every build for the ULP alone — and the two
            # spellings are the same measurement, so there is nothing to
            # prefer between them. A score that actually MOVED takes the
            # yaml's number and drops the string, which is what sends the page
            # to its own rounding.
            keep = prior.get((e["weapon"], _ident(row)))
            if keep is not None and _same_measurement(keep.get("score"), row["score"]):
                if keep.get("score") is not None:
                    row["score"] = float(keep["score"])
                if keep.get("shown") is not None:
                    row["shown"] = keep["shown"]
            # NOTHING THE SCORER WROTE MAY BE DROPPED, asserted rather than
            # trusted. `row = dict(e)` makes it true by construction today; this
            # is what keeps it true, because the two times it broke the code
            # LOOKED right and the field was simply not in the list. A build
            # that would ship a lesser board fails instead.
            missing = [k for k in e if k not in row and k not in BOOKKEEPING]
            if missing:
                raise SystemExit(
                    f"board.json would drop {missing} from {e['weapon']} — "
                    "every field the scorer writes belongs on the page")
            out.setdefault(e["weapon"], []).append(row)
    # BYTE-FOR-BYTE with the scorer's own writer: `serde_json::to_string` over a
    # BTreeMap is compact and key-sorted. Matching it is what makes this
    # function a no-op when the board is already current — otherwise every local
    # build leaves the file dirty, which is a papercut on its own and a real
    # hazard when the next commit sweeps it up over a fresh rescore.
    (APP / "board.json").write_text(
        json.dumps(out, separators=(",", ":"), sort_keys=True), encoding="utf-8"
    )
    print(f"board: {sum(len(v) for v in out.values())} rows -> site/board.json")


# Every heading in `index.html` that is a ROUTE'S OWN, by its id. One HTML file
# answers every route, so all five ship in every page and exactly one of them is
# the heading of the page being served. `w-name` is the weapon route's, empty in
# the shell and filled by `app.js` on boot.
ROUTE_H1 = ("h-home", "h-download", "h-benchmark", "h-support", "w-name")


def one_h1(page: str, keep: str | None, text: str | None = None) -> str:
    """Demote every route heading except this route's, so a page has ONE <h1>.

    A PAGE SAYS WHAT IT IS ABOUT ONCE. One HTML file answers every route, so
    without this each of the 389 pages carries all five headings and a crawler
    reads 389 near-identical pages that each claim to be about the Windows
    client. Styling is by class (`.hero-h`, `.home-h`), never by tag, so a
    demoted heading looks the same.

    `text` FILLS THE KEPT ONE, which is what lets the weapon page have a single
    heading rather than a crawler's copy beside an empty placeholder: `app.js`
    assigns `#w-name` on boot, so writing the name in here is the same string
    arriving earlier, for the reader who runs no JavaScript.
    """
    for hid in ROUTE_H1:
        was = page
        if hid == keep:
            if text is None:
                continue
            page = re.sub(rf'(<h1 id="{hid}"[^>]*>).*?(</h1>)',
                          lambda m: m.group(1) + html_mod.escape(text) + m.group(2),
                          page, count=1, flags=re.S)
        else:
            page = re.sub(rf'<h1 (id="{hid}"[^>]*)>(.*?)</h1>',
                          r"<h2 \1>\2</h2>", page, count=1, flags=re.S)
        if page == was:
            sys.exit(f"index.html: <h1 id=\"{hid}\"> not found — heading not set")
    return page


@functools.lru_cache(maxsize=1)
def gear_names() -> dict:
    """`id` -> display name, for everything a board row can name."""
    out = {}
    for rel in ("mods", "arcanes"):
        for f in (ROOT / "data" / rel).rglob("*.yaml"):
            spec = yload(f.read_text(encoding="utf-8"))
            if isinstance(spec, dict) and spec.get("id") and spec.get("name"):
                out[spec["id"]] = spec["name"]
    return out


@functools.lru_cache(maxsize=1)
def board_asof() -> str:
    """The day the board archive last moved, from git rather than from a clock.

    A BUILD IS REPRODUCIBLE OR THE DATE IS A LIE. `datetime.now()` would stamp
    "today" onto a page whose numbers are a week old, which is the opposite of
    what the date is for; the archive's own last commit is when those numbers
    were actually written, and any checkout of this commit computes the same.
    """
    import subprocess

    r = subprocess.run(
        ("git", "log", "-1", "--format=%cs", "--", "boards"),
        cwd=ROOT, capture_output=True, text=True, encoding="utf-8", errors="replace",
    )
    return r.stdout.strip() if r.returncode == 0 and r.stdout.strip() else ""


@functools.lru_cache(maxsize=1)
def board_best() -> dict:
    """weapon id -> [(ruler name, row)], the best RIVEN-FREE row under each.

    THE NUMBER TRAVELS WITH ITS FIGHT. A score is meaningless without the
    scenario that produced it, and a benchmark's `name` already states that
    scenario in full — enemy, level, count, duration and metric — so a row
    quoted with its ruler's name cannot be flattened into "this weapon does N".
    That is the whole reason it is worth publishing to a reader who will never
    run the fight, machine or human.

    RIVEN-FREE, because a riven is a roll nobody else has. The top row overall
    is frequently one, and a build the reader cannot reproduce is a worse answer
    to "what should I put on this" than the best one they can.
    """
    out: dict = {}
    for f in sorted((ROOT / "boards").glob("*.yaml")):
        board = yload(f.read_text(encoding="utf-8"))
        ruler = yload((ROOT / "data" / "benchmarks" / f.name).read_text(encoding="utf-8"))
        for row in board.get("entries") or ():
            if row.get("riven"):
                continue
            rows = out.setdefault(row["weapon"], {})
            # Entries arrive best-first per weapon, so the first is the best.
            rows.setdefault(board["benchmark"], (ruler["name"], row))
    return {w: list(v.values()) for w, v in out.items()}


def board_sentence(ruler_name: str, row: dict, weapon: str) -> str:
    """One board row as a sentence that carries everything it depends on."""
    names = gear_names()
    gear = [names.get(m, m) for m in (row.get("mods") or ())]
    if row.get("exilus"):
        gear.append(names.get(row["exilus"], row["exilus"]))
    gear += [names.get(a, a) for a in (row.get("arcanes") or ())]
    score = row.get("shown") or f"{row['score']:.4g}"
    asof = f"As of {board_asof()}, t" if board_asof() else "T"
    mode = row.get("mode", "base").replace("_", " ")
    return (f"{asof}he best riven-free {weapon} build on the WFSim board scores "
            f"{score} under {ruler_name} — {mode} mode, wearing "
            f"{', '.join(gear)}.")


def page_ld(name: str, desc: str, url: str, cn: str | None = None) -> dict:
    """One page's structured data: what it is, and where it sits.

    A WebPage rather than a second WebApplication, because a weapon page is a
    page ABOUT something inside one app — `isPartOf` is what says the 389 URLs
    are one product without claiming each is a separate one.

    `about` NAMES THE SUBJECT SEPARATELY FROM THE PAGE. "A page called Braton
    Prime" and "a page about the thing called Braton Prime" are different
    claims, and only the second lets a reader that never renders HTML answer a
    question about the weapon with this page. `dateModified` is what keeps a
    quoted number honest once it has left the site.
    """
    ld: dict = {
        "@context": "https://schema.org",
        "@type": "WebPage",
        "name": name,
        "description": desc,
        "url": url,
        "inLanguage": "en",
        "isPartOf": {"@type": "WebApplication", "name": "WFSim", "url": SITE + "/"},
        "breadcrumb": {"@type": "BreadcrumbList", "itemListElement": [
            {"@type": "ListItem", "position": 1, "name": "WFSim", "item": SITE + "/"},
            {"@type": "ListItem", "position": 2, "name": name, "item": url},
        ]},
    }
    if board_asof():
        ld["dateModified"] = board_asof()
    if cn:
        ld["about"] = {"@type": "Thing", "name": name, "alternateName": cn}
    return ld


def shell(flagged: str, title: str, desc: str, url: str, og_img: str, seo: str,
          keep_h1: str | None = None, name: str | None = None,
          cn: str | None = None) -> str:
    """The app shell carrying ONE page's head and its crawler-visible body.

    Shared by every prerendered page so a new one cannot get half the
    treatment: a per-page <title> with the site-wide OG block still saying
    "WFSim" previews as the site, which is the bug this whole pass exists to
    fix. `seo` is the block removed the moment the app boots.

    `keep_h1` names the route hero this page is about; a weapon page passes
    none, because its heading is the weapon and the shell has no hero for it.
    """
    page = flagged.replace(
        "<title>WFSim — Ultimate Warframe Calculator</title>",
        f"<title>{html_mod.escape(title)}</title>",
    )
    page = re.sub(
        r'<meta name="description" content="[^"]*" />',
        f'<meta name="description" content="{html_mod.escape(desc, quote=True)}" />',
        page,
        count=1,
    )
    for prop, value in (
        ("og:title", title),
        ("og:description", desc),
        ("og:url", url),
        ("og:image", og_img),
    ):
        page = re.sub(
            rf'<meta property="{prop}" content="[^"]*" />',
            f'<meta property="{prop}" content="{html_mod.escape(value, quote=True)}" />',
            page,
            count=1,
        )
    # REPLACED, NOT ADDED. The shell declares one so that the SPA fallback — the
    # answer to every unmatched path — has one; inserting a second here would
    # give every real page two canonicals, which is the same as having none.
    canoned = re.sub(r'<link rel="canonical" href="[^"]*" />',
                     f'<link rel="canonical" href="{url}" />', page, count=1)
    if canoned == page:
        sys.exit("index.html: canonical link not found — per-page canonical not set")
    # THE SHELL'S BLOCK DESCRIBES THE APP, which is true of the home page and of
    # no other. Repeated unchanged it tells an answer engine that all 389 URLs
    # are the same application, which is the machine-readable half of the very
    # sameness the per-page title and canonical exist to break.
    ld = json.dumps(page_ld(name or title, desc, url, cn),
                    ensure_ascii=False, separators=(",", ":"))
    lded = re.sub(r'<script type="application/ld\+json">.*?</script>',
                  '<script type="application/ld+json">' + ld + "</script>",
                  canoned, count=1, flags=re.S)
    if lded == canoned:
        sys.exit("index.html: ld+json block not found — per-page structured data not set")
    # The weapon route's heading is the weapon, so it is filled rather than
    # left as the shell's placeholder; every other route spells its own out.
    page = one_h1(lded, keep_h1, name if keep_h1 == "w-name" else None)
    body = (
        '<div id="seo-fallback">\n' + seo + "  </div>\n"
        "  <script>document.getElementById('seo-fallback').remove()</script>\n  "
    )
    return page.replace("<body>\n  ", "<body>\n  " + body, 1)


def prerender(flagged: str) -> None:
    """Write a real HTML file per weapon, plus robots.txt and sitemap.xml.

    WHY, in one line: before this, every URL returned the identical 16 KB app
    shell — `/weapons/Torid`, `/robots.txt` and `/sitemap.xml` all served the
    same bytes, so a crawler saw one contentless page for the whole site and a
    pasted link had nothing to preview.

    Two things fix that, and they are different problems:

      1. **Per-page HEAD** — title, description, canonical, OG. This is what a
         QQ group or a forum renders when the link is pasted, and it is read
         without running any JavaScript.
      2. **Per-page BODY** — the weapon's actual numbers as text. Google runs
         JS and would eventually see the app render; **Baidu largely does
         not**, and the audience here is Chinese players. So each page states
         its weapon in HTML, and an inline script removes that block the
         moment the app boots. Not cloaking: the text says exactly what the
         app renders, and the visitor sees the app.
    """
    assets = yload((ROOT / "data" / "assets.yaml").read_text(encoding="utf-8"))
    zh = yload((ROOT / "data" / "i18n" / "zh" / "names.yaml").read_text(encoding="utf-8"))
    zh_names = zh.get("weapons", {})

    for spec in roster():
        wid, name = spec["id"], spec["name"]
        cn = zh_names.get(wid)
        atk = spec["attack"]
        dmg = atk["damage"]
        total = sum(dmg.values())
        ms = atk.get("multishot", 1.0)
        # The same sentence a player would write about the weapon: what it is,
        # then the numbers they came to compare.
        facts = (
            f"{spec['class'].replace('_', ' ').title()} · {spec['slot'].title()} · "
            f"Mastery Rank {spec.get('mastery_rank', 0)}"
        )
        stats = (
            f"{total:g} base damage"
            + (f" x{ms:g} multishot" if ms != 1.0 else "")
            + f" ({', '.join(f'{k} {v:g}' for k, v in sorted(dmg.items()))}), "
            f"{atk['crit_chance'] * 100:g}% crit chance, {atk['crit_multiplier']:g}x crit "
            f"multiplier, {atk['status_chance'] * 100:g}% status chance"
        )
        title = f"{name} — Warframe build, damage & DPS | WFSim"
        desc = (
            f"{name}{f' ({cn})' if cn else ''} — {facts}. {stats}. "
            "Build it, simulate the fight, and optimize the mods — "
            "true to in-game numbers."
        )
        card = f"/og/{url_slug(spec)}.png"
        drew = og_card(APP / card.lstrip("/"), name, cn, facts, stats)
        og_img = SITE + card if drew else f"{SITE}/logo.svg"
        url = SITE + wiki_path(spec)

        # THE REST OF WHAT THE ARSENAL SHOWS. The block below is this page's
        # only content that is not the shell, and four numbers of it left the
        # other 99% of the page to speak for the weapon. Everything here is
        # drawn by the app on this same page — the rule this block lives under.
        gear = [
            f"{spec['magazine']:g}-round magazine" if spec.get("magazine") else "",
            f"{spec['reload_seconds']:g} s reload" if spec.get("reload_seconds") else "",
            f"{spec['ammo_max']:g} reserve ammo" if spec.get("ammo_max") else "",
            f"{spec['accuracy']:g} accuracy" if spec.get("accuracy") else "",
            f"{spec['disposition']:g} riven disposition" if spec.get("disposition") else "",
        ]
        detail = ", ".join(x for x in gear if x)
        traits = ", ".join(t.replace("_", " ") for t in (spec.get("traits") or ()))

        # The crawler-visible body, removed as soon as the app takes over.
        seo = (
            f"    <p>{html_mod.escape(name)}"
            f"{f' / {html_mod.escape(cn)}' if cn else ''} — Warframe. "
            f"{html_mod.escape(facts)}</p>\n"
            f"    <p>{html_mod.escape(stats)}.</p>\n"
            + (f"    <p>{html_mod.escape(detail)}.</p>\n" if detail else "")
            + (f"    <p>Traits: {html_mod.escape(traits)}.</p>\n" if traits else "")
            + "".join(f"    <p>{html_mod.escape(board_sentence(rn, row, name))}</p>\n"
                      for rn, row in board_best().get(wid, ()))
            + "    <p>Build, simulate and optimize this weapon at "
            f'<a href="{SITE}/">wfsim.app</a>, or browse '
            f'<a href="{SITE}/weapons">every weapon</a>.</p>\n'
        )
        out = APP / wiki_path(spec).lstrip("/") / "index.html"
        out.parent.mkdir(parents=True, exist_ok=True)
        page = shell(flagged, title, desc, url, og_img, seo, "w-name", name, cn)
        past_the_scanner(lambda: put(out, page))

    # /weapons — THE ADDRESS OF THE ROSTER, and the only page that links to it.
    #
    # Every weapon URL was reachable from the sitemap and from nothing else. A
    # sitemap says a URL exists; a LINK says it is worth reading and what sits
    # near it, and an answer engine asked what this site covers had to fetch 387
    # pages to find out. Now the roster links down and every weapon links back.
    #
    # PLURAL, BECAUSE THE WIKI IS PLURAL — `wiki.warframe.com/w/Weapons`, and
    # `Warframes` and `Mods` beside it. URLs mirror wiki page names (AGENTS.md),
    # so the roster that comes after this one already has its address.
    #
    # THE ROUTER ALREADY SERVES IT: an unmatched path takes `on-home`, and the
    # home view IS the weapon grid, so the visitor lands on the list this page
    # describes. The block below is that grid in HTML, for whoever runs no JS.
    by_slot: dict = {}
    for s in roster():
        by_slot.setdefault(s["slot"], []).append(s)
    grid = ""
    for slot in sorted(by_slot):
        links = ", ".join(f'<a href="{wiki_path(s)}">{html_mod.escape(s["name"])}</a>'
                          for s in by_slot[slot])
        grid += (f"    <h2>{html_mod.escape(slot.title())} "
                 f"({len(by_slot[slot])})</h2>\n    <p>{links}</p>\n")
    wl_desc = (
        f"Every Warframe weapon WFSim models: {len(roster())} across "
        f"{len(by_slot)} slots. Each one can be built, simulated and optimized "
        "in the browser, against numbers measured in game."
    )
    (APP / "weapons").mkdir(parents=True, exist_ok=True)
    put(APP / "weapons" / "index.html", shell(
        flagged,
        f"All {len(roster())} Warframe weapons | WFSim",
        wl_desc,
        SITE + "/weapons",
        f"{SITE}/logo.svg",
        f"    <p>{html_mod.escape(wl_desc)}</p>\n" + grid,
        "h-home",
        "Weapons",
    ))

    # /support — a URL people paste, so it gets the same treatment. Its OG
    # description says what the page IS (running costs, nothing sold); a link
    # that previews as "Ultimate Warframe Calculator" and opens on a donation
    # page is the kind of mismatch that reads as a scam.
    sup_desc = (
        "What it costs to run WFSim, and where to chip in. WFSim is a free, "
        "open-source Warframe calculator: no ads, and no feature locked behind "
        "a payment. A donation covers the domain, the CDN and the measurement "
        "work — it buys no feature and no perk, and nothing here is for sale."
    )
    (APP / "support").mkdir(parents=True, exist_ok=True)
    (APP / "support" / "index.html").write_text(
        shell(
            flagged,
            "Support WFSim — running costs",
            sup_desc,
            SITE + "/support",
            f"{SITE}/logo.svg",
            f"    <p>{html_mod.escape(sup_desc)}</p>\n"
            f'    <p><a href="{SITE}/">wfsim.app</a></p>\n',
            "h-support",
            "Support",
        ),
        encoding="utf-8",
        newline="\n",
    )

    # /download — the URL a reader types after seeing it in a video, so it is
    # prerendered like /support: without its own title, description and
    # canonical it previews as the app's own headline, and a link that says
    # "Ultimate Warframe Calculator" and opens on an executable download is the
    # kind of mismatch that reads as a scam.
    dl_desc = (
        "WFSim as a Windows app: the same calculator on your own machine, "
        "opening instantly, working with no connection and updating itself. "
        "It is not a cut-down version — it carries the same engine the site "
        "serves. Free and open source, AGPL-3.0."
    )
    (APP / "download").mkdir(parents=True, exist_ok=True)
    (APP / "download" / "index.html").write_text(
        shell(
            flagged,
            "Download WFSim for Windows",
            dl_desc,
            SITE + "/download",
            f"{SITE}/logo.svg",
            f"    <p>{html_mod.escape(dl_desc)}</p>\n"
            f'    <p><a href="{SITE}/">wfsim.app</a></p>\n',
            "h-download",
            "Download",
        ),
        encoding="utf-8",
        newline="\n",
    )

    urls = [SITE + "/", SITE + "/weapons", SITE + "/support", SITE + "/download"] + [
        SITE + wiki_path(s) for s in roster()
    ]
    put(
        APP / "sitemap.xml",
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
        + "".join(f"  <url><loc>{u}</loc></url>\n" for u in urls)
        + "</urlset>\n",
    )
    # Without this file the SPA fallback answered /robots.txt with HTML and a
    # 200, which is a soft 404 for every crawler that asks.
    put(APP / "robots.txt", f"User-agent: *\nAllow: /\n\nSitemap: {SITE}/sitemap.xml\n")
    print(f"prerendered {len(urls) - 4} weapon pages + /weapons + /support + /download + "
          f"sitemap.xml + robots.txt — {WROTE[0]} written, {WROTE[1]} already current")


WROTE = [0, 0]  # [written, already current]


def put(path, text: str) -> None:
    """Write `text` to `path`, but only if that is not already what is there.

    A BUILD THAT CHANGES NOTHING SHOULD COST NOTHING. This rewrites 387
    prerendered pages every run, and since the build stamp became a digest of
    the served sources those pages are byte-identical whenever `app.js` and
    `index.html` have not moved — so the run spent eighteen seconds producing
    files it already had, and touched every one of their mtimes doing it.

    COMPARED ON CONTENT, never on a timestamp: an mtime is a claim about when,
    and the question is whether the bytes differ. Reading a file back is cheaper
    than writing it, and far cheaper than what a spurious rewrite costs
    everything that watches this tree.
    """
    path.parent.mkdir(parents=True, exist_ok=True)
    try:
        if path.read_text(encoding="utf-8") == text:
            WROTE[1] += 1
            return
    except (OSError, UnicodeDecodeError):
        pass
    path.write_text(text, encoding="utf-8", newline="\n")
    WROTE[0] += 1


def run(*cmd: str) -> None:
    print("+", " ".join(cmd))
    subprocess.run(cmd, cwd=ROOT, check=True)


def check_data_parses() -> None:
    """EVERY `data/` YAML MUST PARSE, before any of it is compiled into a wasm.

    The engine embeds `data/` at COMPILE TIME and reads most of it LAZILY, so a
    malformed file is not a build error — it is a panic the first time something
    asks for it. Inside a Web Worker that panic settles nothing: the promise the
    page is awaiting never resolves, no exception reaches the console, and the
    app simply stops half-booted with a populated `META` and an empty weapon
    list. That is the least debuggable failure this project can produce, and it
    shipped once (2026-08-18: an unescaped double quote inside a double-quoted
    zh string, which `cargo test` catches and this script did not).

    `cargo test` DOES catch it — `i18n_data::locales()` panics on a parse error
    and a test calls it — so this is not a second source of truth. It is the
    gate on the path that does not run tests: `python scripts/build_site_app.py`
    is what turns data into something a browser loads, and it had no reason to
    look at the data at all.
    """
    try:
        import yaml
    except ImportError:                       # pragma: no cover - dev machines have it
        print("(pyyaml not installed — skipping the data parse check)")
        return
    bad: list[str] = []
    for f in sorted(Path("data").rglob("*.yaml")):
        try:
            yload(f.read_text(encoding="utf-8"))
        except Exception as e:                # noqa: BLE001 - report every one
            bad.append(f"{f}: {str(e).splitlines()[0]}")
    if bad:
        raise SystemExit(
            "data/ does not parse — refusing to build a site around it:\n  "
            + "\n  ".join(bad)
        )


def build_stamp() -> str:
    """The commit this `site/` was generated from, plus WHAT was generated.

    The commit alone is not enough: `site/` is generated from a WORKING TREE,
    which may carry changes that are not in any commit. A `+` marks a dirty
    tree, and the digest after it is what tells two builds of one commit apart.

    IT IS A CONTENT HASH AND NOT A CLOCK, and the difference is not cosmetic.
    `checkBuildMatches` compares this value in the HTML against the one compiled
    into `app.js`, to catch a cached page paired with a newer script — so the
    question it has to answer is "is this the same script", and a timestamp
    answers "was this the same minute". Two builds of identical sources got
    different stamps, which
      1. rewrote all 386 prerendered pages on every run, changing one line in
         each and burying every real diff in the noise; and
      2. sent a reader holding the older HTML through a needless forced reload
         against a byte-identical script.
    A digest of the sources the guard is ABOUT changes exactly when the guard
    needs to fire, and not once otherwise.

    CACHED, because it is substituted in two places — the HTML and `app.js` —
    and those two must be one value. They were two calls to a function reading
    the clock, so a build that crossed a minute boundary between them shipped a
    page and a script that disagreed BY CONSTRUCTION: every visitor would be
    told the page was stale, reload, and be told again.
    """
    import hashlib
    import subprocess

    def git(*a: str) -> str:
        try:
            return subprocess.run(
                ("git", *a), capture_output=True, text=True, check=True
            ).stdout.strip()
        except Exception:
            return ""

    # THE SOURCES THE GUARD IS ABOUT: the script and the markup it expects.
    # `STATIC`, never `APP` — the copies under `site/` already carry a
    # substituted `BUILD_ID`, so hashing those would hash the answer into the
    # question and give a value that moved every run for no reason.
    h = hashlib.sha256()
    for name in ("app.js", "index.html"):
        h.update((STATIC / name).read_bytes())
    return h.hexdigest()[:8]


def build_sha() -> str:
    """WHICH COMMIT this `site/` was generated from, for a human to quote.

    NOT PART OF THE STAMP the pages carry, and that is the whole point: it moves
    on every commit, including the ones that touch nothing a page serves, so
    putting it in the HTML rewrote all 386 prerendered pages every time anybody
    committed anything. It goes into `app.js` alone, and the footer is drawn
    from both at boot.
    """
    import subprocess

    def git(*a: str) -> str:
        try:
            return subprocess.run(
                ("git", *a), capture_output=True, text=True, check=True
            ).stdout.strip()
        except Exception:
            return ""

    sha = git("rev-parse", "--short=8", "HEAD") or "nogit"
    # A DIRTY TREE, ALWAYS. `site/` is written before this is asked, so the
    # build has already dirtied the tree it is measuring — the marker says
    # "generated from a working tree", which is true of every build there is.
    dirty = "+" if git("status", "--porcelain") else ""
    return f"{sha}{dirty}"


_STAMP: "str | None" = None


def stamp_once() -> str:
    """[`build_stamp`], computed once per run — see its CACHED paragraph."""
    global _STAMP
    if _STAMP is None:
        _STAMP = build_stamp()
    return _STAMP


def project_facts() -> dict:
    """What this repository holds, counted at build time.

    `/support` states counts rather than adjectives, and a figure somebody has
    to remember to update is a figure that is wrong within a week.

    Only what the page cannot count for itself is here: the roster is `META`,
    the pools are `META.mod_pools`, the board is `BOARD`. What is left is the
    repository around the shipped data — its tests, its browser checks, and how
    long this has been going.

    Every figure is one a reader can check against the public repository. A
    count of in-game measurements is not, so it is not claimed. A count that
    cannot be taken is omitted rather than guessed, and the page drops any
    figure that is missing.
    """

    def git(*a: str) -> str:
        try:
            return subprocess.run(
                ("git", *a), capture_output=True, text=True, check=True
            ).stdout.strip()
        except Exception:
            return ""

    facts: dict = {}
    # Tracked files only: `target/` carries vendored sources whose tests are
    # not this project's.
    tests = 0
    for rel in git("ls-files", "*.rs").splitlines():
        try:
            tests += (ROOT / rel).read_text(encoding="utf-8", errors="ignore").count("#[test]")
        except OSError:
            pass
    if tests:
        facts["rust_tests"] = tests
    checks = len(list((ROOT / "scripts").glob("check_*.mjs")))
    if checks:
        facts["browser_checks"] = checks
    commits = git("rev-list", "--count", "HEAD")
    if commits.isdigit():
        facts["commits"] = int(commits)
    first = git("log", "--reverse", "--format=%ad", "--date=short")
    if first:
        facts["first_commit_day"] = first.splitlines()[0]
    return facts


def main() -> None:
    check_data_parses()
    run("cargo", "build", "--release", "-p", "wfsim-wasm", "--target", "wasm32-unknown-unknown")
    APP.mkdir(parents=True, exist_ok=True)
    # BINDGEN AND THE SIZE PASS ARE SKIPPED WHEN THE MODULE DID NOT MOVE.
    #
    # `wasm-opt -Oz` is TWENTY-ONE SECONDS of a forty-five second build, and
    # `cargo` is incremental: a run that changed only `app.js` returns in a
    # tenth of a second with a byte-identical `wfsim_wasm.wasm`, and this then
    # spent twenty-one more producing an output it already had.
    #
    # GATED ON THE INPUT'S DIGEST, never on an mtime. Cargo may rewrite the file
    # whether or not its contents changed, so a timestamp answers "was it built
    # again" where the question is "is it the same module".
    #
    # THE STAMP LIVES UNDER `target/`, which is not committed — so a fresh clone
    # has no stamp and does the full pass, and the only direction this can be
    # wrong in is doing the work unnecessarily.
    bg = APP / "pkg" / "wfsim_wasm_bg.wasm"
    stamp = ROOT / "target" / ".wasm-opt-stamp"
    want = hashlib.sha256(WASM.read_bytes()).hexdigest()
    have = stamp.read_text(encoding="utf-8").strip() if stamp.exists() else ""
    if want == have and bg.exists():
        print(f"+ wasm unchanged ({want[:8]}) — bindgen and wasm-opt skipped")
    else:
        run("wasm-bindgen", str(WASM), "--target", "no-modules", "--no-typescript",
            "--out-dir", str(APP / "pkg"))
        # Optional size pass — the app works without it.
        if shutil.which("wasm-opt"):
            run("wasm-opt", "-Oz", "-o", str(bg), str(bg))
        else:
            print("(wasm-opt not found — skipping the size pass)")
        # WRITTEN LAST, so a run that dies half way leaves no stamp claiming an
        # output it never produced.
        stamp.parent.mkdir(parents=True, exist_ok=True)
        stamp.write_text(want, encoding="utf-8")

    for name in ("app.js", "style.css", "worker.js", "logo.svg"):
        shutil.copy2(STATIC / name, APP / name)
    shutil.copytree(STATIC / "pol", APP / "pol", dirs_exist_ok=True)

    html = (STATIC / "index.html").read_text(encoding="utf-8")
    flagged = re.sub(
        r"(\s*)(<script src=\"/app\.js\"></script>)",
        r"\1<script>window.WFSIM_WASM = true;</script>\1\2",
        html,
        count=1,
    )
    if flagged == html:
        sys.exit("index.html: <script src=\"app.js\"> anchor not found — flag not injected")

    # WHICH BUILD THIS IS, stamped into the footer.
    #
    # A fix that is deployed and a fix that is on the reader's screen are two
    # different things, and without a version on the page neither side of a bug
    # report can tell them apart: "still broken" and "still holding the old
    # file" read identically. The dev server ships the placeholder, which is
    # the right answer
    # there — a page saying `dev` is not a deployed build.
    stamped = flagged.replace(
        '<span class="build-stamp" id="build-stamp" title="which build this page is">dev</span>',
        f'<span class="build-stamp" id="build-stamp" '
        f'title="which build this page is">{stamp_once()}</span>',
    )
    if stamped == flagged:
        sys.exit("index.html: build-stamp placeholder not found")
    flagged = stamped
    # …AND THE SAME STAMP IN app.js, so the two can check they are one build.
    # They are separate files with separate caches, and a browser holding an old
    # page with a new script looks for markup that page never had — which is
    # what an iPhone reported on 2026-08-21 (`renderOpt` on `#opt-modes-sect`,
    # an element added ten days earlier). `checkBuildMatches` is the reader.
    app_js = (APP / "app.js").read_text(encoding="utf-8")
    marked = app_js.replace('const BUILD_ID = "dev";',
                            f'const BUILD_ID = "{stamp_once()}";', 1)
    if marked == app_js:
        sys.exit("app.js: BUILD_ID placeholder not found")
    # …AND WHAT THE REPOSITORY HOLDS, for the one page that asks for something.
    # Same rule as the stamp: the dev server keeps the placeholder, because a
    # page that was not generated from a working tree has nothing to count.
    shaed = marked.replace('const BUILD_SHA = "dev";',
                           f'const BUILD_SHA = "{build_sha()}";', 1)
    if shaed == marked:
        sys.exit("app.js: BUILD_SHA placeholder not found")
    counted = shaed.replace("const PROJECT_FACTS = null;",
                             f"const PROJECT_FACTS = {json.dumps(project_facts())};", 1)
    if counted == marked:
        sys.exit("app.js: PROJECT_FACTS placeholder not found")
    (APP / "app.js").write_text(counted, encoding="utf-8", newline=chr(10))
    # THE HOME PAGE IS ITS OWN ROUTE and does not go through `shell` — it keeps
    # the shell's title, description and canonical as written. It still owes the
    # one-<h1> rule: `Benchmark` is a section of this page, not its subject.
    (APP / "index.html").write_text(one_h1(flagged, "h-home"),
                                    encoding="utf-8", newline="\n")
    ship_edge_config()
    ship_art()
    write_board()
    prerender(flagged)

    size = (APP / "pkg" / "wfsim_wasm_bg.wasm").stat().st_size
    print(f"site/ ready — wasm {size / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
