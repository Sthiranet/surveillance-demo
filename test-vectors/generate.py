"""Independent reference implementation of Sthiranet hashing spec v1, used to generate
hashing-v1.json. The Rust crate sthira-core must reproduce every value (see its vectors test).

Run: python3 -I test-vectors/generate.py > test-vectors/hashing-v1.json
     python3 -I test-vectors/generate.py surveillance > test-vectors/surveillance-v1.json

The second form emits vectors for the surveillance/v1 profile: synthetic segment bytes, content
hashes, records, leaves, a 6-leaf root and a head. This script checks the hashing only. The device
signatures are fixed constants produced by the Rust camera (Ed25519 is deterministic, and
ed25519-dalek is tested against RFC 8032); the Rust vectors test checks that the camera with the
seed below reproduces them.

JCS here is json.dumps with sorted keys and no whitespace. That equals RFC 8785 for these inputs
(strings, integers, null; keys in the Basic Multilingual Plane) but not for floats in general.
"""
import hashlib
import json
import sys


def jcs(v):
    return json.dumps(v, sort_keys=True, separators=(",", ":"), ensure_ascii=False)


def h(*parts):
    d = hashlib.sha256()
    for p in parts:
        d.update(p)
    return d.hexdigest()


def leaf(salt, event):
    return h(b"\x00", bytes.fromhex(salt), jcs(event).encode())


def node(l, r):
    return h(b"\x01", bytes.fromhex(l), bytes.fromhex(r))


def split(n):
    k = 1
    while k * 2 < n:
        k *= 2
    return k


def mth(leaves):
    if not leaves:
        return h(b"")
    if len(leaves) == 1:
        return leaves[0]
    k = split(len(leaves))
    return node(mth(leaves[:k]), mth(leaves[k:]))


def path(leaves, m):
    if len(leaves) == 1:
        return []
    k = split(len(leaves))
    if m < k:
        return path(leaves[:k], m) + [mth(leaves[k:])]
    return path(leaves[k:], m - k) + [mth(leaves[:k])]


def head(prev, root, meta):
    return h(b"\x02", bytes.fromhex(prev), bytes.fromhex(root), jcs(meta).encode())


def genesis(chain):
    return h(b"\x03", b"sthiranet/v1/genesis/", chain.encode())


def event(i):
    je = {
        "id": f"JE-{1001 + i}", "entry_date": "2026-11-09", "account": "6400",
        "description": "Freight and logistics", "amount": f"{1200 + i * 1111}.{i:02d}",
        "currency": "USD", "status": "posted", "modified_by": "erp_svc_s1",
    }
    before = None if i % 2 == 0 else dict(je, amount="999.99")
    return {
        "v": 1, "chain": "S1",
        "source": {"connector": "postgresql", "db": "erp_s1", "schema": "public",
                   "table": "journal_entries", "pos": f"0/1A3F{i:03X}", "tx": str(5000 + i)},
        "key": {"id": je["id"]}, "op": "insert" if before is None else "update",
        "before": before, "after": je,
        "actor": {"name": "erp_svc_s1", "kind": "service"},
        "committed_at": f"2026-11-09T10:00:0{i}.000000Z",
        "captured_at": f"2026-11-09T10:00:0{i}.250000Z",
    }


def hashing_vectors():
    events = [event(i) for i in range(5)]
    salts = [f"{i + 1:02x}" * 32 for i in range(5)]
    leaves = [leaf(s, e) for s, e in zip(salts, events)]
    g = genesis("S1")
    root = mth(leaves)
    meta = {"chain": "S1", "seq": 1, "from_pos": events[0]["source"]["pos"], "to_pos": events[-1]["source"]["pos"],
            "from_time": "2026-11-09T09:59:50.000000Z", "to_time": "2026-11-09T10:00:10.000000Z", "event_count": 5}
    empty_meta = dict(meta, seq=2, from_pos=meta["to_pos"], from_time=meta["to_time"],
                      to_time="2026-11-09T10:00:20.000000Z", event_count=0)
    h1 = head(g, root, meta)

    print(json.dumps({
        "spec": "sthiranet hashing v1",
        "events": [{"salt": s, "event": e, "canonical": jcs(e), "leaf": l} for s, e, l in zip(salts, events, leaves)],
        "batch": {"leaves": leaves, "root": root, "proofs": [{"index": i, "path": path(leaves, i)} for i in range(5)]},
        "genesis": {"chain": "S1", "head": g},
        "heads": [
            {"prev_head": g, "root": root, "meta": meta, "canonical_meta": jcs(meta), "head": h1},
            {"prev_head": h1, "root": h(b""), "meta": empty_meta, "canonical_meta": jcs(empty_meta), "head": head(h1, h(b""), empty_meta)},
        ],
    }, indent=2, ensure_ascii=False))


# ── surveillance/v1 ──

CAMERA = {"id": "CAM-DOCK", "site": "WH-PUNE-01", "zone": "dock", "key_id": "ed25519:ea4a6c63e29c520a"}
DEVICE_SEED = "07" * 32
DEVICE_PUBLIC_KEY = "ea4a6c63e29c520abef5507b132ec5f9954776aebebe7b92421eea691446d22c"
DEVICE_SIGS = [
    "fcccc1c25acfbe92df94fe6e6ae42b77b0b30790a2a94d3471a3c4163ea316591b44919cace6d91fa288d986259759f86874a371f627c7c074c8f534c15a920c",
    "7bd73ad167ec38c9d52c17d18d2011fe21aee0f6f9e6161a4c5e3c3018c582c976e28e09245785ade73d8f11639e84d7293ab7a931c12eca0219e6aaf6571b0c",
    "d0781d50f9d17d9ae1a4de0d121a4374ed50082e5e4e961761c4225feaae6f9efa987f181c2ee221aa15ac46c063a50aa14939d03a28870c5fb108d0332b360b",
    "c9a1c245105781b09d9e54915a788458e5664b193162c5813f187c967080ce1f7a28584c8d69c9eb23539c7924f3dbee10f24419a4433c3656a3ce51a8995f0d",
    "178555d7bf05be07984dcb53c11685dd9ce07aea42908cada0cf7568e1d8607a4e1247c98ea4fbafedeeae211b80833a8ba74abbe12d40f14c2494a1919ffc0d",
    "12c3159e45bf94aa95e2f0c0dcf29abe876c3c97d608907d1304aa77ab2846732c008aecf23779605ed2b291378730b4c6d31242d6aff18a9b6b41ec4a03870f",
]
SEGMENT_BYTES = 4096


def synth_bytes(camera, seq, scene, variant):
    out, block = b"", 0
    while len(out) < SEGMENT_BYTES:
        seed = f"sthiranet/surveillance/v1/synthetic|{camera}|{seq}|{scene}|{variant}|{block}"
        out += hashlib.sha256(seed.encode()).digest()
        block += 1
    return out[:SEGMENT_BYTES]


def segment(seq):
    data = synth_bytes(CAMERA["id"], seq, "empty_aisle", "capture")
    s = (seq - 1) * 10
    return {
        "v": 1, "profile": "surveillance/v1", "chain": CAMERA["id"], "camera": CAMERA, "seq": seq,
        "start": f"2026-11-10T08:00:{s:02d}.000000Z",
        "end": f"2026-11-10T08:{(s + 10) // 60:02d}:{(s + 10) % 60:02d}.000000Z",
        "media": {"codec": "h265", "fps": 25, "frames": 250, "bytes": len(data)},
        "content_hash": hashlib.sha256(data).hexdigest(),
        "device_sig": DEVICE_SIGS[seq - 1],
    }


def surveillance_vectors():
    records = [segment(seq) for seq in range(1, 7)]
    salts = [f"{0xA0 + i:02x}" * 32 for i in range(6)]
    leaves = [leaf(s, r) for s, r in zip(salts, records)]
    root = mth(leaves)
    g = genesis(CAMERA["id"])
    meta = {"chain": CAMERA["id"], "seq": 1, "from_pos": f"{1:010d}", "to_pos": f"{6:010d}",
            "from_time": "2026-11-10T08:00:00.000000Z", "to_time": "2026-11-10T08:01:00.000000Z", "event_count": 6}
    print(json.dumps({
        "spec": "sthiranet surveillance/v1 on hashing v1",
        "device": {"seed": DEVICE_SEED, "public_key": DEVICE_PUBLIC_KEY, "start_ms": 1794297600000},
        "segments": [
            {"scene": "empty_aisle", "salt": s, "record": r,
             "signing_payload": jcs({k: v for k, v in r.items() if k != "device_sig"}),
             "canonical": jcs(r), "leaf": l}
            for s, r, l in zip(salts, records, leaves)
        ],
        "batch": {"leaves": leaves, "root": root, "proofs": [{"index": i, "path": path(leaves, i)} for i in range(6)]},
        "head": {"prev_head": g, "root": root, "meta": meta, "canonical_meta": jcs(meta), "head": head(g, root, meta)},
    }, indent=2, ensure_ascii=False))


if __name__ == "__main__":
    surveillance_vectors() if sys.argv[1:] == ["surveillance"] else hashing_vectors()
