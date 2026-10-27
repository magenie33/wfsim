# Local agent coordination

`node scripts/agentctl.mjs` gives coding sessions from any vendor the same
task records, inbox and resource reservations. It uses Node 22.23.4 or newer,
built-in SQLite and Git, without a daemon, SDK, account or model API.

The built-in `node:sqlite` may emit Node's experimental API warning on stderr.
It is not a command failure; inspect the exit code and JSON result. No npm
installation is required. Schema 1 stores sessions and handoffs independently
of the tool implementation.

## Scope and adoption

This is opt-in, for trusted sessions on one machine and one Git clone.
Existing claims in the main checkout's `.handoff.md` remain authoritative
for tasks not explicitly moved here. Read those claims before starting work;
the CLI neither imports them nor detects overlaps with them.

Only migrate a task with its current owner's agreement. Put a pointer to its
CLI task id in its legacy block, then keep its status in the CLI, not in both
places. Do not remove someone else's claim. New tasks may use the CLI, but
announce their scope in `.handoff.md` so sessions using the old process can
see the reservation. That block is a pointer, not another status record.

`AGENTS.md` remains the single engineering entry point. A tool-specific
launcher may point to it; it must not carry a separate copy of the rules.
Project knowledge belongs in `docs/` and the shared private notes, not only
in chat history, a vendor memory store or a local session checkpoint.

## Shared storage and output

All worktrees resolve `git rev-parse --path-format=absolute --git-common-dir`
from the script's checkout and use `wfsim-agents/state.sqlite3` below it.
Running from a different shell directory does not select another store.
Separate clones have separate stores. Do not put the SQLite database on a
network filesystem or sync its live files between machines.

SQLite transactions serialize mutations, with a 15-second busy timeout.
Failures roll back and exit nonzero; operational results are JSON envelopes
`{"v":1,"ok":true,"result":...}` on stdout, errors are
`{"v":1,"ok":false,"error":"..."}` on stderr. Argument errors use the same
JSON envelope and exit 1. Unknown schema versions fail instead of guessing.
`--state-dir <directory>` selects an isolated store for tests; omit it for
shared project work.

The database is local, untracked state, not a release artifact. Task and
resource mutations and sent/acknowledged messages have transactionally
recorded events. Heartbeats are current observations, not an activity log.
`state snapshot` exports a readable JSON view; it is not another authority
or a restorable database backup. SQLite's online backup API or a cold copy
while no writers run can make a consistent database backup. Removing the clone
also removes its default coordination state unless backed up separately.
Never store credentials or sensitive content in task details or messages.

## Start and discover

Run commands from your task worktree. These examples use session ids returned
by `start`; `SESSION_A` and `SESSION_B` are placeholders, not tool names.

```text
node scripts/agentctl.mjs session start --tool copilot
node scripts/agentctl.mjs session list
node scripts/agentctl.mjs task list
node scripts/agentctl.mjs resource list
node scripts/agentctl.mjs state snapshot
```

Each start creates a unique session id, even for two instances of one tool.
The tool label is informational. `--worktree <path>` may explicitly record
the working directory; it must exist. Registration does not create, isolate
or validate a Git worktree. Follow `docs/DEVELOPMENT.md` for task isolation.

Call `session heartbeat --session SESSION_A` during long work and before
external operations. Commands carrying `--session` refresh its heartbeat
when they succeed. `recent` means observed within 15 minutes, not proved
to be computing; `unknown` means no recent heartbeat, not proved dead.
No background watcher is installed, and nothing can wake a closed session.

## Task lifecycle and handoff

```text
node scripts/agentctl.mjs task create task-one --session SESSION_A --title "Implement feature" --scope "scripts/example.py; live: none"
node scripts/agentctl.mjs task claim task-one --session SESSION_A
node scripts/agentctl.mjs task show task-one
```

Task ids are unique. Creation leaves a task pending; claiming it atomically
makes it active with one owner. Different task scopes are descriptive:
the CLI does not infer whether two paths or goals overlap. Check both task
lists and agree on shared files before editing.

`task update`, `task handoff` and `task complete` require `--session`,
`--revision <current revision>` and `--details <JSON object>`. Updates replace
the details; read the current record first and preserve information still
needed. Revision checks reject stale writes. No operation silently overwrites
another owner's task.

Handoff and completion require these nonempty string fields:

| Field | Contents |
| --- | --- |
| `summary` | Current result and boundaries |
| `branch` | Branch and worktree, or explicit absence |
| `commit` | Exact commit, or explicitly uncommitted work |
| `validation` | Exact commands and outcomes, including incomplete checks |
| `next` | Next action or explicit completion |
| `post_live` | Release follow-up or none |

Put the JSON in a shell variable and pass it as `--details`; use your shell's
normal quoting, or pass a UTF-8 JSON file with `--details-file <path>` instead.
These are reports, not independently verified assertions.
Use the validation stages in [`CI.md`](CI.md). A finished feature with passing
affected checks is ready for integration, even when release CI has not run.
State that stage in `summary`; put the exact check commands/results and any
failed, cancelled or unrun CI in `validation`. Use `next` to name the integration
or release work still required. Only a candidate validated on its final commit
is ready for main. The CLI's handoff state transfers ownership; it is not a
release approval. Completing a task does not mean its code has shipped.

Handoff releases task ownership, allowing another vendor's session to claim
it with the same details. Completion marks it done and releases ownership.
Neither is allowed while the task holds resources. End a session with
`session end --session SESSION_A` only after handing off or completing all
tasks and releasing resources. An ended session cannot mutate work.

For an abandoned active task, `task claim <id> --session SESSION_B
--confirm-stopped --note "<evidence>"` is an explicit recovery. It requires
the old heartbeat to be stale or ended, and no resources held by that task.
Verify the old execution stopped first: elapsed time is not evidence.
Recovery invalidates the old owner's future task updates.

## Resources and production boundaries

```text
node scripts/agentctl.mjs resource acquire release:main --session SESSION_A --task task-one
node scripts/agentctl.mjs resource release release:main --session SESSION_A
```

Use the same canonical name for the same resource, for example `release:main`,
`prod:database`, `prod:bot` or `dev:8796`. All sessions must agree on names;
aliases are not automatically detected. Acquisition requires an active task
owned by that session. A held name excludes other acquisitions, including
after heartbeat expiry. Release requires its recorded owner.

If that owner is gone, `resource recover <name> --session SESSION_B
--confirm-stopped --note "<evidence>"` removes the reservation after explicit
confirmation. It does not acquire it, kill a process, cancel CI, revoke
credentials or stop an old deployment. Check those separately before
recovery. Never infer it is safe from a stale session alone.

Reservations are advisory coordination, **not authentication or fencing**.
Any trusted local process with database access can impersonate a session or
bypass the CLI. Existing build locks, protected branches, CI, deployment
approvals and actual credential restrictions remain necessary. Nothing here
changes who may merge, grants production access or automatically deploys.

## Messages and inspection

```text
node scripts/agentctl.mjs message send --session SESSION_A --to SESSION_B --body "Please review task-one"
node scripts/agentctl.mjs message inbox --session SESSION_B
node scripts/agentctl.mjs message ack 1 --session SESSION_B
node scripts/agentctl.mjs state events --after 0 --limit 100
```

Messages persist until inspected; only the addressed session can acknowledge
them through the CLI. Acknowledgment means received, not completed. Agents
must poll their inbox at task boundaries; there is no push delivery or
cross-vendor session discovery. An ended recipient rejects new messages;
leave a durable task handoff instead. Session ids are conventions, not secrets.

## Verification

`node scripts/check_agentctl.mjs` uses temporary stores and competing CLI
processes, using the repository's safe worktree cleanup. It covers
exclusive task/resource acquisition, stale revisions,
cross-vendor handoff, explicit recovery, message acknowledgment, invalid
input, schema rejection and shared Git-worktree storage.
CI runs the same suite on Windows and Linux independently of the site build.
