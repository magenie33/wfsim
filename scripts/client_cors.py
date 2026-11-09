"""Preserve bucket CORS and add website GET access for the public CLI manifest."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import urllib.error
import urllib.request
import xml.etree.ElementTree as ET

import cos

ORIGIN = "https://wfsim.app"
RULE_ID = "wfsim-cli-web-get"


def tag(node):
    return node.tag.rsplit("}", 1)[-1]


def tree(raw):
    root = ET.fromstring(raw)
    if tag(root) != "CORSConfiguration":
        raise ValueError("unexpected bucket CORS response")
    return root


def fingerprint(node):
    return (node.tag, tuple(sorted(node.attrib.items())), (node.text or "").strip(),
            tuple(fingerprint(child) for child in node))


def preserves(before, after):
    old, new = tree(before), tree(after)
    if old.tag != new.tag or old.attrib != new.attrib:
        return False
    previous = [fingerprint(child) for child in old]
    at = 0
    for child in new:
        if at < len(previous) and fingerprint(child) == previous[at]:
            at += 1
    return at == len(previous)


def covered(root):
    for rule in root:
        origins = [n.text for n in rule if tag(n) == "AllowedOrigin"]
        methods = [n.text for n in rule if tag(n) == "AllowedMethod"]
        if tag(rule) == "CORSRule" and (ORIGIN in origins or "*" in origins) and "GET" in methods:
            return True
    return False


def plan(raw):
    root = tree(raw)
    if covered(root):
        return raw
    if any(n.text == RULE_ID for r in root for n in r if tag(n) == "ID"):
        raise ValueError("CLI rule id already exists without the required access")
    namespace = root.tag[:-len("CORSConfiguration")]
    rule = ET.Element(namespace + "CORSRule")
    for key, value in [("ID", RULE_ID), ("AllowedOrigin", ORIGIN), ("AllowedMethod", "GET"), ("MaxAgeSeconds", "600")]:
        ET.SubElement(rule, namespace + key).text = value
    last = max((i for i, n in enumerate(root) if tag(n) == "CORSRule"), default=-1)
    root.insert(last + 1, rule)
    result = ET.tostring(root, encoding="utf-8", xml_declaration=True)
    if not preserves(raw, result):
        raise ValueError("CORS planning changed existing rules or options")
    return result


def request(c, method, body=None):
    req = urllib.request.Request(f"https://{cos.host(c)}/?cors", data=body, method=method)
    req.add_header("Authorization", cos.sign(method, "/", c["secret_id"], c["secret_key"]))
    if body is not None:
        req.add_header("Content-Type", "application/xml")
        req.add_header("Content-MD5", base64.b64encode(hashlib.md5(body).digest()).decode())
    try:
        response = cos._open(req, 30) if method == "GET" else urllib.request.urlopen(req, timeout=30)
        with response:
            raw = response.read(65_537)
            if len(raw) > 65_536:
                raise ValueError("CORS response exceeds the configuration limit")
            return raw
    except urllib.error.HTTPError as error:
        try:
            code = ET.fromstring(error.read()).findtext("Code") or "unknown"
        except ET.ParseError:
            code = "unknown"
        finally:
            error.close()
        if method == "GET" and error.code == 404 and code == "NoSuchCORSConfiguration":
            return None
        raise RuntimeError(f"COS CORS {method}: HTTP {error.code} ({code})") from None


def probe(c):
    req = urllib.request.Request(f"https://{cos.host(c)}/cli/latest.json", headers={"Origin": ORIGIN}, method="GET")
    with cos._open(req, 30) as response:
        allowed = response.headers.get("Access-Control-Allow-Origin")
        if response.status != 200 or allowed not in (ORIGIN, "*"):
            raise ValueError("public CLI manifest still lacks browser GET access")


def ensure(c, output, apply=False, verify_manifest=True):
    output.mkdir(parents=True, exist_ok=True)
    original = request(c, "GET")
    before = original if original is not None else b"<CORSConfiguration/>"
    after = plan(before)
    if not preserves(before, after) or not covered(tree(after)):
        raise ValueError("CORS proposal does not preserve the existing configuration")
    (output / "before.xml").write_bytes(before)
    (output / "proposed.xml").write_bytes(after)
    summary = {"origin": ORIGIN, "methods": ["GET"], "changed": before != after,
               "before_sha256": hashlib.sha256(before).hexdigest(),
               "proposed_sha256": hashlib.sha256(after).hexdigest(),
               "rollback": "DELETE bucket cors" if original is None else "PUT before.xml",
               "applied": False, "manifest_verified": False}
    (output / "plan.json").write_text(json.dumps(summary, indent=2) + "\n")
    if apply:
        if before != after:
            if request(c, "GET") != original:
                raise ValueError("bucket CORS changed since inspection; refusing PUT")
            request(c, "PUT", after)
        readback = request(c, "GET")
        if readback is None:
            raise ValueError("CORS readback is missing")
        (output / "readback.xml").write_bytes(readback)
        if not preserves(before, readback) or not covered(tree(readback)):
            raise ValueError("CORS readback changed existing rules/options or lacks website GET")
        if fingerprint(tree(after)) != fingerprint(tree(readback)):
            raise ValueError("CORS readback differs from the exact proposal")
        if verify_manifest:
            probe(c)
            summary["manifest_verified"] = True
        summary["applied"] = True
        summary["readback_sha256"] = hashlib.sha256(readback).hexdigest()
        (output / "plan.json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--apply", action="store_true")
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--configure-only", action="store_true")
    modes.add_argument("--verify-only", action="store_true")
    parser.add_argument("--output", type=Path, default=Path("target/client-cors"))
    args = parser.parse_args()
    if args.verify_only:
        if args.apply:
            parser.error("--verify-only cannot apply configuration")
        probe(cos.creds())
        print(json.dumps({"origin": ORIGIN, "manifest_verified": True}))
    else:
        print(json.dumps(ensure(cos.creds(), args.output, args.apply, not args.configure_only), indent=2))
