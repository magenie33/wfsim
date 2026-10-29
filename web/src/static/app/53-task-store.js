// A request is frozen before acceptance; only its lifecycle may change.
const USER_TASK_TYPES = new Map();
const USER_TASK_EXECUTORS = new Map([["local", {
  label: "This device", supports: (type) => typeof type.execute === "function",
  run: (task, type, execution) => type.execute(task, execution),
}]]);
const USER_TASK_TERMINAL = new Set(["succeeded", "failed", "cancelled"]);
const taskClone = (value) => JSON.parse(JSON.stringify(value));
const taskInstant = () => new Date().toISOString();

class UserTaskMemoryStore {
  constructor() { this.persistent = false; this.state = { tasks: [] }; }
  async read() { return taskClone(this.state); }
  async change(transform) {
    const state = taskClone(this.state);
    const result = transform(state);
    this.state = state;
    return result == null ? result : taskClone(result);
  }
}

class UserTaskBrowserStore {
  constructor(db) { this.db = db; this.persistent = true; }
  static open() {
    return new Promise((resolve, reject) => {
      let request, settled = false;
      try { request = indexedDB.open("wfsim-user-tasks", 1); } catch (error) { reject(error); return; }
      request.onupgradeneeded = () => request.result.createObjectStore("queue");
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
  change(transform, mode = "readwrite") {
    return new Promise((resolve, reject) => {
      const transaction = this.db.transaction("queue", mode);
      const store = transaction.objectStore("queue");
      const request = store.get("state");
      let result, failure;
      request.onsuccess = () => {
        try {
          const state = request.result || { tasks: [] };
          const value = transform(state);
          result = value == null ? value : taskClone(value);
          if (mode === "readwrite") store.put(state, "state");
        } catch (error) { failure = error; transaction.abort(); }
      };
      transaction.oncomplete = () => resolve(result);
      transaction.onerror = () => reject(failure || transaction.error);
      transaction.onabort = () => reject(failure || transaction.error);
    });
  }
}

class UserTaskQueue {
  constructor({ store, types, engine, lock, executors = USER_TASK_EXECUTORS, changed = () => {}, publish = () => {} }) {
    Object.assign(this, { store, types, engine, lock, executors, changed, publish });
    this.rows = [];
    this.progress = new Map();
    this.controllers = new Map();
    this.waiters = new Map();
    this.draining = false;
  }
  async refresh() {
    this.rows = (await this.store.read()).tasks;
    for (const [id, progress] of this.progress) {
      if (!this.rows.some((task) => task.id === id && task.state === "running" && task.attempt === progress.attempt)) this.progress.delete(id);
    }
    for (const [id, controller] of this.controllers) {
      const task = this.rows.find((row) => row.id === id);
      if (!task || task.state !== "running") controller.abort();
    }
    for (const task of this.rows) if (USER_TASK_TERMINAL.has(task.state) || task.state === "interrupted") {
      const listeners = this.waiters.get(task.id) || [];
      this.waiters.delete(task.id);
      listeners.forEach((resolve) => resolve(taskClone(task)));
    }
    this.changed("state");
  }
  view() {
    return this.rows.map((row) => {
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
    const type = this.types.get(kind);
    if (!type || !this.executors.get(destination)?.supports(type)) throw new Error("This task type cannot run here.");
    const frozen = taskClone(request);
    const captured = taskClone(origin);
    if (type.validate) type.validate(frozen);
    const task = await this.change((state) => {
      const task = { id: crypto.randomUUID(), schema: 1, kind, destination, request: frozen,
        request_schema: type.schema, origin: captured, engine: this.engine,
        state: "queued", generation: 0, attempt: null, progress: null,
        result_ref: null, error: null, created_at: taskInstant(), updated_at: taskInstant(), completed_at: null };
      state.tasks.push(task);
      return task;
    });
    this.wake();
    return task;
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
      Object.assign(task, { state: "cancelled", generation: task.generation + 1,
        updated_at: taskInstant(), completed_at: taskInstant() });
    }));
    this.controllers.get(id)?.abort();
  }
  async retry(id) {
    await this.lock(`task:${id}`, () => this.change((state) => {
      const task = this.find(state, id);
      if (!["failed", "cancelled", "interrupted"].includes(task.state)) throw new Error("This task cannot be retried.");
      if (task.engine !== this.engine) throw new Error("The engine changed. Run this calculation again from its page.");
      const type = this.types.get(task.kind);
      if (!type || !this.executors.get(task.destination || "local")?.supports(type) || task.request_schema !== type.schema) throw new Error("This task type cannot run here.");
      Object.assign(task, { state: "queued", generation: task.generation + 1, attempt: null,
        progress: null, result_ref: null, error: null, completed_at: null, updated_at: taskInstant() });
      state.tasks = state.tasks.filter((row) => row.id !== id).concat(task);
    }));
    this.wake();
  }
  async remove(id) {
    await this.lock(`task:${id}`, () => this.change((state) => {
      const task = this.find(state, id);
      if (!USER_TASK_TERMINAL.has(task.state) && task.state !== "interrupted") throw new Error("Cancel this task before removing it.");
      state.tasks = state.tasks.filter((row) => row.id !== id);
    }));
    this.progress.delete(id);
  }
  async move(id, direction) {
    if (![-1, 1].includes(direction)) return;
    await this.change((state) => {
      const waiting = state.tasks.filter((row) => row.state === "queued");
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
  async recover() {
    for (const task of this.rows.filter((row) => row.state === "running")) {
      await this.lock(`task:${task.id}`, async () => {
        const type = this.types.get(task.kind);
        const reference = type?.recover ? await type.recover(task) : null;
        await this.change((state) => {
          const held = state.tasks.find((row) => row.id === task.id);
          if (!held || held.state !== "running" || held.attempt !== task.attempt) return;
          Object.assign(held, { state: reference ? "succeeded" : "interrupted", result_ref: reference,
            generation: held.generation + 1, updated_at: taskInstant(),
            completed_at: reference ? taskInstant() : null });
        });
      });
    }
  }
  wake() {
    if (this.draining) return;
    this.draining = true;
    this.lock("runner", async () => {
      await this.refresh();
      await this.recover();
      while (await this.runNext()) { /* one heavy task uses the existing worker pool */ }
    }).catch((error) => { this.error = String(error.message || error); this.changed(); })
      .finally(() => {
        this.draining = false;
        this.store.read().then((state) => {
          if (state.tasks.some((task) => task.state === "queued")) this.wake();
        }).catch((error) => { this.error = String(error.message || error); this.changed(); });
      });
  }
  async runNext() {
    if (!(await this.store.read()).tasks.some((row) => row.state === "queued")) return false;
    const task = await this.change((state) => {
      const task = state.tasks.find((row) => row.state === "queued");
      if (!task) return null;
      const type = this.types.get(task.kind);
      if (task.engine !== this.engine || !type || !this.executors.get(task.destination || "local")?.supports(type) || task.request_schema !== type.schema) {
        Object.assign(task, { state: "interrupted", error: "The engine changed. Run this calculation again from its page.", updated_at: taskInstant() });
        return { skipped: true };
      }
      Object.assign(task, { state: "running", attempt: crypto.randomUUID(), generation: task.generation + 1, updated_at: taskInstant() });
      return task;
    });
    if (!task) return false;
    if (task.skipped) return true;
    const type = this.types.get(task.kind), controller = new AbortController();
    this.controllers.set(task.id, controller);
    const current = (held) => held && held.state === "running" && held.attempt === task.attempt && held.generation === task.generation;
    const report = (value) => {
      const message = { kind: "progress", id: task.id, attempt: task.attempt, value };
      this.hearProgress(message);
      this.publish(message);
    };
    try {
      const value = await this.executors.get(task.destination || "local").run(taskClone(task), type, { signal: controller.signal, report });
      await this.lock(`task:${task.id}`, async () => {
        const held = (await this.store.read()).tasks.find((row) => row.id === task.id);
        if (!current(held)) return;
        const reference = await type.save(taskClone(task), value, { persistent: this.store.persistent });
        await this.change((state) => {
          const held = this.find(state, task.id);
          if (current(held)) Object.assign(held, { state: "succeeded", result_ref: reference,
            completed_at: taskInstant(), updated_at: taskInstant() });
        });
      });
    } catch (error) {
      await this.lock(`task:${task.id}`, () => this.change((state) => {
        const held = state.tasks.find((row) => row.id === task.id);
        if (current(held)) Object.assign(held, { state: "failed", error: String(error.message || error),
          completed_at: taskInstant(), updated_at: taskInstant() });
      }));
    } finally {
      this.controllers.delete(task.id);
      this.progress.delete(task.id);
    }
    return true;
  }
}
