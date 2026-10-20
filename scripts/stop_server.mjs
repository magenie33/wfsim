// STOP THE SERVER ON ONE PORT, and nothing else: `node scripts/stop_server.mjs
// <port>`. The machine is shared and every session runs its own wfsim-web, so a
// process is stopped by the PID listening on its port, never by its name
// (AGENTS.md §"Before you start").
import { execSync } from "node:child_process";

const port = Number(process.argv[2]);
if (!Number.isInteger(port) || port <= 0) {
  console.error("usage: node scripts/stop_server.mjs <port>");
  process.exit(2);
}
const win = process.platform === "win32";
const listening = () => {
  try {
    const out = win
      ? execSync(`netstat -ano -p tcp`, { encoding: "utf8" })
        .split(/\r?\n/)
        .filter((l) => /LISTENING/.test(l) && new RegExp(`[:.]${port}\\s`).test(l))
        .map((l) => l.trim().split(/\s+/).pop())
      : execSync(`lsof -ti tcp:${port} -sTCP:LISTEN`, { encoding: "utf8" }).split(/\s+/);
    return [...new Set(out.filter((p) => /^\d+$/.test(p) && p !== "0"))];
  } catch {
    return [];
  }
};
const pids = listening();
if (!pids.length) {
  console.log(`nothing listens on ${port}`);
  process.exit(0);
}
for (const pid of pids) {
  execSync(win ? `taskkill /PID ${pid} /F` : `kill ${pid}`, { stdio: "ignore" });
  console.log(`stopped ${pid} (port ${port})`);
}
