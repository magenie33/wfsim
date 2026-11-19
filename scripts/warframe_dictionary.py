#!/usr/bin/env python3
"""Build the site's dictionary from Warframe's localized names.

Run with --fetch to refresh the source snapshot, or without it to rebuild from
vendor/. The site build serves the committed index and needs no upstream access.
"""

import argparse
import json
import re
import subprocess
from concurrent.futures import ThreadPoolExecutor

import de_export


ROOT = de_export.ROOT
CLIENT = ROOT / "vendor" / "export-plus"
OUT = ROOT / "web" / "src" / "static" / "warframe-dictionary.json"
SOURCE = "https://browse.wf/warframe-public-export-plus/"
LANGUAGES = {
    "en": "English", "zh": "简体中文", "tc": "繁體中文", "de": "Deutsch",
    "es": "Español", "fr": "Français", "it": "Italiano", "ja": "日本語",
    "ko": "한국어", "pl": "Polski", "pt": "Português", "ru": "Русский",
    "th": "ไทย", "tr": "Türkçe", "uk": "Українська",
}
EXPORTS = {
    "Abilities": "Ability", "Achievements": "Challenge", "Animals": "Wildlife",
    "Arcanes": "Arcane", "Avionics": "Mod", "Bounties": "Mission",
    "Bundles": "Item", "Challenges": "Challenge", "Codex": "Codex",
    "Customs": "Cosmetic", "Drones": "Item", "Enemies": "Enemy",
    "Factions": "Faction", "Flavour": "Cosmetic", "FocusUpgrades": "Ability",
    "Gear": "Item", "Intrinsics": "Intrinsic", "Keys": "Item",
    "MissionTypes": "Mission", "RailjackWeapons": "Weapon", "Regions": "Location",
    "Resources": "Resource", "Sentinels": "Companion", "Syndicates": "Syndicate",
    "Systems": "Location", "Upgrades": "Mod", "Warframes": "Warframe",
    "Weapons": "Weapon",
}


def download(url):
    return subprocess.check_output(["curl", "-LfsS", "--retry", "2", "--max-time", "60", url])


def fetch_client(file):
    (CLIENT / file).write_bytes(download(SOURCE + file))


def fetch_relics(lang):
    index = de_export._unlzma(download(de_export.ORIGIN.format(lang=lang))).decode()
    file = f"ExportRelicArcane_{lang}.json"
    key = next(line for line in index.split() if line.startswith(file + "!"))
    path = de_export.CACHE / lang / file
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(download(de_export.CONTENT.format(file=key)))
    path.with_name(file + ".hash").write_text(key)


def fetch():
    CLIENT.mkdir(parents=True, exist_ok=True)
    files = [f"dict.{lang}.json" for lang in LANGUAGES]
    files += [f"Export{name}.json" for name in (*EXPORTS, "Recipes")]
    with ThreadPoolExecutor(max_workers=6) as pool:
        list(pool.map(fetch_client, files))
        list(pool.map(fetch_relics, LANGUAGES))


def display(value):
    # Icon markup has no text in the client. A name with unresolved parameters
    # or an unresolved localization path cannot be displayed.
    if not isinstance(value, str) or value.startswith("/") or "|" in value:
        return ""
    return " ".join(re.sub(r"<[^<>]+>", "", value).split())


def entries(source, export):
    if export == "Enemies":
        return source["avatars"].items()
    if export == "Codex":
        return (entry for group in source.values() for entry in group.items())
    if export == "Systems":
        return enumerate(source)
    return source.items()


def build():
    dictionaries = [de_export._load(CLIENT / f"dict.{lang}.json") for lang in LANGUAGES]
    rows = set()
    item_names = {}

    def add(kind, names):
        names = tuple(map(display, names))
        if names[0] and any(names[1:]):
            rows.add((kind, *names))

    def names_for(key):
        return [words.get(key, "") for words in dictionaries]

    def add_names(kind, records):
        for record in records:
            add(kind, names_for(record.get("name")))

    for export, kind in EXPORTS.items():
        source = de_export._load(CLIENT / f"Export{export}.json")
        for item_id, record in entries(source, export):
            names = names_for(record.get("name"))
            add("Quest" if export == "Keys" and "chainStages" in record else kind, names)
            if isinstance(item_id, str) and item_id.startswith("/Lotus/") and names[0]:
                item_names[item_id] = names
            if export == "Warframes":
                add_names("Ability", record.get("abilities", []))
            if export in ("Intrinsics", "Syndicates"):
                add_names(kind, record.get("ranks", record.get("titles", [])))

    recipes = de_export._load(CLIENT / "ExportRecipes.json")
    blueprint_templates = names_for("/Lotus/Language/Items/BlueprintAndItem")
    for record in recipes.values():
        names = item_names.get(record.get("resultType"))
        if names:
            add("Blueprint", (template.replace("|ITEM|", name) if name and "|ITEM|" in template else ""
                              for template, name in zip(blueprint_templates, names)))

    relics = []
    for lang in LANGUAGES:
        source = de_export._load(de_export.CACHE / lang / f"ExportRelicArcane_{lang}.json")
        relics.append({r["uniqueName"]: r["name"] for r in source["ExportRelicArcane"] if "relicRewards" in r})
    for item_id in relics[0]:
        add("Relic", (localized.get(item_id) for localized in relics))

    if len(rows) < 10_000:
        raise ValueError("dictionary source lost its name records")
    return sorted(rows, key=lambda row: (row[1].casefold(), row))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fetch", action="store_true", help="refresh the Warframe export snapshot")
    if parser.parse_args().fetch:
        fetch()
    rows = build()
    with OUT.open("w", encoding="utf-8", newline="\n") as out:
        def encode(value):
            return json.dumps(value, ensure_ascii=False, separators=(",", ":"))
        out.write('{"languages":' + encode(list(LANGUAGES.items())) + ',"rows":[\n')
        out.write(",\n".join(map(encode, rows)))
        out.write("\n]}\n")
    print(f"{len(rows)} terms in {len(LANGUAGES)} languages")


if __name__ == "__main__":
    main()
