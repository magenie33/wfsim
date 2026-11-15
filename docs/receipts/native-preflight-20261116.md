# Native engine author preflight

Reviewed combined candidate:
`132ab6294239d6afc3ed68285af64371096c8243`, based on main
`646f70e637d1eda1bbafaa2d080de4bfa7f5d5e7` and the native delivery through
`5f1e8add88538096d1d80bca12d83e13ed83f0ef`.
Narrow engine/interface fix:
`697ee0ebc0b69de0f8512a45ae58adad9b08d3f5`.

The owner-selected update path is the shared native input/query/packet engine.
`CORE.md` states that policy, preserves version-bound older work and keeps
unbound scene inputs explicit. This adoption does not certify full gameplay
parity or permit changing a live task's recorded engine contract.

## Actual combination review

All5525 scoped files of the native delivery were compared against candidate132.
Its eight modified and two additional core-scope files equal the corresponding
main646 blobs exactly: four CLI board/operator tools and the existing Kitgun
status normalization/data/tests. The other native-delivery files are retained.
Worker, workflows, generated site and combat-record probe also retain main's
versions. This source comparison does not substitute for the final combined CI.

## Confirmed blockers and corrections

The candidate's layer check rejects `rules::status`↔`rules::upgrades`.
Late target117 query construction belongs to upgrades; the ordinary sampler
is a pure numeric consumer. The unchanged query body is moved into
`rules/upgrades/scalar.rs`, and its fight caller uses that interface. Native
ordinary tests move under the existing `tests/` directory convention, so their
allowed cross-layer references remain test-only. The layer checker itself is
unchanged and passes. No production dependency is hidden by a new exception.

Candidate132's engine test result is1605 passing and two failing M132
assertions. They compare the resolved binary32 query output to unrounded
decimal/binary64 values at1e-12. The correction separates two boundaries:

| assertion | retained expectation |
| --- | --- |
| raw assembly pricing | golden0.078 per tendril and0.39 combined, with the existing1e-12 check |
| resolved status chance | exact widened binary32 of0.078: `0x3d9fbe77`, 0.07800000160932541 |
| Sure Shot resolved query | exact binary32 `0.078_f32 × (1.0_f32 + 0.9_f32)`: `0x3e17c1be`, 0.14820000529289246 |
| additional multishot | unchanged11.0; it does not divide status chance again |

The panel assertions use equality at the materialized width, without a wider
epsilon. Raw assembly/denominator checks remain separate. All measurement
documents and golden values are unchanged. M132's golden SHA-256 remains
`5dc761865296b2a48546d4ede511255af1509493601f8da3c07ba65d8b4f3125`.
No screenshot parts or per-tendril damage are inferred from that measurement.

## Future source-update tracking

Stat117 had no entry in `native-consumers.json` despite its connected late
query and N07A sampler. A real parsed117 value change was visible but had no
code-consumer link. The added regression fails against the old registry with
`target117 changes must link to the actual N07A consumer`. Both numeric117
and the original enum name now link the upgrade scalar, target adapter and
ordinary sampler to the pinned native fixture. The existing native-span,
unknown-field, inheritance and client-identity diff checks remain active.

`NATIVE_BINDINGS.md` additionally records N07A as implemented and separates
its closed numeric path from upstream365/scanned366, live handler installation,
emission multiplicity and final acceptance/application order. A data-only
update changes data; changed native code requires rebinding and affected
kernel validation. Historical output is comparison evidence, not an authority
over measurements or matching-client evidence.

## Fresh source delivery

Tonfa batch `20261115T162246Z-tonfa-native-projection`, manifest
`5e8e5887ad349633bd27365bd730b16410306747e3d9d567bdee3aa4933df747`,
passes all12 payload and866 archive-member size/SHA-256 checks. It closes the
previously unsupported resource decoding:58 resources and69 actor streams,
with5638 submitted native query volumes. Its own contract keeps live
installation/FSM/clocks and target/cover/history/cinematic acceptance open.
Those submissions are not target hits; no measured melee behavior is replaced
with projected query counts. The source-readiness receipt reflects that scope.

## Affected validation and release boundary

- `cargo test -p wfsim-engine --lib --jobs 2`:1607 passed, zero failed.
- `cargo clippy -p wfsim-engine --all-targets --jobs 2 -- -D warnings`: pass.
- Unchanged `check_engine_layers.mjs`: before fails the cycle; after passes.
- `check_client_upgrade_changes.py`: before fails the missing117 link; after
  passes both symbolic/numeric117 and all existing mutation controls.
- Exact late-query body comparison, ten-file main provenance, unchanged
  goldens/measurement files, comment style and whitespace checks pass.

Author logs and machine-readable source/delivery checks are in
`target/preflight/`. The previous native source-vector, full Native/WASM and
matched-request receipts remain separately scoped evidence. No duplicate full
CI, local site build, board rescore or production action ran in this preflight.

Candidate132 precedes the release owner's producer/no-refill update. Final
publication must use the actual post-update combined SHA and its fresh CI and
artifacts. The original166 engine boundary and persistent cancellation fence
remain owned by the release session and original DB operator. Adopting this
engine does not restart refill, rebuild or reissue canceled work. Actual
Phantasma callback cadence, runtime handler installation and full-game parity
remain explicit open boundaries.
