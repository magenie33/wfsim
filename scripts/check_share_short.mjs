// SHORT SHARE LINKS — the worker's half, against a stub store.
//
// A short link is `/weapons/<name>/s/<id>`, and the id names a share code the
// worker stored (worker/index.js §"SHORT SHARE LINKS"). What must hold:
//   - the id is the WORKER's hash of (weapon, code): the same build is one id,
//     another weapon or another build is another, and no client picks one;
//   - what goes in comes back out, byte for byte;
//   - nothing that is not a share code is stored — a link is a build, never a
//     place to park arbitrary text;
//   - the short path serves the weapon's OWN page, so a pasted link previews
//     as that weapon.
//
//   node scripts/check_share_short.mjs
import worker, { SHARE_CODE, SHARE_ID, shareId } from "../worker/index.js";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

// THE STORE, as much of D1 as the endpoint touches: `INSERT OR IGNORE` keeps
// the first row under a key, and `SELECT … WHERE id = ?` reads it.
const shares = new Map();
const LIBRARY = {
  prepare: (sql) => ({
    bind: (...a) => ({
      run: async () => {
        if (/INSERT OR IGNORE INTO shares/.test(sql) && !shares.has(a[0])) {
          shares.set(a[0], { weapon: a[1], code: a[2], at: a[3] });
        }
      },
      first: async () => (/FROM shares/.test(sql) ? shares.get(a[0]) || null : null),
    }),
  }),
};
const served = [];
const ASSETS = {
  fetch: async (req) => {
    served.push(new URL(req.url).pathname);
    return new Response("<html>weapon page</html>", { headers: { "content-type": "text/html" } });
  },
};
const env = { LIBRARY, ASSETS };
const call = (path, init) => worker.fetch(new Request(`https://wfsim.app${path}`, init), env);
const store = (w, c) => call("/api/s", {
  method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ w, c }),
}).then(async (r) => ({ status: r.status, j: await r.json() }));

console.log("short share links");

// A v4 code as the page writes one: version, then the compact alphabet.
const CODE = "42f~B2AXCUAWAbB6BD-0CS--~EO~HfHhHiHe~;b;8;M;P43EP03ESk3E";
const a = await store("Dual_Toxocyst", CODE);
check("a share code is stored and named by a short id", a.status === 200 && a.j.ok && SHARE_ID.test(a.j.id),
  JSON.stringify(a));
check("the id is the worker's own hash of (weapon, code)", a.j.id === await shareId("Dual_Toxocyst", CODE));
const again = await store("Dual_Toxocyst", CODE);
check("the same build is the same id, and one row", again.j.id === a.j.id && shares.size === 1);
const other = await store("Braton_Prime", CODE);
check("another weapon is another id", other.j.ok && other.j.id !== a.j.id);

const got = await call(`/api/s/${a.j.id}`);
const back = await got.json();
check("what went in comes back out", back.ok && back.w === "Dual_Toxocyst" && back.c === CODE, JSON.stringify(back));
check("…and may be cached for ever, since the id is its content",
  /immutable/.test(got.headers.get("cache-control") || ""));
check("any origin may ask (the desktop shell does)", got.headers.get("access-control-allow-origin") === "*");
const missing = await call("/api/s/AAAAAAAAAA");
check("an unknown id is a 404", missing.status === 404);

for (const [what, w, c] of [
  ["text with a space", "Braton", "4hello world"],
  ["markup", "Braton", "4<script>alert(1)</script>"],
  ["no version character", "Braton", "xB2AX"],
  ["a weapon slug with a slash in it", "Braton/../x", CODE],
  ["a code past the ceiling", "Braton", "4" + "A".repeat(2000)],
]) {
  const r = await store(w, c);
  check(`refused: ${what}`, r.status === 400 && !r.j.ok, JSON.stringify(r));
}
check("…and nothing refused was stored", shares.size === 2);

// Every form the page may send fits the worker's alphabet: v4/v3 text and the
// two base64url forms.
for (const c of [CODE, "3102.4.5~~", "1eJyrVkrLz1eyUkpKLFKqBQAdegQp", "0W1sxLDJd"]) {
  check(`the worker accepts the page's form ${c[0]}`, SHARE_CODE.test(c));
}

served.length = 0;
const page = await call(`/weapons/Dual_Toxocyst/s/${a.j.id}`);
check("the short path serves the weapon's own page", page.status === 200 && served[0] === "/weapons/Dual_Toxocyst",
  `${page.status} ${served.join(",")}`);

console.log(failures ? `\n${failures} failed` : "\nshort links store a build and nothing else");
process.exit(failures ? 1 : 0);
