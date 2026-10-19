// SPDX-License-Identifier: AGPL-3.0-or-later
// REMOVE A WORKTREE WITHOUT FOLLOWING A LINK — docs/DEVELOPMENT.md §7.
// `git worktree remove --force` and `rm -rf` on Windows descend into a junction
// and delete the target's contents, so this refuses while any link is inside.
//
//   node scripts/remove_worktree.mjs <worktree> [--force]   remove it, refusing on a link
//   node scripts/remove_worktree.mjs --scan <dir>...         list the links under each dir
import { existsSync, lstatSync, readdirSync, readlinkSync } from "node:fs";
import { join, resolve } from "node:path";
import { execFileSync } from "node:child_process";
import { pathToFileURL } from "node:url";

/// EVERY LINK UNDER A DIRECTORY, never descending into one. `lstat` reports a
/// junction as a symbolic link, which is what makes this walk safe on Windows.
export function links(dir) {
  const found = [];
  const walk = (d) => {
    let names;
    try { names = readdirSync(d); } catch { return; }
    for (const n of names) {
      const p = join(d, n);
      let s;
      try { s = lstatSync(p); } catch { continue; }
      if (s.isSymbolicLink()) {
        let to = "?";
        try { to = readlinkSync(p); } catch {}
        found.push(`${p} -> ${to}`);
      } else if (s.isDirectory()) walk(p);
    }
  };
  if (lstatSync(dir).isSymbolicLink()) return [`${dir} -> ${readlinkSync(dir)}`];
  walk(dir);
  return found;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const args = process.argv.slice(2);
  if (args[0] === "--scan") {
    const all = args.slice(1).filter((d) => existsSync(d)).flatMap((d) => links(resolve(d)));
    for (const l of all) console.log(l);
    process.exit(all.length ? 1 : 0);
  }
  const force = args.includes("--force");
  const wt = args.find((a) => !a.startsWith("--"));
  if (!wt) { console.error("usage: remove_worktree.mjs <worktree> [--force] | --scan <dir>..."); process.exit(2); }
  const found = links(resolve(wt));
  if (found.length) {
    console.error(`refusing: ${found.length} link(s) inside, and removing would delete what they point to:`);
    for (const l of found) console.error(`  ${l}`);
    console.error("remove each LINK alone first (cmd: rmdir <link>, never /s), then run this again.");
    process.exit(1);
  }
  execFileSync("git", ["worktree", "remove", ...(force ? ["--force"] : []), resolve(wt)], { stdio: "inherit" });
}
