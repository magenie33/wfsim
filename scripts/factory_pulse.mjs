// SPDX-License-Identifier: AGPL-3.0-or-later
// THE FACTORY DATABASE'S PULSE, read only — docs/ANALYTICS.md §"Factory calls".
// What the server pays for the volunteers' calls, from the database's side:
// the statements that read most per row returned and that take most time
// (pg_stat_statements), the connections waiting on their client (ClientRead —
// a transaction held open across round trips), the oldest open transaction,
// lock waits and deadlocks. Writes nothing.
//
//   set -a; . private/hetzner/ssh.env; set +a
//   node scripts/factory_pulse.mjs [--json]
import { execFileSync } from "node:child_process";

const { SSH_TARGET, SSH_KEY, SSH_KNOWN_HOSTS } = process.env;
if (!SSH_TARGET || !SSH_KEY) {
  console.error("needs SSH_TARGET and SSH_KEY (private/hetzner/ssh.env)");
  process.exit(2);
}

/// ONE READ-ONLY QUERY on database `wfsim`, rows as objects. The transaction is
/// read only, so a typo here can never change the database.
function rows(sql) {
  const out = execFileSync("ssh", ["-i", SSH_KEY, ...(SSH_KNOWN_HOSTS ? ["-o", `UserKnownHostsFile=${SSH_KNOWN_HOSTS}`] : []),
    "-o", "BatchMode=yes", "-o", "ConnectTimeout=15", SSH_TARGET,
    "sudo -u postgres psql -d wfsim -X -q -At -v ON_ERROR_STOP=1"],
  { input: `BEGIN READ ONLY;\nSELECT coalesce(json_agg(t), '[]') FROM (${sql}) t;\nROLLBACK;\n`, encoding: "utf8", maxBuffer: 64 << 20 });
  const at = out.indexOf("[");
  return at < 0 ? [] : JSON.parse(out.slice(at));
}

const STATEMENTS = (order) => `
  SELECT calls, round(total_exec_time)::bigint AS total_ms, round(mean_exec_time::numeric, 1) AS mean_ms,
         rows, shared_blks_hit + shared_blks_read AS blocks,
         round((shared_blks_hit + shared_blks_read)::numeric / greatest(rows, 1), 1) AS blocks_per_row,
         left(regexp_replace(query, '\\s+', ' ', 'g'), 140) AS query
    FROM pg_stat_statements
   WHERE dbid = (SELECT oid FROM pg_database WHERE datname = 'wfsim') AND calls >= 20
   ORDER BY ${order} DESC LIMIT 20`;

const pulse = {
  at: new Date().toISOString(),
  since: rows("SELECT stats_reset FROM pg_stat_statements_info")[0]?.stats_reset ?? null,
  by_blocks_per_row: rows(STATEMENTS("(shared_blks_hit + shared_blks_read)::numeric / greatest(rows, 1)")),
  by_total_time: rows(STATEMENTS("total_exec_time")),
  connections: rows(`SELECT count(*) AS client_backends,
      count(*) FILTER (WHERE wait_event = 'ClientRead' AND state IN ('active', 'idle in transaction')) AS waiting_on_client,
      count(*) FILTER (WHERE state = 'idle in transaction') AS idle_in_transaction,
      count(*) FILTER (WHERE wait_event_type = 'Lock') AS waiting_on_a_lock,
      round(extract(epoch FROM max(now() - xact_start)) * 1000)::bigint AS oldest_transaction_ms
    FROM pg_stat_activity WHERE backend_type = 'client backend' AND datname = 'wfsim' AND pid <> pg_backend_pid()`)[0],
  locks_not_granted: rows("SELECT locktype, mode, count(*) AS n FROM pg_locks WHERE NOT granted GROUP BY 1, 2")
    .map((x) => `${x.locktype}/${x.mode}: ${x.n}`),
  deadlocks: rows("SELECT deadlocks, xact_commit, xact_rollback FROM pg_stat_database WHERE datname = 'wfsim'")[0],
};

if (process.argv.includes("--json")) {
  console.log(JSON.stringify(pulse, null, 2));
} else {
  const c = pulse.connections;
  console.log(`factory pulse ${pulse.at} (statements since ${pulse.since || "the server started"})`);
  console.log(`connections: ${c.client_backends}, waiting on their client ${c.waiting_on_client}`
    + ` (${c.client_backends ? Math.round((100 * c.waiting_on_client) / c.client_backends) : 0}%), idle in a transaction ${c.idle_in_transaction},`
    + ` on a lock ${c.waiting_on_a_lock}; oldest transaction ${c.oldest_transaction_ms ?? 0} ms`);
  console.log(`locks not granted: ${pulse.locks_not_granted.join(", ") || "none"}; deadlocks ${pulse.deadlocks.deadlocks},`
    + ` commits ${pulse.deadlocks.xact_commit}, rollbacks ${pulse.deadlocks.xact_rollback}`);
  for (const [title, list] of [["most blocks read per row returned", pulse.by_blocks_per_row], ["most total time", pulse.by_total_time]]) {
    console.log(`\n${title}:`);
    for (const s of list) console.log(`  ${String(s.blocks_per_row).padStart(10)} blk/row ${String(s.total_ms).padStart(9)} ms ${String(s.calls).padStart(8)} calls  ${s.query}`);
  }
}
