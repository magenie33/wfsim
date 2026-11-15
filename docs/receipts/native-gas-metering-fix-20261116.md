# Native gas scan correction

The exact failing base is `a36f7e06ffcf7a7f87c09911eef5f6abdfcc8b3e`.
CI43751418263 failed both gas job132926245850/step6's build entry and checks
job132928318932/step10's plain entry. That run remains failed; this correction
does not turn it into a release qualification.

## Cause and correction

The unchanged scanner reports467 top-level items and11 missing annotations:
nine test-only helpers/tests in `secondary_irradiate/tests.rs`, and production
`impl Source` and `impl Listener`. Its first-ten display hides Listener.
Moving only the tests still fails at458 items and names both production
implementations.

The correction puts the unchanged `#[cfg(test)]` module under the existing
`tests/` directory convention and adds `metered` attributes to Source and
Listener. The Rust module name and all seven tests remain intact. Production
method bodies, existing annotations, scanner source, macro weights, SMALL
threshold, gas prices, game data and golden values are unchanged. No scan
exception or wider numerical tolerance is introduced.

## Actual validation

| check | result |
| --- | --- |
| original `node scripts/check_gas.mjs` | exit1, missing test and production items |
| only move tests | exit1, missing Source and Listener |
| complete correction, same plain entry | exit0; all458 production items covered |
| remove Source's production attribute | exit1; Source named |
| remove Listener's production attribute | exit1; Listener named |
| remove the existing SecondaryIrradiate production attribute | exit1; SecondaryIrradiate named |
| restore all attributes, same plain entry | exit0 |
| `PATH=/opt/homebrew/bin:$PATH CARGO_BUILD_JOBS=2 node scripts/check_gas.mjs build` | exit0;51 checks pass |
| `cargo test -p wfsim-engine --lib rules::perks::secondary_irradiate --jobs 2` | seven passed, zero failed |
| `cargo clippy -p wfsim-engine --all-targets --jobs 2 -- -D warnings` | pass |
| engine layers, page bodies, comment style, whitespace | pass |

The build entry builds the actual Native executable and data-apart WASM module
and packs. All nine gas rows retain equal Native/WASM score, gas and work,
including fresh instances, alternate partitions, repeated rows and heartbeats.
The initial local build entry reached packaging but failed because the default
system Python3.9 cannot read `data_packs.py`'s type annotations. That log is
retained. Rerunning unchanged source with installed Python3.14 on PATH passes;
no portability workaround was added to the check.

The annotations remain part of the metered source and engine identity. The
release owner must qualify the final combination and regenerate its artifacts;
an older candidate's CI is not reused. No full CI, production, database, main,
site build, rescore, queue or canceled-work refill/reissue action was performed
by this correction task.

Compact before/after/build logs, three failing negative controls, exact-body
source proof and validation hashes are beside this receipt in
`native-gas-metering-fix-20261116/`. Complete author build/test logs remain in
`target/native-gas-fix/`. The original a36 candidate, old engine snapshots and
frozen8831 preview remain unchanged.

The readable `before.log` strips trailing line spaces from the checker's
truncated diagnostic only. `before.raw.b64` preserves every original byte;
`validation.json` supplies both raw-source and committed-view hashes.
