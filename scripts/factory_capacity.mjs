// SPDX-License-Identifier: AGPL-3.0-or-later
// BACKGROUND READS yield to the request path; execution budgets never change
// witness selection, verification or credit. docs/SERVERS.md.
import { availableParallelism, freemem, totalmem, loadavg } from "node:os";
import { readFileSync } from "node:fs";

const read = (path) => { try { return readFileSync(path, "utf8"); } catch { return ""; } };
const bounded = (n, lo, hi, fallback) => Number.isFinite(n) && n >= lo && n <= hi ? n : fallback;
const psi = (s) => Number(s.match(/^some .*?avg10=([\d.]+)/m)?.[1] || 0);

export function resources() {
  const memory = read("/proc/meminfo"), available = Number(memory.match(/^MemAvailable:\s+(\d+)/m)?.[1] || 0) * 1024 || freemem();
  let cpus = availableParallelism();
  let group = read("/proc/self/cgroup").match(/^0::(.*)$/m)?.[1];
  while (group !== undefined) {
    const [quota, period] = read(`/sys/fs/cgroup${group}/cpu.max`).trim().split(/\s+/).map(Number);
    if (quota > 0 && period > 0) cpus = Math.min(cpus, quota / period);
    if (!group || group === "/") break;
    group = group.slice(0, group.lastIndexOf("/"));
  }
  return { cpus, memory_bytes: totalmem(), available_bytes: available,
    load: loadavg()[0], io_pct: psi(read("/proc/pressure/io")), memory_pct: psi(read("/proc/pressure/memory")) };
}

/// One background operation at a time; a rejected operation releases its turn.
export function backgroundLane() {
  let tail = Promise.resolve();
  return (fn) => {
    const run = tail.then(fn, fn);
    tail = run.catch(() => {});
    return run;
  };
}

export function capacity({ env = process.env, sample = resources, critical = {} } = {}) {
  const duty = bounded(Number(env.WFSIM_BACKGROUND_DUTY_PCT), 1, 25, 5) / 100;
  const ceiling = bounded(Number(env.WFSIM_BACKGROUND_MAX_MS), 60_000, 3_600_000, 3_600_000);
  const fixed = env.WFSIM_BACKGROUND_AUTO !== "0";
  const jobs = new Map();
  let host = sample();
  return {
    observe(name, ms, failed = false) {
      const old = jobs.get(name), elapsed = Math.max(0, ms);
      jobs.set(name, { ms: old ? Math.max(elapsed, old.ms * .8 + elapsed * .2) : elapsed,
        failures: failed ? (old?.failures || 0) + 1 : 0 });
    },
    interval(name, base) {
      if (!fixed) return base;
      host = sample();
      const job = jobs.get(name);
      if (!job) return base;
      const pressure = Math.max(host.io_pct / 100, host.memory_pct / 100,
        Math.max(0, host.load / Math.max(.1, host.cpus) - 1),
        ...Object.entries(critical).map(([name, ms]) => {
          const c = jobs.get(name);
          return c ? Math.max(c.failures ? 1 : 0, c.ms / ms - 1) : 0;
        }));
      return Math.round(Math.max(base, Math.min(ceiling,
        job.ms / duty * (1 + pressure) * 2 ** Math.min(3, job.failures))));
    },
    reading() {
      return { auto: fixed, duty_pct: duty * 100, host,
        jobs: Object.fromEntries([...jobs].map(([name, x]) => [name, { elapsed_ms: Math.round(x.ms), failures: x.failures }])) };
    },
  };
}

/// A daemon shares one rolling pace read between the demand and home tally.
export function paceCache(readPace, ttl = 60_000) {
  let kept = null, pending = null;
  return async (now = Date.now()) => {
    if (kept && now >= kept.at && now - kept.at < ttl) return kept.body;
    if (pending) return pending;
    pending = Promise.resolve().then(() => readPace(now)).then((body) => {
      kept = { at: now, body };
      return body;
    }).finally(() => { pending = null; });
    return pending;
  };
}

/// A reserved session owns the daemon across hosts; losing it stops the owner.
export async function claimDaemon(sql, onLost) {
  const session = await sql.reserve();
  try {
    const [row] = await session.unsafe("SELECT pg_try_advisory_lock(hashtextextended('wfsim-factoryd', 0)) AS held");
    if (!row.held) throw Error("another factory daemon owns this database");
  } catch (e) {
    session.release(); await sql.end(); throw e;
  }
  let released = false, lost = false;
  return {
    lost() { if (!released && !lost) { lost = true; onLost(); } },
    async release() {
      released = true;
      await session.unsafe("SELECT pg_advisory_unlock(hashtextextended('wfsim-factoryd', 0))").catch(() => {});
      session.release(); await sql.end();
    },
  };
}
