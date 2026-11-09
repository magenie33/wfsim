// The public COS manifest remains the only CLI release pointer.
export const CLI_MANIFEST_PATH = "/api/cli/latest";
const LATEST = "https://wfsim-1388973035.cos.ap-shanghai.myqcloud.com/cli/latest.json";
const MAX_BYTES = 65536;

export async function clientRelease(request, fetcher = fetch) {
  if (request.method !== "GET") return new Response(null, { status: 405, headers: { allow: "GET" } });
  try {
    const upstream = await fetcher(new Request(LATEST, {
      headers: { accept: "application/json" }, redirect: "manual", signal: AbortSignal.timeout(10000),
    }));
    if (!upstream.ok || Number(upstream.headers.get("content-length")) > MAX_BYTES) throw Error("unavailable");
    const bytes = await upstream.arrayBuffer();
    if (bytes.byteLength > MAX_BYTES) throw Error("oversized");
    const release = JSON.parse(new TextDecoder().decode(bytes));
    if (release?.v !== 1 || !release.files || typeof release.files !== "object" || Array.isArray(release.files)) throw Error("invalid");
    return new Response(bytes, { headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" } });
  } catch (_) {
    return new Response('{"ok":false,"error":"cli_manifest_unavailable"}', {
      status: 502, headers: { "content-type": "application/json", "cache-control": "no-store" },
    });
  }
}
