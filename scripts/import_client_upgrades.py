# SPDX-License-Identifier: AGPL-3.0-or-later
"""Import native attribute ids and original-instruction fold vectors."""

import argparse
import hashlib
import json
import re
from pathlib import Path
import zipfile

ROOT = Path(__file__).resolve().parents[1]
BATCH = "20261107T040140Z-unified-precision"
MANIFEST = "b35a34aef95fea35f65d356a45790213706654a18df26ab8534b18c0dc394ffa"


def mod_inputs(mappings, ranked, enums, migrate, check):
    index = {(row['owner'], row['array'], row['index']): row for row in ranked}
    outputs = []
    skipped = []
    for row in mappings:
        if row['group'] != 'mods' or 'expected_attribute_operation' not in row:
            continue
        if row['effect_kind'] == 'buff' or len(row['native_candidates']) != 1:
            continue
        source = row['native_candidates'][0]
        native = index.get((source['owner'], source['array'], source['index']))
        if native is None or source['array'] != 'Upgrades' or native['offset']:
            skipped.append({'id': row['id'], 'reason': 'unbound source rank projection'})
            continue
        path = ROOT / row['source_file']
        text = path.read_text()
        owner = re.search(r'^internal_name: (.+)$', text, re.M)
        assert owner and owner.group(1).strip().strip('\"\'') in row['native_roots'], row['id']
        maximum = int(re.search(r'^max_rank: (\d+)', text, re.M).group(1))
        assert maximum == native['max_rank'], (row['id'], maximum, native['max_rank'])
        assert [enums[str(native['spec']['UpgradeType'])], native['spec']['OperationType']] == row['expected_attribute_operation']
        starts = list(re.finditer(r'^( *)- kind: (.+)$', text, re.M))
        position = row['effect_index']
        assert position < len(starts), (row['id'], position, len(starts))
        start = starts[position].start()
        end = starts[position + 1].start() if position + 1 < len(starts) else len(text)
        block = text[start:end]
        assert starts[position].group(2).split('#')[0].strip() == row['effect_kind'], row['id']
        indent = starts[position].group(1) + '  '
        spec = native['spec']
        fields = (f"{indent}UpgradeType: {spec['UpgradeType']}\n"
                  f"{indent}OperationType: {spec['OperationType']}\n"
                  f"{indent}Value: {native['raw_value']}\n"
                  f"{indent}FusionPowerStep: {spec['FusionPowerStep']}\n"
                  f"{indent}FusionPowerCurve: {spec['FusionPowerCurve']}\n")
        if migrate and f'{indent}UpgradeType:' not in block:
            assert len(re.findall(rf'^{indent}rank(?:0|Max): .+$', block, re.M)) == 2, row['id']
            block = re.sub(rf'^{indent}rank0: .+\n', fields, block, count=1, flags=re.M)
            block = re.sub(rf'^{indent}rankMax: .+\n', '', block, count=1, flags=re.M)
            path.write_text(text[:start] + block + text[end:])
        if check or migrate:
            assert fields in block, row['id']
            assert not re.search(rf'^{indent}rank(?:0|Max|_values):', block, re.M), row['id']
        outputs.append({'path': row['source_file'], 'effect_index': position,
                        'id': row['id'], 'kind': row['effect_kind'], 'source': source})
    assert len(outputs) == 150 and not skipped, (len(outputs), skipped)
    return outputs


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--exchange", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--migrate-mods", action="store_true")
    parser.add_argument("--check-mods", action="store_true")
    args = parser.parse_args()
    folder = args.exchange / "batches" / BATCH
    raw = (folder / "manifest.json").read_bytes()
    ready = json.loads((folder / "READY.json").read_bytes())
    assert hashlib.sha256(raw).hexdigest() == MANIFEST == ready["manifest_sha256"]
    manifest = json.loads(raw)
    for name in ["evidence.zip", "calculation-verification.json"]:
        expected = next(row for row in manifest["files"] if row["path"] == name)
        data = (folder / name).read_bytes()
        assert len(data) == expected["bytes"]
        assert hashlib.sha256(data).hexdigest() == expected["sha256"]
    with zipfile.ZipFile(folder / "evidence.zip") as archive:
        consumed = {}
        for name in ["enums/enum-evidence.json", "data/operation-order-vectors.json",
                     "data/entries.json", "data/conditional-rank-inputs.json", "data/extra-unlock-verification.json",
                     "data/effect-mapping-coverage.json",
                     "native/apply-rank-to-entry.asm.txt", "native/conditional-chance-and-duration.asm.txt",
                     "native/conditional-stack-count.asm.txt", "native/extra-upgrade-rank-and-unlock.asm.txt",
                     "native/upgrade-manager-apply.asm.txt", "native/upgrade-operation-buckets.asm.txt"]:
            data = archive.read(name)
            expected = next(row for row in manifest["archive_members"]["evidence.zip"] if row["path"] == name)
            assert len(data) == expected["bytes"]
            assert hashlib.sha256(data).hexdigest() == expected["sha256"]
            consumed[name] = data
    enums = json.loads(consumed["enums/enum-evidence.json"])
    vectors = json.loads(consumed["data/operation-order-vectors.json"])
    assert len(enums["upgrades"]) == 428 and len(vectors) == 1115
    fixture = {"schema": 1, "batch": BATCH, "manifest_sha256": MANIFEST,
               "client_build": manifest["client_build"],
               "scope": "Native operation folding and enum identity; filtering and installation are caller-owned",
               "upgrades": enums["upgrades"], "vectors": vectors}
    ids = ["// SPDX-License-Identifier: AGPL-3.0-or-later",
           "// Native enum vocabulary; scripts/import_client_upgrades.py verifies the source binding.",
           "use super::UpgradeType;", "", "#[wfsim_gas_macro::metered]", "impl UpgradeType {"]
    for key, value in sorted(enums["upgrades"].items(), key=lambda row: int(row[0])):
        ids.append(f"    pub const {value}: Self = Self({key});")
    ids.extend(["}", ""])
    outputs = {
        ROOT / "engine/src/rules/upgrades/ids.rs": "\n".join(ids),
        ROOT / "tests/fixtures/client/upgrade-operations-2026-10-28.json": json.dumps(fixture, indent=2) + "\n",
    }
    native_ids = {value: int(key) for key, value in enums['upgrades'].items()}
    entries = json.loads(consumed['data/entries.json'])
    ranked = []
    for entry in entries:
        if entry['projection_status'] != 'native-scalar-rank-projection':
            continue
        transform = entry['rank_transform']
        ranks = entry['ranks']
        offset = transform['rank_offset']
        assert [row['rank'] for row in ranks] == list(range(offset, offset + len(ranks)))
        ranked.append({'owner': entry['owner'], 'array': entry['array'], 'index': entry['index'],
                       'raw_value': entry['base']['raw_token'], 'max_rank': transform['max_rank'], 'offset': offset,
                       'spec': {'UpgradeType': native_ids[entry['upgrade_type']], 'OperationType': entry['operation'],
                                'Value': float(entry['base']['raw_token']), 'FusionPowerStep': float(transform['step_token']),
                                'FusionPowerCurve': transform['curve_token']},
                       'bits': [row['float32_bits_hex_le'] for row in ranks]})
    assert len(ranked) == 4629 and sum(len(row['bits']) for row in ranked) == 20479
    conditional = []
    for entry in json.loads(consumed['data/conditional-rank-inputs.json']):
        inputs = entry['inputs'].copy()
        for field in ['ChanceScalesWithRank', 'DurationScalesWithRank']:
            assert inputs[field] in [0, 1]
            inputs[field] = bool(inputs[field])
        conditional.append({'owner': entry['path'], 'inputs': inputs,
                            'ranks': [[row['chance_bits_le'], row['duration_bits_le'], row['max_stacks']]
                                      for row in entry['ranks']]})
    assert len(conditional) == 1324 and sum(len(row['ranks']) for row in conditional) == 5398
    unlocks = json.loads(consumed['data/extra-unlock-verification.json'])
    assert len(unlocks) == 697
    rank_fixture = {'schema': 1, 'batch': BATCH, 'manifest_sha256': MANIFEST,
                    'scope': 'Native rank and conditional-state construction inputs; runtime selection remains caller-owned',
                    'entries': ranked, 'conditional': conditional, 'unlocks': unlocks,
                    'mod_inputs': mod_inputs(json.loads(consumed['data/effect-mapping-coverage.json']), ranked,
                                             enums['upgrades'], args.migrate_mods, args.check_mods),
                    'unprojected': [{'owner': row['owner'], 'array': row['array'], 'index': row['index'],
                                    'status': row['projection_status']} for row in entries
                                   if row['projection_status'] != 'native-scalar-rank-projection']}
    outputs[ROOT / 'tests/fixtures/client/upgrade-ranks-2026-10-28.json'] = json.dumps(rank_fixture, indent=2) + '\n'
    status = {}
    status_pins = [
        ("contexts", "20261114T031604Z-status-arcane-contexts", "59a45246a8b48064228aaa999cffea1dd9de1633978545a3b96830fbb4b90f5b"),
        ("lifecycle", "20261114T085659Z-status-arcane-lifecycle", "a804daa5f7aa3cf422dd51a6313563d5e9c8f468b10e93986f6f98b4f3b92393"),
    ]
    for key, batch, pin in status_pins:
        directory = args.exchange / "batches" / batch
        raw = (directory / "manifest.json").read_bytes()
        assert hashlib.sha256(raw).hexdigest() == pin == json.loads((directory / "READY.json").read_bytes())["manifest_sha256"]
        index = json.loads(raw)
        data = (directory / "client-evidence.zip").read_bytes()
        expected = next(row for row in index["files"] if row["path"] == "client-evidence.zip")
        assert len(data) == expected["bytes"] and hashlib.sha256(data).hexdigest() == expected["sha256"]
        with zipfile.ZipFile(directory / "client-evidence.zip") as archive:
            payload = archive.read("results.json")
            expected = next(row for row in index["archive_members"]["client-evidence.zip"] if row["path"] == "results.json")
            assert len(payload) == expected["bytes"] and hashlib.sha256(payload).hexdigest() == expected["sha256"]
            status[key] = {"batch": batch, "manifest_sha256": pin, "results": json.loads(payload)}
    outputs[ROOT / "tests/fixtures/client/status-arcanes-2026-10-28.json"] = json.dumps(status, indent=2) + "\n"
    for path, text in outputs.items():
        if args.check:
            assert path.read_text() == text, str(path)
        else:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(text)
    print(f"Verified {len(enums['upgrades'])} native ids, {len(vectors)} folds, 20479 rank values, 5398 conditional inputs and 150 mod consumers")


if __name__ == "__main__":
    main()
