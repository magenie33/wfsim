#!/usr/bin/env python3
"""Assemble verified frozen ranges into existing archive parts and a hot COPY stream."""

import argparse
from contextlib import ExitStack
import gzip
import hashlib
import json
import os
from pathlib import Path
import resource
import tempfile
import uuid

from raw_inbox_frozen_range import END, HEADER, exact, row_fields, sync_directory, sync_file
from raw_inbox_rollover import MAX_BODY, RestoreConnection, clean_archive_lock, emit, prove_restore, sha


FIELDS = ("id", "at", "record", "initiator", "family_source", "observation")
NULL_FIELD = b"\xff\xff\xff\xff"


def ranges(work, generation):
    prepared = []
    for folder in work.glob(f"{generation}.*-*"):
        receipt_path = folder / "receipt.json"
        record = json.loads(receipt_path.read_bytes())
        if record["v"] != 1 or record["control"]["generation"] != str(generation):
            raise RuntimeError("range receipt has another archive identity")
        if folder.name != f'{generation}.{record["start"]}-{record["end"]}':
            raise RuntimeError("range path does not identify its receipt")
        prepared.append((record, folder))
    prepared.sort(key=lambda item: item[0]["start"])
    if not prepared:
        raise RuntimeError("no proven physical ranges")
    control = prepared[0][0]["control"]
    freeze = prepared[0][0].get("freeze")
    block = 0
    for record, _ in prepared:
        if (record["control"] != control or record.get("freeze") != freeze
            or record["start"] != block or record["end"] <= block):
            raise RuntimeError("the proven ranges must describe one continuous frozen source")
        block = record["end"]
    if block * control["block_size"] != control["bytes"]:
        raise RuntimeError("verified ranges do not cover the frozen source heap")
    return control,freeze,prepared


def read_group(path, columns, facts=None):
    with gzip.open(path, "rb") as stream:
        first = exact(stream, len(HEADER))
        if first != HEADER:
            raise RuntimeError("invalid binary COPY header")
        digest = hashlib.sha256(first)
        while (values := row_fields(stream, columns)) is not None:
            raw = bytes((0, columns)) + b"".join(values)
            digest.update(raw)
            yield values, raw
        digest.update(END)
        if stream.read(1) or (facts is not None and digest.hexdigest() != facts["sha256"]):
            raise RuntimeError("range payload no longer matches its restored bytes")


def separate(target, prepared):
    counts = {"hot":0,"cold":0}
    hot_path = target / "hot.copy.gz"
    hot_digest = hashlib.sha256(HEADER)
    with ExitStack() as stack, gzip.open(hot_path, "wb", compresslevel=1) as hot:
        bins = {}
        hot.write(HEADER)
        for record, folder in prepared:
            expected = record["counts"]
            if sum(expected.values()) != record["source_rows"]:
                raise RuntimeError("range count does not account for the source")
            for lane in counts:
                found = 0
                for values, raw in read_group(folder / (lane + ".copy.gz"), 7, record["groups"][lane]):
                    found += 1
                    if lane == "hot":
                        six = b"\x00\x06" + b"".join(values[:6])
                        hot.write(six)
                        hot_digest.update(six)
                    else:
                        if values[6] != NULL_FIELD:
                            raise RuntimeError("cold payload contains an unfinished queue position")
                        prefix = values[0][4:6].decode("ascii")
                        if len(prefix) != 2 or any(letter not in "0123456789abcdef" for letter in prefix):
                            raise RuntimeError("cold payload is not keyed by an archival UUID")
                        if prefix not in bins:
                            bins[prefix] = stack.enter_context(gzip.open(target / (prefix + ".bin"), "wb", compresslevel=1))
                            bins[prefix].write(HEADER)
                        bins[prefix].write(raw)
                if found != expected[lane]:
                    raise RuntimeError("range lane count changed")
                counts[lane] += found
        hot.write(END)
        hot_digest.update(END)
        for stream in bins.values():
            stream.write(END)
    sync_file(hot_path)
    return counts, hot_digest.hexdigest()


def pack(target, control, service, max_rows):
    part = control["sealed_parts"] + 1
    parts = []
    members = []
    member_bytes = 0

    def write(restore):
        nonlocal part, member_bytes
        document = f'{{"generation":"{control["generation"]}","part":{part},"rows":['.encode()
        body = document + b",".join(members) + b"]}"
        if len(body) > MAX_BODY:
            raise RuntimeError("assembled packet exceeded its wire bound")
        digest = sha(body)
        path = target / f"{part}-{digest}.json.gz"
        with path.open("xb") as destination:
            destination.write(gzip.compress(body, compresslevel=1, mtime=0))
            destination.flush()
            os.fsync(destination.fileno())
        readback = gzip.decompress(path.read_bytes())
        if sha(readback) != digest:
            raise RuntimeError("the packed object changed before independent restore")
        prove_restore(readback, service, control["cluster"], execute=restore.query)
        packet = json.loads(readback)["rows"]
        first, last = packet[0]["input"]["id"], packet[-1]["input"]["id"]
        parts.append(dict(part=part,first=first,last=last,rows=len(packet),digest=digest))
        members.clear()
        member_bytes = 0
        part += 1

    last = control["after"] or ""
    with RestoreConnection(service) as restore:
        for path in sorted(target.glob("*.bin")):
            entries = sorted(read_group(path, 7), key=lambda pair: pair[0][0])
            for values, _ in entries:
                record = {name: (None if value == NULL_FIELD else value[4:].decode("utf8"))
                          for name,value in zip(FIELDS,values)}
                if record["id"] <= last:
                    raise RuntimeError("assembled archive repeats or crosses the sealed source range")
                last = record["id"]
                addition = json.dumps(dict(input=record,position=None,portable=True),
                                      ensure_ascii=False,separators=(",", ":")).encode()
                if len(addition) + 257 > MAX_BODY:
                    raise RuntimeError("one input exceeds the archive packet bound")
                if len(members) == max_rows or member_bytes + len(addition) + 256 > MAX_BODY:
                    write(restore)
                members.append(addition)
                member_bytes += len(addition) + 1
            path.unlink()
        if members:
            write(restore)
    return parts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--work",type=Path,required=True)
    parser.add_argument("--generation",type=uuid.UUID,required=True)
    parser.add_argument("--restore-service",required=True)
    parser.add_argument("--rows",type=int,choices=(512,2048),default=512)
    args = parser.parse_args()
    soft, hard = resource.getrlimit(resource.RLIMIT_NOFILE)
    if soft < 300:
        resource.setrlimit(resource.RLIMIT_NOFILE,(300,hard))
    with clean_archive_lock(args.work):
        control,freeze,prepared = ranges(args.work,args.generation)
        final = args.work / f"{args.generation}.assembled"
        if final.exists():
            raise RuntimeError("the verified archive plan already exists")
        with tempfile.TemporaryDirectory(prefix=".assembled-",dir=args.work) as temporary:
            root = Path(temporary)
            counts, hot_sha = separate(root,prepared)
            parts = pack(root,control,args.restore_service,args.rows)
            if sum(p["rows"] for p in parts) != counts["cold"]:
                raise RuntimeError("archive part counts do not match the verified ranges")
            manifest = {"v":1,"control":control,"freeze":freeze,"counts":counts,"hot_sha256":hot_sha,"parts":parts,
                        "ranges":[record for record,_ in prepared]}
            plan = root / "manifest.json"
            plan.write_text(json.dumps(manifest,separators=(",", ":"))+"\n")
            spool = root / "spool.json"
            spool.write_text(json.dumps(dict(v=1,generation=str(args.generation),
                cloud_through=control["sealed_parts"]),separators=(",", ":"))+"\n")
            sync_file(plan)
            sync_file(spool)
            sync_directory(root)
            root.rename(final)
            sync_directory(args.work)
    emit(dict(path=str(final),counts=counts,parts=len(parts)))


if __name__ == "__main__":
    main()
