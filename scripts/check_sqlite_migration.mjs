// Working copies retain source captures, row content, constraints and unfinished catalog state.
import assert from "node:assert/strict";
import {DatabaseSync} from "node:sqlite";
import {mkdtempSync,copyFileSync,readFileSync,rmSync} from "node:fs";
import {tmpdir} from "node:os";
import {join} from "node:path";
import {createHash} from "node:crypto";
import {INVENTORY_TABLES,migrateSqliteTables} from "./sqlite_schema.mjs";

const directory=mkdtempSync(join(tmpdir(),"wfsim-sqlite-boundaries-"));
const original=join(directory,"capture.sqlite"),working=join(directory,"working.sqlite");
const hash=path=>createHash("sha256").update(readFileSync(path)).digest("hex");
let db=new DatabaseSync(original),passed=0;
const check=(name,ok)=>{assert.ok(ok,name);passed++;console.log("ok "+name);};
try {
  db.exec("PRAGMA foreign_keys=ON");
  for(const name of Object.keys(INVENTORY_TABLES)) {
    db.exec(`CREATE TABLE ${name}(id INTEGER PRIMARY KEY,body BLOB NOT NULL,revision TEXT NOT NULL UNIQUE)`);
    db.prepare(`INSERT INTO ${name} VALUES(?,?,?)`).run(17,Buffer.from("原始内容\0"+name),"original-"+name);
  }
  db.exec(`CREATE INDEX owners_revision ON owners(revision);
    CREATE TABLE relation_fixture_links(id INTEGER PRIMARY KEY,owner INTEGER REFERENCES owners(id));
    INSERT INTO relation_fixture_links VALUES(7,17);`);
  const before=Object.fromEntries(Object.keys(INVENTORY_TABLES).map(name=>[name,db.prepare(`SELECT * FROM ${name}`).all()]));
  db.close();const captureHash=hash(original);copyFileSync(original,working);db=new DatabaseSync(working);
  assert.throws(()=>migrateSqliteTables(db,{records:"entity_inventory_builds",owners:"entity_inventory_builds"}),/already another table|already exists/i);
  check("a real SQL failure after the first rename rolls back every earlier rename",db.prepare("PRAGMA user_version").get().user_version===0
    && db.prepare("SELECT count(*) n FROM sqlite_schema WHERE name IN ('records','owners')").get().n===2
    && !db.prepare("SELECT 1 FROM sqlite_schema WHERE name='entity_inventory_builds'").get());
  migrateSqliteTables(db);
  for(const [old,name] of Object.entries(INVENTORY_TABLES))assert.deepEqual(db.prepare(`SELECT * FROM ${name}`).all(),before[old]);
  check("all catalog rows retain exact ids, binary content and revision tokens",true);
  check("old SQL relation names are removed without runtime aliases",Object.keys(INVENTORY_TABLES).every(name=>!db.prepare("SELECT 1 FROM sqlite_schema WHERE name=?").get(name)));
  check("foreign keys and indexes follow metadata renames",db.prepare("PRAGMA foreign_key_check").all().length===0
    && db.prepare("PRAGMA foreign_key_list(relation_fixture_links)").get().table==="relation_inventory_owners"
    && db.prepare("SELECT tbl_name FROM sqlite_schema WHERE name='owners_revision'").get().tbl_name==="relation_inventory_owners");
  migrateSqliteTables(db);
  check("reopening the current version is idempotent",db.prepare("PRAGMA user_version").get().user_version===1);
  check("every application table belongs to a named class",db.prepare("SELECT name FROM sqlite_schema WHERE type='table'").all()
    .every(row=>/^(entity|record|relation|state)_/.test(row.name)));
  db.exec("PRAGMA user_version=2");assert.throws(()=>migrateSqliteTables(db),/unsupported SQLite layout version/);
  check("a future version is refused without changing its layout",db.prepare("PRAGMA user_version").get().user_version===2);
  check("the protected source capture remains byte identical",hash(original)===captureHash);
  console.log(JSON.stringify({ok:true,checks:passed,transport:"node-sqlite",tables:Object.keys(INVENTORY_TABLES).length}));
} finally {db.close();rmSync(directory,{recursive:true,force:true});}
