/// Keep HTTP certificate validation reachable while the website changes hosts.
export default {
  async fetch(request, env) {
    const path = new URL(request.url).pathname;
    if (!["GET", "HEAD"].includes(request.method)
      || !/^\/\.well-known\/acme-challenge\/[A-Za-z0-9_-]+$/.test(path)) {
      return new Response("not found", { status: 404 });
    }
    const response = await fetch(new URL(path, env.WEBSITE_ACME_ORIGIN).toString(),
      { method: request.method, redirect: "manual" });
    return response.status >= 300 && response.status < 400
      ? new Response("validation unavailable", { status: 502 }) : response;
  },
};
