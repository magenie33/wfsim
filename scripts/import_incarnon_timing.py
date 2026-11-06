#!/usr/bin/env python3
"""Check or apply the pinned client mode-switch times to Incarnon firearms.

uv run --with PyYAML python scripts/import_incarnon_timing.py --check
The fixture preserves source tokens; YAML uses the shortest matching float32.
"""

import argparse
import json
from pathlib import Path
import re
import struct

import yaml


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "tests/fixtures/incarnon/switching-2026-10-28.json"


def f32(value):
    return struct.pack("<f", float(value))


def decimal(token):
    value = struct.unpack("<f", f32(token))[0]
    for precision in range(1, 10):
        text = format(value, f".{precision}g")
        if f32(text) == f32(token):
            return text if "." in text or "e" in text else text + ".0"
    raise ValueError("No float32 decimal for " + token)


def transform(text, entry, leave):
    pattern = r"(?m)^  transmute_in_seconds:[^\n]*\n(?:  transmute_out_seconds:[^\n]*\n)?"
    replacement = ("  # see notes: incarnon_package_transition_time\n"
                   f"  transmute_in_seconds: {decimal(entry)}\n"
                   f"  transmute_out_seconds: {decimal(leave)}\n")
    note = "  # see notes: incarnon_package_transition_time\n"
    text = text.replace(note, "")
    text, count = re.subn(pattern, lambda _: replacement, text)
    if count != 1:
        raise ValueError("Expected one gauge transition block")
    text = re.sub(r"(?m)^(pseudo_reload:\n(?:  [^\n]*\n)*?)  reload_seconds:[^\n]*\n", r"\1", text)
    return text


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_mutually_exclusive_group(required=True)
    actions.add_argument("--check", action="store_true")
    actions.add_argument("--apply", action="store_true")
    args = parser.parse_args()
    fixture = json.loads(SOURCE.read_text())
    profiles = {r["internal_name"]: r for r in fixture["weapons"]}
    if len(profiles) != len(fixture["weapons"]):
        raise ValueError("Duplicate internal path in timing fixture")
    catalog = {}
    for path in sorted((ROOT / "data/weapons").rglob("*.yaml")):
        text = path.read_text()
        spec = yaml.safe_load(text)
        if spec and "id" in spec:
            catalog[spec["id"]] = (path, text, spec)
    errors, checked = [], 0
    for identity, (path, text, spec) in catalog.items():
        if spec.get("form") != "incarnon" or "gauge_form" not in spec:
            continue
        base = catalog[spec["transforms_from"]][2]
        source = profiles[base["internal_name"]]
        entry = source["entry"]["mode_switch_time"]
        leave = source["exit"]["mode_switch_time"]
        expected = transform(text, entry, leave)
        if args.apply:
            if expected != text:
                path.write_text(expected)
        elif expected != text:
            errors.append(f"{identity}: expected entry {decimal(entry)}, exit {decimal(leave)}, derived cycle duration")
        checked += 1
    if not checked:
        raise ValueError("No Incarnon firearm timings checked")
    if errors:
        parser.exit(1, "\n".join(errors) + "\n")
    print(f"{'Applied' if args.apply else 'Checked'} {checked} Incarnon firearm timings")


if __name__ == "__main__":
    main()
