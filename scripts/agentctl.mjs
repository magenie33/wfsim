// Local, tool-neutral coordination. See docs/COORDINATION.md.
import { DatabaseSync } from "node:sqlite";
import { randomUUID } from "node:crypto";
import { mkdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { execFileSync } from "node:child_process";

const VERSION = 1;
const instant = (seconds = 0) => new Date(Date.now() + seconds * 1000).toISOString();
const commands = {
  session: { start: ["tool", "worktree?"], list: [], heartbeat: ["session"], end: ["session"] },
  task: {
    create: ["@id", "session", "title", "scope"], list: [], show: ["@id"],
    claim: ["@id", "session", "confirm-stopped?", "note?"],
    update: ["@id", "session", "revision", "details?", "details-file?"],
    handoff: ["@id", "session", "revision", "details?", "details-file?"],
    complete: ["@id", "session", "revision", "details?", "details-file?"],
  },
  resource: {
    acquire: ["@name", "session", "task"], release: ["@name", "session"],
    recover: ["@name", "session", "confirm-stopped?", "note"], list: [],
  },
  message: { send: ["session", "to", "body"], inbox: ["session"], ack: ["@id", "session"] },
  state: { snapshot: [], events: ["after?", "limit?"] },
};

function parse(argv) {
  if (argv.includes("--help") || !argv.length) {
    console.log("node scripts/agentctl.mjs [--state-dir DIR] <group> <action> [arguments]");
    for (const [group, actions] of Object.entries(commands))
      for (const [action, fields] of Object.entries(actions))
        console.log(`  ${group} ${action} ${fields.map(f => f.startsWith("@") ? f.slice(1) : `--${f}`).join(" ")}`);
    return null;
  }
  const options = {}, positional = [];
  for (let i = 0; i < argv.length; i++) {
    const token = argv[i];
    if (!token.startsWith("--")) { positional.push(token); continue; }
    const key = token.slice(2);
    if (Object.hasOwn(options, key)) throw new Error(`duplicate option --${key}`);
    if (key === "confirm-stopped") options[key] = true;
    else {
      const value = argv[++i];
      if (value === undefined || value.startsWith("--") || !value.trim())
        throw new Error(`--${key} needs a nonblank value`);
      options[key] = value;
    }
  }
  const [group, action, ...values] = positional;
  const fields = commands[group]?.[action];
  if (!fields) throw new Error("unknown command; use --help");
  const names = fields.map(f => f.replace(/^@/, "").replace(/\?$/, ""));
  for (const key of Object.keys(options))
    if (key !== "state-dir" && !names.includes(key)) throw new Error(`unknown option --${key}`);
  let index = 0;
  for (const field of fields) {
    const key = field.replace(/^@/, "").replace(/\?$/, "");
    if (field.startsWith("@")) {
      if (Object.hasOwn(options, key)) throw new Error(`${key} must be positional`);
      options[key] = values[index++];
    }
    if (!field.endsWith("?") && options[key] === undefined) throw new Error(`missing ${key}`);
  }
  if (index !== values.length) throw new Error("unexpected positional argument");
  for (const key of ["session", "to", "task", "name", ...(group === "task" ? ["id"] : [])])
    if (options[key] !== undefined && !/^[a-z0-9][a-z0-9._:-]{0,119}$/.test(options[key]))
      throw new Error(`${key}: use 1-120 lowercase letters, digits, . _ : -`);
  for (const key of ["revision", "limit", "after", ...(group === "message" ? ["id"] : [])]) {
    if (options[key] === undefined) continue;
    const n = Number(options[key]);
    if (!/^\d+$/.test(options[key]) || !Number.isSafeInteger(n) || n < (key === "after" ? 0 : 1))
      throw new Error(`${key} must be a ${key === "after" ? "nonnegative" : "positive"} safe integer`);
    options[key] = n;
  }
  if (group === "task" && ["update", "handoff", "complete"].includes(action)
      && Number(options.details !== undefined) + Number(options["details-file"] !== undefined) !== 1)
    throw new Error("provide exactly one of --details or --details-file");
  return { group, action, ...options };
}

function defaultDirectory() {
  const root = dirname(dirname(fileURLToPath(import.meta.url)));
  return join(execFileSync("git", ["-C", root, "rev-parse", "--path-format=absolute", "--git-common-dir"],
    { encoding: "utf8" }).trim(), "wfsim-agents");
}

class Store {
  constructor(directory) {
    this.directory = resolve(directory);
    mkdirSync(this.directory, { recursive: true });
    this.db = new DatabaseSync(join(this.directory, "state.sqlite3"));
    this.db.exec("PRAGMA busy_timeout=15000; PRAGMA foreign_keys=ON");
  }
  rows(sql, ...params) { return this.db.prepare(sql).all(...params); }
  write(sql, ...params) { return this.db.prepare(sql).run(...params); }
  one(table, key, value) {
    // Table and key are internal constants, never CLI input.
    const row = this.db.prepare(`SELECT * FROM ${table} WHERE ${key}=?`).get(value);
    if (!row) throw new Error(`unknown ${table}: ${value}`);
    return row;
  }
  begin() {
    this.db.exec("BEGIN IMMEDIATE");
    const version = this.db.prepare("PRAGMA user_version").get().user_version;
    if (version !== 0 && version !== VERSION) throw new Error(`unsupported state schema ${version}; expected ${VERSION}`);
    this.db.exec(`
      CREATE TABLE IF NOT EXISTS sessions (
        id TEXT PRIMARY KEY, tool TEXT NOT NULL, worktree TEXT NOT NULL,
        heartbeat TEXT NOT NULL, ended INTEGER NOT NULL DEFAULT 0);
      CREATE TABLE IF NOT EXISTS tasks (
        id TEXT PRIMARY KEY, title TEXT NOT NULL, scope TEXT NOT NULL,
        state TEXT NOT NULL DEFAULT 'pending', owner TEXT REFERENCES sessions(id),
        revision INTEGER NOT NULL DEFAULT 0, details TEXT NOT NULL DEFAULT '{}');
      CREATE TABLE IF NOT EXISTS resources (
        name TEXT PRIMARY KEY, owner TEXT NOT NULL REFERENCES sessions(id),
        task TEXT NOT NULL REFERENCES tasks(id), acquired TEXT NOT NULL);
      CREATE TABLE IF NOT EXISTS messages (
        id INTEGER PRIMARY KEY, sender TEXT NOT NULL REFERENCES sessions(id),
        recipient TEXT NOT NULL REFERENCES sessions(id), body TEXT NOT NULL, at TEXT NOT NULL, acknowledged TEXT);
      CREATE TABLE IF NOT EXISTS events (
        id INTEGER PRIMARY KEY, at TEXT NOT NULL, actor TEXT,
        action TEXT NOT NULL, subject TEXT NOT NULL, details TEXT NOT NULL);
      PRAGMA user_version=1;
    `);
  }
  event(actor, action, subject, details) {
    this.write("INSERT INTO events(at,actor,action,subject,details) VALUES (?,?,?,?,?)",
      instant(), actor, action, String(subject), JSON.stringify(details));
  }
  session(actor) {
    const row = this.one("sessions", "id", actor);
    if (row.ended) throw new Error("session ended; register a new session");
    this.write("UPDATE sessions SET heartbeat=? WHERE id=?", instant(), actor);
  }
  owned(actor, id) {
    const task = this.one("tasks", "id", id);
    if (task.owner !== actor || task.state !== "active") throw new Error("task is not active and owned by this session");
    return task;
  }
  free(id) {
    const held = this.rows("SELECT name FROM resources WHERE task=?", id);
    if (held.length) throw new Error(`release task resources first: ${held.map(r => r.name).join(", ")}`);
  }
  sessions() {
    const cutoff = instant(-900);
    return this.rows("SELECT * FROM sessions ORDER BY id").map(row => ({
      ...row, presence: row.ended ? "ended" : row.heartbeat >= cutoff ? "recent" : "unknown",
    }));
  }
  task(id) {
    const task = this.one("tasks", "id", id);
    return { ...task, details: JSON.parse(task.details) };
  }
  tasks() { return this.rows("SELECT id FROM tasks ORDER BY id").map(r => this.task(r.id)); }
  run(a) {
    const { group, action } = a;
    let actor = a.session ?? null;
    if (actor) this.session(actor);
    if (group === "session") {
      if (action === "start") {
        actor = randomUUID();
        const worktree = resolve(a.worktree ?? process.cwd());
        if (!statSync(worktree).isDirectory()) throw new Error("worktree directory does not exist");
        this.write("INSERT INTO sessions VALUES (?,?,?,?,0)", actor, a.tool, worktree, instant());
        this.event(actor, "session.start", actor, { tool: a.tool, worktree });
        return { session: actor };
      }
      if (action === "list") return this.sessions();
      if (action === "heartbeat") return { session: actor, heartbeat: this.one("sessions", "id", actor).heartbeat };
      if (this.rows("SELECT id FROM tasks WHERE owner=?", actor).length)
        throw new Error("handoff or complete owned tasks before ending");
      if (this.rows("SELECT name FROM resources WHERE owner=?", actor).length)
        throw new Error("release resources before ending");
      this.write("UPDATE sessions SET ended=1 WHERE id=?", actor);
    } else if (group === "task") {
      if (action === "list") return this.tasks();
      if (action === "show") return this.task(a.id);
      if (action === "create") {
        this.write("INSERT INTO tasks(id,title,scope) VALUES (?,?,?)", a.id, a.title, a.scope);
      } else if (action === "claim") {
        const task = this.task(a.id);
        if (!["pending", "handoff"].includes(task.state)) {
          if (!a["confirm-stopped"] || !a.note)
            throw new Error("task is owned or terminal; recovery requires --confirm-stopped and --note");
          if (task.state !== "active" || !task.owner) throw new Error("only an active task can be recovered");
          const previous = this.one("sessions", "id", task.owner);
          if (previous.heartbeat >= instant(-900) && !previous.ended)
            throw new Error("owner has a recent heartbeat; ask it to hand off");
          this.free(a.id);
        } else if (a["confirm-stopped"] || a.note) throw new Error("recovery options apply only to an owned active task");
        this.write("UPDATE tasks SET owner=?,state='active',revision=revision+1 WHERE id=?", actor, a.id);
        this.event(actor, "task.previous", a.id, task);
      } else {
        const task = this.owned(actor, a.id);
        if (task.revision !== a.revision) throw new Error(`stale task revision; current revision is ${task.revision}`);
        const details = JSON.parse(a["details-file"] ? readFileSync(a["details-file"], "utf8") : a.details);
        if (!details || typeof details !== "object" || Array.isArray(details)) throw new Error("details must be a JSON object");
        if (action !== "update") {
          const required = ["summary", "branch", "commit", "validation", "next", "post_live"];
          if (required.some(k => typeof details[k] !== "string" || !details[k].trim()))
            throw new Error(`handoff/complete needs nonblank string fields: ${required.join(", ")}`);
          this.free(a.id);
        }
        const state = { update: "active", handoff: "handoff", complete: "done" }[action];
        this.write("UPDATE tasks SET details=?,state=?,owner=?,revision=revision+1 WHERE id=?",
          JSON.stringify(details), state, state === "active" ? actor : null, a.id);
      }
    } else if (group === "resource") {
      if (action === "list") return this.rows("SELECT * FROM resources ORDER BY name");
      if (action === "acquire") {
        this.owned(actor, a.task);
        const held = this.rows("SELECT * FROM resources WHERE name=?", a.name);
        if (held.length) throw new Error(`resource already held: ${JSON.stringify(held[0])}; heartbeat expiry never releases it`);
        this.write("INSERT INTO resources VALUES (?,?,?,?)", a.name, actor, a.task, instant());
      } else {
        const held = this.one("resources", "name", a.name);
        if (action === "release" && held.owner !== actor) throw new Error("resource belongs to another session");
        if (action === "recover" && !a["confirm-stopped"])
          throw new Error("recovery requires explicit --confirm-stopped after checking the old operation");
        this.write("DELETE FROM resources WHERE name=?", a.name);
        this.event(actor, "resource.previous", a.name, held);
      }
    } else if (group === "message") {
      if (action === "send") {
        if (this.one("sessions", "id", a.to).ended) throw new Error("recipient ended; use task handoff instead");
        const { lastInsertRowid } = this.write("INSERT INTO messages(sender,recipient,body,at) VALUES (?,?,?,?)",
          actor, a.to, a.body, instant());
        const id = Number(lastInsertRowid);
        this.event(actor, "message.send", id, { to: a.to });
        return { id };
      }
      if (action === "inbox") return this.rows("SELECT * FROM messages WHERE recipient=? ORDER BY id", actor);
      if (this.one("messages", "id", a.id).recipient !== actor) throw new Error("only the recipient can acknowledge a message");
      this.write("UPDATE messages SET acknowledged=? WHERE id=?", instant(), a.id);
    } else if (group === "state") {
      if (action === "events") return this.rows("SELECT * FROM events WHERE id>? ORDER BY id LIMIT ?",
        a.after ?? 0, a.limit ?? 100).map(row => ({ ...row, details: JSON.parse(row.details) }));
      return {
        schema: VERSION, directory: this.directory, sessions: this.sessions(), tasks: this.tasks(),
        resources: this.rows("SELECT * FROM resources ORDER BY name"),
        messages: this.rows("SELECT * FROM messages ORDER BY id"),
        legacy_claims: "Read the main checkout's .handoff.md separately; not imported or overridden.",
      };
    }
    const subject = a.id ?? a.name ?? actor;
    const { session, "state-dir": directory, ...payload } = a;
    if (group === "task") payload.task = this.task(a.id);
    this.event(actor, `${group}.${action}`, subject, payload);
    return group === "task" ? this.task(a.id) : { action: `${group}.${action}`, subject };
  }
}

let store;
try {
  const args = parse(process.argv.slice(2));
  if (args) {
    store = new Store(args["state-dir"] ?? defaultDirectory());
    store.begin();
    const result = store.run(args);
    store.db.exec("COMMIT");
    console.log(JSON.stringify({ v: VERSION, ok: true, result }));
  }
} catch (error) {
  if (store?.db.isTransaction) store.db.exec("ROLLBACK");
  console.error(JSON.stringify({ v: VERSION, ok: false, error: error.message }));
  process.exitCode = 1;
} finally {
  store?.db.close();
}
