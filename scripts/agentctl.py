#!/usr/bin/env python3
"""Local, tool-neutral coordination. See docs/COORDINATION.md."""
import argparse
from datetime import datetime, timedelta, timezone
import json
import pathlib
import re
import sqlite3
import subprocess
import sys
import uuid

VERSION = 1
STALE_SECONDS = 900
SCHEMA = (
    """CREATE TABLE IF NOT EXISTS sessions (
        id TEXT PRIMARY KEY, tool TEXT NOT NULL, worktree TEXT NOT NULL,
        heartbeat TEXT NOT NULL, ended INTEGER NOT NULL DEFAULT 0)""",
    """CREATE TABLE IF NOT EXISTS tasks (
        id TEXT PRIMARY KEY, title TEXT NOT NULL, scope TEXT NOT NULL,
        state TEXT NOT NULL DEFAULT 'pending', owner TEXT REFERENCES sessions(id),
        revision INTEGER NOT NULL DEFAULT 0, details TEXT NOT NULL DEFAULT '{}')""",
    """CREATE TABLE IF NOT EXISTS resources (
        name TEXT PRIMARY KEY, owner TEXT NOT NULL REFERENCES sessions(id),
        task TEXT NOT NULL REFERENCES tasks(id), acquired TEXT NOT NULL)""",
    """CREATE TABLE IF NOT EXISTS messages (
        id INTEGER PRIMARY KEY, sender TEXT NOT NULL REFERENCES sessions(id),
        recipient TEXT NOT NULL REFERENCES sessions(id), body TEXT NOT NULL,
        at TEXT NOT NULL, acknowledged TEXT)""",
    """CREATE TABLE IF NOT EXISTS events (
        id INTEGER PRIMARY KEY, at TEXT NOT NULL, actor TEXT,
        action TEXT NOT NULL, subject TEXT NOT NULL, details TEXT NOT NULL)""",
)


class Conflict(Exception):
    pass


def instant(offset_seconds=0):
    return (datetime.now(timezone.utc) + timedelta(seconds=offset_seconds)).isoformat(
        timespec="milliseconds").replace("+00:00", "Z")


def reject_constant(value):
    raise ValueError(f"not a JSON constant: {value}")


def text(value):
    if not value.strip():
        raise argparse.ArgumentTypeError("must not be blank")
    return value


def identifier(value):
    if not re.fullmatch(r"[a-z0-9][a-z0-9._:-]{0,119}", value):
        raise argparse.ArgumentTypeError("use 1-120 lowercase letters, digits, . _ : -")
    return value


def positive(value):
    number = int(value)
    if number <= 0:
        raise argparse.ArgumentTypeError("must be positive")
    return number


def default_directory():
    root = pathlib.Path(__file__).resolve().parent.parent
    result = subprocess.run(
        ["git", "-C", str(root), "rev-parse", "--path-format=absolute", "--git-common-dir"],
        capture_output=True, text=True, encoding="utf-8", check=True,
    )
    return pathlib.Path(result.stdout.strip()) / "wfsim-agents"


class Store:
    def __init__(self, directory):
        self.directory = pathlib.Path(directory).resolve()
        self.directory.mkdir(parents=True, exist_ok=True)
        self.db = sqlite3.connect(self.directory / "state.sqlite3", timeout=15)
        self.db.row_factory = sqlite3.Row
        self.db.execute("PRAGMA foreign_keys = ON")

    def begin(self):
        self.db.execute("BEGIN IMMEDIATE")
        version = self.db.execute("PRAGMA user_version").fetchone()[0]
        if version not in (0, VERSION):
            raise Conflict(f"unsupported state schema {version}; expected {VERSION}")
        for statement in SCHEMA:
            self.db.execute(statement)
        self.db.execute(f"PRAGMA user_version = {VERSION}")

    def rows(self, sql, params=()):
        return [dict(row) for row in self.db.execute(sql, params)]

    def one(self, table, key, value):
        # Table and key are internal constants, never CLI input.
        row = self.db.execute(f"SELECT * FROM {table} WHERE {key} = ?", (value,)).fetchone()
        if row is None:
            raise Conflict(f"unknown {table}: {value}")
        return dict(row)

    def event(self, actor, action, subject, details):
        self.db.execute(
            "INSERT INTO events(at,actor,action,subject,details) VALUES (?,?,?,?,?)",
            (instant(), actor, action, subject, json.dumps(details, ensure_ascii=False)),
        )

    def session(self, actor):
        row = self.one("sessions", "id", actor)
        if row["ended"]:
            raise Conflict("session ended; register a new session")
        self.db.execute("UPDATE sessions SET heartbeat=? WHERE id=?", (instant(), actor))
        return row

    def owned_task(self, actor, task_id):
        task = self.one("tasks", "id", task_id)
        if task["owner"] != actor or task["state"] != "active":
            raise Conflict("task is not active and owned by this session")
        return task

    def free_resources(self, task_id):
        held = self.rows("SELECT name FROM resources WHERE task=?", (task_id,))
        if held:
            raise Conflict(f"release task resources first: {[r['name'] for r in held]}")

    def sessions(self):
        cutoff = instant(-STALE_SECONDS)
        return [
            {**row, "presence": "ended" if row["ended"] else
             "recent" if row["heartbeat"] >= cutoff else "unknown"}
            for row in self.rows("SELECT * FROM sessions ORDER BY id")
        ]

    def task(self, task_id):
        task = self.one("tasks", "id", task_id)
        task["details"] = json.loads(task["details"])
        return task

    def run(self, args):
        group, action = args.group, args.action
        actor = getattr(args, "session", None)
        if actor:
            self.session(actor)
        if group == "session":
            if action == "start":
                actor = str(uuid.uuid4())
                worktree = str(pathlib.Path(args.worktree).resolve())
                if not pathlib.Path(worktree).is_dir():
                    raise Conflict("worktree directory does not exist")
                self.db.execute("INSERT INTO sessions VALUES (?,?,?,?,0)",
                                (actor, args.tool, worktree, instant()))
                self.event(actor, "session.start", actor, {"tool": args.tool, "worktree": worktree})
                return {"session": actor}
            if action == "list":
                return self.sessions()
            if action == "heartbeat":
                return {"session": actor, "heartbeat": self.one("sessions", "id", actor)["heartbeat"]}
            if action == "end":
                if self.rows("SELECT id FROM tasks WHERE owner=?", (actor,)):
                    raise Conflict("handoff or complete owned tasks before ending")
                if self.rows("SELECT name FROM resources WHERE owner=?", (actor,)):
                    raise Conflict("release resources before ending")
                self.db.execute("UPDATE sessions SET ended=1 WHERE id=?", (actor,))
        elif group == "task":
            if action == "create":
                self.db.execute("INSERT INTO tasks(id,title,scope) VALUES (?,?,?)",
                                (args.id, args.title, args.scope))
            elif action == "list":
                return [self.task(r["id"]) for r in self.rows("SELECT id FROM tasks ORDER BY id")]
            elif action == "show":
                return self.task(args.id)
            elif action == "claim":
                task = self.task(args.id)
                if task["state"] not in ("pending", "handoff"):
                    if not args.confirm_stopped or not args.note:
                        raise Conflict("task is owned or terminal; recovery requires --confirm-stopped and --note")
                    if task["state"] != "active" or not task["owner"]:
                        raise Conflict("only an active task can be recovered")
                    previous = self.one("sessions", "id", task["owner"])
                    if previous["heartbeat"] >= instant(-STALE_SECONDS) and not previous["ended"]:
                        raise Conflict("owner has a recent heartbeat; ask it to hand off")
                    self.free_resources(args.id)
                elif args.confirm_stopped or args.note:
                    raise Conflict("recovery options apply only to an owned active task")
                self.db.execute("UPDATE tasks SET owner=?,state='active',revision=revision+1 WHERE id=?",
                                (actor, args.id))
                self.event(actor, "task.previous", args.id, task)
            else:
                task = self.owned_task(actor, args.id)
                if task["revision"] != args.revision:
                    raise Conflict(f"stale task revision; current revision is {task['revision']}")
                raw_details = (args.details_file.read_text(encoding="utf-8")
                               if args.details_file else args.details)
                details = json.loads(raw_details, parse_constant=reject_constant)
                if not isinstance(details, dict):
                    raise Conflict("details must be a JSON object")
                if action in ("handoff", "complete"):
                    required = ("summary", "branch", "commit", "validation", "next", "post_live")
                    if any(not isinstance(details.get(key), str) or not details[key].strip() for key in required):
                        raise Conflict(f"handoff/complete needs nonblank string fields: {', '.join(required)}")
                    self.free_resources(args.id)
                state = {"update": "active", "handoff": "handoff", "complete": "done"}[action]
                self.db.execute(
                    "UPDATE tasks SET details=?,state=?,owner=?,revision=revision+1 WHERE id=?",
                    (json.dumps(details, ensure_ascii=False), state, actor if state == "active" else None, args.id),
                )
        elif group == "resource":
            if action == "list":
                return self.rows("SELECT * FROM resources ORDER BY name")
            if action == "acquire":
                self.owned_task(actor, args.task)
                held = self.rows("SELECT * FROM resources WHERE name=?", (args.name,))
                if held:
                    raise Conflict(f"resource already held: {held[0]}; heartbeat expiry never releases it")
                self.db.execute("INSERT INTO resources VALUES (?,?,?,?)",
                                (args.name, actor, args.task, instant()))
            else:
                held = self.one("resources", "name", args.name)
                if action == "release" and held["owner"] != actor:
                    raise Conflict("resource belongs to another session")
                if action == "recover" and not args.confirm_stopped:
                    raise Conflict("recovery requires explicit --confirm-stopped after checking the old operation")
                self.db.execute("DELETE FROM resources WHERE name=?", (args.name,))
                self.event(actor, "resource.previous", args.name, held)
        elif group == "message":
            if action == "send":
                recipient = self.one("sessions", "id", args.to)
                if recipient["ended"]:
                    raise Conflict("recipient ended; use task handoff instead")
                cursor = self.db.execute(
                    "INSERT INTO messages(sender,recipient,body,at) VALUES (?,?,?,?)",
                    (actor, args.to, args.body, instant()),
                )
                self.event(actor, "message.send", str(cursor.lastrowid), {"to": args.to})
                return {"id": cursor.lastrowid}
            if action == "inbox":
                return self.rows("SELECT * FROM messages WHERE recipient=? ORDER BY id", (actor,))
            message = self.one("messages", "id", args.id)
            if message["recipient"] != actor:
                raise Conflict("only the recipient can acknowledge a message")
            self.db.execute("UPDATE messages SET acknowledged=? WHERE id=?", (instant(), args.id))
        elif group == "state":
            if action == "events":
                events = self.rows("SELECT * FROM events WHERE id>? ORDER BY id LIMIT ?",
                                   (args.after, args.limit))
                return [{**row, "details": json.loads(row["details"])} for row in events]
            return {
                "schema": VERSION, "directory": str(self.directory),
                "sessions": self.sessions(),
                "tasks": [self.task(r["id"]) for r in self.rows("SELECT id FROM tasks ORDER BY id")],
                "resources": self.rows("SELECT * FROM resources ORDER BY name"),
                "messages": self.rows("SELECT * FROM messages ORDER BY id"),
                "legacy_claims": "Read the main checkout's .handoff.md separately; not imported or overridden.",
            }
        subject = getattr(args, "id", getattr(args, "name", actor))
        payload = {key: str(value) if isinstance(value, pathlib.Path) else value
                   for key, value in vars(args).items()
                   if key not in ("state_dir", "session", "group", "action")}
        if group == "task":
            payload["task"] = self.task(args.id)
        self.event(actor, f"{group}.{action}", str(subject), payload)
        return self.task(args.id) if group == "task" else {"action": f"{group}.{action}", "subject": subject}


def parser():
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument("--state-dir", type=pathlib.Path, help="isolated store for tests; omit for shared work")
    groups = cli.add_subparsers(dest="group", required=True)
    commands = {
        "session": ("start", "list", "heartbeat", "end"),
        "task": ("create", "list", "show", "claim", "update", "handoff", "complete"),
        "resource": ("acquire", "release", "recover", "list"),
        "message": ("send", "inbox", "ack"),
        "state": ("snapshot", "events"),
    }
    for group, actions in commands.items():
        sub = groups.add_parser(group).add_subparsers(dest="action", required=True)
        for action in actions:
            p = sub.add_parser(action)
            if (group == "session" and action in ("heartbeat", "end") or
                group == "task" and action not in ("list", "show") or
                group == "resource" and action != "list" or group == "message"):
                p.add_argument("--session", required=True, type=identifier)
            if group == "session" and action == "start":
                p.add_argument("--tool", required=True, type=text, help="informational label, not identity")
                p.add_argument("--worktree", default=str(pathlib.Path.cwd()), type=text)
            if group == "task" and action != "list":
                p.add_argument("id", type=identifier)
            if group == "task" and action == "create":
                p.add_argument("--title", required=True, type=text)
                p.add_argument("--scope", required=True, type=text, help="paths and live resources affected")
            if group == "task" and action == "claim":
                p.add_argument("--confirm-stopped", action="store_true")
                p.add_argument("--note", type=text)
            if group == "task" and action in ("update", "handoff", "complete"):
                p.add_argument("--revision", required=True, type=positive)
                details = p.add_mutually_exclusive_group(required=True)
                details.add_argument("--details", help="JSON object; replaces the previous details")
                details.add_argument("--details-file", type=pathlib.Path, help="UTF-8 JSON object file")
            if group == "resource" and action != "list":
                p.add_argument("name", type=identifier)
            if group == "resource" and action == "acquire":
                p.add_argument("--task", required=True, type=identifier)
            if group == "resource" and action == "recover":
                p.add_argument("--confirm-stopped", action="store_true")
                p.add_argument("--note", required=True, type=text)
            if group == "message" and action == "send":
                p.add_argument("--to", required=True, type=identifier)
                p.add_argument("--body", required=True, type=text)
            if group == "message" and action == "ack":
                p.add_argument("id", type=positive)
            if group == "state" and action == "events":
                p.add_argument("--after", type=int, default=0)
                p.add_argument("--limit", type=positive, default=100)
    return cli


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stderr.reconfigure(encoding="utf-8")
    args = parser().parse_args()
    store = None
    try:
        store = Store(args.state_dir if args.state_dir else default_directory())
        store.begin()
        result = store.run(args)
        store.db.commit()
        print(json.dumps({"v": VERSION, "ok": True, "result": result}, ensure_ascii=False))
        return 0
    except (Conflict, sqlite3.Error, OSError, ValueError, subprocess.CalledProcessError) as error:
        if store:
            store.db.rollback()
        print(json.dumps({"v": VERSION, "ok": False, "error": str(error)}, ensure_ascii=False), file=sys.stderr)
        return 1
    finally:
        if store:
            store.db.close()


if __name__ == "__main__":
    sys.exit(main())
