// THE SITE'S OWN FILES, served by whichever worker the request reached: the
// edge worker (worker/edge.js) answers them where the reader is, and the
// placed worker (worker/index.js) the same way when it is asked directly.
// Nothing here reads a database or a secret — docs/WASM.md §"Two workers".

// ---- THE SITE AS AN AGENT READS IT -----------------------------------------
//
// A page with a markdown twin (`build_site_app.py` `ship_agent_files`) serves
// the twin to `Accept: text/markdown`, and names it and the discovery documents
// in a `Link` header either way — docs/AGENT.md §Machine-readable.

/// The twin of a page that has one, or null.
function markdownTwin(path) {
  if (path === "/") return "/index.md";
  if (path === "/weapons") return "/weapons.md";
  const m = path.match(/^\/weapons\/([^/.]+)$/);
  return m ? `/weapons/${m[1]}.md` : null;
}

/// The page a twin is the twin of, or null — the inverse of `markdownTwin`.
function twinOf(path) {
  if (path === "/index.md") return "/";
  if (path === "/weapons.md") return "/weapons";
  const m = path.match(/^\/weapons\/([^/.]+)\.md$/);
  return m ? `/weapons/${m[1]}` : null;
}

/// A twin fetched by its own address names its page as canonical: without it
/// a search engine finds two copies of every weapon and picks neither.
async function twinFile(request, env, page) {
  const md = await env.ASSETS.fetch(request);
  if (!md.ok || (md.headers.get("content-type") || "").includes("text/html")) return md;
  const headers = new Headers(md.headers);
  headers.set("link", `<https://wfsim.app${page}>; rel="canonical"`);
  return new Response(md.body, { status: md.status, headers });
}

/// Whether the client ranks markdown at least as high as html (RFC 9110 q).
function prefersMarkdown(request) {
  const q = {};
  for (const part of (request.headers.get("accept") || "").split(",")) {
    const [type, ...params] = part.trim().toLowerCase().split(";");
    const w = params.map((p) => p.trim()).find((p) => p.startsWith("q="));
    q[type] = w ? Number(w.slice(2)) || 0 : 1;
  }
  const md = q["text/markdown"] ?? 0;
  return md > 0 && md >= (q["text/html"] ?? 0);
}

const DISCOVERY_LINKS = [
  '</llms.txt>; rel="describedby"; type="text/plain"',
  '</.well-known/api-catalog>; rel="api-catalog"',
  '</.well-known/agent-skills/index.json>; rel="describedby"; type="application/json"',
  '</.well-known/mcp/server-card.json>; rel="service-desc"; type="application/json"',
];


async function agentPage(request, env, twin) {
  const link = [`<${twin}>; rel="alternate"; type="text/markdown"`, ...DISCOVERY_LINKS].join(", ");
  if ((request.method === "GET" || request.method === "HEAD") && prefersMarkdown(request)) {
    const url = new URL(request.url);
    url.pathname = twin;
    const md = await env.ASSETS.fetch(new Request(url.toString(), request));
    // The SPA fallback answers a missing twin with html: serve the page instead.
    if (md.ok && !(md.headers.get("content-type") || "").includes("text/html")) {
      const headers = new Headers(md.headers);
      headers.set("content-type", "text/markdown; charset=utf-8");
      headers.set("vary", "Accept");
      headers.set("link", link);
      return new Response(md.body, { status: 200, headers });
    }
  }
  const page = await env.ASSETS.fetch(request);
  const headers = new Headers(page.headers);
  headers.append("vary", "Accept");
  headers.set("link", link);
  return new Response(page.body, { status: page.status, headers });
}
/// THE PREFIXES THAT ARE CONTENT-ADDRESSED: nothing under them is ever html.
const HASHED = ["/asset/", "/pkg/", "/ocr/"];

/// The response for a request the site's files answer alone, or null for one
/// that needs the worker's databases.
export async function edgeServe(request, env) {
  const path = new URL(request.url).pathname;
  const twin = markdownTwin(path);
  if (twin) return agentPage(request, env, twin);
  const page = twinOf(path);
  if (page) return twinFile(request, env, page);
  if (!HASHED.some((p) => path.startsWith(p))) return null;
  // A HASHED FILE THAT IS GONE IS A 404, AND NOT THE APP.
  //
  // `not_found_handling: single-page-application` answers every unmatched
  // path with index.html and a 200, which is right for a route and wrong for
  // a content-addressed file: an edge holding a page from the previous build
  // asks for `/asset/app.<old>.js`, gets HTML, runs it as JavaScript, and the
  // app never boots. `build_site_app.py` keeps the previous generation so this
  // is rare; this is what makes it LOUD when it happens anyway.
  const asset = await env.ASSETS.fetch(request);
  if ((asset.headers.get("content-type") || "").includes("text/html")) {
    return new Response("not found", { status: 404, headers: { "content-type": "text/plain" } });
  }
  return asset;
}
