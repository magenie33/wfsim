#!/usr/bin/env python3
"""Transcribe Incarnon evolution names from the CN wiki — never translate them.

Evolution strings exist in NO export: DE ships none, WFCD has no entity for
them (docs/DATA_SOURCES.md). The CN wiki is the only source, and the rule is
absolute — "A STRING IS TRANSCRIBED, NEVER TRANSLATED" (owner, 2026-08-03).
DE's Chinese is routinely non-literal (Commodore's Fortune is 准将沐福), so a
name derived from the English is wrong more often than not: five Boar Prime
names were translated that way once and four of the five were wrong.

    python scripts/cn_evolution_names.py            # report what it would add
    python scripts/cn_evolution_names.py --write    # append to data/i18n/zh/evolutions.yaml

TWO THINGS THIS SCRIPT WILL NOT DO. It never overwrites an existing name — a
deliberate divergence and the comment explaining it survive, exactly as
`wfcd_i18n.py fill` behaves. And it never guesses: a perk it cannot match with
confidence is REPORTED and left empty, because an empty field is a question
and a wrong name is an answer.

FETCHING IS `curl`, DELIBERATELY. The wiki's Cloudflare reads the TLS
fingerprint, not the User-Agent: the same request answers 200 to curl and 403
to Python's urllib, from the same machine in the same minute (2026-08-07). A
fetch written the obvious way concludes the wall is up and leaves names empty
that could have been read.

MATCHING IS BY NUMBERS, not by position. The CN page mirrors the English one's
row order, so position would usually work — and "usually" is how a whole tier
silently ends up shifted by one. Every perk carries numbers in its text
(+50, 40%, 3x, 6s) and those survive translation unchanged, so the numbers are
the join key. A perk whose numbers do not pick out exactly one candidate is
left for a human.
"""

import io
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "data" / "i18n" / "zh" / "evolutions.yaml"
API = "https://warframe.huijiwiki.com/api.php"
UA = ("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
      "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
CACHE = ROOT / "web" / "cache" / "cnwiki"

TIERS = {"I": 1, "II": 2, "III": 3, "IV": 4, "V": 5}


def fetch(page):
    """The page's wikitext, or None. Cached: the wall opens and closes."""
    CACHE.mkdir(parents=True, exist_ok=True)
    f = CACHE / (re.sub(r"[^\w一-鿿]", "_", page) + ".json")
    if not f.exists():
        r = subprocess.run(
            ["curl", "-s", "--max-time", "60", "-A", UA, "--get",
             "--data-urlencode", "page=" + page,
             API + "?action=parse&prop=wikitext&format=json"],
            capture_output=True)
        f.write_bytes(r.stdout)
    try:
        d = json.loads(f.read_text(encoding="utf-8"))
    except Exception:
        return None
    if "error" in d or "parse" not in d:
        return None
    return d["parse"]["wikitext"]["*"]


def numbers(s):
    """Every number in a string, as text — the join key across languages."""
    return tuple(re.findall(r"\d+(?:\.\d+)?", s))


def cn_perks(text):
    """`{tier: [(name, numbers, card text)]}` from the CN evolution section.

    TWO BODIES, and they are not the same body. NUMBERS are read from the perk
    and everything nested under it, because a value can be a line down. The
    CARD TEXT is the perk's OWN line only — the nested lines are the wiki's
    commentary (how the gauge fills, what counts as a weakpoint), and folding
    them in produced a "card" three sentences long that ended in the page's
    category links.
    """
    out, tier = {}, None
    for line in text.split("\n"):
        m = re.match(r"^\*'''进化阶段([IVX]+)'''", line)
        if m:
            tier = TIERS.get(m.group(1))
            continue
        if tier is None:
            continue
        m = re.match(r"^\*\*'''([^']+)'''", line)
        if m:
            out.setdefault(tier, []).append([m.group(1).strip("：: "), line, line])
        elif out.get(tier) and line.startswith("***"):
            out[tier][-1][1] += line
            # TIER 1 IS THE FORM, and its own line is only its name — the
            # sentence a card would show is the first line under it.
            if clean(out[tier][-1][2]) == out[tier][-1][0]:
                out[tier][-1][2] = line
    return {t: [(n, numbers(n + nums), card) for n, nums, card in v]
            for t, v in out.items()}


def cn_perks_table(text):
    """The OTHER shape the CN wiki writes an evolution ladder in.

    An adapter's own page uses a bullet list (`*'''进化阶段II'''` …
    `**'''假意撤退'''`). A NATURAL Incarnon has no adapter page, and its
    weapon page carries a table instead — one row per tier, the tier's perks
    inside a single cell as `'''名字''':` followed by their text. Reading only
    the bullet shape found nothing on either of them.
    """
    out, tier = {}, None
    for line in text.split("\n"):
        m = re.match(r"^\|\s*进化阶段\s*(\d+)", line)
        if m:
            tier = int(m.group(1))
            continue
        if tier is None:
            continue
        if line.startswith("|}"):
            tier = None
            continue
        opened = False
        for name in re.findall(r"'''([^']{2,20})'''\s*:?<?", line):
            # A number in bold is a value, not a perk name.
            if re.search(r"[\d%+]", name):
                continue
            out.setdefault(tier, []).append([name.strip("：: "), "", ""])
            opened = True
        if out.get(tier):
            out[tier][-1][1] += line
            # In a table cell the perk's text follows its name on the NEXT
            # lines, so the accumulation IS the card — every line that did not
            # OPEN a perk belongs to the one before it. Testing for "contains
            # bold" instead dropped every body, because a body's own value is
            # bold too (`'''-30%''' 后坐力`), and eight Onos perks came out
            # named with no card at all.
            if not opened:
                out[tier][-1][2] += " " + line
    return {t: [(n, numbers(n + nums), card) for n, nums, card in v]
            for t, v in out.items()}


WIKI_MARKUP = [
    (r"\[\[[^\]|]*\|([^\]]*)\]\]", r"\1"),      # [[Heat|火焰]] -> 火焰
    (r"\[\[([^\]]*)\]\]", r"\1"),
    (r"\{\{[DAM]\|([^}|]*)\}\}", r"\1"),        # {{D|Heat}} -> Heat
    (r"\{\{[^}]*\|text\}\}", ""),
    (r"\{\{[^}]*\}\}", ""),
    (r"<span[^>]*>|</span>|<br\s*/?>|<[^>]+>", " "),
    (r"'''", ""),
    (r"^[\*\|:]+\s*", ""),
]


def clean(s):
    for pat, rep in WIKI_MARKUP:
        s = re.sub(pat, rep, s)
    return re.sub(r"\s{2,}", " ", s).strip(" ：:|")


def pick_variant(s, nums):
    """Keep THIS variant's number where the page prints every variant's.

    The CN wiki writes a family's differing values inline —
    `+50（原版）/ +40（Prime）` — and a card belongs to one weapon. Where our
    own description names one of those numbers, the others are dropped; where
    it names none of them (or more than one), the whole run is kept, because
    guessing which half is ours is how a card ends up quoting the Prime's
    numbers at a base weapon.
    """
    def one(m):
        run = m.group(0)
        parts = re.split(r"\s*/\s*", run)
        keep = [p for p in parts if set(re.findall(r"\d+(?:\.\d+)?", p)) & set(nums)]
        return keep[0] if len(keep) == 1 else run
    return re.sub(r"[+\-\d.%x]+\s*（[^）]*）(?:\s*/\s*[+\-\d.%x]+\s*（[^）]*）)+", one, s)


def our_perks():
    """{weapon: {tier: [(perk_id, english_name, numbers)]}}"""
    out = {}
    for p in sorted((ROOT / "data" / "evolutions").glob("*.yaml")):
        t = p.read_text(encoding="utf-8")
        w = re.search(r"^weapon: (\S+)$", t, re.M)
        tier = re.search(r"^tier: (\d+)$", t, re.M)
        name = re.search(r"^name: (.+)$", t, re.M)
        desc = re.search(r'^description: "(.*)"$', t, re.M)
        if not (w and tier and name):
            continue
        out.setdefault(w.group(1), {}).setdefault(int(tier.group(1)), []).append(
            (p.stem, name.group(1).strip(), numbers(desc.group(1) if desc else ""),
             desc.group(1) if desc else ""))
    return out


def existing():
    if not OUT.exists():
        return {}, OUT.read_text(encoding="utf-8") if OUT.exists() else ""
    t = OUT.read_text(encoding="utf-8")
    have = dict(re.findall(r"^  ([\w.-]+): (.+)$", t, re.M))
    return have, t


# The variant words this roster uses. A weapon id is `<variant>_<base>` or
# `<base>_<variant>`, and stripping them finds the family's namesake — the
# weapon the adapter, and therefore the CN page, is named after.
VARIANTS = ("prime", "wraith", "vandal", "prisma", "rakta", "telos", "dex",
            "mk1", "synoid", "secura", "kuva", "tenet")


def family_base(weapon_id):
    parts = [p for p in weapon_id.split("_") if p not in VARIANTS]
    return "_".join(parts) or weapon_id


def zh_weapon_names():
    t = (ROOT / "data" / "i18n" / "zh" / "names.yaml").read_text(encoding="utf-8")
    out, section = {}, None
    for line in t.split("\n"):
        m = re.match(r"^(\w+):\s*$", line)
        if m:
            section = m.group(1)
        m = re.match(r"^  ([\w.-]+): (.+)$", line)
        if m and section == "weapons":
            out[m.group(1)] = m.group(2).strip()
    return out


def resolve(ours, zh, have, consensus, add, unresolved, nopage, by_english, text_of=None):
    text_of = {} if text_of is None else text_of
    for weapon, tiers in sorted(ours.items()):
        want = [p for t in tiers.values() for p in t if p[0] not in have]
        if not want:
            continue
        # The evolution page is `<武器>灵化之源` for an adapter; a natural
        # Incarnon has no adapter and carries its ladder on its own page.
        # ONE ADAPTER, ONE PAGE. The Genesis is the FAMILY's — every variant
        # carries the same ladder — and the CN page is named after the base
        # weapon, so a Prime or a Wraith has no page of its own to find.
        titles = []
        for wid in (weapon, family_base(weapon)):
            if zh.get(wid):
                titles += [zh[wid] + "灵化之源", zh[wid]]
        wiki = None
        for title in titles:
            wiki = fetch(title)
            if wiki and "进化阶段" in wiki:
                break
            wiki = None
        if not wiki:
            nopage.append((weapon, "no CN page with an evolution ladder"))
            continue
        cn = cn_perks(wiki) or cn_perks_table(wiki)
        for tier, mine in sorted(tiers.items()):
            cands = cn.get(tier, [])
            todo = [(pid, en, nums) for pid, en, nums, _ in mine if pid not in have]
            en_desc = {pid: d for pid, _, _, d in mine}
            taken = set()

            def claim(pid, name):
                add[pid] = name
                taken.add(name)
                by_english.setdefault(en_of[pid], set()).add(name)
                # THE CARD TEXT COMES WITH THE NAME. The engine refuses a
                # half-transcribed evolution — a card headed 灵化形态 whose
                # body is still English — so the body is read from the same
                # line, in the same pass.
                body = next((c[2] for c in cands if c[0] == name), "")
                body = clean(body).replace(name, "", 1).strip(" ：:")
                if body:
                    text_of[pid] = pick_variant(body, dict(nums_of)[pid])

            en_of = {pid: en for pid, en, _ in todo}
            nums_of = [(pid, nums) for pid, _, nums in todo]
            # PASS 0 — CONSENSUS ACROSS PAGES, and it outranks this page.
            #
            # A page can be wrong about its own weapon. The Dera's has 迅速判决
            # against a magazine-capacity line and 扩充齐发 against a
            # projectile-speed one, which is the opposite of what those two
            # names carry on sixteen other pages — so matching by numbers alone
            # faithfully reproduced that page's swap. Where a perk's English
            # name has one Chinese name on two or more OTHER weapons, and that
            # name is among this tier's candidates, the many win.
            for pid, en, nums in list(todo):
                want = consensus.get(en)
                if want and any(c[0] == want for c in cands) and want not in taken:
                    claim(pid, want)
                    todo = [x for x in todo if x[0] != pid]
            # PASS 1 — the numbers, which survive translation unchanged.
            for pid, en, nums in list(todo):
                hits = [c for c in cands if nums and set(nums) <= set(c[1])
                        and c[0] not in taken]
                if len(hits) == 1:
                    claim(pid, hits[0][0])
                    todo = [x for x in todo if x[0] != pid]
            # PASS 2 — the same perk, already read on another weapon. This is
            # a CHECK rather than a guess: the name is only accepted if it is
            # one of THIS tier's candidates, so a coincidence of English names
            # across two families cannot put a wrong string in.
            for pid, en, _ in list(todo):
                known = by_english.get(en, set()) - taken
                hits = [c[0] for c in cands if c[0] in known]
                if len(hits) == 1:
                    claim(pid, hits[0])
                    todo = [x for x in todo if x[0] != pid]
            # PASS 3 — one perk and one candidate left in the row. Nothing to
            # be ambiguous with.
            left = [c[0] for c in cands if c[0] not in taken
                    and c[0] not in set(add.values())]
            if len(todo) == 1 and len(left) == 1:
                claim(todo[0][0], left[0])
                todo = []
            # PASS 4 — THE ENGLISH INSIDE THE CHINESE. The CN wiki links its
            # stat words back to the English pages (`[[Projectile Speed|投射物
            #速度]]`), so a candidate's own text carries English tokens that
            # can be matched against our description. No translation is
            # involved: the English is theirs, not ours.
            #
            # THIS REPLACED A POSITIONAL PASS, which was wrong and looked
            # right. Our perks are read in FILENAME order, not the wiki's row
            # order, so "same index in both lists" meant nothing — it swapped
            # Evolved Autoloader with Swift Deliverance on the Dera, Kinetic
            # Baffle with Frictionless Flight on the Felarx, and Marksman's
            # Hand with Ready Retaliation on the Dex Sybaris. Every one of
            # those pairs looked plausible in the output.
            for pid, en, _ in list(todo):
                mine_words = set(re.findall(r"[A-Za-z][A-Za-z ]{3,}", en_desc[pid]))
                mine_words = {w.strip().lower() for w in mine_words}
                hits = []
                for c in cands:
                    if c[0] in taken:
                        continue
                    theirs = {w.strip().lower()
                              for w in re.findall(r"\[\[([A-Za-z][^\]|]*)\|", c[2])}
                    if theirs & mine_words:
                        hits.append(c[0])
                if len(hits) == 1:
                    claim(pid, hits[0])
                    todo = [x for x in todo if x[0] != pid]
            for pid, en, _ in todo:
                unresolved.append((pid, en, tier, [c[0] for c in cands]))


def main():
    write = "--write" in sys.argv
    have, text = existing()
    ours, zh = our_perks(), zh_weapon_names()
    english_of = {p[0]: p[1] for t in ours.values() for v in t.values() for p in v}

    # ROUNDS, UNTIL IT STOPS MOVING. A round reads every page and counts what
    # each English perk name was called; the next round re-reads them with that
    # count in hand, so a page that disagrees with sixteen others loses. One
    # round cannot do it — the disagreement is only visible once every page has
    # been read — and two are not always enough either, because a name settled
    # by round two is evidence round three can use (the Dex Sybaris's row is
    # only decidable once the Sybaris's has been).
    consensus, disputed = {}, []
    add, unresolved, nopage, by_english, text_of = {}, [], [], {}, {}
    for _ in range(4):
        add, unresolved, nopage, by_english, text_of = {}, [], [], {}, {}
        resolve(ours, zh, have, consensus, add, unresolved, nopage, by_english, text_of)
        tally = {}
        for pid, name in add.items():
            tally.setdefault(english_of[pid], {}).setdefault(name, 0)
            tally[english_of[pid]][name] += 1
        nxt, disputed = {}, []
        for en, counts in tally.items():
            best = max(counts, key=counts.get)
            rest = {k: v for k, v in counts.items() if k != best}
            if not rest:
                # No disagreement, but still evidence: a name read on one page
                # is what that perk is called on the next.
                if counts[best] >= 1:
                    nxt[en] = best
                continue
            if counts[best] >= 2 and counts[best] > max(rest.values()):
                nxt[en] = best
                disputed.append((en, best, counts[best], rest))
        if nxt == consensus:
            break
        consensus = nxt

    for en, best, n, rest in sorted(disputed):
        print("  ~ %-26s %s (%d pages) over %s"
              % (en, best, n, ", ".join("%s (%d)" % kv for kv in sorted(rest.items()))))
    for pid, name in sorted(add.items()):
        print("  + %-40s %s" % (pid, name))
    print("\n%d names read, %d unresolved, %d weapons with no page"
          % (len(add), len(unresolved), len(nopage)))
    for w, why in nopage:
        print("  ! %-24s %s" % (w, why))
    for pid, en, tier, cands in unresolved[:40]:
        print("  ? %-40s (tier %d, %r) candidates: %s" % (pid, tier, en, cands))
    if len(unresolved) > 40:
        print("  ? …and %d more" % (len(unresolved) - 40))

    if not write:
        print("\n(re-run with --write)")
        return 0
    if add:
        # INTO THE `evolutions:` SECTION, not onto the end of the file. This
        # file has two of them — names and card text — and appending blindly
        # filed 447 names under `evolution_descriptions:`, where the loader
        # never looks and `wfcd_i18n.py check` still reported them unnamed.
        lines, body, done = text.split("\n"), [], False
        for i, line in enumerate(lines):
            body.append(line)
            if line.startswith("evolutions:") and not done:
                mark = i
                done = True
        assert done, "no `evolutions:` section in " + str(OUT)
        # IMMEDIATELY AFTER THE HEADER, which is the only place in the section
        # that is certainly not inside something. Two other insertion points
        # were tried and both were wrong: after the section's last LINE put the
        # names past a comment block that opens the next section, where the
        # reader (`wfcd_i18n.overlay_section` stops at column 0) never saw
        # them; after its last KEY line landed inside a value, because a card
        # text may run onto a second line and that continuation starts in
        # column 0 too. Order inside a YAML mapping means nothing, so the top
        # is as good as the bottom and cannot split anything.
        end = mark + 1
        new = ["", "  # Transcribed by scripts/cn_evolution_names.py from the CN wiki."]
        new += ["  %s: %s" % kv for kv in sorted(add.items())]
        out = lines[:end] + new + lines[end:]

        # THE CARD TEXT GOES IN TOO, in its own section. The engine refuses a
        # half-transcribed evolution: `a_transcribed_evolution_has_both_a_name
        # _and_its_text` fails on a card headed in Chinese whose body is still
        # English, and it is right to — that is a worse page than an untouched
        # one.
        txt = [p for p in sorted(text_of) if p in add]
        if txt:
            for i, line in enumerate(out):
                if line.startswith("evolution_descriptions:"):
                    at = i + 1
                    block = ["", "  # Transcribed by scripts/cn_evolution_names.py."]
                    block += ['  %s: "%s"' % (p, text_of[p].replace('"', "'")) for p in txt]
                    out = out[:at] + block + out[at:]
                    break
        io.open(OUT, "w", encoding="utf-8", newline="\n").write("\n".join(out))
        print("\nadded %d names to %s" % (len(add), OUT.relative_to(ROOT)))
    return 0


raise SystemExit(main())
