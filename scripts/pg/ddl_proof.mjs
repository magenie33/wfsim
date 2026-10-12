// THE POSTGRES DDL APPLIES, in PGlite: worker/pg/schema.sql and library.sql in
// one database, accounts.sql in another, and the accounts trigger behaves.
//   node scripts/pg/ddl_proof.mjs
import { readFileSync } from "node:fs";
import { PGlite } from "./node_modules/@electric-sql/pglite/dist/index.js";

const read = (f) => readFileSync(new URL(`../../worker/pg/${f}`, import.meta.url), "utf8");
const lib = new PGlite();
await lib.exec(read("schema.sql"));
await lib.exec(read("library.sql"));
await lib.query("INSERT INTO builds (id, at, record) VALUES ($1, $2, $3)", ["b", "t", JSON.stringify({ weapon: "torid" })]);
const w = (await lib.query("SELECT weapon FROM builds")).rows[0].weapon;
const acc = new PGlite();
await acc.exec(read("accounts.sql"));
await acc.exec(`INSERT INTO accounts (id, created_at, username) VALUES ('a', 't', 'u');
  INSERT INTO identities (provider, subject, account, label, linked_at) VALUES ('google', 's1', 'a', 'l', 't'), ('github', 's2', 'a', 'l', 't');
  INSERT INTO sessions (token_hash, account, expires_at) VALUES ('h', 'a', 't');`);
await acc.exec("DELETE FROM identities WHERE subject = 's1'");
const kept = (await acc.query("SELECT count(*)::int n FROM accounts")).rows[0].n;
await acc.exec("DELETE FROM identities WHERE subject = 's2'");
const gone = (await acc.query("SELECT (SELECT count(*) FROM accounts)::int a, (SELECT count(*) FROM sessions)::int s")).rows[0];
const ok = w === "torid" && kept === 1 && gone.a === 0 && gone.s === 0;
console.log(ok ? "ok   the DDL applies; a build's weapon is read; the last slot takes its account and sessions"
  : `FAIL ${JSON.stringify({ w, kept, gone })}`);
process.exit(ok ? 0 : 1);
