# SPDX-License-Identifier: AGPL-3.0-or-later
"""Count warmed API heap requests through the already-built native libraries."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess

HARNESS = r'''
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
struct Counting;
static ENABLED: AtomicBool = AtomicBool::new(false);
static LIVE: AtomicU64 = AtomicU64::new(0);
static PEAK: AtomicU64 = AtomicU64::new(0);
static ALLOC: AtomicU64 = AtomicU64::new(0);
static REALLOC: AtomicU64 = AtomicU64::new(0);
static FREE: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let live = LIVE.fetch_add(layout.size() as u64, Relaxed) + layout.size() as u64;
            if ENABLED.load(Relaxed) { ALLOC.fetch_add(1, Relaxed); BYTES.fetch_add(layout.size() as u64, Relaxed); PEAK.fetch_max(live, Relaxed); }
        }
        pointer
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        LIVE.fetch_sub(layout.size() as u64, Relaxed);
        if ENABLED.load(Relaxed) { FREE.fetch_add(1, Relaxed); }
    }
    unsafe fn realloc(&self, pointer: *mut u8, old: Layout, size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(pointer, old, size) };
        if !next.is_null() {
            if size >= old.size() { LIVE.fetch_add((size-old.size()) as u64, Relaxed); }
            else { LIVE.fetch_sub((old.size()-size) as u64, Relaxed); }
            if ENABLED.load(Relaxed) { REALLOC.fetch_add(1, Relaxed); BYTES.fetch_add(size as u64, Relaxed); PEAK.fetch_max(LIVE.load(Relaxed), Relaxed); }
        }
        next
    }
}
#[global_allocator] static HEAP: Counting = Counting;
fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let calls: serde_json::Value = serde_json::from_slice(&std::fs::read(&arguments[1]).unwrap()).unwrap();
    let mut rows = Vec::new();
    for call in calls.as_array().unwrap() {
        let path = call["path"].as_str().unwrap();
        let body = call["body_text"].as_str().unwrap();
        let warm = wfsim_webapi::route_text(path, body);
        assert_eq!(serde_json::from_str::<serde_json::Value>(&warm).unwrap()["ok"], true);
        drop(warm);
        let start = LIVE.load(Relaxed);
        for counter in [&ALLOC, &REALLOC, &FREE, &BYTES] { counter.store(0, Relaxed); }
        PEAK.store(start, Relaxed);
        ENABLED.store(true, Relaxed);
        let result = wfsim_webapi::route_text(path, body);
        ENABLED.store(false, Relaxed);
        let returned_live_delta_bytes = LIVE.load(Relaxed) as i64-start as i64;
        let counters = serde_json::json!({"allocations":ALLOC.load(Relaxed),"reallocations":REALLOC.load(Relaxed),
            "deallocations":FREE.load(Relaxed),"requested_bytes":BYTES.load(Relaxed),
            "peak_additional_live_bytes":PEAK.load(Relaxed)-start,"returned_live_delta_bytes":returned_live_delta_bytes});
        rows.push(format!("{{\"id\":{},\"counts\":{},\"result\":{}}}",call["id"],counters,result));
    }
    std::fs::write(&arguments[2],format!("[{}]",rows.join(","))).unwrap();
}
'''


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline-worktree', type=Path, required=True)
    parser.add_argument('--candidate-worktree', type=Path, required=True)
    parser.add_argument('--calls', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    receipt = {'complete': False, 'calls_sha256': digest(args.calls), 'scope': 'System allocator request counts in warmed direct API calls; not CPU timing or browser allocations', 'builds': {}}
    for name, root in [('A', args.baseline_worktree), ('B', args.candidate_worktree)]:
        folder = args.output / name
        folder.mkdir()
        source = folder / 'probe.rs'
        source.write_text(HARNESS)
        dependencies = root.resolve() / 'target/release/deps'
        libraries = {p.name: digest(p) for p in dependencies.glob('*.rlib')}
        extern = []
        selected = {}
        for library in ['wfsim_webapi', 'serde_json']:
            path = max(dependencies.glob(f'lib{library}-*.rlib'), key=lambda p: p.stat().st_mtime)
            extern += ['--extern', f'{library}={path}']
            selected[library] = path.name
        command = ['rustc', '--edition=2021', '-O', '-C', 'codegen-units=1', '-L', f'dependency={dependencies}', *extern, str(source), '-o', str(folder / 'probe')]
        with (folder / 'build.log').open('wb') as log:
            subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=True)
        subprocess.run([str((folder / 'probe').resolve()), str(args.calls.resolve()), str((folder / 'results.json').resolve())], check=True)
        assert libraries == {p.name: digest(p) for p in dependencies.glob('*.rlib')}, 'libraries changed during measurement'
        receipt['builds'][name] = {'command': command, 'selected_libraries': selected, 'library_sha256': libraries,
                                 'probe_sha256': digest(folder / 'probe'), 'source_sha256': digest(source), 'results_sha256': digest(folder / 'results.json')}
    receipt['complete'] = True
    (args.output / 'receipt.json').write_text(json.dumps(receipt, indent=2) + '\n')
    print('Both native allocation probes completed; results must also match the frozen API responses.')


if __name__ == '__main__':
    main()
