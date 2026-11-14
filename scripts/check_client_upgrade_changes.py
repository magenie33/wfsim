# SPDX-License-Identifier: AGPL-3.0-or-later
"""Prove source changes, new filters, inheritance and native rules cannot disappear in a diff."""

import copy
from client_upgrade_changes import compare, differences, ROOT
import json


def main():
    baseline = {'entries': [{'owner': '/mod', 'array': 'Upgrades', 'index': 0,
                            'fields': {'UpgradeType': 'WEAPON_CRIT_CHANCE', 'Value': '0.15', 'ValidProcTypes': '{}'}}],
                'objects': [{'path': '/mod', 'ancestry_leaf_first': ['/mod', '/old-parent']}],
                'enums': {'244': 'WEAPON_CRIT_CHANCE'}, 'native_spans': {'native/fold.asm.txt': 'old'},
                'members': {'scripts/condition.lua': 'old'},
                'manifest': {'client_build': 'one', 'inputs': {'exe_sha256': 'old'}}}
    bindings = [{'id': 'card', 'path': 'data/mods/card.yaml', 'effect_index': 0,
                 'source': {'owner': '/mod', 'array': 'Upgrades', 'index': 0}}]
    unchanged = compare(baseline, baseline, bindings)
    assert all(not value for key, value in unchanged.items() if key.endswith('_changes'))
    changed = copy.deepcopy(baseline)
    changed['entries'][0]['fields']['Value'] = '0.15000001'
    changed['entries'][0]['fields']['ValidProcTypes'] = '{PT_ELECTROCUTION}'
    changed['entries'][0]['fields']['NewNativeField'] = '1'
    changed['objects'][0]['ancestry_leaf_first'] = ['/mod', '/new-parent']
    changed['objects'].append({'path': '/new-parent', 'fields': {'CanReproc': '0'}})
    changed['enums']['244'] = 'CHANGED_ENUM'
    changed['native_spans']['native/fold.asm.txt'] = 'changed'
    changed['members']['scripts/condition.lua'] = 'changed'
    changed['manifest']['inputs']['exe_sha256'] = 'changed'
    attributes = json.loads((ROOT / "tests/fixtures/client/native-consumers.json").read_text())["attributes"]
    result = compare(baseline, changed, bindings, attributes)
    assert result["entry_changes"][0]["rule_consumers"][0]["UpgradeType"] == 244
    assert "engine/src/rules/critical.rs" in result["entry_changes"][0]["rule_consumers"][0]["consumers"]
    fields = {row['field'] for row in result['entry_changes'][0]['changes']}
    assert {'/fields/Value', '/fields/ValidProcTypes', '/fields/NewNativeField'} <= fields
    assert result['entry_changes'][0]['engine_consumers'][0]['id'] == 'card'
    assert next(row for row in result['object_changes'] if row['owner'] == '/new-parent')['engine_consumers']
    assert result['enum_changes'] and result['native_span_changes'] and result['client_identity_changes']
    assert result['artifact_member_changes']
    assert differences(-0.0, 0.0) and differences(1, 1.0)
    assert differences({'a/b': 1}, {'a/b': 2})[0]['field'] == '/a~1b'
    print('Self-comparison is empty; value/filter/new-field/ancestry/enum/native-code/identity mutations are detected.')


if __name__ == '__main__':
    main()
