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


def roster() -> list[dict]:
    """The weapons that get a page: one per WEAPON, not per form.

    `default_form` is the arsenal's form and therefore the roster row
    (engine::weapons_data::roster does the same filter) — a bow's tapped shot
    and an Incarnon form are forms of a weapon, not separate pages.
    """
    out = []
    for f in sorted((ROOT / "data" / "weapons").rglob("*.yaml")):
        spec = yaml.safe_load(f.read_text(encoding="utf-8"))
        if spec.get("default_form"):
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
    img.save(out, "PNG", optimize=True)
    return True


def ship_art() -> None:
    """Copy the art cache into `site/img/`, so the static deployment serves
    game art from ITS OWN ORIGIN.

    It used to load straight from the CDN, and that CDN is a redirector:
    `cdn.warframestat.us/img/X.png` answers 301 to raw.githubusercontent.com.
    GitHub is unreliable-to-blocked from mainland China, which is where the
    players are — so the app's own images were the slowest, least reliable
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
        shutil.copy2(cache / name, out / name)
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
    # It also ends a churn: every local site build used to rewrite board.json
    # WITHOUT this field, so the file came back dirty after each run and a
    # careless commit could ship it over a fresh rescore.
    # A whole score is `10` in the yaml and `10.0` in the json, so the key has
    # to be the NUMBER rather than its spelling.
    def _score_key(v):
        try:
            return repr(float(v))
        except (TypeError, ValueError):
            return repr(v)

    prior: dict = {}
    board_path = APP / "board.json"
    if board_path.exists():
        try:
            for weapon, rows in json.loads(board_path.read_text(encoding="utf-8")).items():
                for r in rows:
                    if r.get("shown") is not None:
                        prior[(weapon, _score_key(r.get("score")))] = r["shown"]
        except (ValueError, AttributeError):
            pass  # unreadable or an older shape: fall through and omit `shown`

    out: dict = {}
    for f in sorted((ROOT / "data" / "benchmarks" / "boards").glob("*.yaml")):
        b = yaml.safe_load(f.read_text(encoding="utf-8")) or {}
        for e in b.get("entries") or []:
            row = {
                "benchmark": b.get("benchmark"),
                "source": b.get("source", ""),
                # FLOAT, always: the yaml writes a whole score as `10` and the
                # scorer emits `10.0` from an f64. Two spellings of one number
                # is a diff on every build.
                "score": float(e["score"]) if e.get("score") is not None else None,
                "mods": e.get("mods", []),
                "evolutions": e.get("evolutions", []),
                "arcanes": e.get("arcanes", []),
            }
            # Only when the SCORE still matches: a moved score's old display
            # string is wrong, and the page's fallback is right for it.
            keep = prior.get((e["weapon"], _score_key(e.get("score"))))
            if keep is not None:
                row["shown"] = keep
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
        out.write_text(
            shell(flagged, title, desc, url, og_img, seo), encoding="utf-8", newline="\n"
        )

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

    urls = [SITE + "/", SITE + "/support"] + [SITE + wiki_path(s["name"]) for s in roster()]
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
    print(f"prerendered {len(urls) - 2} weapon pages + /support + sitemap.xml + robots.txt")


def run(*cmd: str) -> None:
    print("+", " ".join(cmd))
    subprocess.run(cmd, cwd=ROOT, check=True)


def main() -> None:
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
    (APP / "index.html").write_text(flagged, encoding="utf-8", newline="\n")
    ship_art()
    write_board()
    prerender(flagged)

    size = (APP / "pkg" / "wfsim_wasm_bg.wasm").stat().st_size
    print(f"site/ ready — wasm {size / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
