# SPDX-License-Identifier: AGPL-3.0-or-later
"""Offline replay of frozen popular fights against real Windows DLLs.

api replays unchanged board/runs; lanes distribute cases. prepared compares
generic runs/fold/score to opaque ABI v1; lanes distribute identical spans
within each short row. Its final scores are not comparable to api timings.
"""

import argparse
from concurrent.futures import ThreadPoolExecutor, wait
import ctypes as c
from dataclasses import dataclass
import hashlib
from itertools import combinations
import json
from pathlib import Path
import platform
import statistics
import sys
from time import perf_counter_ns

sys.dont_write_bytecode = True
from engine_compare import first_difference


ROOT = Path(__file__).resolve().parents[1]
FROZEN = ROOT / 'target/popular-perf'
PGO_HASH = 'e151124790854be03e8c905b6f4486c26cab68ee181abed38cee8850033d3b2e'
PACK_8CE = '95ae3fe8a95e2df003a70d4c48f5037bacb6939922fb70ed06c7c2cca95c963d'
RUNS = b'/api/board/runs'
RULER = '__frozen_explicit_metric__'
SIZE = c.POINTER(c.c_size_t)
U64_MAX = (1 << 64) - 1
GOLDEN_GAP = 0x9E3779B97F4A7C15
WORK_FIELDS = ('runs', 'shots', 'pellets', 'procs', 'dot_ticks', 'field_ticks', 'kills', 'hits', 'body_shots', 'gas')


def encode(value):
    return json.dumps(value, separators=(',', ':'), allow_nan=False).encode()


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def verified(path, expected=None):
    raw = path.read_bytes()
    if expected is not None and sha(raw) != expected.lower():
        raise ValueError(f'SHA-256 mismatch: {path}')
    return raw


def sibling(path, name):
    if Path(name).name != name:
        raise ValueError(f'not a sibling asset: {name}')
    return path.with_name(name)


def answer(raw):
    value = json.loads(raw)
    if value.get('ok') is not True:
        raise ValueError(f'DLL rejected request: {value!r}')
    return value


def mapped(pool, fn, items):
    if pool is None:
        return list(map(fn, items))
    jobs = []
    try:
        for item in items:
            jobs.append(pool.submit(fn, item))
        return [job.result() for job in jobs]
    finally:
        # An error cannot leave a lane using a handle the caller is dropping.
        wait(jobs)


class Native:
    def __init__(self, name, kind, path):
        self.name, self.group, self.board = name, None, None
        path = path.resolve(strict=True)
        meta, expected = {}, None
        if kind != 'embedded':
            manifest = path.read_bytes()
            spec = json.loads(manifest)
            meta.update(manifest=str(path), manifest_sha256=sha(manifest), engine=spec['engine'])
            expected = spec['sha256']
            if kind == 'sfe':
                if not path.name.endswith('.dll.build.json'):
                    raise ValueError('Simfleet manifest must end in .dll.build.json')
                path = path.with_name(path.name.removesuffix('.build.json'))
                meta.update({k: spec.get(k) for k in ('source', 'wrapper', 'cpu', 'pgo', 'patches')})
                if expected == PGO_HASH and spec['engine'] == '8ce96e275dc2':
                    self.group = (spec['engine'], PACK_8CE)
                    meta.update(data_sha256=PACK_8CE, pack='embedded, identity pinned by release evidence')
            else:
                pack = sibling(path, spec['data'])
                path = sibling(path, spec['file'])
                meta.update(pack=str(pack), data_sha256=spec['data_sha256'])
                self.group = (spec['engine'], spec['data_sha256'])
        meta.update(path=str(path), sha256=sha(verified(path, expected)), kind=kind)
        self.lib = c.CDLL(str(path))
        prefix = 'sfe' if kind == 'sfe' else 'wfsim'
        self.call = self.bind(prefix + '_api', c.c_void_p, c.c_char_p, c.c_size_t, c.c_char_p, c.c_size_t, SIZE)
        self.free = self.bind(prefix + '_free', None, c.c_void_p, c.c_size_t)
        if kind == 'official':
            data = verified(pack, spec['data_sha256'])
            install = self.bind('wfsim_install_data', c.c_void_p, c.c_char_p, c.c_size_t, SIZE)
            error = self.take(install, data, len(data))
            if error:
                raise ValueError(f'{name}: pack rejected: {error.decode()}')
        if kind == 'sfe':
            if self.bind('sfe_engine_id', c.c_char_p)().decode() != meta['engine']:
                raise ValueError(f'{name}: unexpected runtime engine ID')
            meta['capabilities'] = self.bind('sfe_capabilities', c.c_uint32)()
            if meta['capabilities'] & 1:
                self.board = self.bind('sfe_board_abi', c.c_uint32)()
            if self.board == 1:
                self.new = self.bind('sfe_board_new', c.c_void_p, c.c_char_p, c.c_size_t)
                self.runs = self.bind('sfe_board_runs', c.c_int64, c.c_void_p, c.c_uint32, c.c_uint32)
                self.score = self.bind('sfe_board_score', c.c_void_p, c.c_void_p, SIZE)
                self.drop = self.bind('sfe_board_drop', None, c.c_void_p)
        self.meta = dict(meta, board_abi=self.board, comparison_group=self.group)

    def bind(self, name, result, *args):
        fn = getattr(self.lib, name)
        fn.restype, fn.argtypes = result, args
        return fn

    def take(self, fn, *args):
        size = c.c_size_t()
        ptr = fn(*args, c.byref(size))
        if not ptr:
            raise ValueError(f'{self.name}: {fn.__name__} returned NULL')
        try:
            return c.string_at(ptr, size.value)
        finally:
            self.free(ptr, size.value)

    def api(self, route, body):
        return self.take(self.call, route, len(route), body, len(body))

    def prepared(self, case, pool):
        ctx = self.new(case.plan, len(case.plan))
        if not ctx:
            raise ValueError(f'{self.name}: board creation failed: {case.id}')
        try:
            progress = mapped(pool, lambda span: self.runs(ctx, *span), case.spans)
            if min(progress) < 0 or max(progress) != case.count:
                raise ValueError(f'{self.name}: failed or incomplete ranges: {progress}')
            return self.take(self.score, ctx), None
        finally:
            self.drop(ctx)


@dataclass
class Case:
    id: str
    body: dict
    mode: str
    batch: int

    def __post_init__(self):
        self.start, self.count, self.request = (self.body[k] for k in ('from', 'count', 'request'))
        runs, seed = self.request['runs'], self.request['seed']
        if any(type(v) is not int for v in (self.start, self.count, runs, seed)) or not (
                0 <= self.start < self.start + self.count <= runs <= 1_000_000 and 0 <= seed < 1 << 64):
            raise ValueError(f'{self.id}: invalid frozen run/seed contract')
        self.wire = encode(self.body)
        if self.mode == 'prepared':
            if self.start or not self.request.get('metric'):
                raise ValueError(f'{self.id}: short-row test needs from=0 and explicit metric')
            self.short = dict(self.request, runs=self.count)
            self.spans = [(n, min(self.batch, self.count - n)) for n in range(0, self.count, self.batch)]
            self.parts = [encode(dict(request=self.short, **{'from': n, 'count': k})) for n, k in self.spans]
            self.plan = encode(dict(request=self.short, ruler=RULER))


def gas_of(value, start, count, seed):
    if value.get('ok') is not True or len(value['shards']) != count:
        raise ValueError(f'expected successful response with {count} shards')
    for index, shard in enumerate(value['shards'], start):
        records = shard['index']
        if len(records) != 1:
            raise ValueError(f'expected a single run key at index {index}')
        hi, lo = (records[0][k] for k in ('hi', 'lo'))
        state = seed ^ ((index * GOLDEN_GAP) & U64_MAX)
        nums = (hi, lo, shard['runs'], shard['gas'])
        if (any(type(v) is not int for v in nums) or nums[:3] != (state >> 32, state & 0xFFFFFFFF, 1)
                or not 0 <= nums[3] <= U64_MAX):
            raise ValueError(f'invalid shard identity/workload at index {index}')
    return sum(s['gas'] for s in value['shards'])


def generic_row(lib, case, pool):
    shards = []
    for (start, count), raw in zip(case.spans, mapped(pool, lambda b: lib.api(RUNS, b), case.parts), strict=True):
        part = answer(raw)
        gas_of(part, start, count, case.request['seed'])
        shards.extend(part['shards'])
    fold = encode(dict(request=case.short, acc=None, pieces=shards))
    acc = answer(lib.api(b'/api/board/fold', fold))['acc']
    score = encode(dict(request=case.short, ruler=RULER, acc=acc))
    return lib.api(b'/api/board/score', score), dict(ok=True, shards=shards)


def timed(fn):
    start = perf_counter_ns()
    value = fn()
    return (perf_counter_ns() - start) / 1_000_000, value


def compare(variants, results, timings, cases, required):
    comparisons = []
    for (a, la, _), (b, lb, _) in combinations(variants, 2):
        group = la.group if la.group and la.group == lb.group else None
        explicit = frozenset((a, b)) in required
        known_conflict = any(la.meta.get(k) and lb.meta.get(k) and la.meta[k] != lb.meta[k]
                             for k in ('engine', 'data_sha256'))
        diffs = {case.id: diff for case in cases
                 if (diff := first_difference(results[a][case.id], results[b][case.id])) is not None}
        check = bool(group or explicit or la is lb)
        row = dict(A=a, B=b, comparison_group=group, required=check, explicitly_required=explicit,
                   known_engine_data_conflict=known_conflict, first_difference_by_case=diffs,
                   verdict=('output-mismatch' if check and diffs else
                            'different-or-unverified-engine-data' if known_conflict or not check else
                            'exact-on-selected-cases'))
        if check and not known_conflict and not diffs and timings[a][cases[0].id] and timings[b][cases[0].id]:
            row['timing_basis'] = ('verified-group' if group else 'same-dll' if la is lb else
                                   'operator-required-pair; binary provenance not independently verified')
            row['timings'] = []
            for case in cases:
                ma, mb = (statistics.median(timings[k][case.id]) / case.count for k in (a, b))
                row['timings'].append(dict(id=case.id, msA=ma, msB=mb, speedup=ma / mb))
        comparisons.append(row)
    return comparisons


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--mode', choices=('api', 'prepared'), default='api')
    p.add_argument('--threads', type=int, default=1)
    p.add_argument('--batch', type=int, default=1, help='runs per prepared-mode span')
    p.add_argument('--repeats', type=int, default=2, help='timed corpus passes per variant, alternating order')
    p.add_argument('--smoke', action='store_true', help='two default cases, no timed repeats')
    p.add_argument('--check-only', action='store_true', help='check all selected cases without timed repeats')
    p.add_argument('--case', action='append', help='select exact frozen case ID (repeatable)')
    for kind in ('official', 'sfe', 'embedded'):
        p.add_argument('--' + kind, action='append', default=[], metavar='NAME=PATH')
    p.add_argument('--require-equal', action='append', default=[], metavar='A=B',
                   help='fail on any full-result diff for this pair; names default to the generic door')
    p.add_argument('--fights', type=Path, help='frozen {id,path,body} array (default: target/popular-perf/fights.json)')
    p.add_argument('--sample', type=Path, help='optional sample.json weapon coverage check for custom fights')
    p.add_argument('--reference', type=Path, help='simulation_cost JSON (API only, observed results, no timings)')
    p.add_argument('--out', required=True, type=Path, help='path to a new JSON report; never overwrites')
    args = p.parse_args()
    out = args.out.resolve()
    if out.suffix.lower() != '.json' or out.exists():
        p.error('--out must be a new .json file')
    if sys.platform != 'win32' or c.sizeof(c.c_void_p) != 8 or platform.machine().lower() not in ('amd64', 'x86_64'):
        p.error('requires x64 Windows Python and CPU support for the selected DLLs (v3 by default)')
    if min(args.threads, args.batch, args.repeats) < 1 or (args.reference and args.mode != 'api'):
        p.error('invalid counts or reference mode')

    fight_path = (args.fights or FROZEN / 'fights.json').resolve()
    sample_path = args.sample or (None if args.fights else FROZEN / 'sample.json')
    sample_path = sample_path.resolve() if sample_path else None
    fights = fight_path.read_bytes()
    sample = sample_path.read_bytes() if sample_path else None
    frozen = json.loads(fights)
    ids = [v['id'] for v in frozen]
    weapons = {v['weapon'] for v in json.loads(sample)['samples']} if sample is not None else None
    if not ids or len(set(ids)) != len(ids) or any(v['path'].encode() != RUNS or (weapons is not None and
                                               v['body']['request']['weapon'] not in weapons) for v in frozen):
        raise ValueError('inconsistent frozen fights/sample matrix')
    smoke_ids = ids[:2] if args.fights else ('torid:single:0', 'torid:multi:0')
    selected = set(args.case or (smoke_ids if args.smoke else ids))
    if not selected.issubset(ids):
        p.error(f'unknown case IDs: {sorted(selected.difference(ids))}')
    cases = [Case(v['id'], v['body'], args.mode, args.batch) for v in frozen if v['id'] in selected]

    specs = {}
    for kind in ('official', 'sfe', 'embedded'):
        for spec in getattr(args, kind):
            name, sep, path = spec.partition('=')
            if not sep or not name or ':' in name or name in specs:
                p.error('use unique NAME=PATH labels without colons')
            specs[name] = (kind, Path(path))
    if not specs:
        p.error('supply --official NAME=manifest, --sfe NAME=manifest or --embedded NAME=DLL')
    libs = {name: Native(name, *spec) for name, spec in specs.items()}
    doors = ('api',) if args.mode == 'api' else ('generic-row', 'prepared')
    variants = [(f'{name}:{door}', lib, door) for name, lib in libs.items() for door in doors
                if door != 'prepared' or lib.board == 1]
    if args.mode == 'prepared' and not any(d == 'prepared' for _, _, d in variants):
        p.error('no selected DLL exposes prepared/board ABI v1')

    results = {key: {} for key, _, _ in variants}
    required = set()
    for spec in args.require_equal:
        pair = frozenset(k if ':' in k else f'{k}:{doors[0]}' for k in spec.split('='))
        if spec.count('=') != 1 or len(pair) != 2 or not pair.issubset(results):
            p.error(f'--require-equal needs two distinct active names: {spec!r}; choices: {list(results)}')
        required.add(pair)
    timings = {key: {case.id: [] for case in cases} for key in results}
    counters = {key: {} for key in results}
    ref, frozen_shards, passes = None, {}, []
    if args.reference:
        raw = args.reference.read_bytes()
        rows = json.loads(raw)
        if [r['id'] for r in rows] != ids:
            raise ValueError('simulation_cost reference IDs/order mismatch')
        expected = {r['id']: r['result'] for r in rows}
        for case in cases:
            gas_of(expected[case.id], case.start, case.count, case.request['seed'])
        ref = dict(path=str(args.reference.resolve()), sha256=sha(raw), scope='observed output only')

    def run(key, lib, door, pool, trial):
        start = perf_counter_ns()
        if door == 'api':
            rows = mapped(pool, lambda case: timed(lambda: (lib.api(RUNS, case.wire), None)), cases)
        else:
            def call(case):
                return lib.prepared(case, pool) if door == 'prepared' else generic_row(lib, case, pool)
            rows = [timed(lambda: call(case)) for case in cases]
        wall_ms = (perf_counter_ns() - start) / 1_000_000
        for case, (ms, (raw, split)) in zip(cases, rows, strict=True):
            value = answer(raw)
            shards = value if door == 'api' else frozen_shards[lib.name][case.id]
            gas = gas_of(shards, case.start, case.count, case.request['seed'])
            # Same integer rates in both frozen source revisions (fight/gas.rs).
            work = min(U64_MAX, gas * 50_000 // 1_000_000 + case.count * 400_000)
            count = dict(runs=case.count, gas=gas, work_from_gas=work)
            if door != 'api':
                if split is not None and (diff := first_difference(frozen_shards[lib.name][case.id], split)) is not None:
                    raise ValueError(f'{key} {case.id}: short-row shards changed: {diff}')
                fields = value['counts']
                want_counts = {field: sum(s[field] for s in shards['shards']) for field in WORK_FIELDS}
                if (type(value['score']) is not float or value['metric'] != case.request['metric'] or
                        first_difference(want_counts, fields) is not None or value['work'] != work or
                        any(type(v) is not int or not 0 <= v <= U64_MAX for v in [value['work'], *fields.values()])):
                    raise ValueError(f'{key} {case.id}: invalid score/metric/work/counts')
                count['work'] = value['work']
            diff = first_difference(results[key].setdefault(case.id, value), value)
            if diff is not None:
                raise ValueError(f'{key} {case.id}: unstable on repeated input: {diff}')
            counters[key][case.id] = count
            if trial is not None:
                timings[key][case.id].append(ms)
        if trial is not None:
            passes.append(dict(variant=key, pass_index=trial, wall_ms=wall_ms))
            print(f'{key} pass {trial + 1}: {wall_ms:.3f} ms', flush=True)

    with ThreadPoolExecutor(max_workers=args.threads) as workers:
        pool = workers if args.threads > 1 else None
        if args.mode == 'prepared':
            for name, lib in libs.items():
                frozen_shards[name] = {case.id: answer(lib.api(RUNS, case.wire)) for case in cases}
        for key, lib, door in variants:
            run(key, lib, door, pool, None)
        preflight = compare(variants, results, timings, cases, required)
        mismatch = any(row['verdict'] == 'output-mismatch' for row in preflight)
        if not (args.smoke or args.check_only or mismatch):
            for trial in range(args.repeats):
                for key, lib, door in (variants if trial % 2 == 0 else reversed(variants)):
                    run(key, lib, door, pool, trial)

    if ref:
        ref['differences'] = {key: {case.id: diff for case in cases
                                  if (diff := first_difference(expected[case.id], values[case.id])) is not None}
                              for key, values in results.items()}
    comparisons = compare(variants, results, timings, cases, required)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open('x', encoding='utf-8') as stream:
        json.dump(dict(mode=args.mode, smoke=args.smoke, check_only=args.check_only,
                       threads=args.threads, host=platform.platform(),
                       batch=args.batch if args.mode == 'prepared' else None, python=sys.version,
                       comparison_scope='full shards' if args.mode == 'api' else 'score/metric/work/all counts, opaque ABI has no shard output',
                       sample=str(sample_path) if sample_path else None, sample_sha256=sha(sample) if sample is not None else None,
                       fights=str(fight_path), fights_sha256=sha(fights),
                       cases=[dict(id=case.id, seed=case.request['seed'], start=case.start, count=case.count,
                                   original_runs=case.request['runs'], request_sha256=sha(case.wire)) for case in cases],
                       libraries={name: lib.meta for name, lib in libs.items()}, reference=ref, comparisons=comparisons,
                       counters=counters, milliseconds=timings, passes=passes, results=results, frozen_shards=frozen_shards),
                  stream, indent=2, allow_nan=False)
    print(f'{len(cases)} cases, {len(variants)} variants: {out}')
    for row in comparisons:
        print(f"{row['A']} vs {row['B']}: {row['verdict']}; {len(row['first_difference_by_case'])} differing cases")
    return int(mismatch or bool(ref and any(ref['differences'].values())))


if __name__ == '__main__':
    sys.exit(main())
