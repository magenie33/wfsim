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
| `web/src/static/`, `data/i18n/`, `worker/`, `bot/`, MCP source | Quick checks, build, services, pipeline, all browser checks and all parity shards |
| Generated `site/`, `desktop/payload/`, `mcp/engine.js`, `data/board_state.yaml` only | Quick checks and published-board legality |
| Engine, optimizer, webapi, game data, Rust dependencies, tooling, workflow or any other path | Full CI |

The product group skips Rust tests, gas equivalence, module reproducibility and
published-board legality. Its Rust and game-data inputs have not changed; it
still builds the current site and checks every browser group, including parity.
Scripts and new paths are full CI until their dependencies are explicitly known.
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

`full=true` runs every job and disables result reuse. A weekly scheduled run
does the same on main. Scheduled coverage supplements the release checks; core
changes still require full validation before release.

A rebase, squash, cherry-pick or added fix changes the SHA and requires
validation of the resulting candidate. Freeze the candidate after it passes;
a fast-forward of that exact SHA can reuse its evidence when it covers the
selected jobs. Do not request a second full run just because the same validated
commit moves to main. A newer branch run cancels the previous run for that
branch; main runs and shipping are not cancelled in progress.

## Required gate and reuse

`ci-required` runs even when dependencies fail or skip. It requires the plan,
quick checks and commit checks to succeed, plus every selected product job.
Unexpected skips, cancellations, missing results and failures block the gate.
Use `ci-required` as the required status check in branch protection. Selection
does not skip the workflow itself. The run summary records each path's class,
the selected jobs and any reused run.

A reusable run must be completed and successful, belong to this repository's
CI workflow, name the exact same commit and contain unexpired `site-built` and
`ci-proof` artifacts. PR runs are never release evidence. The proof binds the
commit, policy digest and successful jobs. A proof must cover every job selected
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
and fleet work conservation. The descent cases share the same assertions and
budgets; each candidate-run setting executes once. `scripts/ci_rust.mjs` discovers
the real test list and runs exact names, refusing a missing protected test or
empty group. New ordinary whole-scope tests join the ordinary group.

The additional page checks run in five independent groups: accounts and sharing,
views, card rendering, board heads and library integration. Each command belongs
to one group. `check_ci_partitions` rejects missing, duplicate or empty groups;
the required gate waits for every matrix member. A failed group can be retried
without repeating the other groups. `fail-fast: false` preserves their results.
Riven-request checks spread their extra ruler cases across all four parity
jobs, retaining each weapon, ruler, card shape and link-code input. The partition
check compares the full case set with the unbalanced layout.

System dependency installation skips packages already installed. Site builds
install the CJK font and linker; publishing also installs the Linux shell's GUI
dependencies. Setup is noninteractive, has bounded network and process waits,
and prints the package-manager log on failure. A failed setup supplies no
successful validation or release evidence. Mac library builds cache dependencies
and workspace artifacts separately per architecture; a cache failure falls back
to compiling, and every build still runs its native/WASM comparison.

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
