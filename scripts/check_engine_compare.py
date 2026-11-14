# SPDX-License-Identifier: AGPL-3.0-or-later
"""Check comparison failures using actual subprocesses and retained output."""

import argparse
import json
from pathlib import Path
import struct
import sys
import tempfile

from engine_compare import compare, digest, first_difference, load_rows, load_snapshot, write_json


def check(condition, message):
    if not condition:
        raise AssertionError(message)


check(first_difference({"a": 1, "b": 2}, {"b": 2, "a": 1}) is None, "object key order")
for left, right, kind in [(1, True, "type"), (1, 1.0, "type"), (0.0, -0.0, "float64"),
                           (1.0, struct.unpack(">d", bytes.fromhex("3ff0000000000001"))[0], "float64"),
                           ([1, 2], [2, 1], "value"), ([1], [1, 2], "length"),
                           ({"a": 0}, {}, "presence")]:
    check(first_difference(left, right)["kind"] == kind, kind)
check(first_difference({"a/b": {"~": 1}}, {"a/b": {"~": 2}})["pointer"] == "/a~1b/~0", "JSON pointer")

with tempfile.TemporaryDirectory(prefix="wfsim-engine-compare-") as temporary:
    root = Path(temporary)
    coverage_calls = [
        {'id': 'active', 'path': '/api/simulate', 'coverage': {'more_procs_than': 'control'}},
        {'id': 'control', 'path': '/api/simulate'},
    ]
    coverage_rows = [{'id': name, 'milliseconds': [1], 'result': {'ok': True, 'procs': count}}
                     for name, count in [('active', 2), ('control', 1)]]
    coverage_file = root / 'coverage.json'
    write_json(coverage_file, coverage_rows)
    load_rows(coverage_file, coverage_calls)
    coverage_rows[0]['result']['procs'] = 1
    write_json(coverage_file, coverage_rows)
    try:
        load_rows(coverage_file, coverage_calls)
    except ValueError as error:
        check('additional procs' in str(error), 'counterfactual coverage diagnostic')
    else:
        raise AssertionError('an inactive Arcane passed differential coverage')
    calls = root / "calls.json"
    write_json(calls, [{"id": "record", "path": "/api/log", "body": {}}])

    def snapshot(name, *, amount=1.0, dropped=0, unstable=False):
        folder = root / name
        folder.mkdir()
        source = folder / "runner.py"
        source.write_text(
            "import json,pathlib,sys\n"
            "calls=json.loads(pathlib.Path(sys.argv[1]).read_text())\n"
            f"amount={amount!r}\n"
            + ("amount+=len(list(pathlib.Path(sys.argv[2]).parent.glob('*.json')))\n" if unstable else "")
            + f"rows=[dict(id=c['id'],milliseconds=[1],result=dict(ok=True,dropped={dropped},events=[dict(kind='damage',damage=amount)])) for c in calls]\n"
            "pathlib.Path(sys.argv[2]).write_text(json.dumps(rows))\n",
            encoding="utf-8")
        wrapper = folder / ("runner.cmd" if sys.platform == "win32" else "runner")
        if sys.platform == "win32":
            wrapper.write_text(f'@"{sys.executable}" "{source}" %*\n', encoding="utf-8")
        else:
            wrapper.write_text(f'#!{sys.executable}\nexec(compile(open({str(source)!r}).read(), {str(source)!r}, "exec"))\n', encoding="utf-8")
            wrapper.chmod(0o755)
        write_json(folder / "manifest.json", {"schema": 1, "executables": {
            "simulation_cost": {"file": wrapper.name, "sha256": digest(wrapper.read_bytes())}}})
        return folder

    baseline, same, changed = snapshot("A"), snapshot("same"), snapshot("changed", amount=2.0)

    def run(candidate, label):
        return compare(argparse.Namespace(baseline=baseline, candidate=candidate, calls=calls,
                                           output=root / label, rounds=1, repeats=1))

    check(run(same, "equal") == 0, "A/A must pass")
    check(run(changed, "changed-result") == 1, "changed gameplay result must fail")
    changed_report = json.loads((root / "changed-result/report.json").read_text())
    check(changed_report["differences"][0]["pointer"] == "/events/0/damage", "first semantic difference")
    unstable = snapshot("unstable", unstable=True)
    check(run(unstable, "repeat-drift") == 1, "unstable process result must fail")
    report = json.loads((root / "repeat-drift/report.json").read_text())
    check(any(row["comparison"] == "unstable" for row in report["differences"]), "repeat drift classification")
    truncated = snapshot("truncated", dropped=1)
    try:
        run(truncated, "truncated-output")
    except ValueError as error:
        check("truncated" in str(error), "truncation diagnostic")
    else:
        raise AssertionError("truncated record passed")
    executable = changed / ("runner.cmd" if sys.platform == "win32" else "runner")
    executable.write_bytes(executable.read_bytes() + b"\n")
    try:
        load_snapshot(changed)
    except ValueError:
        pass
    else:
        raise AssertionError("changed executable passed")
    write_json(calls, [{"id": "duplicate", "path": "/api/log", "body": {}}] * 2)
    try:
        run(same, "duplicate-input")
    except ValueError:
        pass
    else:
        raise AssertionError("duplicate case identity passed")
    write_json(calls, [{"id": "record", "path": "/api/log", "body": {},
                        "coverage": {"event_kinds": ["shot"]}}])
    try:
        run(same, "missing-enabler")
    except ValueError as error:
        check("required event" in str(error), "coverage diagnostic")
    else:
        raise AssertionError("an unexercised mechanism passed")

print("Engine comparison rejects numeric drift, event changes, unstable runs, truncation and changed artifacts.")
