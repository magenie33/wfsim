import { isIP } from "node:net";

/// Only the authenticated website proxy may restore the public URL and address.
export function websiteRequest(request, env) {
  const token = request.headers.get("x-wfsim-website");
  if (!token) return request;
  if (!env.WEBSITE_PROXY_SECRET || token !== env.WEBSITE_PROXY_SECRET) {
    return new Response("forbidden", { status: 403 });
  }
  const ip = request.headers.get("x-wfsim-website-ip") || "";
  if (!isIP(ip)) return new Response("invalid address", { status: 400 });
  const url = new URL(request.url);
  url.protocol = "https:";
  url.host = "wfsim.app";
  const headers = new Headers(request.headers);
  headers.delete("x-wfsim-website");
  headers.delete("x-wfsim-website-ip");
  headers.delete("x-wfsim-relay");
  headers.delete("x-wfsim-client-ip");
  headers.set("cf-connecting-ip", ip);
  headers.set("cf-ipcountry", "XX");
  const restored = new Request(url, new Request(request, { headers }));
  // The proxy's country is not the reader's; leave geography unknown.
  Object.defineProperty(restored, "cf", { value: { country: "XX" } });
  return restored;
}
