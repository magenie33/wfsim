#!/usr/bin/env python3
"""Render docs/mechanics/ into static HTML pages and markdown twins for site/."""

from __future__ import annotations

import html as html_mod
import json
import re
from pathlib import Path
from typing import Callable
from urllib.parse import quote

import yaml

YAML_LOADER = getattr(yaml, "CSafeLoader", yaml.SafeLoader)

# Common English words that collide with weapon names in general prose.
EN_SKIP = frozenset({
    "Bo", "Fang", "Gaze", "Heat", "Jaw", "Mire", "Pox", "Seer", "Viper",
    "Flux", "Sonic", "Dual", "Twin", "Prime", "Wraith", "Vandal", "Prisma",
    "Kuva", "Tenet", "Coda", "Arca", "Dex", "Sancti", "Secura", "Synoid",
    "Telos", "Vaykor", "Rakta",
})

# Short Chinese words that collide with non-weapon prose.
ZH_SKIP = frozenset({"翁", "毒", "光", "剑", "弓", "爪"})

# Safe shorthand aliases used in prose/tables, kept only if target exists in roster.
ZH_ALIASES = {
    "赤毒·努寇": "kuva_nukor",
    "普通努寇": "nukor",
    "信条·冷冻": "tenet_glaxion",
    "信条·循环": "tenet_cycron",
    "托里德": "torid",
    "野猪": "boar",
    "伊格尼斯": "ignis",
    "幻离子": "phantasma",
    "安培克斯": "amprex",
    "视使之触": "ocucor",
    "翠雀": "larkspur",
    "送葬者": "cortege",
    "虫置": "vermisplicer_primary",
    "凝目": "gaze_primary",
    "奏凯": "laetum",
    "兰卡": "lanka",
    "绝路": "rubico",
    "守望者": "vectis",
    "金工火神": "vulkar",
    "狙击特昂": "snipetron",
    "伯斯顿": "burston",
    "拉特昂": "latron",
    "席芭莉丝": "sybaris",
    "凤殁": "phenmor",
    "米特尔": "miter",
    "太始弓": "nataruk",
    "迅发电浆炮": "acceltra",
    "阿利乌双枪": "akarius",
    "异化者": "catabolyst",
    "光谱切割器": "spectra",
    "咖玛腕甲枪": "gammacor",
    "原子矿融炮": "atomos",
    "量子切割器": "quanta",
    "导热聚焦枪": "convectrix",
    "噬菌者": "phage",
    "安柏勒斯": "embolist",
    "盗贼": "furis",
    "赘骨": "onos",
    "阿索代": "athodai",
    "巴什穆": "basmu",
    "冰凇": "verglas",
    "猎豹": "panthera",
    "泰瑟步枪": "tazicor",
}

EN_SLOT_ALIASES = {
    "Gaze (Primary)": "gaze_primary",
    "Gaze (Secondary)": "gaze_secondary",
    "Vermisplicer (Primary Kitgun)": "vermisplicer_primary",
    "Vermisplicer (Primary)": "vermisplicer_primary",
    "Vermisplicer (Secondary Kitgun)": "vermisplicer_secondary",
    "Vermisplicer (Secondary)": "vermisplicer_secondary",
}

ZH_SLOT_ALIASES = {
    "凝目（主手）": "gaze_primary",
    "凝目（副手）": "gaze_secondary",
    "虫置（主手组装枪）": "vermisplicer_primary",
    "虫置（主手）": "vermisplicer_primary",
    "虫置（副手组装枪）": "vermisplicer_secondary",
    "虫置（副手）": "vermisplicer_secondary",
}

ZH_EXTRA_ALLOW = frozenset({
    "恐惧", "憎恨", "和谐", "战刃 Prime", "驱魔之刃", "翁 Prime",
})

EN_EXTRA_ALLOW = frozenset({
    "Gaze", "Pox", "Seer", "Viper",
})


def load_catalog(root: Path) -> dict:
    path = root / "docs" / "mechanics" / "catalog.yaml"
    return yaml.load(path.read_text(encoding="utf-8"), Loader=YAML_LOADER)


class WeaponLinker:
    """Longest-match-first linker mapping weapon names to /weapons/<Wiki_Name>."""

    def __init__(self, mapping: dict[str, tuple[str, str, str]], is_en: bool):
        self.mapping = mapping
        self.is_en = is_en
        names = sorted(mapping.keys(), key=lambda s: (-len(s), s))
        if not names:
            self.rx = None
        else:
            pat = "|".join(re.escape(n) for n in names)
            self.rx = re.compile(rf"(?<![A-Za-z0-9_])(?:{pat})(?![A-Za-z0-9_])")

    def link_plain_html(self, escaped_text: str, seen: dict[str, tuple[str, str]]) -> str:
        if not self.rx or not escaped_text:
            return escaped_text

        def repl(m: re.Match) -> str:
            label = m.group(0)
            href, canonical, icon = self.mapping[label]
            seen.setdefault(href, (canonical, icon))
            img = (
                f'<img class="wicon" src="{icon}" alt="" width="22" height="15" loading="lazy" decoding="async">'
                if icon
                else ""
            )
            return f'<a class="wlink" href="{href}">{img}<span>{label}</span></a>'

        return self.rx.sub(repl, escaped_text)

    def link_plain_md(self, text: str, site_url: str) -> str:
        if not self.rx or not text:
            return text

        def repl(m: re.Match) -> str:
            label = m.group(0)
            href, _, _ = self.mapping[label]
            return f"[{label}]({site_url}{href})"

        return self.rx.sub(repl, text)


def asset_icon_url(raw: str | None) -> str:
    if not raw:
        return ""
    name = raw[5:] if str(raw).startswith("wiki:") else str(raw)
    return "/img/" + quote(name + ".webp", safe="/._()-")


def build_weapon_linkers(
    root: Path,
    roster_specs: list[dict],
    wiki_path_fn: Callable[[dict], str],
) -> dict[str, WeaponLinker]:
    assets_raw = yaml.load(
        (root / "data" / "assets.yaml").read_text(encoding="utf-8"),
        Loader=YAML_LOADER,
    )
    weapon_icons = assets_raw.get("weapons", {}) if isinstance(assets_raw, dict) else {}

    by_id: dict[str, tuple[str, str, str]] = {}
    en_map: dict[str, tuple[str, str, str]] = {}
    for spec in roster_specs:
        wid = spec["id"]
        name = spec["name"]
        href = wiki_path_fn(spec)
        icon = asset_icon_url(weapon_icons.get(wid))
        by_id[wid] = (href, name, icon)
        if name not in EN_SKIP or name in EN_EXTRA_ALLOW:
            en_map[name] = (href, name, icon)

    zh_raw = yaml.load(
        (root / "data" / "i18n" / "zh" / "names.yaml").read_text(encoding="utf-8"),
        Loader=YAML_LOADER,
    )
    zh_weapons = zh_raw.get("weapons", {}) if isinstance(zh_raw, dict) else {}
    zh_map: dict[str, tuple[str, str, str]] = {}
    for wid, (href, en_name, icon) in by_id.items():
        cn = zh_weapons.get(wid)
        if isinstance(cn, str):
            cn = cn.strip()
            if len(cn) >= 2 and cn not in ZH_SKIP:
                zh_map.setdefault(cn, (href, cn, icon))
        if en_name in en_map and len(en_name) >= 4:
            zh_map.setdefault(en_name, (href, zh_weapons.get(wid, en_name), icon))

    for alias, wid in EN_SLOT_ALIASES.items():
        if wid in by_id:
            en_map[alias] = by_id[wid]

    for alias, wid in ZH_SLOT_ALIASES.items():
        if wid in by_id:
            href, _, icon = by_id[wid]
            zh_map[alias] = (href, alias, icon)

    for alias, wid in ZH_ALIASES.items():
        if wid in by_id:
            href, _, icon = by_id[wid]
            display = zh_weapons.get(wid, alias)
            zh_map.setdefault(alias, (href, display, icon))

    return {
        "en": WeaponLinker(en_map, is_en=True),
        "zh": WeaponLinker(zh_map, is_en=False),
    }


def slugify_heading(text: str, used: set[str], fallback_idx: int) -> str:
    clean = re.sub(r"`|\*\*|\*", "", text).strip()
    ascii_part = re.sub(r"[^a-zA-Z0-9]+", "-", clean).strip("-").lower()
    base = ascii_part if len(ascii_part) >= 3 else f"section-{fallback_idx}"
    slug = base
    n = 2
    while slug in used:
        slug = f"{base}-{n}"
        n += 1
    used.add(slug)
    return slug


INLINE_RE = re.compile(
    r"(\*\*`[^`]+`\*\*|\*\*[^*\n]+\*\*|`[^`]+`|\*[^*\n]+\*|\[[^\]]+\]\([^)]+\))"
)


def render_inline_html(
    text: str,
    linker: WeaponLinker | None,
    seen: dict[str, str],
    allow_links: bool = True,
) -> str:
    out: list[str] = []
    pos = 0
    for m in INLINE_RE.finditer(text):
        if m.start() > pos:
            chunk = html_mod.escape(text[pos : m.start()])
            out.append(linker.link_plain_html(chunk, seen) if (linker and allow_links) else chunk)
        tok = m.group(0)
        if tok.startswith("**`") and tok.endswith("`**"):
            out.append(f"<strong><code>{html_mod.escape(tok[3:-3])}</code></strong>")
        elif tok.startswith("**") and tok.endswith("**"):
            inner = render_inline_html(tok[2:-2], linker, seen, allow_links=allow_links)
            out.append(f"<strong>{inner}</strong>")
        elif tok.startswith("`") and tok.endswith("`"):
            out.append(f"<code>{html_mod.escape(tok[1:-1])}</code>")
        elif tok.startswith("*") and tok.endswith("*"):
            inner = render_inline_html(tok[1:-1], linker, seen, allow_links=allow_links)
            out.append(f"<em>{inner}</em>")
        elif tok.startswith("["):
            lm = re.match(r"\[([^\]]+)\]\(([^)]+)\)", tok)
            if lm:
                lbl = html_mod.escape(lm.group(1))
                href = html_mod.escape(lm.group(2), quote=True)
                ext = ' target="_blank" rel="noopener"' if href.startswith("http") else ""
                out.append(f'<a href="{href}"{ext}>{lbl}</a>')
            else:
                out.append(html_mod.escape(tok))
        pos = m.end()
    if pos < len(text):
        chunk = html_mod.escape(text[pos:])
        out.append(linker.link_plain_html(chunk, seen) if (linker and allow_links) else chunk)
    return "".join(out)


def render_mermaid_as_flow(block_lines: list[str]) -> str:
    """Turn simple Mermaid flowchart lines into semantic HTML steps (zero JS)."""
    nodes: dict[str, str] = {}
    edges: list[tuple[str, str, str]] = []
    node_re = re.compile(r'([A-Za-z0-9_]+)\["([^"]+)"\]')
    edge_re = re.compile(r'([A-Za-z0-9_]+)(?:\["[^"]+"\])?\s*-->(?:\|"([^"]+)"\|)?\s*([A-Za-z0-9_]+)')
    for raw in block_lines:
        ln = raw.strip()
        if not ln or ln.startswith("flowchart") or ln.startswith("graph"):
            continue
        for nm in node_re.finditer(ln):
            nodes[nm.group(1)] = nm.group(2).replace("<br/>", " · ").replace("<br>", " · ")
        em = edge_re.search(ln)
        if em:
            edges.append((em.group(1), em.group(2) or "", em.group(3)))
    if not nodes:
        code = html_mod.escape("\n".join(block_lines))
        return f'<pre class="mech-formula"><code>{code}</code></pre>'

    items = []
    for nid, label in nodes.items():
        outs = [f"{('[' + lbl + '] ' if lbl else '')}→ {nodes.get(dst, dst)}" for src, lbl, dst in edges if src == nid]
        arrow_html = (
            '<div class="mf-next">' + " &nbsp;·&nbsp; ".join(html_mod.escape(x) for x in outs) + "</div>"
            if outs
            else ""
        )
        items.append(
            f'<li class="mf-node"><div class="mf-label">{html_mod.escape(label)}</div>{arrow_html}</li>'
        )
    return '<ol class="mech-flow">' + "".join(items) + "</ol>"


def link_markdown_body(md_text: str, linker: WeaponLinker, site_url: str) -> str:
    """Auto-link weapon names inside markdown prose/tables while preserving code and links."""
    lines = md_text.splitlines()
    out: list[str] = []
    in_fence = False
    for line in lines:
        s = line.strip()
        if s.startswith("```"):
            in_fence = not in_fence
            out.append(line)
            continue
        if in_fence or s.startswith("#") or s.startswith("> **Author") or s.startswith("> **整理与署名"):
            out.append(line)
            continue
        # Link only outside inline code spans and existing markdown links.
        parts = re.split(r"(`[^`]+`|\[[^\]]+\]\([^)]+\))", line)
        rebuilt = []
        for part in parts:
            if not part or part.startswith("`") or (part.startswith("[") and "](" in part):
                rebuilt.append(part)
            else:
                rebuilt.append(linker.link_plain_md(part, site_url))
        out.append("".join(rebuilt))
    return "\n".join(out).rstrip() + "\n"


def render_markdown_html(
    md_text: str,
    linker: WeaponLinker,
    widget_html: str = "",
) -> tuple[str, list[tuple[int, str, str]], dict[str, str]]:
    """Convert article markdown into HTML, TOC entries, and referenced weapons."""
    lines = md_text.splitlines()
    out: list[str] = []
    toc: list[tuple[int, str, str]] = []
    seen_weapons: dict[str, str] = {}
    used_ids: set[str] = set()
    widget_inserted = False
    i, n = 0, len(lines)
    sec_counter = 0

    def render_cell(txt: str, allow_links: bool = True) -> str:
        subs = [s.strip() for s in re.split(r"<br\s*/?>", txt)]
        return "<br>".join(render_inline_html(s, linker, seen_weapons, allow_links=allow_links) for s in subs)

    while i < n:
        line = lines[i]
        s = line.strip()
        if not s:
            i += 1
            continue
        if s == "---":
            out.append("<hr>")
            i += 1
            continue
        if s.startswith("```"):
            lang = s[3:].strip().lower()
            block: list[str] = []
            i += 1
            while i < n and not lines[i].strip().startswith("```"):
                block.append(lines[i])
                i += 1
            i += 1
            if lang == "mermaid":
                out.append(render_mermaid_as_flow(block))
            else:
                out.append(f'<pre class="mech-formula"><code>{html_mod.escape(chr(10).join(block))}</code></pre>')
            continue
        if s.startswith("|") and i + 1 < n and lines[i + 1].strip().startswith("|"):
            tbl_lines: list[str] = []
            while i < n and lines[i].strip().startswith("|"):
                tbl_lines.append(lines[i])
                i += 1
            rows, aligns = [], []
            for idx, ln in enumerate(tbl_lines):
                cells = [c.strip() for c in ln.strip().strip("|").split("|")]
                if idx == 1 and all(re.match(r"^:?-{2,}:?$", c) for c in cells):
                    for c in cells:
                        if c.startswith(":") and c.endswith(":"):
                            aligns.append("center")
                        elif c.endswith(":"):
                            aligns.append("right")
                        else:
                            aligns.append("left")
                    continue
                rows.append(cells)
            if rows:
                thead = "<tr>" + "".join(
                    f'<th style="text-align:{aligns[ci] if ci < len(aligns) else "left"}">'
                    f"{render_cell(c, allow_links=False)}</th>"
                    for ci, c in enumerate(rows[0])
                ) + "</tr>"
                body_rows = []
                for r in rows[1:]:
                    tds = "".join(
                        f'<td style="text-align:{aligns[ci] if ci < len(aligns) else "left"}">'
                        f"{render_cell(c, allow_links=True)}</td>"
                        for ci, c in enumerate(r)
                    )
                    body_rows.append(f"<tr>{tds}</tr>")
                out.append(
                    f'<div class="mech-table-wrap"><table class="mech-table"><thead>{thead}</thead>'
                    f'<tbody>{"".join(body_rows)}</tbody></table></div>'
                )
            continue
        if s.startswith(">"):
            bq: list[str] = []
            while i < n and lines[i].strip().startswith(">"):
                bq.append(re.sub(r"^>\s?", "", lines[i].strip()))
                i += 1
            # Skip redundant attribution header already shown in page chrome.
            joined = "\n".join(bq)
            if joined.startswith("**整理与署名**") or joined.startswith("**Author & Attribution**"):
                continue
            inner_html = "<br>".join(
                render_inline_html(ln, linker, seen_weapons, allow_links=True) for ln in bq if ln
            )
            out.append(f'<blockquote class="mech-note">{inner_html}</blockquote>')
            continue
        hm = re.match(r"^(#{1,4})\s+(.*)$", s)
        if hm:
            lvl = len(hm.group(1))
            raw_title = hm.group(2).strip()
            if lvl == 1:
                i += 1
                continue
            sec_counter += 1
            hid = slugify_heading(raw_title, used_ids, sec_counter)
            clean_txt = re.sub(r"`|\*\*|\*", "", raw_title).strip()
            if lvl in (2, 3):
                toc.append((lvl, hid, clean_txt))
            title_html = render_inline_html(raw_title, None, seen_weapons, allow_links=False)
            out.append(
                f'<h{lvl} id="{hid}">{title_html}'
                f'<a class="mech-anchor" href="#{hid}" aria-label="Link to section">#</a></h{lvl}>'
            )
            if widget_html and not widget_inserted and lvl == 2 and (
                "Magnetic" in clean_txt or "磁力" in clean_txt
            ) and sec_counter >= 4:
                out.append(widget_html)
                widget_inserted = True
            i += 1
            continue
        bm = re.match(r"^(\s*)([*\-])\s+(.*)$", line)
        if bm:
            items_html: list[str] = []
            while i < n:
                m_item = re.match(r"^(\s*)([*\-])\s+(.*)$", lines[i])
                if not m_item:
                    break
                indent = len(m_item.group(1))
                text_parts = [m_item.group(3).strip()]
                i += 1
                while (
                    i < n
                    and lines[i].startswith("  ")
                    and not re.match(r"^\s*([*\-]|\d+\.)\s+", lines[i])
                    and not lines[i].strip().startswith("```")
                    and lines[i].strip()
                ):
                    text_parts.append(lines[i].strip())
                    i += 1
                cls = ' class="sub"' if indent >= 2 else ""
                body = "<br>".join(render_inline_html(tp, linker, seen_weapons) for tp in text_parts)
                items_html.append(f"<li{cls}>{body}</li>")
            out.append('<ul class="mech-list">' + "".join(items_html) + "</ul>")
            continue
        om = re.match(r"^(\s*)(\d+)\.\s+(.*)$", line)
        if om:
            items_html = []
            while i < n:
                m_item = re.match(r"^(\s*)(\d+)\.\s+(.*)$", lines[i])
                if not m_item:
                    break
                text_parts = [m_item.group(3).strip()]
                i += 1
                while (
                    i < n
                    and lines[i].startswith("  ")
                    and not re.match(r"^\s*([*\-]|\d+\.)\s+", lines[i])
                    and not lines[i].strip().startswith("```")
                    and lines[i].strip()
                ):
                    text_parts.append(lines[i].strip())
                    i += 1
                body = "<br>".join(render_inline_html(tp, linker, seen_weapons) for tp in text_parts)
                items_html.append(f"<li>{body}</li>")
            out.append('<ol class="mech-list">' + "".join(items_html) + "</ol>")
            continue
        # Footer italics line already covered by site footer.
        if (s.startswith("*本文由") or s.startswith("*Published by")) and s.endswith("*"):
            i += 1
            continue
        out.append(f"<p>{render_inline_html(s, linker, seen_weapons)}</p>")
        i += 1

    return "\n".join(out), toc, seen_weapons


def magnetic_widget_html(lang: str) -> str:
    zh = lang == "zh"
    lbl_title = "磁力破防电实时试算（单精度公式直算）" if zh else "Magnetic Break Calculator (Single-Precision Formula)"
    lbl_pool = "最大护盾 / 超宏防护" if zh else "Max Shield / Overguard"
    lbl_w = "击破武器档位（原生弱点 W）" if zh else "Breaking Weapon Tier (Innate W)"
    lbl_n = "破防时磁力层数 (1–10)" if zh else "Magnetic Stacks (1–10)"
    lbl_elec = "击破武器 +电击元素%" if zh else "Breaking Weapon +Electricity %"
    lbl_sd = "+异常伤害%（S² 双吃）" if zh else "+Status Damage % (S²)"
    lbl_proxy = "目标弱点代理 (V, A)" if zh else "Target Proxy (V, A)"
    lbl_norm = "基础源值 → 归一化" if zh else "Source → Normalized"
    lbl_s1 = "第一段破防直伤包" if zh else "Stage 1 Break Packet"
    lbl_s2 = "第二段电击每跳" if zh else "Stage 2 Electric Tick"
    opts_w = (
        '<option value="0.0">W = 0.0 (赤毒·努寇 / 全部近战)</option>'
        '<option value="0.2">W = 0.2 (托里德 / 信条·循环)</option>'
        '<option value="0.4">W = 0.4 (守望者 Prime)</option>'
        '<option value="0.5" selected>W = 0.5 (奏凯 / 伯斯顿 Prime)</option>'
        '<option value="1.0">W = 1.0 (兰卡 / 绝路 Prime)</option>'
        if zh
        else '<option value="0.0">W = 0.0 (Kuva Nukor / All Melee)</option>'
        '<option value="0.2">W = 0.2 (Torid / Tenet Cycron)</option>'
        '<option value="0.4">W = 0.4 (Vectis Prime)</option>'
        '<option value="0.5" selected>W = 0.5 (Laetum / Burston Prime)</option>'
        '<option value="1.0">W = 1.0 (Lanka / Rubico Prime)</option>'
    )
    opts_p = (
        '<option value="1,2" selected>常规人形 (V=1.0, A=2.0)</option>'
        '<option value="0.05,2">爆破型吞噬者 (V=0.05, A=2.0)</option>'
        '<option value="0,1">科腐巴鲍 / 追随者 (无代理倍率)</option>'
        if zh
        else '<option value="1,2" selected>Standard Humanoid (V=1.0, A=2.0)</option>'
        '<option value="0.05,2">Demolisher Devourer (V=0.05, A=2.0)</option>'
        '<option value="0,1">Techrot Babau / Acolytes (no proxy mult)</option>'
    )
    return f"""<div class="mech-calc" id="magnetic-calculator">
<h3>{lbl_title}</h3>
<div class="mc-grid">
  <label>{lbl_pool}<input id="mc-pool" type="number" value="103623" min="1" step="1000"></label>
  <label>{lbl_w}<select id="mc-w">{opts_w}</select></label>
  <label>{lbl_n}<input id="mc-n" type="number" value="10" min="1" max="10"></label>
  <label>{lbl_elec}<input id="mc-elec" type="number" value="165" min="0" step="15"></label>
  <label>{lbl_sd}<input id="mc-sd" type="number" value="0" min="0" step="10"></label>
  <label>{lbl_proxy}<select id="mc-proxy">{opts_p}</select></label>
</div>
<div class="mc-out">
  <div><span>{lbl_norm}</span><strong id="mc-norm">3,454</strong><small id="mc-q">Q = 5,181</small></div>
  <div><span>{lbl_s1}</span><strong id="mc-s1">6,908</strong><small>{"身体 · 剥甲" if zh else "Body · unarmored"}</small></div>
  <div><span>{lbl_s2}</span><strong id="mc-s2">9,154</strong><small id="mc-tot">6 ticks: 54,924</small></div>
</div>
</div>
<script>(function(){{
  var f=Math.fround;
  function g(id){{return document.getElementById(id);}}
  function run(){{
    var pool=Math.max(0,Number(g("mc-pool").value)||0);
    var W=Number(g("mc-w").value)||0;
    var N=Math.min(10,Math.max(1,Number(g("mc-n").value)||1));
    var elec=(Number(g("mc-elec").value)||0)/100;
    var S=1+(Number(g("mc-sd").value)||0)/100;
    var p=g("mc-proxy").value.split(",").map(Number), V=p[0], A=p[1];
    var Q=Math.trunc(f(f(f(pool)*f(0.005))*f(N)));
    var norm=Q, pb;
    if(V===0&&A===1){{pb=f(f(Q)*f(S));}}
    else{{
      var div=f(f(1)+f(Math.max(W,0)*V));
      norm=Math.floor(f(f(f(Q)/div)+f(0.5)));
      pb=f(f(f(norm)*f(S))*f(A));
    }}
    var seed=f(f(pb)*f(S));
    var tick=f(Math.max(1,f(f(0.5)*f(1+seed)))*f(1+elec));
    var s1=Math.round(pb), s2=Math.round(tick);
    g("mc-norm").textContent=norm.toLocaleString();
    g("mc-q").textContent="Q = "+Q.toLocaleString();
    g("mc-s1").textContent=s1.toLocaleString();
    g("mc-s2").textContent=s2.toLocaleString();
    g("mc-tot").textContent="6 ticks: "+(s2*6).toLocaleString();
  }}
  ["mc-pool","mc-w","mc-n","mc-elec","mc-sd","mc-proxy"].forEach(function(id){{
    var el=g(id); if(el){{el.addEventListener("input",run); el.addEventListener("change",run);}}
  }});
  run();
}})();</script>"""


MECH_CSS = """
.mech-wrap{max-width:1060px;margin:0 auto;padding:20px 18px 64px}
.mech-head{margin:0 0 18px;padding-bottom:14px;border-bottom:1px solid var(--line)}
.mech-head h1{margin:0 0 8px;font-size:24px;line-height:1.25}
.mech-head p{margin:0;color:var(--text-2);max-width:780px}
.mech-cards{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:14px}
@media(max-width:760px){.mech-cards{grid-template-columns:1fr}}
.mech-card{display:flex;flex-direction:column;gap:8px;padding:18px 20px;background:var(--surface);border:1px solid var(--line);border-radius:10px;text-decoration:none;color:var(--text)}
.mech-card:hover{border-color:var(--accent)}
.mech-card h2{margin:0;font-size:17px;line-height:1.35;color:var(--text)}
.mech-card p{margin:0;font-size:13.5px;color:var(--text-2);line-height:1.55}
.mech-card-meta{margin-top:auto;padding-top:8px;border-top:1px solid var(--line);display:flex;justify-content:space-between;font-size:12px;color:var(--muted)}
.mech-layout{display:grid;grid-template-columns:minmax(0,1fr) 248px;gap:22px;align-items:start}
@media(max-width:880px){.mech-layout{grid-template-columns:1fr}.mech-aside{order:-1}}
.mech-article{background:var(--surface);border:1px solid var(--line);border-radius:10px;padding:24px 28px;min-width:0}
@media(max-width:600px){.mech-article{padding:16px}}
.mech-crumb{font-size:12.5px;color:var(--muted);margin-bottom:8px;display:flex;gap:6px;flex-wrap:wrap;align-items:center}
.mech-crumb a{color:var(--text-2);text-decoration:none}
.mech-crumb a:hover{color:var(--accent)}
.mech-article h1{margin:0 0 8px;font-size:24px;line-height:1.28}
.mech-meta{display:flex;gap:14px;flex-wrap:wrap;font-size:12.5px;color:var(--muted);padding-bottom:14px;margin-bottom:18px;border-bottom:1px solid var(--line)}
.mech-meta a{color:var(--accent);text-decoration:none}
.mech-article h2{margin:26px 0 10px;padding-bottom:6px;border-bottom:1px solid var(--line);font-size:18px;line-height:1.35}
.mech-article h3{margin:18px 0 8px;font-size:15.5px;color:var(--text)}
.mech-article h4{margin:14px 0 6px;font-size:14px;color:var(--text-2)}
.mech-anchor{margin-left:6px;font-size:12px;color:var(--muted);text-decoration:none;opacity:.55}
.mech-anchor:hover{opacity:1;color:var(--accent)}
.mech-article p{margin:0 0 10px;line-height:1.68}
.mech-article hr{border:0;border-top:1px solid var(--line);margin:20px 0}
.mech-article code{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:.9em;background:var(--panel2);padding:1px 5px;border-radius:4px}
.mech-formula{margin:12px 0;padding:11px 14px;background:var(--panel3);border:1px solid var(--line);border-left:3px solid var(--accent);border-radius:6px;overflow-x:auto;font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:12.5px;line-height:1.6}
.mech-formula code{background:none;padding:0}
.mech-note{margin:12px 0;padding:10px 14px;background:var(--panel3);border:1px solid var(--line);border-left:3px solid var(--gold);border-radius:6px;color:var(--text-2);font-size:13.5px;line-height:1.6}
.mech-list{margin:6px 0 12px;padding-left:20px;line-height:1.65}
.mech-list li{margin-bottom:4px}
.mech-list li.sub{margin-left:18px;list-style-type:circle}
.mech-table-wrap{overflow-x:auto;margin:12px 0}
.mech-table{width:100%;border-collapse:collapse;font-size:13px;line-height:1.5}
.mech-table th{background:var(--panel2);border-bottom:1px solid var(--line);padding:7px 9px;font-size:12px;color:var(--text-2);white-space:nowrap}
.mech-table td{border-bottom:1px solid var(--line);padding:7px 9px;vertical-align:top}
.mech-table tr:hover td{background:var(--panel3)}
.wlink{display:inline-flex;align-items:center;gap:4px;color:var(--accent);text-decoration:none;vertical-align:baseline}
.wlink span{border-bottom:1px dotted color-mix(in srgb,var(--accent) 45%,transparent)}
.wlink:hover span{border-bottom-style:solid}
.wicon{width:22px;height:15px;object-fit:contain;flex:none;vertical-align:-2px;filter:drop-shadow(0 0 1px rgba(0,0,0,.35))}
.mech-wlist a{display:inline-flex;align-items:center;gap:4px;font-size:12px;padding:3px 7px;border-radius:5px;background:var(--panel2);border:1px solid var(--line);color:var(--text);text-decoration:none}
.mech-wlist a:hover{border-color:var(--accent);color:var(--accent)}
.mech-flow{list-style:none;margin:12px 0;padding:12px 14px;background:var(--panel3);border:1px solid var(--line);border-radius:8px;display:grid;gap:8px}
.mf-node{padding:7px 10px;background:var(--surface);border:1px solid var(--line);border-radius:6px}
.mf-label{font-weight:600;font-size:13px;color:var(--text)}
.mf-next{font-size:12px;color:var(--text-2);margin-top:3px}
.mech-aside{position:sticky;top:64px;display:flex;flex-direction:column;gap:12px}
.mech-box{background:var(--surface);border:1px solid var(--line);border-radius:10px;padding:13px 14px}
.mech-box h3{margin:0 0 8px;font-size:11.5px;text-transform:uppercase;letter-spacing:.05em;color:var(--muted)}
.mech-toc{list-style:none;margin:0;padding:0;display:flex;flex-direction:column;gap:4px;font-size:12.5px}
.mech-toc li.l3{padding-left:10px;font-size:12px}
.mech-toc a{color:var(--text-2);text-decoration:none}
.mech-toc a:hover{color:var(--accent)}
.mech-wlist{display:flex;flex-wrap:wrap;gap:5px}
.mech-cite{font-family:ui-monospace,SFMono-Regular,Menlo,Consolas,monospace;font-size:11px;background:var(--panel3);border:1px solid var(--line);border-radius:6px;padding:7px;color:var(--text-2);word-break:break-all;margin:0 0 8px}
.mech-calc{margin:16px 0;padding:14px 16px;background:var(--panel3);border:1px solid var(--line);border-radius:8px}
.mech-calc h3{margin:0 0 10px;font-size:14px}
.mc-grid{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:8px;margin-bottom:10px}
@media(max-width:640px){.mc-grid{grid-template-columns:1fr}}
.mc-grid label{display:flex;flex-direction:column;gap:3px;font-size:11.5px;color:var(--muted)}
.mc-grid input,.mc-grid select{padding:5px 8px;font:inherit;font-size:13px;background:var(--surface);border:1px solid var(--line);border-radius:6px;color:var(--text)}
.mc-out{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:8px;padding:10px;background:var(--surface);border:1px solid var(--line);border-radius:6px}
@media(max-width:640px){.mc-out{grid-template-columns:1fr}}
.mc-out span{display:block;font-size:11.5px;color:var(--muted)}
.mc-out strong{display:block;font-size:18px;font-variant-numeric:tabular-nums;color:var(--accent)}
.mc-out small{display:block;font-size:11.5px;color:var(--text-2)}
.mech-catalog{margin:22px 0 6px;border:1px solid var(--line);border-radius:8px;background:var(--panel3);padding:10px 14px}
.mech-catalog>summary{cursor:pointer;font-weight:600;font-size:14px;color:var(--text);list-style:none;display:flex;align-items:center;justify-content:space-between;gap:8px}
.mech-catalog>summary::-webkit-details-marker{display:none}
.mech-catalog>summary::after{content:"▸";color:var(--muted);font-size:13px;transition:transform .12s}
.mech-catalog[open]>summary::after{transform:rotate(90deg)}
.mech-cat-body{margin-top:12px}
.mech-filter{width:100%;max-width:360px;padding:6px 10px;margin:0 0 12px;font:inherit;font-size:13px;background:var(--surface);border:1px solid var(--line);border-radius:6px;color:var(--text)}
.mech-filter:focus{outline:none;border-color:var(--accent)}
.mech-tier{margin:10px 0 14px}
.mech-tier h4{margin:0 0 7px;font-size:13px;color:var(--text)}
.mech-tier-grid{display:flex;flex-wrap:wrap;gap:6px}
.mech-chip{display:inline-flex;align-items:center;gap:4px;padding:3px 8px;border-radius:6px;font-size:12.5px;background:var(--surface);border:1px solid var(--line);color:var(--text);text-decoration:none}
.mech-chip:hover{border-color:var(--accent);color:var(--accent)}
.mech-subcat{margin-top:10px;padding:8px 10px;border:1px solid var(--line);border-radius:6px;background:var(--surface)}
.mech-subcat>summary{cursor:pointer;font-size:12.5px;color:var(--text-2);font-weight:600}
.mech-topnav{display:flex;align-items:center;gap:6px;margin-left:auto;flex-wrap:wrap}
.mech-topnav a{padding:4px 9px;border-radius:6px;font-size:13px;color:var(--text-2);text-decoration:none}
.mech-topnav a:hover,.mech-topnav a.cur{background:var(--panel2);color:var(--text)}
.mech-topnav a.cur{font-weight:600}
"""


def page_shell(
    *,
    lang: str,
    title: str,
    description: str,
    canonical_url: str,
    en_url: str,
    zh_url: str,
    md_path: str,
    style_href: str,
    ld_blocks: list[dict],
    body_html: str,
) -> str:
    e = html_mod.escape
    html_lang = "zh-Hans" if lang == "zh" else "en"
    other_href = "/mechanics" + en_url.split("/mechanics", 1)[1] if lang == "zh" else "/mechanics" + zh_url.split("/mechanics", 1)[1]
    other_lbl = "English" if lang == "zh" else "中文"
    nav_weapons = "武器" if lang == "zh" else "Weapons"
    nav_bench = "基准榜" if lang == "zh" else "Benchmark"
    nav_mech = "机制" if lang == "zh" else "Mechanics"
    nav_method = "方法" if lang == "zh" else "Method"
    mech_home = "/mechanics/zh" if lang == "zh" else "/mechanics"
    method_home = "/method/zh" if lang == "zh" else "/method"
    ld_html = "".join(
        f'<script type="application/ld+json">{json.dumps(b, ensure_ascii=False)}</script>\n'
        for b in ld_blocks
    )
    return f"""<!doctype html>
<html lang="{html_lang}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{e(title)} | WFSim</title>
<meta name="description" content="{e(description)}">
<link rel="canonical" href="{e(canonical_url)}">
<link rel="alternate" hreflang="en" href="{e(en_url)}">
<link rel="alternate" hreflang="zh-Hans" href="{e(zh_url)}">
<link rel="alternate" hreflang="x-default" href="{e(en_url)}">
<link rel="alternate" type="text/markdown" href="{e(md_path)}">
<meta property="og:type" content="article">
<meta property="og:site_name" content="WFSim">
<meta property="og:title" content="{e(title)} | WFSim">
<meta property="og:description" content="{e(description)}">
<meta property="og:url" content="{e(canonical_url)}">
<meta property="og:image" content="https://wfsim.app/logo.svg">
<meta name="twitter:card" content="summary">
<link rel="stylesheet" href="{e(style_href)}">
<link rel="icon" type="image/svg+xml" href="/logo.svg">
{ld_html}<style>{MECH_CSS}</style>
<script>(function(){{try{{var t=localStorage.getItem("wfsim-theme");if(t==="dark"||t==="light")document.documentElement.setAttribute("data-theme",t);}}catch(e){{}}}})();</script>
</head>
<body>
<header class="topbar">
  <div class="topbar-inner">
    <a class="brand" href="/">WF<span>Sim</span></a>
    <nav class="mech-topnav">
      <a href="/weapons">{nav_weapons}</a>
      <a href="/benchmark">{nav_bench}</a>
      <a href="{mech_home}" class="cur">{nav_mech}</a>
      <a href="{method_home}">{nav_method}</a>
      <a href="{e(other_href)}" lang="{"en" if lang == "zh" else "zh-Hans"}">{other_lbl}</a>
    </nav>
  </div>
</header>
<main class="mech-wrap">
{body_html}
</main>
<footer class="footer">
  <div class="footer-inner">
    <span>{"WF模拟 是非官方粉丝项目，与 Digital Extremes 无关。" if lang == "zh" else "WFSim is an unofficial fan project, not affiliated with or endorsed by Digital Extremes."}</span>
    · <a href="/weapons">{nav_weapons}</a> · <a href="{mech_home}">{nav_mech}</a> · <a href="{method_home}">{nav_method}</a> · <a href="{e(md_path)}">Markdown</a>
  </div>
</footer>
</body>
</html>
"""


SLOT_LABELS = {
    "primary": ("Primary", "主武器"),
    "secondary": ("Secondary", "副武器"),
    "melee": ("Melee", "近战"),
    "archgun": ("Archgun", "重型枪械"),
    "sentinel": ("Companion", "同伴武器"),
    "exalted": ("Exalted", "显赫武器"),
}


def build_beam_catalog(
    root: Path,
    roster_specs: list[dict],
    wiki_path_fn: Callable[[dict], str],
    weapon_icons: dict,
    zh_weapons: dict,
    lang: str,
    site_url: str,
) -> tuple[str, str]:
    """Generate a collapsible full-roster continuous beam table from data/weapons/."""
    zh = lang == "zh"
    roster_by_id = {s["id"]: s for s in roster_specs}
    rows: list[dict] = []
    for f in sorted((root / "data" / "weapons").rglob("*.yaml")):
        d = yaml.load(f.read_text(encoding="utf-8"), Loader=YAML_LOADER)
        if not isinstance(d, dict) or not d.get("id"):
            continue
        atk = d.get("attack")
        if not isinstance(atk, dict) or atk.get("trigger") != "held":
            continue
        wid = d["id"]
        parent = d.get("transforms_from") or d.get("inherits") or wid
        target = roster_by_id.get(wid) or roster_by_id.get(parent)
        if not target:
            continue
        tid = target["id"]
        href = wiki_path_fn(target)
        icon = asset_icon_url(weapon_icons.get(wid) or weapon_icons.get(tid))
        en_name = d.get("name") or target["name"]
        cn_name = zh_weapons.get(wid) or zh_weapons.get(tid) or en_name
        # Keep form distinction when an alt-fire shares the base weapon's display name.
        if wid.endswith("_alt") and "(" not in en_name:
            en_name = f"{en_name} (Alt-Fire)"
            cn_name = f"{cn_name}（次要）"
        elif wid.endswith("_beam") and "(" not in en_name:
            en_name = f"{en_name} (Alt-Fire)"
            cn_name = f"{cn_name}（次要射线）"
        display = cn_name if zh else en_name
        slot = target.get("slot", "other")
        slot_lbl = SLOT_LABELS.get(slot, (slot, slot))[1 if zh else 0]
        fr = float(atk.get("fire_rate") or 0)
        ammo = float(atk.get("ammo_cost") or 0.5)
        bm = atk.get("beam") or {}
        ramp = bm.get("ramp") or {}
        floor = d.get("beam_ramp_floor")
        floor = 0.2 if floor is None else float(floor)
        rise = float(ramp.get("rise_seconds") or (0.8 if wid.startswith("panthera") else 0.6))
        first = 1.0 if floor >= 1.0 else floor + (1.0 - floor) * min(1.0, 1.0 / (fr * rise)) if fr > 0 else floor
        rows.append({
            "display": display,
            "search": f"{en_name} {cn_name} {slot_lbl}".lower(),
            "href": href,
            "icon": icon,
            "slot": slot_lbl,
            "fr": fr,
            "ammo": ammo,
            "floor": floor,
            "first": first,
        })
    rows.sort(key=lambda r: (r["slot"], r["display"].lower()))

    e = html_mod.escape
    tr_html = []
    md_rows = [
        "",
        "## " + ("附录：全站持续型射线武器完整索引" if zh else "Appendix: Complete Continuous Beam Weapon Index"),
        "",
        "| " + ("武器 | 槽位 | 基础射速 | 每跳弹耗 | 基础下限 → 基础首跳" if zh else "Weapon | Slot | Base FR | Ammo/Tick | Base Floor → Base Tick 1") + " |",
        "| --- | :---: | :---: | :---: | :---: |",
    ]
    for r in rows:
        img = f'<img class="wicon" src="{r["icon"]}" alt="" width="22" height="15" loading="lazy" decoding="async">' if r["icon"] else ""
        floor_txt = "100%" if r["floor"] >= 1.0 else f"{r['floor']*100:.0f}% → {r['first']*100:.1f}%"
        tr_html.append(
            f'<tr data-k="{e(r["search"])}">'
            f'<td><a class="wlink" href="{r["href"]}">{img}<span>{e(r["display"])}</span></a></td>'
            f'<td style="text-align:center">{e(r["slot"])}</td>'
            f'<td style="text-align:center">{r["fr"]:g}</td>'
            f'<td style="text-align:center">{r["ammo"]:g}</td>'
            f'<td style="text-align:center">{floor_txt}</td></tr>'
        )
        md_rows.append(
            f"| [{r['display']}]({site_url}{r['href']}) | {r['slot']} | {r['fr']:g} | {r['ammo']:g} | {floor_txt} |"
        )

    title = f"展开查看全部 {len(rows)} 个持续型射线形态索引（自动同步自 WFSim 武器数据）" if zh else f"Complete Continuous Beam Weapon Index ({len(rows)} modes, generated from WFSim data)"
    ph = "搜索武器名称或槽位…" if zh else "Filter by weapon name or slot…"
    hdr = ("武器", "槽位", "基础射速", "每跳弹耗", "基础下限 → 基础首跳") if zh else ("Weapon", "Slot", "Base FR", "Ammo/Tick", "Base Floor → Tick 1")
    html = f"""<details class="mech-catalog" id="full-beam-index">
<summary>{title}</summary>
<div class="mech-cat-body">
<input class="mech-filter" type="search" placeholder="{ph}" aria-label="{ph}" data-cat-filter="beam">
<div class="mech-table-wrap"><table class="mech-table"><thead><tr><th>{hdr[0]}</th><th style="text-align:center">{hdr[1]}</th><th style="text-align:center">{hdr[2]}</th><th style="text-align:center">{hdr[3]}</th><th style="text-align:center">{hdr[4]}</th></tr></thead><tbody data-cat-rows="beam">{"".join(tr_html)}</tbody></table></div>
</div>
</details>
<script>(function(){{
  var inp=document.querySelector('[data-cat-filter="beam"]');
  if(!inp)return;
  inp.addEventListener("input",function(){{
    var q=inp.value.trim().toLowerCase();
    document.querySelectorAll('[data-cat-rows="beam"] tr').forEach(function(tr){{
      tr.style.display=!q||tr.getAttribute("data-k").indexOf(q)!==-1?"":"none";
    }});
  }});
}})();</script>"""
    return html, "\n".join(md_rows) + "\n"


def build_weakness_catalog(
    roster_specs: list[dict],
    wiki_path_fn: Callable[[dict], str],
    weapon_icons: dict,
    zh_weapons: dict,
    lang: str,
    site_url: str,
) -> tuple[str, str]:
    """Generate a collapsible full-roster native weakpoint (W) tier index from data/weapons/."""
    zh = lang == "zh"
    tiers: dict[float, list[dict]] = {}
    melee_list: list[dict] = []
    for s in roster_specs:
        wb = s.get("weakness_bonus")
        if wb is None:
            continue
        w = round(float(wb), 2)
        wid = s["id"]
        en_name = s["name"]
        cn_name = zh_weapons.get(wid) or en_name
        item = {
            "display": cn_name if zh else en_name,
            "search": f"{en_name} {cn_name} w={w}".lower(),
            "href": wiki_path_fn(s),
            "icon": asset_icon_url(weapon_icons.get(wid)),
        }
        if s.get("slot") == "melee" and w == 0.0:
            melee_list.append(item)
        else:
            tiers.setdefault(w, []).append(item)

    e = html_mod.escape
    firearm_total = sum(len(v) for v in tiers.values())
    melee_list.sort(key=lambda x: x["display"].lower())
    blocks_html: list[str] = []
    md_lines = [
        "",
        "## " + ("附录：全站武器原生弱点加成 W 完整索引" if zh else "Appendix: Complete Weapon Native Weakpoint (W) Index"),
        "",
    ]

    for w in sorted(tiers.keys()):
        items = sorted(tiers[w], key=lambda x: x["display"].lower())
        net = 2.0 / (1.0 + w)
        if zh:
            heading = f"W = {w:.1f}（常规人形净倍率 {net:.2f}× · {len(items)} 把非近战武器" + (f" + 全部 {len(melee_list)} 把近战" if w == 0.0 else "") + "）"
        else:
            heading = f"W = {w:.1f} (Humanoid net {net:.2f}× · {len(items)} non-melee" + (f" + all {len(melee_list)} melee" if w == 0.0 else "") + ")"
        chips = "".join(
            f'<a class="mech-chip" data-k="{e(it["search"])}" href="{it["href"]}">'
            + (f'<img class="wicon" src="{it["icon"]}" alt="" width="22" height="15" loading="lazy" decoding="async">' if it["icon"] else "")
            + f'<span>{e(it["display"])}</span></a>'
            for it in items
        )
        sub = ""
        if w == 0.0 and melee_list:
            m_chips = "".join(
                f'<a class="mech-chip" data-k="{e(it["search"])}" href="{it["href"]}">'
                + (f'<img class="wicon" src="{it["icon"]}" alt="" width="22" height="15" loading="lazy" decoding="async">' if it["icon"] else "")
                + f'<span>{e(it["display"])}</span></a>'
                for it in melee_list
            )
            m_lbl = f"展开全部 {len(melee_list)} 把近战武器（全部为 W = 0.0）" if zh else f"Show all {len(melee_list)} melee weapons (all W = 0.0)"
            sub = f'<details class="mech-subcat" data-melee-sub><summary>{m_lbl}</summary><div class="mech-tier-grid" style="margin-top:8px">{m_chips}</div></details>'
        blocks_html.append(f'<div class="mech-tier" data-w-tier><h4>{heading}</h4><div class="mech-tier-grid">{chips}</div>{sub}</div>')
        md_lines.append(f"### {heading}")
        md_lines.append("")
        md_lines.append(", ".join(f"[{it['display']}]({site_url}{it['href']})" for it in items))
        md_lines.append("")

    title = (
        f"展开查看全站武器原生弱点 W 完整索引（{firearm_total} 把非近战 + {len(melee_list)} 把近战，自动同步自 WFSim 数据）"
        if zh
        else f"Complete Weapon Native Weakpoint (W) Index ({firearm_total} non-melee + {len(melee_list)} melee, generated from WFSim data)"
    )
    ph = "输入任意中英文武器名快速查 W 档位…" if zh else "Filter any weapon name to look up its W tier…"
    html = f"""<details class="mech-catalog" id="full-weakness-index">
<summary>{title}</summary>
<div class="mech-cat-body">
<input class="mech-filter" type="search" placeholder="{ph}" aria-label="{ph}" data-cat-filter="weakness">
{"".join(blocks_html)}
</div>
</details>
<script>(function(){{
  var inp=document.querySelector('[data-cat-filter="weakness"]');
  if(!inp)return;
  inp.addEventListener("input",function(){{
    var q=inp.value.trim().toLowerCase();
    document.querySelectorAll('[data-w-tier]').forEach(function(tier){{
      var any=false;
      tier.querySelectorAll('.mech-chip').forEach(function(a){{
        var ok=!q||a.getAttribute("data-k").indexOf(q)!==-1;
        a.style.display=ok?"":"none";
        if(ok)any=true;
      }});
      tier.style.display=any?"":"none";
      var sub=tier.querySelector('[data-melee-sub]');
      if(sub&&q){{
        var mHit=false;
        sub.querySelectorAll('.mech-chip').forEach(function(a){{if(a.style.display!=="none")mHit=true;}});
        sub.open=mHit;
      }}
    }});
  }});
}})();</script>"""
    return html, "\n".join(md_lines) + "\n"


def ship_mechanics(
    root: Path,
    app_dir: Path,
    site_url: str,
    style_href: str,
    roster_specs: list[dict],
    wiki_path_fn: Callable[[dict], str],
    put_fn: Callable[[Path, str], None],
) -> tuple[list[str], list[dict]]:
    """Build /mechanics and /mechanics/zh index + article pages and .md twins."""
    cat = load_catalog(root)
    linkers = build_weapon_linkers(root, roster_specs, wiki_path_fn)
    assets_raw = yaml.load((root / "data" / "assets.yaml").read_text(encoding="utf-8"), Loader=YAML_LOADER)
    weapon_icons = assets_raw.get("weapons", {}) if isinstance(assets_raw, dict) else {}
    zh_raw = yaml.load((root / "data" / "i18n" / "zh" / "names.yaml").read_text(encoding="utf-8"), Loader=YAML_LOADER)
    zh_weapons = zh_raw.get("weapons", {}) if isinstance(zh_raw, dict) else {}
    articles = cat["articles"]
    urls: list[str] = []
    meta_entries: list[dict] = []

    for lang in ("en", "zh"):
        prefix = "/mechanics" if lang == "en" else "/mechanics/zh"
        idx_meta = cat["index"][lang]
        cards_html: list[str] = []
        idx_md_lines = [
            f"# {idx_meta['title']}",
            "",
            idx_meta["description"],
            "",
        ]
        for art in articles:
            slug = art["slug"]
            loc = art[lang]
            art_path = f"{prefix}/{slug}"
            en_art_url = f"{site_url}/mechanics/{slug}"
            zh_art_url = f"{site_url}/mechanics/zh/{slug}"
            art_url = en_art_url if lang == "en" else zh_art_url
            md_twin_path = f"{art_path}.md"

            raw_md = (root / "docs" / "mechanics" / loc["source"]).read_text(encoding="utf-8")
            cat_html, cat_md = "", ""
            if art.get("catalog") == "beam-roster":
                cat_html, cat_md = build_beam_catalog(
                    root, roster_specs, wiki_path_fn, weapon_icons, zh_weapons, lang, site_url
                )
            elif art.get("catalog") == "weakness-tiers":
                cat_html, cat_md = build_weakness_catalog(
                    roster_specs, wiki_path_fn, weapon_icons, zh_weapons, lang, site_url
                )
            linked_md = link_markdown_body(raw_md, linkers[lang], site_url) + cat_md
            put_fn(app_dir / md_twin_path.lstrip("/"), linked_md)

            widget = magnetic_widget_html(lang) if art.get("widget") == "magnetic-break" else ""
            body_html, toc, seen_weapons = render_markdown_html(raw_md, linkers[lang], widget_html=widget)
            if cat_html:
                body_html += "\n" + cat_html
                cat_id = "full-beam-index" if art.get("catalog") == "beam-roster" else "full-weakness-index"
                cat_toc_lbl = "全武器索引表" if lang == "zh" else "Full Weapon Index"
                toc.append((2, cat_id, cat_toc_lbl))

            toc_html = "".join(
                f'<li class="l{lvl}"><a href="#{hid}">{html_mod.escape(lbl)}</a></li>'
                for lvl, hid, lbl in toc
            )
            wlist_html = "".join(
                f'<a href="{href}">'
                + (f'<img class="wicon" src="{icon}" alt="" width="22" height="15" loading="lazy" decoding="async">' if icon else "")
                + f"{html_mod.escape(name)}</a>"
                for href, (name, icon) in list(seen_weapons.items())[:28]
            )
            cite_ref = (
                f"&lt;ref&gt;[{en_art_url} WFSim Mechanics: {html_mod.escape(art['en']['title'])}], "
                f"updated {art['updated']}.&lt;/ref&gt;"
            )
            lbl_toc = "本页目录" if lang == "zh" else "On This Page"
            lbl_weapons = f"文中武器 ({len(seen_weapons)})" if lang == "zh" else f"Weapons Referenced ({len(seen_weapons)})"
            lbl_cite = "引用格式" if lang == "zh" else "Cite"
            lbl_updated = f"更新日期：{art['updated']}" if lang == "zh" else f"Updated {art['updated']}"
            lbl_md = "纯文本 Markdown" if lang == "zh" else "Plain Markdown"

            article_body = f"""<div class="mech-layout">
<article class="mech-article">
  <div class="mech-crumb"><a href="{prefix}">{"机制" if lang == "zh" else "Mechanics"}</a> / <span>{html_mod.escape(loc["short_title"])}</span></div>
  <h1>{html_mod.escape(loc["title"])}</h1>
  <div class="mech-meta"><span>{lbl_updated}</span><span>{html_mod.escape(art["category"][lang])}</span><a href="{md_twin_path}">{lbl_md}</a></div>
  {body_html}
</article>
<aside class="mech-aside">
  <div class="mech-box"><h3>{lbl_toc}</h3><ul class="mech-toc">{toc_html}</ul></div>
  <div class="mech-box"><h3>{lbl_cite}</h3><div class="mech-cite">{cite_ref}</div></div>
  <div class="mech-box"><h3>{lbl_weapons}</h3><div class="mech-wlist">{wlist_html}</div></div>
</aside>
</div>"""

            ld_blocks: list[dict] = [
                {
                    "@context": "https://schema.org",
                    "@type": "TechArticle",
                    "headline": loc["title"],
                    "description": loc["description"],
                    "inLanguage": "zh-Hans" if lang == "zh" else "en",
                    "dateModified": art["updated"],
                    "url": art_url,
                    "author": {"@type": "Organization", "name": "WFSim", "url": f"{site_url}/"},
                    "publisher": {"@type": "Organization", "name": "WFSim", "url": f"{site_url}/"},
                    "about": {"@type": "VideoGame", "name": "Warframe"},
                }
            ]
            if loc.get("faq"):
                ld_blocks.append({
                    "@context": "https://schema.org",
                    "@type": "FAQPage",
                    "mainEntity": [
                        {
                            "@type": "Question",
                            "name": item["q"],
                            "acceptedAnswer": {"@type": "Answer", "text": item["a"]},
                        }
                        for item in loc["faq"]
                    ],
                })

            page_html = page_shell(
                lang=lang,
                title=loc["title"],
                description=loc["description"],
                canonical_url=art_url,
                en_url=en_art_url,
                zh_url=zh_art_url,
                md_path=md_twin_path,
                style_href=style_href,
                ld_blocks=ld_blocks,
                body_html=article_body,
            )
            put_fn(app_dir / art_path.lstrip("/") / "index.html", page_html)
            urls.append(art_url)

            cards_html.append(
                f'<a class="mech-card" href="{art_path}">'
                f'<h2>{html_mod.escape(loc["title"])}</h2>'
                f'<p>{html_mod.escape(loc["description"])}</p>'
                f'<div class="mech-card-meta"><span>{html_mod.escape(art["category"][lang])}</span>'
                f'<span>{art["updated"]}</span></div></a>'
            )
            idx_md_lines.append(f"- [{loc['title']}]({site_url}{md_twin_path}): {loc['description']}")
            if lang == "en":
                meta_entries.append({
                    "slug": slug,
                    "title": loc["title"],
                    "description": loc["description"],
                    "md_url": f"{site_url}{md_twin_path}",
                    "html_url": art_url,
                    "queries": loc.get("queries", []),
                    "weapons_linked": len(seen_weapons),
                })

        idx_url = f"{site_url}{prefix}"
        idx_md_path = f"{prefix}.md"
        put_fn(app_dir / idx_md_path.lstrip("/"), "\n".join(idx_md_lines) + "\n")
        idx_body = (
            f'<div class="mech-head"><h1>{html_mod.escape(idx_meta["title"])}</h1>'
            f'<p>{html_mod.escape(idx_meta["description"])}</p></div>'
            f'<div class="mech-cards">{"".join(cards_html)}</div>'
        )
        idx_ld = [{
            "@context": "https://schema.org",
            "@type": "CollectionPage",
            "name": idx_meta["title"],
            "description": idx_meta["description"],
            "inLanguage": "zh-Hans" if lang == "zh" else "en",
            "url": idx_url,
        }]
        idx_html = page_shell(
            lang=lang,
            title=idx_meta["title"],
            description=idx_meta["description"],
            canonical_url=idx_url,
            en_url=f"{site_url}/mechanics",
            zh_url=f"{site_url}/mechanics/zh",
            md_path=idx_md_path,
            style_href=style_href,
            ld_blocks=idx_ld,
            body_html=idx_body,
        )
        put_fn(app_dir / prefix.lstrip("/") / "index.html", idx_html)
        urls.append(idx_url)

    return urls, meta_entries


if __name__ == "__main__":
    import argparse
    import sys

    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import build_site_app as bsa

    ap = argparse.ArgumentParser(description="Build /mechanics static pages standalone for preview or tests.")
    ap.add_argument("--out", default=str(Path(__file__).resolve().parent.parent / "target" / "mechanics-preview"))
    args = ap.parse_args()
    out_dir = Path(args.out)
    out_dir.mkdir(parents=True, exist_ok=True)
    # Copy style.css, logo.svg, and referenced same-origin weapon icons for standalone preview.
    static = bsa.STATIC
    (out_dir / "style.css").write_bytes((static / "style.css").read_bytes())
    (out_dir / "logo.svg").write_bytes((static / "logo.svg").read_bytes())
    urls, entries = ship_mechanics(
        bsa.ROOT, out_dir, bsa.SITE, "/style.css", bsa.roster(), bsa.wiki_path, bsa.put
    )
    src_img = bsa.ROOT / "site" / "img"
    if src_img.is_dir():
        dst_img = out_dir / "img"
        dst_img.mkdir(parents=True, exist_ok=True)
        for html_file in out_dir.rglob("index.html"):
            for m in re.finditer(r'src="/img/([^"]+)"', html_file.read_text(encoding="utf-8")):
                fname = m.group(1)
                fsrc = src_img / fname
                fdst = dst_img / fname
                if fsrc.exists() and not fdst.exists():
                    fdst.write_bytes(fsrc.read_bytes())
    print(f"Built {len(urls)} mechanics pages into {out_dir}")
    for ent in entries:
        print(f"  {ent['slug']}: {ent['weapons_linked']} distinct weapons auto-linked")
