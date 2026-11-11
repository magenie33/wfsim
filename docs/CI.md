# CI selection and release evidence

`scripts/ci_policy.mjs` selects jobs from the changed paths. A pull request uses
its merge base with the target branch; a main push uses the push's previous
commit; a branch dispatch uses its merge base with `origin/main`. Renames include
both paths. An unavailable base, empty diff or unknown path selects everything.
Mixed changes take the union of their requirements.

Selection requires a successful trusted CI run on the comparison base's last
source commit. Generated release and board commits are skipped when finding
that source, using the same exclusions as shipping. If the preceding source is
unverified, failed or cancelled, the new commit runs full CI even for a UI or
docs-only diff. This prevents an unverified core change from riding a later
small change. Successful legacy full CI runs can establish this baseline; PR
runs cannot. API errors also fall back to full validation.

| Changes | Required checks |
| --- | --- |
| Markdown under `docs/`, `README.md`, `LICENSE` | Quick checks and commit messages |
| `docs/CLIENTS.md`, `docs/measurements/` | Full CI: the build reads the contract and counts measurements |
| MCP source and its exclusive checks | Quick checks, build, artifact contracts and MCP checks |
| Bot source and its exclusive checks | Quick checks, build, artifact contracts, bot authentication, packaging and card rendering |
| Static UI, localization and checks used only by frontend jobs | Quick checks, build, all browser and parity shards, MCP and bot checks |
| Worker inputs, declared client protocol fixtures and service checks | Quick checks, build, services, pipeline, all browser checks and all parity shards |
| Generated `site/`, `desktop/payload/`, `mcp/engine.js`, `data/board_state.yaml` only | Quick checks and published-board legality |
| Engine, optimizer, webapi, game data, Rust dependencies, tooling, workflow or any other path | Full CI |

MCP and bot requirements combine when both change. A shared input or unknown
script selects full CI. Consumer selection never reuses a different commit's
site artifact. Generated files mixed with consumers also select full CI.

The product group skips Rust tests, gas equivalence, module reproducibility and
published-board legality. Its Rust and game-data inputs have not changed; it
still builds the current site and checks every browser group, including parity.
Browser check paths come from the selected jobs' commands in `ci.yml`; they
are not a second list. Other scripts and unknown paths select full CI.
Generated snapshots never supply a release build. `site/` remains generated and
must not be committed by hand.

## Development and release batches

Development commits and releases have separate validation stages. Work stays
on task branches while features are developed; a commit or backup push does
not require full CI. Formal releases normally collect ready features into
one or two batches a day; the release owner chooses when a batch closes.

| Stage | Required evidence | Handoff status |
| --- | --- | --- |
| Development | Quick checks and tests for the changed behavior; inspect affected callers | In progress |
| Finished feature | Affected checks pass; exact commit, commands, limitations and pending release checks recorded | Ready for integration |
| Release candidate | All included changes combined on an integration branch; required CI succeeds on that final commit | Ready for main |
| Publication | Confirmed manifest, same validated commit, serial main push and live verification | Live after verification |

Run the cheap checks and relevant Rust tests before handing off; browser
checks use the dev server. Mechanics still require their measurement/golden
coverage, and search changes still require `wfsim-truth`; batching does not
waive domain checks. CI owns the full site build.

Do not dispatch full CI after every subfeature or small follow-up fix. Finish
local checks and review first. Early branch CI is appropriate when a specific
risk needs the CI environment, or when explicitly requested; record the reason
and outcome. Pull requests retain their required CI. Report an existing CI
failure or cancellation explicitly; pending release checks must not conceal a
known defect. Work with unresolved relevant failures is blocked for integration.

The merger takes only explicitly handed-off, ready features, leaving unfinished
work on its task branch. A feature handoff does not require its own full CI run.
When build admission changes, refresh the committed board snapshot with
`gh workflow run publish.yml --ref <integration-branch> -f revalidate_only=true`
before final validation. This checks the committed snapshot through the same
admission helper as CI, removes illegal rows and rebuilds indexes without
fetching the whole library or score table. Legal row bytes, measurement clocks
and the submission count remain intact; listed counts follow the survivors.
It adds no rows or scores. A full projection without that flag still reads the
existing facts, and the daily scheduled projection still runs in full. Both
write to the requested branch for dispatches and main for schedules. Inspect
the generated result, then validate the resulting candidate.
Before moving the batch to main, run CI on the integration branch's final
commit. The path selection above chooses the required jobs; a batch containing
core or unknown changes runs full CI. Separate branches' green results never
prove that their combination works. Fix failures and validate the resulting
candidate before publishing. The manifest records included commits, engine
changes, rescore decisions and deployment follow-up.

To validate the release candidate:

```sh
gh workflow run ci.yml --ref <branch>
gh workflow run ci.yml --ref <branch> -f full=true
```

`full=true` runs every job, including the full-catalog search fixtures, and
disables result reuse. A weekly scheduled run does the same on main. Solver
enumeration, scheduling and scoring changes also require those fixtures on
ordinary dispatches. Unknown inputs fail closed to this coverage. The exact CI
wiring and non-search tooling paths declared in `ci_policy.mjs` retain full
ordinary validation without adding full-catalog search fixtures. Those tools
cover topbar lookup and library intake/publication. Search test execution
(`ci_rust.mjs`) and Rust dependency changes
still require the deep fixtures. Other core changes retain all correctness
jobs with bounded fleet invariant fixtures.

A rebase, squash, cherry-pick or added fix changes the SHA and requires
validation of the resulting candidate. Freeze the candidate after it passes;
a fast-forward of that exact SHA can reuse its evidence when it covers the
selected jobs. Do not request a second full run just because the same validated
commit moves to main. A newer branch run cancels the previous run for that
branch; main runs and shipping are not cancelled in progress.

Frontend-only changes retain every browser, parity, MCP and bot check, a fresh
site, the build/simulate/solve smoke and production-runtime contracts. Static UI
and localization inputs do not invalidate unchanged SQL or factory stress
fixtures. Protocol checks load their frontend fixtures through
`ci_client_inputs.mjs`; selection reads the same declaration and promotes those
inputs to service coverage. Direct frontend reads in service checks fail the
selection check. Shared worker and headless sources use the generator's own
input declaration to select backend coverage before their generated copies
change. Localization read directly by a service fixture follows the same rule.
Worker changes retain complete service/pipeline/factory validation; mixed core,
snapshot or unknown inputs retain the conservative full selection. A frontend
proof cannot substitute for backend or numerical coverage.

## Urgent scoped validation

`fast=true` with a reason selects registered maintenance changes. It runs native
board tests and clippy, service and pipeline checks, focused compute-page checks,
published-board legality and a fresh site artifact. Changed paths and function
boundaries must fit `scripts/ci_fast.mjs`: general engine/data, task protocol, table
schemas, incremental inventory runtime and daemon scheduling do not qualify.
The parent identity repair permits only re-canonicalizing restored mod ranks
with the existing engine helper. Named engine regressions, native admission
checks and engine board tests are mandatory; all other engine changes remain
outside fast coverage. This profile also selects native/WASM gas equality and
the native library builds for its changed engine.
The registered queue uniqueness fence may add only its trigger and unique index;
its fourteen-day refresh and pipeline checks remain mandatory.
The capture window exception changes only its bounded fetch size. Its service
regression must retain one snapshot across pages and preserve prior files on failure.
The comparison base must be verified. The proof records its base and exact
paths; reuse and shipping recheck them. `full=true` does not reuse a fast proof.
Prepared publication may skip the source-only system dependency step, with its
missing-tool regression. Source and legacy desktop builds retain their font
and linker checks; all prepared-component and release proof checks still run.

```sh
gh workflow run ci.yml --ref <branch> -f fast=true -f reason="<maintenance release>"
```

For an incident, manually dispatch CI with `hotfix=true` and a reason. The
diff must fit the profiles in `scripts/ci_hotfix.mjs`; each profile declares
its exact paths and required checks. Profiles combine when a diff spans them.
Unregistered paths, an unverified baseline, an empty diff, a missing reason
or `full=true` reject the request. Renames include both paths.

| Profile | Changes | Required checks |
| --- | --- | --- |
| `factory-sql` | `worker/pg/sync.sql` and its registered regression checks | Factory, eligibility, fairness, settlement, retention and protocol checks; queued settlement and the sync procedure also run together |
| `ci` | CI selection, gates, partitions and the hotfix workflow/runner | Selection, partitions and hotfix boundary/proof regressions |

Quick checks, commit checks and coordination checks always run. Documentation
may accompany a supported change; the client contract and measurements do not
qualify. Engine, game data, UI, database table schemas and unknown tooling
retain their ordinary coverage. A new profile adds explicit paths and tests
that reject out-of-scope changes and incomplete evidence.

```sh
gh workflow run ci.yml --ref <branch> -f hotfix=true -f reason="<incident>"
GITHUB_REPOSITORY=magenie33/wfsim node scripts/ci_runs.mjs hotfix <full-sha> <run-id>
```

The gate records the exact commit, baseline, diff, profiles, checks and policy
digest in `ci-proof`. Failed, skipped or cancelled required checks block it.
Hotfix runs are labelled `hotfix:` and cannot establish a regular CI baseline,
replace full CI, or supply a site release artifact. The verification command
checks the successful run and artifact against the current policy and actual
diff before an operator applies the change.

An authorized emergency push advances the exact validated commit to main.
The main push starts ordinary CI in the background; its result stays separate.
For a SQL hotfix, retain the installed function definitions for rollback,
apply only the validated replacements as their owning role, and verify real
task claims, request errors and fleet throughput. A source push alone does not
replace installed database functions. A site release still uses the ordinary
build and release proof.

## Required gate and reuse

`ci-required` runs even when dependencies fail or skip. It requires the plan,
quick checks and commit checks to succeed, plus every selected product job.
Unexpected skips, cancellations, missing results and failures block the gate.
The explicitly optional native-library producers are separate: missing slices
retain the clients' WASM fallback. Their results must be present, and a failed
producer supplies no artifact; it cannot waive a required correctness job.
Use `ci-required` as the required status check in branch protection. Selection
does not skip the workflow itself. The run summary records each path's class,
the selected jobs and any reused run.
Job outputs carry execution fields; path classifications stay in the summary
and logs so large release batches fit the process environment limit.

A reusable run must be completed and successful, belong to this repository's
CI workflow, name the exact same commit and contain unexpired `site-built`,
`ci-proof`, `release-desktop` and `release-live` artifacts. PR runs are never release evidence. The proof binds the
commit, policy digest, successful jobs and full-catalog depth. A proof must cover every job selected
by the later run: a product-only proof cannot replace Rust or gas checks.
Reused proofs cannot be reused again; the original run
remains the authority. Missing or inaccessible evidence causes checks to run.

`ship.yml` accepts a fresh full or selected proof with a checked site artifact
on the source SHA. It can read the original branch run after a fast-forward to
main; a docs-only, generated-only, PR or reuse-only run supplies no release
artifact. Publishing continues to use the checked build and its existing
serial release workflow.

## Test execution

Quick source checks run before the expensive build and Rust jobs. The workspace,
ordinary webapi and exhaustive accuracy tests have separate jobs. Whole-scope
search tests split into ordinary cases, single-run descent, screened descent
and fleet work conservation. The three invariant fixtures use twelve legal
mods and no arcane choices, retaining full builds, multiple starts, worker
splits and the original fight budgets. They assert intermediate screening and
full evaluation, multiple descent steps and matching final results. Their
original full-catalog versions run in three independent deep jobs when required.
`scripts/ci_rust.mjs` discovers
the real test list and runs exact names, refusing a missing protected test or
empty group. New ordinary whole-scope tests join the ordinary group.

The main page checks run in controls, calculation and community groups. The
additional page checks run in independent groups: accounts and sharing,
views, three builder/search groups, board heads and library integration. Builder
groups balance measured command durations while retaining each complete command.
Bot card rendering
runs with the bot checks. Each command belongs
to one group. `check_ci_partitions` rejects missing, duplicate or empty groups;
the required gate waits for every matrix member. A failed group can be retried
without repeating the other groups. `fail-fast: false` preserves their results.
Riven-request checks spread their extra ruler cases across all four parity
jobs, retaining each weapon, ruler, card shape and link-code input. The partition
check compares the full case set with the unbalanced layout.

System dependency installation skips packages already installed. Site builds
install the CJK font and linker; the desktop preparation job installs its GUI
dependencies. Publishing a verified prepared desktop uses the publish setup.
Setup is noninteractive, has bounded network and process waits,
and prints the package-manager log on failure. Downloads run before package
installation. A failed Ubuntu runner mirror gets one bounded retry using the
official HTTPS archive, with copied source configuration and the same archive
signature checks. Installation then uses only downloaded packages; installation
errors are not retried. A failed setup supplies no
successful validation or release evidence. Mac library builds cache dependencies
and workspace artifacts separately per architecture; a cache failure falls back
to compiling, and every build still runs its native/WASM comparison.

Build setup validates the complete declared art set and pinned OCR/library
bytes before compiling the site. The preflight and the full build use the same
helpers. It writes no generated files.

Service and SQL protocol jobs start after quick checks and production-runtime
contracts without waiting for the site. The runtime job uses the release-pinned
Wrangler runtime and compatibility settings, including a real unsupported-Request
negative control. SQL capture and cancellation also run through the actual TCP
Postgres driver under a non-superuser role before native compilation. Full
native scorer parity remains in the pipeline job.

Factory cases are declared in `scripts/factory_shards.mjs`. Both settlement
modes retain every original random seed and fixed regression. Random cases run
in independent shards; fixed groups run once per mode. Every shard produces a
receipt binding its source hashes, mode, seeds and completed assertions. The
required coverage job rejects missing or duplicate receipts, wrong sources and
unfinished cases. The default local command still runs the whole original suite.
Queued settlement, one-call sync and their combined mode run in three separate
protocol jobs. Every original command/mode pair remains required. These jobs
start beside native inventory checks, with their own isolated databases; the
required gate and reusable proof require every selected mode.

Native Linux and Mac slices build beside site validation, without
release credentials. Published engine targets skip compilation. A producer
stamps each artifact with engine, module, data, source and exact bytes; shipping
downloads from the checked CI run and verifies that stamp before signing.
Secretless desktop and live-runtime producers are required companions of a site
release. Their prepared artifacts come from that same qualified run. A bad
present artifact blocks promotion; unavailable legacy artifacts use the normal
build path. The required gate and reusable proof both enforce the companion jobs.
The Linux producer may restore by its complete input digest. It still verifies
the cached receipt against the official artifact from a successful repository
CI run and checks every output byte before issuing the current run's receipt.
An unproven cache rebuilds normally. PRs exercise secretless preparation, but
their runs cannot supply release evidence or trusted reuse.

`scripts/full_work.mjs` registers expensive validation, site and native builds,
deployment, intake, scoring, board projection and client resource updates. Each
entry names its authoritative inputs, invalidation rule, resource bound, durable
boundary, evidence and rollback. Unknown dependencies retain full validation.
`ci-timing.json` reports job execution, expensive steps and time before each job
starts. That last measurement includes dependencies and runner queue time; it
does not claim to separate them. Unreported cache hits remain unknown.

The shared browser harness polls readiness instead of always waiting thirteen
seconds. Explicit test waits remain explicit. Chrome process/debugging-target
startup gets one bounded retry with stderr and isolated profiles; assertions
and page boot failures are never retried. Parity shards keep running when
another shard fails.

The test profile optimizes dependencies as well as workspace code, retaining
debug assertions and overflow checks. Search tests repeatedly parse and exchange
builds through serde and YAML. This profile changes test execution only; release
build settings are separate.

PRs build their own site from public, digest-checked build inputs without bucket
credentials. Browser jobs always consume that run's `site-built`. Build caches
accelerate compilation; they are never accepted as a successful check.

`node scripts/check_ci_policy.mjs` exercises selection, mixed changes, renames,
failed and cancelled gates, and rejection of unrelated or insufficient proofs.
Change the workflow, selection and gate tests together.
