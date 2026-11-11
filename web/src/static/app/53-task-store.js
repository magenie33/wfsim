// A request is frozen before acceptance; only its lifecycle may change.
const USER_TASK_TYPES = new Map();
const USER_TASK_EXECUTORS = new Map([["local", {
  label: "This device", reorder: true, supports: (type) => typeof type.execute === "function",
  run: (task, type, execution) => type.execute(task, execution),
}]]);
const USER_TASK_TERMINAL = new Set(["succeeded", "failed", "cancelled"]);
const taskClone = (value) => JSON.parse(JSON.stringify(value));
const taskInstant = () => new Date().toISOString();
const taskSources = task => task.sources?.length ? task.sources : [{ id: task.id, origin: task.origin, created_at: task.created_at }];
const taskIdentityInput = (kind, request, origin) => kind === "appraisal" ? { request, ruler: origin.ruler } : request;
const taskVerb = kind => kind === "simulation" ? "simulate_report" : kind === "optimization" ? "optimize_report" : kind;

class UserTaskMemoryStore {
  constructor() { this.persistent = false; this.state = { tasks: [] }; this.values = new Map(); }
  async read() { return taskClone(this.state); }
  async value(key, value) {
    if (arguments.length > 1) this.values.set(key, taskClone(value));
    return this.values.has(key) ? taskClone(this.values.get(key)) : null;
  }
  async entries(prefix) { return [...this.values].filter(([key]) => key.startsWith(prefix)).map(([, value]) => taskClone(value)); }
  async takeValues(prefix, limit) {
    return [...this.values].filter(([key]) => key.startsWith(prefix)).sort(([a], [b]) => a.localeCompare(b))
      .slice(0, limit).map(([key, value]) => ({ key, value: taskClone(value) }));
  }
  async countValues(prefix) { return [...this.values.keys()].filter(key => key.startsWith(prefix)).length; }
  async putOnce(entries) {
    for (const { key, marker, value } of entries) if (!this.values.has(marker)) {
      this.values.set(marker, true); this.values.set(key, taskClone(value));
    }
  }
  async writeValues(entries) {
    for (const { key, value, remove } of entries) {
      if (remove) this.values.delete(key); else this.values.set(key, taskClone(value));
    }
  }
  async change(transform) {
    const state = taskClone(this.state);
    const result = transform(state);
    this.state = state;
    return result == null ? result : taskClone(result);
  }
}

const USER_TASK_STORES = { tasks:"entity_calculation_tasks", events:"record_task_completions", commands:"state_task_commands",
  order:"state_task_queue", artifacts:"entity_calculation_reports", values:"state_task_client_data",
  uploads:"state_build_uploads", deliveries:"record_build_deliveries", streams:"state_build_streams", authorizations:"state_build_authorizations" };
function userTaskValueStore(key) {
  if (key.startsWith("artifact:")) return USER_TASK_STORES.artifacts;
  if (key.startsWith("optimization-build-upload:")) return USER_TASK_STORES.uploads;
  if (/^optimization-build-(seen|refused):/.test(key)) return USER_TASK_STORES.deliveries;
  if (key.startsWith("optimization-build-source:")) return USER_TASK_STORES.streams;
  if (key.startsWith("optimization-build-receipt:")) return USER_TASK_STORES.authorizations;
  return USER_TASK_STORES.values;
}
function writeUserTaskState(tx,state) {
  for (const name of ["tasks","events","commands"]) {
    const store=tx.objectStore(USER_TASK_STORES[name]);store.clear();
    for (const row of state[name] || []) store.add(row,row.id);
  }
  const {tasks,events,commands,...other}=state;
  tx.objectStore(USER_TASK_STORES.order).put({...other,order:tasks.map(task=>task.id),
    event_order:(events || []).map(event=>event.id),command_order:(commands || []).map(command=>command.id)},"state");
}

class UserTaskBrowserStore {
  constructor(db) { this.db = db; this.persistent = true; }
  static open() {
    return new Promise((resolve, reject) => {
      let request, settled = false;
      try { request = indexedDB.open("wfsim-user-tasks", 2); } catch (error) { reject(error); return; }
      request.onupgradeneeded = () => {
        const db=request.result,tx=request.transaction;
        for(const name of Object.values(USER_TASK_STORES)) if(!db.objectStoreNames.contains(name))db.createObjectStore(name);
        if(!db.objectStoreNames.contains("queue"))return;
        const cursor=tx.objectStore("queue").openCursor();
        cursor.onsuccess=()=>{
          const row=cursor.result;
          if(!row){db.deleteObjectStore("queue");return;}
          if(row.key==="state")writeUserTaskState(tx,row.value);
          else tx.objectStore(userTaskValueStore(row.key)).add(row.value,row.key);
          row.continue();
        };
      };
      request.onsuccess = () => {
        if (settled) { request.result.close(); return; }
        settled = true;
        request.result.onversionchange = () => request.result.close();
        resolve(new UserTaskBrowserStore(request.result));
      };
      request.onerror = () => { settled = true; reject(request.error); };
      request.onblocked = () => { settled = true; reject(new Error("Task storage is unavailable.")); };
    });
  }
  read() { return this.change((state) => state, "readonly"); }
  value(key, value) {
    const write = arguments.length > 1;
    return new Promise((resolve, reject) => {
      const name=userTaskValueStore(key),tx = this.db.transaction(name, write ? "readwrite" : "readonly");
      const request = write ? tx.objectStore(name).put(value, key) : tx.objectStore(name).get(key);
      tx.oncomplete = () => resolve(write ? value : request.result || null);
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error);
    });
  }
  entries(prefix) {
    return new Promise((resolve, reject) => {
      const name=userTaskValueStore(prefix),tx = this.db.transaction(name, "readonly");
      const request = tx.objectStore(name).getAll(IDBKeyRange.bound(prefix, prefix + "\uffff"));
      tx.oncomplete = () => resolve(request.result);
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error);
    });
  }
  takeValues(prefix, limit) {
    return new Promise((resolve, reject) => {
      const name=userTaskValueStore(prefix),tx = this.db.transaction(name, "readonly"), rows = [];
      const request = tx.objectStore(name).openCursor(IDBKeyRange.bound(prefix, prefix + "\uffff"));
      request.onsuccess = () => {
        const cursor = request.result;
        if (cursor && rows.length < limit) { rows.push({ key: cursor.key, value: cursor.value }); cursor.continue(); }
      };
      tx.oncomplete = () => resolve(rows);
      tx.onerror = () => reject(tx.error); tx.onabort = () => reject(tx.error);
    });
  }
  countValues(prefix) {
    return new Promise((resolve, reject) => {
      const name=userTaskValueStore(prefix),tx = this.db.transaction(name, "readonly");
      const request = tx.objectStore(name).count(IDBKeyRange.bound(prefix, prefix + "\uffff"));
      tx.oncomplete = () => resolve(request.result);
      tx.onerror = () => reject(tx.error); tx.onabort = () => reject(tx.error);
    });
  }
  putOnce(entries) {
    return new Promise((resolve, reject) => {
      const names=[...new Set(entries.flatMap(({key,marker})=>[userTaskValueStore(key),userTaskValueStore(marker)]))];
      if(!names.length){resolve();return;}
      const tx = this.db.transaction(names, "readwrite");
      for (const { key, marker, value } of entries) {
        const store=tx.objectStore(userTaskValueStore(key)),markers=tx.objectStore(userTaskValueStore(marker));
        const request = markers.get(marker);
        request.onsuccess = () => { if (request.result === undefined) { markers.put(true, marker); store.put(value, key); } };
      }
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error); tx.onabort = () => reject(tx.error);
    });
  }
  writeValues(entries) {
    return new Promise((resolve, reject) => {
      const names=[...new Set(entries.map(({key})=>userTaskValueStore(key)))];
      if(!names.length){resolve();return;}
      const tx = this.db.transaction(names, "readwrite");
      try {
        for (const { key, value, remove } of entries) {
          const store=tx.objectStore(userTaskValueStore(key));
          if (remove) store.delete(key); else store.put(value, key);
        }
      } catch(error) {tx.abort();reject(error);return;}
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error); tx.onabort = () => reject(tx.error);
    });
  }
  change(transform, mode = "readwrite") {
    return new Promise((resolve, reject) => {
      const names=["tasks","events","commands","order"].map(name=>USER_TASK_STORES[name]);
      const transaction = this.db.transaction(names, mode);
      const requests=[...names.slice(0,3).map(name=>transaction.objectStore(name).getAll()),
        transaction.objectStore(USER_TASK_STORES.order).get("state")];
      let result, failure;
      let remaining=requests.length;
      const ready=()=>{
        if(--remaining)return;
        try {
          const [tasks,events,commands,metadata]=requests.map(request=>request.result);
          const {order=[],event_order=[],command_order=[],...other}=metadata || {};
          const ordered=(rows,ids)=>{
            const positions=new Map(ids.map((id,index)=>[id,index]));
            return rows.sort((a,b)=>(positions.get(a.id)??Infinity)-(positions.get(b.id)??Infinity));
          };
          const state={...other,tasks:ordered(tasks,order),events:ordered(events,event_order),commands:ordered(commands,command_order)};
          const value = transform(state);
          result = value == null ? value : taskClone(value);
          if (mode === "readwrite") writeUserTaskState(transaction,state);
        } catch (error) { failure = error; transaction.abort(); }
      };
      requests.forEach(request=>{request.onsuccess=ready;});
      transaction.oncomplete = () => resolve(result);
      transaction.onerror = () => reject(failure || transaction.error);
      transaction.onabort = () => reject(failure || transaction.error);
    });
  }
}

// Build delivery has its own durable outbox; search reports retain only their ranking.
class OptimizationBuildOutbox {
  constructor({ store, lock, engine, consent, send, failed = () => {}, schedule = (work, ms) => setTimeout(work, ms),
    coalesce = () => new Promise(resolve => setTimeout(resolve, 250)) }) {
    Object.assign(this, { store, lock, engine, consent, send, failed, schedule, coalesce });
    this.prefix = "optimization-build-upload:";
    this.running = null; this.timer = null; this.again = false; this.backoff = 10_000;
  }
  static canonical(value) {
    return Array.isArray(value) ? value.map(OptimizationBuildOutbox.canonical) : value && typeof value === "object"
      ? Object.fromEntries(Object.keys(value).sort().map(key => [key, OptimizationBuildOutbox.canonical(value[key])])) : value;
  }
  async add(builds, source = {}) {
    if (!builds?.length) return;
    source = taskClone(source);
    const sourceKey = JSON.stringify(OptimizationBuildOutbox.canonical(source));
    const unique = new Map(builds.map(build => {
      const frozen = taskClone(build);
      return [JSON.stringify(OptimizationBuildOutbox.canonical(frozen)), frozen];
    }));
    const entries = await Promise.all([...unique].map(async ([text, build]) => {
      const hash = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(JSON.stringify([this.engine, sourceKey, text])));
      const id = [...new Uint8Array(hash)].map(byte => byte.toString(16).padStart(2, "0")).join("");
      const marker = "optimization-build-seen:" + id;
      return { key: this.prefix + (source.explicit ? "explicit:" : "consent:") + id, marker,
        value: { marker, build: taskClone(build), source: taskClone(source) } };
    }));
    await this.store.putOnce(entries);
    this.wake();
  }
  wake() {
    if (this.running) { this.again = true; return; }
    if (this.timer != null) return;
    this.again = false;
    this.running = this.lock("optimization-build-uploads", () => this.drain()).catch(error => {
      this.failed(error); this.again = false;
      this.timer = this.schedule(() => { this.timer = null; this.wake(); }, this.backoff);
      this.backoff = Math.min(300_000, this.backoff * 2);
    }).finally(() => {
      this.running = null;
      if (this.again && this.timer == null) this.wake();
    });
  }
  async drain() {
    for (;;) {
      let prefix = this.prefix + (this.consent() ? "" : "explicit:");
      let rows = await this.store.takeValues(prefix, 128);
      if (!rows.length) return;
      if (rows.length < 128) {
        await this.coalesce();
        prefix = this.prefix + (this.consent() ? "" : "explicit:");
        rows = await this.store.takeValues(prefix, 128);
        if (!rows.length) return;
      }
      const source = rows[0].value.source, scope = JSON.stringify(source), batch = [];
      let bytes = new TextEncoder().encode(JSON.stringify({ ...this.envelope(source), builds: [] })).length;
      const rejected = [];
      for (const row of rows) {
        if (JSON.stringify(row.value.source) !== scope) continue;
        const size = new TextEncoder().encode(JSON.stringify(row.value.build)).length;
        if (size > 4096) { rejected.push(row); continue; }
        if (batch.length && bytes + size + 1 > 131072) break;
        batch.push(row); bytes += size + (batch.length > 1 ? 1 : 0);
      }
      if (rejected.length) await this.settle(rejected.map(row => ({ row, verdict: { ok: false, error: "The build is too large.", retryable: false } })));
      if (!batch.length) continue;
      if (!source.explicit && !this.consent()) return;
      const results = await this.send(source, batch.map(row => row.value.build));
      if (!Array.isArray(results) || results.length !== batch.length) throw new Error("The build upload returned an incomplete acknowledgment.");
      await this.settle(batch.map((row, i) => ({ row, verdict: results[i] })));
      if (results.some(result => !result?.ok && result?.retryable !== false)) throw new Error("Some builds could not be uploaded.");
      this.backoff = 10_000;
    }
  }
  envelope(source) {
    const { kind, explicit, ...credentials } = source;
    return kind && kind !== "public" ? credentials : {};
  }
  async settle(entries) {
    const changes = []; let refusal = null;
    for (const { row, verdict } of entries) if (verdict?.ok || verdict?.retryable === false) {
      if (!verdict.ok) refusal ||= verdict.error || "The build upload was refused.";
      changes.push({ key: row.key, remove: true }, { key: row.value.marker,
        value: verdict.ok ? { state: "sent" } : { state: "refused", error: verdict.error || "refused" } });
      if (!verdict.ok) changes.push({ key: row.key.replace(this.prefix, "optimization-build-refused:"),
        value: { ...row.value, state: "refused", error: verdict.error || "refused" } });
    }
    if (changes.length) await this.store.writeValues(changes);
    if (refusal) this.failed(new Error(refusal));
  }
}

class UserTaskQueue {
  constructor({ store, types, engine, lock, normalize = async (_kind, request) => ({ request }), executors = USER_TASK_EXECUTORS, changed = () => {}, publish = () => {}, retryDelivery = () => {} }) {
    Object.assign(this, { store, types, engine, lock, normalize, executors, changed, publish, retryDelivery });
    this.rows = [];
    this.progress = new Map();
    this.controllers = new Map();
    this.waiters = new Map();
    this.draining = new Set();
  }
  async refresh() {
    const state = await this.store.read();
    this.rows = state.tasks;
    this.events = state.events || [];
    for (const [id, progress] of this.progress) {
      if (!this.rows.some((task) => task.id === id && task.state === "running" && task.attempt === progress.attempt)) this.progress.delete(id);
    }
    for (const [id, controller] of this.controllers) {
      const task = this.rows.find((row) => row.id === id);
      if (!task || task.state !== "running" || controller.attempt !== task.attempt) controller.abort();
    }
    for (const task of this.rows) if (USER_TASK_TERMINAL.has(task.state) || task.state === "interrupted") {
      const listeners = this.waiters.get(task.id) || [];
      this.waiters.delete(task.id);
      listeners.forEach((resolve) => resolve(taskClone(task)));
    }
    this.changed("state");
  }
  view() {
    return this.rows.filter((row) => !row.removed_at).map((row) => {
      row = { ...row, delivery_pending: this.events?.some((event) => event.task === row.id && !event.delivered_at) || false };
      const progress = this.progress.get(row.id);
      return progress && progress.attempt === row.attempt && row.state === "running"
        ? { ...row, progress: progress.value } : row;
    });
  }
  list() { return taskClone(this.view()); }
  async change(transform) {
    const result = await this.store.change(transform);
    await this.refresh();
    this.publish({ kind: "changed" });
    return result;
  }
  async create(kind, request, origin, destination = "local") {
    return (await this.createMany([{ kind, request, origin, destination }]))[0];
  }
  async createMany(entries) {
    entries = entries.map(entry => taskClone(entry));
    await this.prepare();
    const tasks = await Promise.all(entries.map(async ({ kind, request, origin, destination = "local" }) => {
      const type = this.types.get(kind);
      if (!type || !this.executors.get(destination)?.supports(type)) throw new Error("This task type cannot run here.");
      if (!origin || typeof origin !== "object" || Array.isArray(origin) || !Object.keys(origin).length)
        throw new Error("A calculation task requires a source.");
      const submitted = taskClone(request), context = taskClone(origin);
      const normalized = await this.normalize(kind, submitted);
      const frozen = taskClone(normalized.request), captured = { ...context, input_key: computationCanon(computationInput(taskVerb(kind), submitted)), input_aliases: normalized.aliases || {} };
      if (captured.board) captured.board = computationReferences(captured.board, normalized.aliases);
      if (type.validate) type.validate(frozen);
      const computation = await computationId(taskVerb(kind), taskIdentityInput(kind,frozen,captured));
      const source = { id: crypto.randomUUID(), origin: captured, created_at: taskInstant() };
      return { id: crypto.randomUUID(), computation, sources: [source], schema: 1, kind, destination, request: frozen,
        request_schema: type.schema, origin: captured, engine: this.engine,
        state: "queued", generation: 0, attempt: null, progress: null,
        result_ref: null, error: null, created_at: taskInstant(), updated_at: taskInstant(), completed_at: null };
    }));
    const accepted = await this.change((state) => tasks.map(task => {
      const held = state.tasks.find(row => row.computation === task.computation && (["queued", "running"].includes(row.state) || state.events?.some(event => event.task === row.id && !event.delivered_at)));
      if (held) {
        (held.sources ||= taskSources(held)).push(...task.sources);
        held.updated_at = taskInstant();
        held.removed_at = null;
        return held;
      }
      state.tasks.push(task); return task;
    }));
    this.wake();
    return accepted;
  }

  async prepare() {
    // Existing queued entries retain their inputs, attempts and delivery state.
  }

  wait(id) {
    const task = this.rows.find((row) => row.id === id);
    if (!task || USER_TASK_TERMINAL.has(task.state) || task.state === "interrupted") return Promise.resolve(task && taskClone(task));
    return new Promise((resolve) => this.waiters.set(id, [...(this.waiters.get(id) || []), resolve]));
  }
  find(state, id) {
    const task = state.tasks.find((row) => row.id === id);
    if (!task) throw new Error("This task is no longer available.");
    return task;
  }
  async cancel(id) {
    await this.lock(`task:${id}`, () => this.change((state) => {
      const task = this.find(state, id);
      if (USER_TASK_TERMINAL.has(task.state)) return;
      if (task.destination !== "local") (state.commands ||= []).push({ id: crypto.randomUUID(), action: "cancel", destination: task.destination,
        remote: task.remote_command || task.attempt, delivered_at: null });
      Object.assign(task, { state: "cancelled", generation: task.generation + 1,
        updated_at: taskInstant(), completed_at: taskInstant() });
    }));
    this.controllers.get(id)?.abort();
    this.deliverCommands();
  }
  async retry(id) {
    await this.lock(`task:${id}`, () => this.change((state) => {
      const task = this.find(state, id);
      if (!["failed", "cancelled", "interrupted"].includes(task.state)) throw new Error("This task cannot be retried.");
      const type = this.types.get(task.kind);
      if (!type || !this.executors.get(task.destination || "local")?.supports(type) || task.request_schema !== type.schema) throw new Error("This task type cannot run here.");
      const held = state.tasks.find(row => row.id !== id && row.computation === task.computation && (["queued", "running"].includes(row.state) || state.events?.some(event => event.task === row.id && !event.delivered_at)));
      if (held) {
        (held.sources ||= taskSources(held)).push({ id: crypto.randomUUID(), origin: taskClone(task.origin), created_at: taskInstant() });
        return held;
      }
      Object.assign(task, { state: "queued", generation: task.generation + 1, attempt: null,
        progress: null, result_ref: null, error: null, remote_command: null, handoff: null, completed_at: null, updated_at: taskInstant() });
      state.tasks = state.tasks.filter((row) => row.id !== id).concat(task);
    }));
    this.wake();
  }
  async remove(id) {
    this.deliverEvents();
    await this.lock(`task:${id}`, () => this.change((state) => {
      const task = this.find(state, id);
      if (!USER_TASK_TERMINAL.has(task.state) && task.state !== "interrupted") throw new Error("Cancel this task before removing it.");
      if (state.events?.some((event) => event.task === id && !event.delivered_at)) task.removed_at = taskInstant();
      else state.tasks = state.tasks.filter((row) => row.id !== id);
    }));
    this.progress.delete(id);
  }
  async move(id, direction) {
    if (![-1, 1].includes(direction)) return;
    await this.change((state) => {
      const task = this.find(state, id);
      if (!this.executors.get(task.destination || "local")?.reorder) throw new Error("This executor controls its own waiting order.");
      const waiting = state.tasks.filter((row) => row.state === "queued" && (row.destination || "local") === (task.destination || "local"));
      const index = waiting.findIndex((row) => row.id === id);
      const neighbor = index >= 0 && waiting[index + direction];
      if (!neighbor) return;
      const left = state.tasks.findIndex((row) => row.id === id);
      const right = state.tasks.findIndex((row) => row.id === neighbor.id);
      [state.tasks[left], state.tasks[right]] = [state.tasks[right], state.tasks[left]];
    });
  }
  hearProgress(message) {
    const task = this.rows.find((row) => row.id === message.id && row.state === "running" && row.attempt === message.attempt);
    if (!task) return;
    this.progress.set(task.id, { attempt: message.attempt, value: taskClone(message.value) });
    this.changed("progress");
  }
  async recover(destination = "local") {
    for (const task of this.rows.filter((row) => row.handoff && row.destination === destination && !USER_TASK_TERMINAL.has(row.state))) {
      try { await this.transfer(task.id, task.handoff.destination); } catch (_) { /* the retained local attempt remains recoverable */ }
    }
    for (const task of this.rows.filter((row) => row.state === "running" && (row.destination || "local") === destination)) {
      await this.lock(`task:${task.id}`, async () => {
        const type = this.types.get(task.kind);
        const reference = type?.recover ? await type.recover(task) : null;
        await this.change((state) => {
          const held = state.tasks.find((row) => row.id === task.id);
          if (!held || held.state !== "running" || held.attempt !== task.attempt) return;
          Object.assign(held, { state: reference ? "succeeded" : destination !== "local" ? "queued" : "interrupted", result_ref: reference,
            generation: held.generation + 1, remote_command: held.remote_command || (destination !== "local" ? held.attempt : null), updated_at: taskInstant(),
            completed_at: reference ? taskInstant() : null });
          if (reference && !(state.events || []).some((event) => event.id === task.attempt))
            (state.events ||= []).push({ id: task.attempt, task: task.id, delivered_at: null });
        });
      });
    }
  }
  wake() {
    for (const destination of this.executors.keys()) this.wakeDestination(destination);
  }
  wakeDestination(destination) {
    if (this.draining.has(destination)) return;
    this.draining.add(destination);
    this.lock(`runner:${destination}`, async () => {
      await this.prepare();
      await this.refresh();
      await this.recover(destination);
      this.deliverEvents(); this.deliverCommands();
      while (await this.runNext(destination)) { /* each executor owns its admission policy */ }
    }).catch((error) => { this.error = String(error.message || error); this.changed(); })
      .finally(() => {
        this.draining.delete(destination);
        this.store.read().then((state) => {
          if (state.tasks.some((task) => task.state === "queued" && (task.destination || "local") === destination)) this.wakeDestination(destination);
        }).catch((error) => { this.error = String(error.message || error); this.changed(); });
      });
  }
  async runNext(destination = null) {
    const eligible = (row) => row.state === "queued" && (!destination || (row.destination || "local") === destination);
    if (!(await this.store.read()).tasks.some(eligible)) return false;
    const task = await this.change((state) => {
      const task = state.tasks.find(eligible);
      if (!task) return null;
      const type = this.types.get(task.kind);
      if (!type || !this.executors.get(task.destination || "local")?.supports(type) || task.request_schema !== type.schema) {
        Object.assign(task, { state: "interrupted", error: "The request schema changed. Run this calculation again from its page.", updated_at: taskInstant() });
        return { skipped: true };
      }
      Object.assign(task, { submitted_engine: task.submitted_engine || task.engine, engine: this.engine, state: "running", attempt: crypto.randomUUID(), generation: task.generation + 1, updated_at: taskInstant() });
      return task;
    });
    if (!task) return false;
    if (task.skipped) return true;
    const type = this.types.get(task.kind), controller = new AbortController();
    controller.attempt = task.attempt;
    this.controllers.set(task.id, controller);
    const current = (held) => held && held.state === "running" && held.attempt === task.attempt && held.generation === task.generation;
    const report = (value) => {
      const message = { kind: "progress", id: task.id, attempt: task.attempt, value };
      this.hearProgress(message);
      this.publish(message);
    };
    try {
      const checkpoint = async (value) => {
        if (value === undefined) return this.store.value(`checkpoint:${task.id}`);
        await this.lock(`task:${task.id}`, async () => {
          if (current((await this.store.read()).tasks.find((row) => row.id === task.id)))
            await this.store.value(`checkpoint:${task.id}`, { engine: task.engine, schema: task.request_schema, value });
        });
      };
      const run = () => this.executors.get(task.destination || "local").run(taskClone(task), type, { signal: controller.signal, report, checkpoint });
      const value = (task.destination || "local") === "local" ? await this.lock("resource:local", run) : await run();
      await this.lock(`task:${task.id}`, async () => {
        const held = (await this.store.read()).tasks.find((row) => row.id === task.id);
        if (!current(held)) return;
        const reference = await type.save(taskClone({ ...task, sources: taskSources(held) }), value, { persistent: this.store.persistent });
        await this.store.value(`checkpoint:${task.id}`, null);
        await this.change((state) => {
          const held = this.find(state, task.id);
          if (current(held)) {
            Object.assign(held, { state: "succeeded", result_ref: reference, completed_at: taskInstant(), updated_at: taskInstant(), read_at: null });
            (state.events ||= []).push({ id: task.attempt, task: task.id, delivered_at: null });
          }
        });
      });
    } catch (error) {
      await this.lock(`task:${task.id}`, () => this.change((state) => {
        const held = state.tasks.find((row) => row.id === task.id);
        if (current(held)) Object.assign(held, { state: "failed", error: String(error.message || error),
          completed_at: taskInstant(), updated_at: taskInstant() });
      }));
    } finally {
      if (this.controllers.get(task.id) === controller) this.controllers.delete(task.id);
      if (this.progress.get(task.id)?.attempt === task.attempt) this.progress.delete(task.id);
    }
    this.deliverEvents();
    return true;
  }
  async transfer(id, destination) {
    const executor = this.executors.get(destination);
    const task = await this.lock(`task:${id}`, () => this.change((state) => {
      const task = this.find(state, id);
      if (!executor?.accept || !executor.supports(this.types.get(task.kind))) throw new Error("This task cannot be transferred.");
      if (USER_TASK_TERMINAL.has(task.state)) throw new Error("This task has already finished.");
      if (task.destination === destination) return null;
      task.handoff ||= { command: crypto.randomUUID(), destination };
      return task;
    }));
    if (!task) return;
    const accepted = await executor.accept(task, task.handoff.command);
    let transferred = false;
    await this.lock(`task:${id}`, () => this.change((state) => {
      const held = state.tasks.find((row) => row.id === id);
      if (held && !USER_TASK_TERMINAL.has(held.state) && held.handoff?.command === task.handoff.command) {
        if (held.destination !== "local") (state.commands ||= []).push({ id: crypto.randomUUID(), action: "cancel", destination: held.destination,
          remote: held.remote_command || held.attempt, delivered_at: null });
        Object.assign(held, { state: "queued", destination, remote_command: accepted.id, attempt: null, generation: held.generation + 1,
          handoff: null, error: null, updated_at: taskInstant() });
        transferred = true;
      } else (state.commands ||= []).push({ id: crypto.randomUUID(), action: "cancel", destination, remote: accepted.id, delivered_at: null });
    }));
    if (transferred) this.controllers.get(id)?.abort();
    this.deliverCommands(); this.wake();
  }
  deliverCommands() {
    if (this.commandDelivery) return this.commandDelivery;
    this.commandError = null;
    this.commandDelivery = this.lock("commands", async () => {
      for (const command of ((await this.store.read()).commands || []).filter((row) => !row.delivered_at)) {
        await this.executors.get(command.destination)?.cancel?.(command.remote);
        await this.change((state) => { const row = state.commands.find((row) => row.id === command.id); if (row) row.delivered_at = taskInstant(); });
      }
    }).catch((error) => { this.commandError = String(error.message || error); this.changed(); this.retryDelivery(); })
      .finally(() => { this.commandDelivery = null; this.changed(); });
    return this.commandDelivery;
  }
  async markRead() {
    await this.change((state) => {
      for (const task of state.tasks) if (USER_TASK_TERMINAL.has(task.state)) task.read_at ||= taskInstant();
    });
  }
  deliverEvents() {
    if (this.eventDelivery) return this.eventDelivery;
    this.eventError = null;
    this.eventDelivery = this.lock("events", async () => {
      for (const event of ((await this.store.read()).events || []).filter((row) => !row.delivered_at)) {
        const task = (await this.store.read()).tasks.find((row) => row.id === event.task);
        const type = task && this.types.get(task.kind);
        try {
          if (task?.attempt === event.id && type?.completed) for (const source of taskSources(task)) {
            if (event.delivered_sources?.includes(source.id)) continue;
            await type.completed(taskClone({ ...task, source_id: source.id, origin: source.origin }));
            await this.change(state => {
              const row = state.events?.find(row => row.id === event.id);
              if (row) (row.delivered_sources ||= []).push(source.id);
            });
          }
        }
        catch (error) { this.eventError = String(error.message || error); this.changed(); if (error.retryable !== false) this.retryDelivery(); continue; }
        const pending = await this.change((state) => {
          const row = state.events?.find((row) => row.id === event.id);
          const current = state.tasks.find(task => task.id === event.task);
          const pending = current?.attempt === event.id && type?.completed && taskSources(current).some(source => !row?.delivered_sources?.includes(source.id));
          if (row && !pending) row.delivered_at = taskInstant();
          state.tasks = state.tasks.filter((task) => !task.removed_at || state.events.some((event) => event.task === task.id && !event.delivered_at));
          return pending;
        });
        if (pending) this.retryDelivery();
      }
    }).catch((error) => { this.eventError = String(error.message || error); this.changed(); })
      .finally(() => { this.eventDelivery = null; this.changed(); });
    return this.eventDelivery;
  }
}

// Automatic work replaces one slot; a superseded generation cannot report.
class UserTransientTask {
  constructor(kind) { this.kind = kind; this.generation = 0; this.current = null; }
  replace(request, origin) {
    this.generation += 1;
    this.current = { id: this.kind, kind: this.kind, generation: this.generation, transient: true,
      request: taskClone(request), origin: taskClone(origin), state: "running", progress: null };
    return this.generation;
  }
  report(generation, state, progress) {
    if (this.generation !== generation || this.current?.generation !== generation) return false;
    Object.assign(this.current, { state, progress: taskClone(progress) });
    return true;
  }
  cancel() { this.generation += 1; if (this.current) this.current.state = "cancelled"; }
}
