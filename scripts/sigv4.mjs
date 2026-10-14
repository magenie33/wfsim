// AWS SIGNATURE VERSION 4, the part an S3 PUT needs, on Node's own crypto: R2
// speaks the S3 API, and the work signal is one small object written every few
// seconds, which no SDK is worth. Checked against AWS's published example in
// scripts/check_settler.mjs.
import { createHash, createHmac } from "node:crypto";

const sha256 = (data) => createHash("sha256").update(data).digest("hex");
const hmac = (key, data) => createHmac("sha256", key).update(data).digest();
/// S3's path encoding: every byte but the unreserved ones, `/` kept between segments.
const encodePath = (p) => p.split("/").map((s) => encodeURIComponent(s).replace(/[!'()*]/g, (c) => "%" + c.charCodeAt(0).toString(16).toUpperCase())).join("/");

/// THE HEADERS THAT SIGN ONE REQUEST. `headers` are the ones to sign besides
/// host, x-amz-date and x-amz-content-sha256 (which this adds); `amzDate` is
/// `YYYYMMDDTHHMMSSZ`. Returns every header to send, `authorization` included.
export function signV4({ method, url, headers = {}, body = "", payloadHash = null, accessKey, secretKey,
  region = "auto", service = "s3", amzDate }) {
  const u = new URL(url), date = amzDate.slice(0, 8);
  const hash = payloadHash || sha256(body);
  const all = { ...Object.fromEntries(Object.entries(headers).map(([k, v]) => [k.toLowerCase(), String(v).trim()])),
    host: u.host, "x-amz-date": amzDate, "x-amz-content-sha256": hash };
  const names = Object.keys(all).sort();
  const query = [...u.searchParams].map(([k, v]) => [encodeURIComponent(k), encodeURIComponent(v)]).sort()
    .map(([k, v]) => `${k}=${v}`).join("&");
  const canonical = [method, encodePath(decodeURIComponent(u.pathname)), query,
    names.map((n) => `${n}:${all[n]}\n`).join(""), names.join(";"), hash].join("\n");
  const scope = `${date}/${region}/${service}/aws4_request`;
  const toSign = ["AWS4-HMAC-SHA256", amzDate, scope, sha256(canonical)].join("\n");
  const key = hmac(hmac(hmac(hmac(`AWS4${secretKey}`, date), region), service), "aws4_request");
  const signature = createHmac("sha256", key).update(toSign).digest("hex");
  return { ...all, authorization: `AWS4-HMAC-SHA256 Credential=${accessKey}/${scope}, SignedHeaders=${names.join(";")}, Signature=${signature}`,
    signature, canonical };
}

export const amzDateOf = (ms = Date.now()) => new Date(ms).toISOString().replace(/[-:]/g, "").replace(/\.\d{3}/, "");

/// ONE OBJECT PUT to an S3-compatible bucket (R2: `endpoint` is
/// `https://<account>.r2.cloudflarestorage.com`, region `auto`), path-style.
/// A PUT THAT NEVER ANSWERS is given up after `timeoutMs`: an await on it would
/// hold whatever waits behind it for good.
export async function putObject({ endpoint, bucket, key, body, contentType, cacheControl, accessKey, secretKey, now = Date.now(), timeoutMs = 15_000 }) {
  const url = `${endpoint.replace(/\/+$/, "")}/${bucket}/${key}`;
  const signed = signV4({ method: "PUT", url, body, accessKey, secretKey, amzDate: amzDateOf(now),
    headers: { "content-type": contentType, "cache-control": cacheControl } });
  const { signature: _s, canonical: _c, host: _h, ...send } = signed;
  const r = await fetch(url, { method: "PUT", headers: send, body, signal: AbortSignal.timeout(timeoutMs) });
  if (!r.ok) throw new Error(`put ${key}: HTTP ${r.status} ${(await r.text()).slice(0, 200)}`);
  return r.status;
}
