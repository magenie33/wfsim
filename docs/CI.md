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

## Running and batching

Run quick checks and tests for the changed behavior locally, then push the
branch. CI owns the full site build. To validate a branch:

```sh
gh workflow run ci.yml --ref <branch>
gh workflow run ci.yml --ref <branch> -f full=true
```

`full=true` runs every job and disables result reuse. A weekly scheduled run
does the same on main. Scheduled coverage supplements the release checks; core
changes still require full validation before release.

Combine ready changes onto one integration branch and validate that final
commit. A rebase, squash or cherry-pick changes the SHA and requires validation
of the new combination. A fast-forward of an already validated SHA can reuse
that commit's full evidence. Never infer integration success from separate
branches' results. A newer branch run cancels the previous run for that branch;
main runs and shipping are not cancelled in progress.

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
commit, policy digest and successful jobs. Only full proofs can skip product
jobs in a later run. Reused proofs cannot be reused again; the original run
remains the authority. Missing or inaccessible evidence causes checks to run.

`ship.yml` accepts a fresh full or selected proof with a checked site artifact
on the source SHA. It can read the original branch run after a fast-forward to
main; a docs-only, generated-only, PR or reuse-only run supplies no release
artifact. Publishing continues to use the checked build and its existing
serial release workflow.

## Test execution

Quick source checks run before the expensive build and Rust jobs. Rust tests
run in four groups: the workspace excluding webapi, ordinary webapi tests,
whole-scope search tests, and the webapi exhaustive accuracy test. Their filters
partition the suite; no assertion or test is removed. `fail-fast: false` retains
the other groups' diagnostics when one fails.

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
