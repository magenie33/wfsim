// A HASHED FILE THAT IS GONE IS A 404, AND NOT THE APP.
//
// `not_found_handling: single-page-application` answers every unmatched path
// with index.html and a 200. That is right for a route and catastrophic for a
// content-addressed file: an edge holding a page from the previous build asks
// for `/asset/app.<old>.js`, is handed HTML, runs it as JavaScript, and the app
// never boots — the board empty, nothing submittable, every surface gone, and
// nothing in the console but a syntax error in a file that looks like it
// loaded. The site is up, every endpoint is healthy, and the reader sees
// nothing. That is the quietest failure this deployment can produce.
//
// TWO HALVES, AND BOTH ARE HERE. `build_site_app.py` keeps one previous
// generation so a deploy does not tear; the worker turns a miss into a real
// 404 so that when it tears anyway it says so. This asserts the second and
// the NEGATIVE CONTROL beside it — an ordinary route must still reach the SPA,
// because a worker that 404s those has broken every deep link on the site.
import worker from "../worker/index.js";
import fs from "node:fs";

let failures = 0;
const check = (what, ok, detail = "") => {
  console.log(`  ${ok ? "ok " : "FAIL"}  ${what}${ok || !detail ? "" : `   ${detail}`}`);
  if (!ok) failures++;
};

// THE SPA FALLBACK, AS THE PLATFORM SERVES IT: index.html, 200, text/html, for
// any path with no file behind it. A stub that answered 404 would be testing a
// deployment nobody has.
const SHELL = "<!doctype html><title>WFSim</title><script src=/asset/app.abc.js></script>";
const env = {
  ASSETS: {
    fetch: async (req) => {
      const path = new URL(req.url).pathname;
      if (FILES.has(path)) {
        return new Response(FILES.get(path)[1],
          { headers: { "content-type": FILES.get(path)[0] } });
      }
      return new Response(SHELL, { status: 200, headers: { "content-type": "text/html" } });
    },
  },
};
const FILES = new Map([
  ["/asset/app.aaaaaaaaaaaa.js", ["text/javascript", "console.log(1)"]],
  ["/pkg/wfsim_wasm_bg.aaaaaaaaaaaa.wasm", ["application/wasm", "\0asm"]],
  ["/index.html", ["text/html", SHELL]],
]);

const get = (path) => worker.fetch(new Request(`https://wfsim.app${path}`), env);

// ---- the file that is gone ------------------------------------------------
for (const path of ["/asset/app.deadbeefdead.js", "/asset/style.deadbeefdead.css",
                    "/pkg/wfsim_wasm.deadbeefdead.js",
                    "/pkg/wfsim_wasm_bg.deadbeefdead.wasm"]) {
  const r = await get(path);
  const body = await r.text();
  check(`a deleted ${path.split("/")[1]} file is a 404`, r.status === 404,
        `${r.status} ${r.headers.get("content-type")}`);
  // THE STATUS IS NOT THE POINT ON ITS OWN. What must never happen is the
  // SHELL arriving under a script's name, whatever the status beside it.
  check(`...and what comes back is not the app's own html`,
        !body.includes("<!doctype html>"), body.slice(0, 60));
}

// ---- the file that is there -----------------------------------------------
const live = await get("/asset/app.aaaaaaaaaaaa.js");
check("a hashed file that exists is served untouched",
      live.status === 200 && (await live.text()) === "console.log(1)", String(live.status));
check("...with its own content type, not rewritten",
      live.headers.get("content-type") === "text/javascript",
      String(live.headers.get("content-type")));

const wasm = await get("/pkg/wfsim_wasm_bg.aaaaaaaaaaaa.wasm");
check("...and so is the wasm module",
      wasm.status === 200 && wasm.headers.get("content-type") === "application/wasm",
      `${wasm.status} ${wasm.headers.get("content-type")}`);

// ---- THE NEGATIVE CONTROL -------------------------------------------------
//
// Every route on this site is a path with no file behind it. A worker that
// answered those with a 404 would have taken down every deep link, every
// prerendered weapon URL and the app itself — a bigger outage than the one
// above, shipped as its fix.
for (const path of ["/weapons/Torid", "/benchmark", "/thanks", "/"]) {
  const r = await get(path);
  check(`${path} still reaches the app (negative control)`,
        r.status === 200 && (await r.text()).includes("<!doctype html>"),
        String(r.status));
}

// ---- and the build keeps a generation, which is the half that prevents it ---
//
// READ OFF THE SCRIPT, because the behaviour only shows up across two builds
// and this check runs in milliseconds. What it asserts is that nothing clears
// those directories wholesale any more: `rmtree` on either is the bug.
const build = fs.readFileSync(new URL("./build_site_app.py", import.meta.url), "utf8");
check("the site build no longer clears the hashed directories",
      !/shutil\.rmtree\(out\)/.test(build),
      (build.match(/.*rmtree.*/g) || []).join(" ; "));
check("...and each of them keeps one previous generation",
      (build.match(/keep_one_generation\(out,/g) || []).length === 2,
      String((build.match(/keep_one_generation\(out,/g) || []).length));

console.log(failures
  ? `\n${failures} failed`
  : "\na deleted hashed file says so, and every route still reaches the app");
process.exit(failures ? 1 : 0);
