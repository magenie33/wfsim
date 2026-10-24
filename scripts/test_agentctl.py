#!/usr/bin/env python3
"""Coordination invariants, using isolated stores and real competing processes."""
import concurrent.futures
from contextlib import closing
from datetime import datetime, timedelta, timezone
import json
import os
import pathlib
import sqlite3
import subprocess
import sys
import tempfile
import unittest

SCRIPT = pathlib.Path(os.environ.get("AGENTCTL_TEST_SCRIPT", pathlib.Path(__file__).with_name("agentctl.py")))


class CoordinationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.directory = pathlib.Path(self.temp.name)
        self.store = self.directory / "state"
        self.a = self.call("session", "start", "--tool", "vendor-a")["session"]
        self.b = self.call("session", "start", "--tool", "vendor-b")["session"]
        self.create("one")

    def raw(self, *args):
        return subprocess.run(
            [sys.executable, str(SCRIPT), "--state-dir", str(self.store), *map(str, args)],
            capture_output=True, text=True, encoding="utf-8",
        )

    def call(self, *args):
        result = self.raw(*args)
        self.assertEqual(result.returncode, 0, result.stderr)
        return json.loads(result.stdout)["result"]

    def fails(self, *args, contains):
        result = self.raw(*args)
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertIn(contains, json.loads(result.stderr)["error"])

    def create(self, task):
        return self.call("task", "create", task, "--session", self.a,
                         "--title", "Shared work", "--scope", "scripts; live: none")

    def claim(self, task="one", actor=None):
        return self.call("task", "claim", task, "--session", actor or self.a)

    def age(self, actor):
        old = (datetime.now(timezone.utc) - timedelta(hours=1)).isoformat(
            timespec="milliseconds").replace("+00:00", "Z")
        with closing(sqlite3.connect(self.store / "state.sqlite3")) as db, db:
            db.execute("UPDATE sessions SET heartbeat=? WHERE id=?", (old, actor))

    def details(self):
        return json.dumps({"summary": "Ready to continue", "branch": "task-one",
                           "commit": "not committed yet", "validation": "targeted tests passed",
                           "next": "review and commit", "post_live": "none", "tool_note": "可接手"})

    def test_only_one_process_claims_a_task(self):
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            results = list(pool.map(
                lambda actor: self.raw("task", "claim", "one", "--session", actor), (self.a, self.b)))
        self.assertEqual(sorted(r.returncode for r in results), [0, 1])
        winner = self.call("task", "show", "one")
        self.assertEqual(winner["revision"], 1)
        self.assertIn(winner["owner"], (self.a, self.b))

    def test_first_use_is_safe_under_concurrent_processes(self):
        self.store = self.directory / "fresh-state"
        with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
            results = list(pool.map(
                lambda number: self.raw("session", "start", "--tool", f"vendor-{number}"), range(4)))
        self.assertEqual([r.returncode for r in results], [0, 0, 0, 0],
                         [r.stderr for r in results])
        self.assertEqual(len(self.call("session", "list")), 4)

    def test_handoff_between_vendors_preserves_details_and_blocks_old_owner(self):
        self.claim()
        self.call("task", "handoff", "one", "--session", self.a,
                  "--revision", 1, "--details", self.details())
        self.call("session", "end", "--session", self.a)
        task = self.claim(actor=self.b)
        self.assertEqual(task["revision"], 3)
        self.assertEqual(task["details"]["tool_note"], "可接手")
        self.fails("task", "update", "one", "--session", self.a, "--revision", 1,
                   "--details", "{}", contains="session ended")
        self.call("task", "complete", "one", "--session", self.b,
                  "--revision", 3, "--details", self.details())
        self.call("session", "end", "--session", self.b)
        events = self.call("state", "events")
        self.assertIn("task.handoff", [e["action"] for e in events])

    def test_revisions_and_owner_are_checked(self):
        self.claim()
        self.fails("task", "update", "one", "--session", self.b, "--revision", 1,
                   "--details", "{}", contains="owned")
        self.call("task", "update", "one", "--session", self.a, "--revision", 1, "--details", "{}")
        self.fails("task", "update", "one", "--session", self.a, "--revision", 1,
                   "--details", "{}", contains="stale task revision")
        self.fails("task", "handoff", "one", "--session", self.a, "--revision", 2,
                   "--details", "{}", contains="needs nonblank")
        self.assertEqual(self.call("task", "show", "one")["revision"], 2)

    def test_handoff_file_survives_shell_quoting_and_missing_file_is_an_error(self):
        self.claim()
        details = self.directory / "handoff details.json"
        self.fails("task", "update", "one", "--session", self.a, "--revision", 1,
                   "--details-file", details, contains="No such file")
        details.write_text(self.details(), encoding="utf-8")
        handoff = self.call("task", "handoff", "one", "--session", self.a,
                            "--revision", 1, "--details-file", details)
        self.assertEqual(handoff["details"]["tool_note"], "可接手")
        details.unlink()
        events = self.call("state", "events")
        event = next(e for e in events if e["action"] == "task.handoff")
        self.assertEqual(event["details"]["task"]["details"]["tool_note"], "可接手")

    def test_stale_presence_is_unknown_not_permission_to_steal(self):
        self.claim()
        self.age(self.a)
        sessions = self.call("session", "list")
        self.assertEqual(next(s for s in sessions if s["id"] == self.a)["presence"], "unknown")
        self.fails("task", "claim", "one", "--session", self.b, contains="recovery requires")
        self.call("task", "claim", "one", "--session", self.b,
                  "--confirm-stopped", "--note", "Operator confirmed old operation stopped")
        self.fails("task", "update", "one", "--session", self.a, "--revision", 1,
                   "--details", "{}", contains="owned")
        self.fails("task", "claim", "one", "--session", self.a,
                   "--confirm-stopped", "--note", "try", contains="recent heartbeat")

    def test_resource_exclusion_and_explicit_recovery(self):
        self.claim()
        self.create("two")
        self.claim("two", self.b)
        self.call("resource", "acquire", "prod:bot", "--session", self.a, "--task", "one")
        self.age(self.a)
        self.fails("resource", "acquire", "prod:bot", "--session", self.b,
                   "--task", "two", contains="already held")
        self.fails("resource", "release", "prod:bot", "--session", self.b, contains="another session")
        self.fails("resource", "recover", "prod:bot", "--session", self.b,
                   "--note", "not checked", contains="confirm-stopped")
        self.fails("task", "claim", "one", "--session", self.b, "--confirm-stopped",
                   "--note", "stopped", contains="release task resources")
        self.call("resource", "recover", "prod:bot", "--session", self.b,
                  "--confirm-stopped", "--note", "Operator checked the old process is stopped")
        self.call("resource", "acquire", "prod:bot", "--session", self.b, "--task", "two")
        self.fails("task", "handoff", "two", "--session", self.b, "--revision", 1,
                   "--details", self.details(), contains="release task resources")
        self.fails("session", "end", "--session", self.b, contains="owned tasks")
        self.call("resource", "release", "prod:bot", "--session", self.b)

    def test_resource_race_has_one_winner(self):
        self.claim()
        self.create("two")
        self.claim("two", self.b)
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            results = list(pool.map(
                lambda pair: self.raw("resource", "acquire", "release:main",
                                      "--session", pair[0], "--task", pair[1]),
                ((self.a, "one"), (self.b, "two"))))
        self.assertEqual(sorted(r.returncode for r in results), [0, 1])

    def test_messages_are_persistent_and_ack_is_recipient_only(self):
        message = self.call("message", "send", "--session", self.a,
                            "--to", self.b, "--body", "Review task one")
        self.fails("message", "ack", message["id"], "--session", self.a, contains="recipient")
        inbox = self.call("message", "inbox", "--session", self.b)
        self.assertEqual(inbox[0]["body"], "Review task one")
        self.assertIsNone(inbox[0]["acknowledged"])
        self.call("message", "ack", message["id"], "--session", self.b)
        self.assertIsNotNone(self.call("message", "inbox", "--session", self.b)[0]["acknowledged"])
        self.call("session", "end", "--session", self.b)
        self.fails("message", "send", "--session", self.a, "--to", self.b,
                   "--body", "later", contains="recipient ended")

    def test_invalid_input_and_future_schema_fail_without_task_mutation(self):
        self.claim()
        self.fails("task", "update", "one", "--session", self.a, "--revision", 1,
                   "--details", "[]", contains="JSON object")
        self.fails("task", "update", "one", "--session", self.a, "--revision", 1,
                   "--details", "{", contains="Expecting")
        self.fails("task", "update", "one", "--session", self.a, "--revision", 1,
                   "--details", '{"number":NaN}', contains="not a JSON constant")
        self.assertEqual(self.call("task", "show", "one")["revision"], 1)
        with closing(sqlite3.connect(self.store / "state.sqlite3")) as db, db:
            db.execute("PRAGMA user_version = 99")
        self.fails("task", "list", contains="unsupported state schema")

    def test_snapshot_does_not_import_or_overwrite_legacy_claims(self):
        legacy = self.directory / ".handoff.md"
        legacy.write_text("Existing tasks must remain untouched.", encoding="utf-8")
        snapshot = self.call("state", "snapshot")
        self.assertEqual(snapshot["schema"], 1)
        self.assertEqual(len(snapshot["tasks"]), 1)
        self.assertIn("not imported", snapshot["legacy_claims"])
        self.assertEqual(legacy.read_text(encoding="utf-8"), "Existing tasks must remain untouched.")

    def test_default_store_is_shared_by_git_worktrees(self):
        repo = self.directory / "repo"
        worktree = self.directory / "worktree"
        repo.mkdir()
        def git(*args):
            subprocess.run(["git", "-C", str(repo), *map(str, args)],
                           check=True, capture_output=True)
        git("init", "--quiet")
        git("-c", "user.name=Test", "-c", "user.email=test@example.invalid",
            "commit", "--allow-empty", "--no-verify", "-m", "initial")
        git("worktree", "add", "--quiet", "-b", "other", worktree)
        try:
            for checkout in (repo, worktree):
                (checkout / "scripts").mkdir()
                (checkout / "scripts" / "agentctl.py").write_bytes(SCRIPT.read_bytes())
            def run(checkout, *args):
                result = subprocess.run([sys.executable, str(checkout / "scripts" / "agentctl.py"), *args],
                                        capture_output=True, text=True, encoding="utf-8")
                self.assertEqual(result.returncode, 0, result.stderr)
                return json.loads(result.stdout)["result"]
            actor = run(repo, "session", "start", "--tool", "a", "--worktree", str(repo))["session"]
            run(repo, "task", "create", "shared", "--session", actor,
                "--title", "Across worktrees", "--scope", "none")
            self.assertEqual(run(worktree, "task", "list")[0]["id"], "shared")
            self.assertEqual(run(repo, "state", "snapshot")["directory"],
                             run(worktree, "state", "snapshot")["directory"])
        finally:
            remove = pathlib.Path(__file__).with_name("remove_worktree.mjs")
            subprocess.run(["node", str(remove), str(worktree), "--force"],
                           cwd=repo, check=True, capture_output=True)


if __name__ == "__main__":
    unittest.main()
