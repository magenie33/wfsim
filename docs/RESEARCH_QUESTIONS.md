# Research question workflow

The shared Drive file `requests/OPEN_QUESTIONS.md` is the single active research
queue for weapons, Mods, abilities and other game mechanics. It holds questions
that available client evidence cannot answer. [UNMODELLED.md](UNMODELLED.md)
owns simulator coverage; a known mechanic awaiting code is integration work.

## Start every session with the queue

Read the queue at session start and after a context reset. From the main WFSim
checkout, use the portable reader in the existing shared Drive link:

```text
python3 private/warframe-data-drive/tools/read_research_queue.py --exchange private/warframe-data-drive
```

From the Windows research checkout, use:

```text
python .exchange/tools/read_research_queue.py --exchange .exchange
```

The reader verifies `requests/OPEN_QUESTIONS.READY.json`, byte length and
SHA-256. A missing or mismatched pair is incomplete sync, not an empty queue.
Worktrees use the main checkout's existing link; never add a checkout link.
The research repository's `docs/EXCHANGE.md` owns the publication and receipt
protocol. Git owns the procedure and reader; Drive owns the active questions.
Older request snapshots and this document are not separate active lists.
If the shared folder is unavailable, record that state and continue unrelated
work; do not replace an unreadable queue with an empty one.

## Add and resolve questions

The integration side publishes the queue, with one claimed writer at a time.
Before concluding that client evidence cannot answer a modelling question,
check the existing questions and evidence. Add or extend an entry with a stable
id, exact internal paths, priority, established facts, the precise unknown
and the evidence needed to answer it. Do not fill the gap with a guessed rule.
Record evidence scope so a source-only request cannot inherit simulator or
wiki assumptions.

The research side reads the queue first and returns new immutable batches
referencing question ids. It leaves the active list unchanged. The integration
side checks for returned answers at session start, verifies batch bytes and
interpretation, archives each accepted question and its answer under
`receipts/research-questions/`, then removes it from the active list.

Partial answers and answers awaiting verification remain visible. An answered
question can leave the research queue before simulator implementation; its
receipt records integration separately. A contradicted answer reopens the
original id. Each queue replacement publishes the complete file before its
updated READY guard and is read back before being reported as published.
