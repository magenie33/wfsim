// The task center and the original riven forms expose the same operations.
AGENT_ACTIONS.push(
  {
    id: "shell.tasks.read", query: true, what: "Read this browser's calculation queue and result links, including automatic work.", anchor: "#user-tasks",
    args: { limit: { kind: "number", min: 1, max: 100, what: "maximum rows, default 50" } },
    async run({ limit = 50 }) {
      await initUserTasks();
      return { tasks: userTaskItems().slice(-limit).map((task) => ({ id: task.id, kind: task.kind, state: task.state,
        ...USER_TASK_TYPES.get(task.kind)?.describe?.(task), destination: task.destination || "local", progress: task.progress ? { phase: task.progress.phase, done: task.progress.done, total: task.progress.total } : null,
        result: task.result_ref ? USER_TASK_TYPES.get(task.kind)?.resultLink?.(task) : null, error: task.error || null })) };
    },
  },
  {
    id: "shell.tasks.manage", writes: "none", what: "Cancel, retry, remove or reorder a calculation in the task center.", anchor: "#user-tasks",
    args: { task: { kind: "string", required: true, what: "id from shell.tasks.read" }, action: { kind: "string", required: true,
      what: "cancel, retry, remove, up, down, reset" } },
    async run({ task, action }) { if (["community", "local"].includes(action)) return agentNo("reader_gesture_required"); await initUserTasks(); await userTaskAction(action, task); return { text: "task updated" }; },
  },
  {
    id: "builder.rivens.import", writes: "none", what: "Import card text using the same official stat reader as screenshots and chat. The saved cards remain editable on the Rivens page.",
    anchor: "#riven-text, #riven-text-import", needs_weapon: true,
    args: { text: { kind: "string", required: true, what: "card stat names and values, cards separated by blank lines" } },
    async run({ text }) { return { imported: await importTypedRivens(text) }; },
  },
  {
    id: "builder.rivens.appraise", writes: "none", what: "Queue appraisals of saved rivens under an official ruler, using the standard search and replay.",
    anchor: "#riven-appraisals, #riven-all", needs_weapon: true,
    args: { cards: { kind: "array", required: true, what: "saved riven ids" }, ruler: { kind: "string", required: true, what: "official ruler id" } },
    async run({ cards, ruler }) { const tasks = await submitSavedAppraisals(cards, ruler); return { tasks: tasks.map((task) => task.id) }; },
  }
);
