// THE OWNER WRITES TO READERS — docs/ACCOUNTS.md §"Notices". A notice to every
// reader, or to one account by its username; listed, and withdrawn. Writes the
// accounts database over ssh, as its own role, so the row keeps its owner.
//
//   set -a; . private/hetzner/ssh.env; . private/hetzner/accounts.env; set +a
//   node scripts/notice_send.mjs --all --zh "…" --en "…" [--link /path] [--until 2026-11-01]
//   node scripts/notice_send.mjs --to <username> --zh "…" [--en "…"] [--link /path]
//   node scripts/notice_send.mjs --list
//   node scripts/notice_send.mjs --withdraw <id>
//   add --dry to print the SQL and send nothing
import { execFileSync } from "node:child_process";
import { randomBytes } from "node:crypto";

const args = process.argv.slice(2);
const flag = (name) => args.includes(`--${name}`);
const value = (name) => { const i = args.indexOf(`--${name}`); return i >= 0 ? args[i + 1] : undefined; };
const die = (m) => { console.error(m); process.exit(2); };

/// A LITERAL NOTHING IN IT CAN CLOSE: dollar-quoted under a tag made for it.
function lit(s) {
  if (s == null) return "NULL";
  let tag;
  do { tag = `n${randomBytes(4).toString("hex")}`; } while (String(s).includes(tag));
  return `$${tag}$${s}$${tag}$`;
}

function sql() {
  if (flag("list")) {
    return `SELECT n.id, coalesce(a.username, '(everyone)'), n.kind, n.created_at, coalesce(n.expires_at, ''), left(coalesce(n.text_zh, n.text_en, n.params, ''), 60)
      FROM notices n LEFT JOIN accounts a ON a.id = n.account ORDER BY n.created_at DESC LIMIT 20;`;
  }
  if (flag("withdraw")) {
    const id = Number(value("withdraw"));
    if (!Number.isInteger(id) || id <= 0) die("--withdraw takes a notice id (see --list)");
    return `DELETE FROM notices WHERE id = ${id} RETURNING id;`;
  }
  const zh = value("zh"), en = value("en");
  if (!zh && !en) die("a notice needs --zh, --en or both");
  const link = value("link");
  if (link && !/^\/(?!\/)/.test(link)) die("--link is a path on the site, like /utility/reminders");
  const until = value("until");
  const expires = until ? new Date(until) : null;
  if (until && (isNaN(expires) || expires < new Date())) die("--until is a date in the future, like 2026-11-01");
  const to = value("to");
  if (!to && !flag("all")) die("say who: --all, or --to <username>");
  if (to && flag("all")) die("--all or --to, not both");
  const kind = to ? "message" : "announcement";
  const account = to ? `(SELECT id FROM accounts WHERE username = ${lit(to)})` : "NULL";
  // A USERNAME NOBODY HOLDS writes nothing, and says so, rather than a notice to everyone.
  const guard = to ? `WHERE ${account} IS NOT NULL` : "";
  return `INSERT INTO notices (account, kind, text_en, text_zh, link, created_at, expires_at)
    SELECT ${account}, '${kind}', ${lit(en)}, ${lit(zh)}, ${lit(link)}, ${lit(new Date().toISOString())}, ${lit(expires && expires.toISOString())} ${guard}
    RETURNING id;`;
}

const statement = sql();
if (flag("dry")) { console.log(statement); process.exit(0); }
const { SSH_TARGET, SSH_KEY, SSH_KNOWN_HOSTS, PG_ACCOUNTS_DB } = process.env;
if (!SSH_TARGET || !SSH_KEY || !PG_ACCOUNTS_DB) die("needs private/hetzner/ssh.env and accounts.env in the environment");
const out = execFileSync("ssh", ["-i", SSH_KEY, ...(SSH_KNOWN_HOSTS ? ["-o", `UserKnownHostsFile=${SSH_KNOWN_HOSTS}`] : []),
  "-o", "BatchMode=yes", "-o", "ConnectTimeout=15", SSH_TARGET,
  `sudo -u postgres psql -d ${PG_ACCOUNTS_DB} -At -F ' | ' -v ON_ERROR_STOP=1 -q`],
{ input: `SET ROLE wfsim_accounts;\n${statement}\n`, encoding: "utf8" }).trim();
if (!out && !flag("list")) die(flag("withdraw") ? "no notice has that id" : "nothing written: no account has that username");
console.log(out);
