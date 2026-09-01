#!/usr/bin/env python3
"""Package the in-browser builder into site/ (docs/WASM.md phase 4).

Steps:
  1. cargo build --release -p wfsim-wasm --target wasm32-unknown-unknown
  2. wasm-bindgen --target no-modules  ->  site/app/pkg/
  3. wasm-opt -Oz (if available; optional)
  4. copy web/src/static/{index.html,app.js,style.css,worker.js,logo.svg,pol/} -> site/
  5. inject <script>window.WFSIM_WASM = true;</script> into the copied
     index.html — that flag flips app.js's api() from fetch to worker RPC.
  6. copy the art cache into site/img/ (same-origin art — see ship_art)
  7. prerender one HTML file per weapon (its own title/description/OG plus a
     crawler-visible summary), and write sitemap.xml + robots.txt — see
     prerender() for why a single shell was not enough.

wrangler already serves site/ at wfsim.app, so after this script the builder
lives at wfsim.app/ and every simulation runs on the visitor's own CPU.

Prereqs: rustup target add wasm32-unknown-unknown;
         cargo install wasm-bindgen-cli --version <matching Cargo.lock>.
"""

import html as html_mod
import json
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

import yaml

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
        spec = yaml.safe_load(f.read_text(encoding="utf-8"))
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


def wiki_path(name: str) -> str:
    """The URL a weapon lives at — the English wiki page name, spaces to
    underscores (AGENTS.md: URLs mirror wiki page names; ids never appear)."""
    return "/weapons/" + wiki_name(name).replace(" ", "_")


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
    past_the_scanner(lambda: img.save(out, "PNG", optimize=True))
    return True


def ship_art() -> None:
    """Copy the art cache into `site/img/`, so the static deployment serves
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
    assets = yaml.safe_load((ROOT / "data" / "assets.yaml").read_text(encoding="utf-8"))
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
        for spec in [yaml.safe_load(f.read_text(encoding="utf-8"))]
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
    for name in sorted(want):
        past_the_scanner(lambda: shutil.copy2(cache / name, out / name))
    size = sum((out / n).stat().st_size for n in want)
    print(f"art: {len(want)} images -> site/img/ ({size / 1e6:.1f} MB)")


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
    for f in sorted((ROOT / "data" / "benchmarks" / "boards").glob("*.yaml")):
        b = yaml.safe_load(f.read_text(encoding="utf-8")) or {}
        for e in b.get("entries") or []:
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
            row = {k: v for k, v in e.items() if k not in ("fp", "weapon")}
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
            missing = [k for k in e if k not in row and k not in ("weapon", "fp")]
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


def shell(flagged: str, title: str, desc: str, url: str, og_img: str, seo: str) -> str:
    """The app shell carrying ONE page's head and its crawler-visible body.

    Shared by every prerendered page so a new one cannot get half the
    treatment: a per-page <title> with the site-wide OG block still saying
    "WFSim" previews as the site, which is the bug this whole pass exists to
    fix. `seo` is the block removed the moment the app boots.
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
    page = page.replace(
        '<meta property="og:type" content="website" />',
        '<meta property="og:type" content="website" />\n'
        f'  <link rel="canonical" href="{url}" />',
        1,
    )
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
    assets = yaml.safe_load((ROOT / "data" / "assets.yaml").read_text(encoding="utf-8"))
    zh = yaml.safe_load((ROOT / "data" / "i18n" / "zh" / "names.yaml").read_text(encoding="utf-8"))
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
        card = f"/og/{wiki_name(name).replace(' ', '_')}.png"
        drew = og_card(APP / card.lstrip("/"), name, cn, facts, stats)
        og_img = SITE + card if drew else f"{SITE}/logo.svg"
        url = SITE + wiki_path(name)

        # The crawler-visible body, removed as soon as the app takes over.
        seo = (
            f"    <h1>{html_mod.escape(name)}"
            f"{f' / {html_mod.escape(cn)}' if cn else ''} — Warframe</h1>\n"
            f"    <p>{html_mod.escape(facts)}</p>\n"
            f"    <p>{html_mod.escape(stats)}.</p>\n"
            "    <p>Build, simulate and optimize this weapon at "
            f'<a href="{SITE}/">wfsim.app</a>.</p>\n'
        )
        out = APP / wiki_path(name).lstrip("/") / "index.html"
        out.parent.mkdir(parents=True, exist_ok=True)
        page = shell(flagged, title, desc, url, og_img, seo)
        past_the_scanner(lambda: out.write_text(page, encoding="utf-8", newline="\n"))

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
            "    <h1>Support WFSim</h1>\n"
            f"    <p>{html_mod.escape(sup_desc)}</p>\n"
            f'    <p><a href="{SITE}/">wfsim.app</a></p>\n',
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
            "    <h1>WFSim for Windows</h1>\n"
            f"    <p>{html_mod.escape(dl_desc)}</p>\n"
            f'    <p><a href="{SITE}/">wfsim.app</a></p>\n',
        ),
        encoding="utf-8",
        newline="\n",
    )

    urls = [SITE + "/", SITE + "/support", SITE + "/download"] + [
        SITE + wiki_path(s["name"]) for s in roster()
    ]
    (APP / "sitemap.xml").write_text(
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
        + "".join(f"  <url><loc>{u}</loc></url>\n" for u in urls)
        + "</urlset>\n",
        encoding="utf-8",
        newline="\n",
    )
    # Without this file the SPA fallback answered /robots.txt with HTML and a
    # 200, which is a soft 404 for every crawler that asks.
    (APP / "robots.txt").write_text(
        f"User-agent: *\nAllow: /\n\nSitemap: {SITE}/sitemap.xml\n",
        encoding="utf-8",
        newline="\n",
    )
    print(f"prerendered {len(urls) - 3} weapon pages + /support + /download + sitemap.xml + robots.txt")


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
            yaml.safe_load(f.read_text(encoding="utf-8"))
        except Exception as e:                # noqa: BLE001 - report every one
            bad.append(f"{f}: {str(e).splitlines()[0]}")
    if bad:
        raise SystemExit(
            "data/ does not parse — refusing to build a site around it:\n  "
            + "\n  ".join(bad)
        )


def build_stamp() -> str:
    """The commit this `site/` was generated from, plus when — UTC.

    The commit alone is not enough: `site/` is generated from a WORKING TREE,
    which may carry changes that are not in any commit, so the timestamp is
    what tells two builds of one commit apart. A `+` marks a dirty tree for
    the same reason.
    """
    import datetime
    import subprocess

    def git(*a: str) -> str:
        try:
            return subprocess.run(
                ("git", *a), capture_output=True, text=True, check=True
            ).stdout.strip()
        except Exception:
            return ""

    sha = git("rev-parse", "--short=8", "HEAD") or "nogit"
    dirty = "+" if git("status", "--porcelain") else ""
    when = datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%d %H:%M")
    return f"{sha}{dirty} · {when}Z"


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
    run("wasm-bindgen", str(WASM), "--target", "no-modules", "--no-typescript",
        "--out-dir", str(APP / "pkg"))

    # Optional size pass — the app works without it.
    if shutil.which("wasm-opt"):
        bg = APP / "pkg" / "wfsim_wasm_bg.wasm"
        run("wasm-opt", "-Oz", "-o", str(bg), str(bg))
    else:
        print("(wasm-opt not found — skipping the size pass)")

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
        f'title="which build this page is">{build_stamp()}</span>',
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
                            f'const BUILD_ID = "{build_stamp()}";', 1)
    if marked == app_js:
        sys.exit("app.js: BUILD_ID placeholder not found")
    # …AND WHAT THE REPOSITORY HOLDS, for the one page that asks for something.
    # Same rule as the stamp: the dev server keeps the placeholder, because a
    # page that was not generated from a working tree has nothing to count.
    counted = marked.replace("const PROJECT_FACTS = null;",
                             f"const PROJECT_FACTS = {json.dumps(project_facts())};", 1)
    if counted == marked:
        sys.exit("app.js: PROJECT_FACTS placeholder not found")
    (APP / "app.js").write_text(counted, encoding="utf-8", newline=chr(10))
    (APP / "index.html").write_text(flagged, encoding="utf-8", newline="\n")
    ship_art()
    write_board()
    prerender(flagged)

    size = (APP / "pkg" / "wfsim_wasm_bg.wasm").stat().st_size
    print(f"site/ ready — wasm {size / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
