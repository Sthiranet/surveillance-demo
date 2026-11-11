# Sthiranet · Warehouse Video Integrity Demo

A small web app for live presentations. A 3PL warehouse's CCTV footage is fingerprinted per 10-second segment,
sealed per minute into a Merkle root, and anchored on a ledger co-signed by a counterparty. When a shortage on a
consignment is disputed, every party can check whether the footage on the 3PL's own servers is **genuine and
complete**. 

It shows the three things a signed camera alone cannot do:

| Value proposition | Where the demo shows it |
| --- | --- |
| **Completeness**: what is missing becomes visible | Scene 5: a deleted segment or minute is a gap against the ledger |
| **A counterparty witness at capture time** | Scenes 6 and 7: re-signed footage still fails; the 3PL cannot rewrite the ledger alone |
| **Shared verification without sharing footage** | Scenes 1 and 3: only hashes travel; the vendor verifies from an evidence package |

The demo estabilishes integrity of the video, whether it can be trusted or not.

## The cast

| Party | Id | Role |
| --- | --- | --- |
| 3PL warehouse operator | `WH3PL` | Owns three cameras (`CAM-DOCK`, `CAM-CAGE`, `CAM-DISPATCH`) and the recorder. Submits every anchor. |
| Counterparty | `CPTY` | Co-signs every anchor at capture. Selectable role: the 3PL's insurer, the camera provider, or Sthiranet. |
| Vendor (goods owner) | `VENDOR` | Not on the ledger. Verifies the incident window from an evidence package. Can be promoted to member. |

## Run it

Requires Rust (stable, 1.94 or newer) and Node.js 22.12 or newer (only to build the UI).

```
make install         # once: UI dependencies
make start           # builds the UI and the server, serves http://localhost:3000
```

Use a different port with `PORT=4000 make start`. For UI work, `make dev` runs the API on :3000 and Vite
with hot reload on http://localhost:5173.

The demo opens on a live network at demo time 08:30: every minute anchored. The screen shows the last ten minutes (08:20 to 08:30), which is also the default incident window and where the delivery of consignment C-1047 happens. Time is simulated; it advances 10 seconds per recorded segment. **Reset demo** (click twice)
restores the starting point.

### Presenting

The left rail walks the eight scenes in order. Each scene has one or two buttons.

1. **Live network.** Record a minute and watch it fill, seal and anchor.
2. **Incident.** A shortage on consignment C-1047; fix the window in question.
3. **Clean verification.** By the counterparty on its own ledger copy, and by the vendor from a package.
4. **Modification.** Splice a segment, or loop earlier footage. The exact segment is pinpointed.
5. **Deletion.** Delete a segment or a whole minute. The gap shows.
6. **Covering tracks.** Re-sign with the camera's own key, or renumber around a gap. Still caught.
7. **Rewriting the ledger.** The 3PL tries to archive an anchor alone. Rejected.
8. **Outcome.** The verdict: can the footage be relied on.

Click any minute on a timeline, then any of its six segments, to choose where an attack lands.
**Show cryptographic detail** opens the Merkle tree and anchor of the selected minute, each party's ledger
copy, what the synchronizer saw, and the activity log.

## Test it

```
make test            # every Rust test, plus the UI type-check
make check           # rustfmt, clippy with -D warnings, UI type-check
```

| Layer | What is tested |
| --- | --- |
| Kernel (`sthira-core`, `sthira-ledger`) | RFC 6962 and RFC 8785 reference tests, `hashing-v1.json` vectors, Merkle proof property tests, ledger co-signing rules |
| Profile (`sthira-profile-surveillance`) | `surveillance-v1.json` vectors (records, signatures, leaves, root, head) and the tamper matrix: every attack is detected, untouched storage is clean, property-tested over random segments, bytes and positions |
| Node and verify (`sthira-node`, `sthira-verify`) | Seal and anchor, empty minute, trail verification, evidence packages (any altered record, proof, anchor or missing item fails) |
| App (`surveillance-director`) | `the_story`: scenes 1–8 end to end; HTTP smoke tests; error statuses; OpenAPI route list |

`make vectors` regenerates both vector files with the independent Python implementation in `test-vectors/`.

## How the code is laid out

The layers follow Sthiranet's architecture: a kernel that knows nothing about any industry, a connector
(profile) per industry, and a node layer between them. Crates talk through traits, so a new industry is a
new profile crate and a new demo server; the kernel and node crates are reused unchanged.

```
crates/
  sthira-core                  kernel     hashing spec v1, Merkle trees, and the Record / Describe traits
  sthira-ledger                kernel     AnchorLedger trait + SimLedger (in-memory stand-in for Canton)
  sthira-node                  node       ChainNode<R: Record>: pending → seal → anchor, off-ledger store
  sthira-verify                node       verify_trail and evidence packages, generic over R
  sthira-profile-surveillance  connector  SegmentRecord, Camera (Ed25519), footage, checks, tamper
demo/
  surveillance-director        server     the only crate that names concrete types from every layer
  ui                           React UI   typed from the server's OpenAPI spec
```

`cargo tree -p sthira-node -p sthira-verify` shows no profile crate: the node layer sees a record only as
something that implements `Record` and `Describe`.

This repository was built in conjuction with the audit trail demo. What changed in the shared crates: the finance
`Event` left the kernel, `SubsidiaryNode` became `ChainNode<R>`, `verify_trail` became generic, chains are
created from a `ChainSpec`, and evidence packages were added to `sthira-verify`.

## Hosting (free hosting on Render)

The repo carries a `Dockerfile` (UI + server in one small image) and a `render.yaml` Blueprint for a free
Render web service.

1. In Render: **New → Blueprint**, pick the repository. Render reads `render.yaml` and creates the
   `sthiranet-surveillance-demo` service on the free plan.
2. Share the `https://sthiranet-surveillance-demo-….onrender.com` link once the first deploy finishes.

To try the image locally:
`docker build -t sthiranet-surveillance-demo . && docker run --rm -p 3000:3000 sthiranet-surveillance-demo`.

### Cleaning up

| Command | What it removes |
| --- | --- |
| `make stop` | Stops a running demo server. All demo state lives only in that process's memory. Nothing is written to disk. |
| `make clean` | `make stop`, then every build output: `target/`, the built UI, Vite's cache and the generated `openapi.json`. |
| `make distclean` | `make clean`, plus `demo/ui/node_modules`. Run `make install` before the next build. |

## What is simplified

- **No real Canton.** The ledger is an in-memory simulation of the behaviour the story relies on: each
  party keeps its own copy, every member co-signs, a fork or skipped sequence is refused, archiving needs
  every signatory, and the synchronizer sees only recipients and sizes.
- **No real video.** A segment is 4 KiB of deterministic bytes plus a scene tag for the pictogram. The
  camera key lives in memory, not in a secure element. No C2PA manifests or ONVIF Media Signing streams.
- **The cryptography is real.** SHA-256 content hashes, Ed25519 device signatures, RFC 8785 canonical JSON,
  32-byte salts, RFC 6962 Merkle trees and inclusion proofs, chained heads (hashing spec v1).
- **The vendor's evidence** is the package (records, salts, proofs, anchors) plus the clips of the incident
  window only. Outside a dispute nothing but hashes leaves the 3PL.
- **No Memory Preservence** State is in memory and resets when the server restarts.
