"""Resumable, bounded-space preparation of a frozen generation's cold objects."""

import gzip
import hashlib
import json
import os
import re
import struct

from raw_inbox_frozen_range import END, HEADER, exact, row_fields, sync_directory
from raw_inbox_rollover import MAX_BODY, read_local_object, sha


FIELDS = ("id","at","record","initiator","family_source","observation")
NULL = b"\xff\xff\xff\xff"
NAME = re.compile(r"([0-9]+)-[0-9a-f]{64}\.json\.gz\Z")


def store(path, value):
    next_path = path.with_suffix(".next")
    with next_path.open("w") as output:
        json.dump(value,output,separators=(",",":"))
        output.flush()
        os.fsync(output.fileno())
    next_path.replace(path)
    sync_directory(path.parent)


def copy_frame(values):
    row = bytearray(b"\x00\x07")
    for field in FIELDS:
        value = values[field]
        if value is None:
            row.extend(NULL)
        else:
            data = value.encode("utf8")
            row.extend(struct.pack("!i",len(data)))
            row.extend(data)
    row.extend(NULL)
    return row


def consolidate(root, prepared, state_path, state):
    from raw_inbox_frozen_assemble import read_group

    for path in root.glob("bin-*.gz"):
        prefix = path.stem[4:]
        committed = state["lengths"].get(prefix,0)
        if path.stat().st_size<committed:
            raise RuntimeError("a committed cold bin is shorter than its receipt")
        with path.open("r+b") as file:
            file.truncate(committed)
    while state["ranges_done"]<len(prepared):
        first = state["ranges_done"]
        batch = prepared[first:first+16]
        evidence = {}
        rows = {}
        for receipt, folder in batch:
            staged = {}
            old = folder / "cold.copy.gz"
            count = 0
            for values,raw in read_group(old,7,receipt["groups"]["cold"]):
                if values[-1]!=NULL:
                    raise RuntimeError("cold range carries a FIFO position")
                prefix = values[0][4:6].decode("ascii")
                if len(prefix)!=2 or any(char not in "0123456789abcdef" for char in prefix):
                    raise RuntimeError("cold range has a nonarchival identifier")
                staged.setdefault(prefix,bytearray()).extend(raw)
                rows[prefix] = rows.get(prefix,0)+1
                count += 1
            if count!=receipt["counts"]["cold"]:
                raise RuntimeError("cold range count no longer matches its verified receipt")
            for prefix,data in staged.items():
                packed = gzip.compress(HEADER+data+END,compresslevel=1,mtime=0)
                with (root / ("bin-"+prefix+".gz")).open("ab") as file:
                    file.write(packed)
                evidence.setdefault(prefix,hashlib.sha256()).update(packed)
        batch_proof = {}
        for prefix,digest in sorted(evidence.items()):
            path = root / ("bin-"+prefix+".gz")
            begin = state["lengths"].get(prefix,0)
            end = path.stat().st_size
            observed = hashlib.sha256()
            with path.open("rb") as file:
                os.fsync(file.fileno())
                file.seek(begin)
                for block in iter(lambda:file.read(1024*1024),b""):
                    observed.update(block)
            if observed.digest()!=digest.digest() or end<=begin:
                raise RuntimeError("the appended prefix did not match the captured rows")
            state["lengths"][prefix] = end
            state["counts"][prefix] = state["counts"].get(prefix,0)+rows[prefix]
            batch_proof[prefix] = [begin,end,digest.hexdigest()]
        state["evidence"].append(batch_proof)
        state["ranges_done"] += len(batch)
        store(state_path,state)
        for _,folder in batch:
            old = folder / "cold.copy.gz"
            old.unlink(missing_ok=True)
            sync_directory(folder)
    state["phase"] = "packing"
    store(state_path,state)


def verified_bin(path, prefix, state):
    with path.open("rb") as compressed:
        for witness in state["evidence"]:
            if prefix not in witness:
                continue
            start,end,expected = witness[prefix]
            if compressed.tell()!=start:
                raise RuntimeError("the acknowledged bin contains a gap")
            digest = hashlib.sha256()
            while compressed.tell()<end:
                block = exact(compressed,min(1024*1024,end-compressed.tell()))
                digest.update(block)
            if digest.hexdigest()!=expected:
                raise RuntimeError("a previously acknowledged prefix digest changed")
        if compressed.read(1):
            raise RuntimeError("the durable prefix contains an unacknowledged tail")
    with gzip.open(path,"rb") as archive:
        while first := archive.read(len(HEADER)):
            if first!=HEADER:
                raise RuntimeError("prefix input is not PostgreSQL binary COPY")
            while (values := row_fields(archive,7)) is not None:
                yield values


def finalize_prefix(root, path, prefix, state, max_rows, control):
    payloads = sorted(verified_bin(path,prefix,state),key=lambda values:values[0])
    if len(payloads)!=state["counts"][prefix]:
        raise RuntimeError("prefix members do not account for all verified input")
    part = control["sealed_parts"]+len(state["parts"])+1
    last = state["parts"][-1]["last"] if state["parts"] else control["after"] or ""
    members = []
    member_bytes = 0
    source = hashlib.sha256()

    def seal():
        nonlocal part,member_bytes,source
        front = f'{{"generation":"{control["generation"]}","part":{part},"rows":['.encode()
        body = front+b",".join(members)+b"]}"
        if len(body)>MAX_BODY:
            raise RuntimeError("packed archive part exceeds its fixed byte limit")
        digest = sha(body)
        output = root / f"{part}-{digest}.json.gz"
        with output.open("xb") as target:
            target.write(gzip.compress(body,compresslevel=1,mtime=0))
            target.flush()
            os.fsync(target.fileno())
        receipt = json.loads(read_local_object(output,digest))
        observed = hashlib.sha256()
        for entry in receipt["rows"]:
            if entry["position"] is not None or entry["portable"] is not True:
                raise RuntimeError("packed cold input acquired queue ownership")
            observed.update(copy_frame(entry["input"]))
        if observed.digest()!=source.digest():
            raise RuntimeError("packed bytes did not reproduce the independently restored source")
        rows = receipt["rows"]
        state["parts"].append(dict(part=part,first=rows[0]["input"]["id"],
            last=rows[-1]["input"]["id"],rows=len(rows),digest=digest))
        part += 1
        member_bytes = 0
        members.clear()
        source = hashlib.sha256()

    for fields in payloads:
        record = {label:(None if value==NULL else value[4:].decode("utf8"))
                  for label,value in zip(FIELDS,fields)}
        if record["id"]<=last or fields[-1]!=NULL:
            raise RuntimeError("a cold input repeats, crosses the archived prefix, or owns a FIFO position")
        last = record["id"]
        body = json.dumps(dict(input=record,position=None,portable=True),ensure_ascii=False,
                          separators=(",",":")).encode()
        if len(body)+257>MAX_BODY:
            raise RuntimeError("an archived input exceeds the packet byte limit")
        if len(members)==max_rows or member_bytes+len(body)+256>MAX_BODY:
            seal()
        members.append(body)
        member_bytes += len(body)+1
        source.update(b"\x00\x07"+b"".join(fields))
    if members:
        seal()


def assemble_stream(work,generation,control,freeze,prepared,max_rows):
    identity = sha(json.dumps([control,freeze,[record for record,_ in prepared],max_rows],
                              sort_keys=True,separators=(",",":")).encode())
    root = work / f"{generation}.packing"
    root.mkdir(exist_ok=True)
    state_path = root / "packing.json"
    if state_path.exists():
        state = json.loads(state_path.read_bytes())
        if state["identity"]!=identity:
            raise RuntimeError("the active packing work has another source or part width")
    else:
        state = dict(identity=identity,phase="consolidating",ranges_done=0,lengths={},counts={},
                     evidence=[],parts=[],done_prefixes=[])
        store(state_path,state)
    if state["phase"]=="consolidating":
        if any(not (root/("bin-"+prefix+".gz")).is_file() or
               (root/("bin-"+prefix+".gz")).stat().st_size<size for prefix,size in state["lengths"].items()):
            raise RuntimeError("the committed cold staging contents are incomplete")
        for record,path in prepared[:state["ranges_done"]]:
            removed = path / "cold.copy.gz"
            if removed.exists():
                removed.unlink()
                sync_directory(path)
        consolidate(root,prepared,state_path,state)
    if state["phase"]=="packing":
        accepted = control["sealed_parts"]+len(state["parts"])
        for file in root.iterdir():
            match = NAME.fullmatch(file.name)
            if match and int(match[1])>accepted:
                file.unlink()
        for prefix in sorted(state["counts"]):
            path = root / ("bin-"+prefix+".gz")
            if prefix not in state["done_prefixes"]:
                finalize_prefix(root,path,prefix,state,max_rows,control)
                state["done_prefixes"].append(prefix)
                store(state_path,state)
            path.unlink(missing_ok=True)
        state["phase"] = "complete"
        store(state_path,state)
    counts = {lane:sum(record["counts"][lane] for record,_ in prepared) for lane in ("hot","cold")}
    if (sum(state["counts"].values())!=counts["cold"]
        or sum(part["rows"] for part in state["parts"])!=counts["cold"]):
        raise RuntimeError("verified cold counts differ from the final packed ledger")
    return root,counts,state["parts"]
