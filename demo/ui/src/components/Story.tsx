// The scene rail: the eight scenes of the story, each with what it proves and the action that plays it.
import { useState, type ReactNode } from 'react';
import type { ArchiveAttempt, StateView, TamperMode, Verifier, VerifyResponse } from '../api/client';
import { MINUTE, STORY_END_MINUTES, WINDOW_MINUTES, type Slot } from '../model';
import { hm, hms, partyName, plural } from '../ui';

export type Actions = {
  recordMinute: () => void;
  raiseIncident: (from: number, to: number) => void;
  exportPackage: () => void;
  verify: (v: Verifier) => void;
  tamper: (mode: TamperMode, cover: boolean) => void;
  rewrite: () => void;
  addVendor: () => void;
};

type Ctx = {
  s: StateView;
  /** The slot the next tamper action hits. */
  picked?: Slot;
  result: VerifyResponse | null;
  rewriteResult: ArchiveAttempt | null;
  busy: boolean;
  a: Actions;
};

/** The three things a signed camera alone cannot do: what the demo exists to show. */
const VALUE = {
  hashes: 'Shared verification without sharing footage',
  witness: 'A counterparty witness at capture time',
  complete: 'Completeness: what is missing becomes visible',
};

const SCENES: { title: string; value?: string }[] = [
  { title: 'Live network', value: VALUE.hashes },
  { title: 'Incident' },
  { title: 'Clean verification', value: VALUE.hashes },
  { title: 'Modification' },
  { title: 'Deletion', value: VALUE.complete },
  { title: 'Covering tracks', value: VALUE.witness },
  { title: 'Rewriting the ledger', value: VALUE.witness },
  { title: 'Outcome' },
];

function VerifyButtons({ c }: { c: Ctx }) {
  const off = c.busy || !c.s.incident;
  return (
    <div className="btn-col">
      <button className="btn btn-primary" disabled={off} onClick={() => c.a.verify('CPTY')}>
        Verify as the {c.s.counterparty.name.toLowerCase()}
      </button>
      <button className="btn" disabled={off} onClick={() => c.a.verify('VENDOR')}>
        Verify as the vendor <small>(evidence package)</small>
      </button>
      {!c.s.incident && <p className="hint">Raise the incident in scene 2 first.</p>}
    </div>
  );
}

function Target({ c }: { c: Ctx }) {
  const p = c.picked;
  if (!p) return null;
  return (
    <p className="target">
      Target: <b>{p.cam}</b> · <b>{hms(p.start)}</b>
      {p.seg ? '' : ' · nothing stored in this slot'}
      <span className="hint"> (pick another segment on the right)</span>
    </p>
  );
}

function Attack({ c, label, mode, cover = false }: { c: Ctx; label: string; mode: TamperMode; cover?: boolean }) {
  return (
    <button className="btn btn-attack" disabled={c.busy || !c.picked?.seg?.batchSeq} onClick={() => c.a.tamper(mode, cover)}>
      {label}
    </button>
  );
}

function IncidentForm({ c }: { c: Ctx }) {
  const start = Date.parse(`${c.s.clock.start.slice(0, 23)}Z`);
  const sealed = Date.parse(`${c.s.clock.sealedTo.slice(0, 23)}Z`);
  // Default: the ten minutes of the delivery, 08:20 to 08:30.
  const [to, setTo] = useState(Math.min(start + STORY_END_MINUTES * MINUTE, sealed));
  const [from, setFrom] = useState(Math.max(start, Math.min(start + STORY_END_MINUTES * MINUTE, sealed) - WINDOW_MINUTES * MINUTE));
  const marks: number[] = [];
  for (let t = start; t <= sealed; t += 5 * MINUTE) marks.push(t);
  return (
    <>
      <div className="form-row">
        <label>
          From
          <select
            value={from}
            onChange={(e) => {
              const v = Number(e.target.value);
              setFrom(v);
              if (to <= v) setTo(Math.min(v + 5 * MINUTE, sealed));
            }}
          >
            {marks.slice(0, -1).map((t) => (
              <option key={t} value={t}>
                {hm(t)}
              </option>
            ))}
          </select>
        </label>
        <label>
          To
          <select value={to} onChange={(e) => setTo(Number(e.target.value))}>
            {marks.filter((t) => t > from).map((t) => (
              <option key={t} value={t}>
                {hm(t)}
              </option>
            ))}
          </select>
        </label>
      </div>
      <button className="btn btn-primary" disabled={c.busy || to <= from} onClick={() => c.a.raiseIncident(from, to)}>
        {c.s.incident ? 'Raise it again with this window' : 'Raise the incident'}
      </button>
    </>
  );
}

function body(n: number, c: Ctx): ReactNode {
  const { s } = c;
  const cp = s.counterparty.name.toLowerCase();
  switch (n) {
    case 1:
      return (
        <>
          <p>
            Three cameras at {s.cameras[0]?.site} record around the clock. Each 10-second segment is fingerprinted and
            signed by its camera. Every minute, the six fingerprints are sealed into one Merkle root and anchored on the
            ledger, co-signed by the 3PL and the {cp}.
          </p>
          <p>The video stays on the 3PL's own recorder. Only hashes travel.</p>
          <button className="btn btn-primary" disabled={c.busy || s.auto} onClick={c.a.recordMinute}>
            Record the next minute
          </button>
          <p className="hint">Or switch on Auto-record in the top bar.</p>
        </>
      );
    case 2:
      return (
        <>
          <p>
            A vendor's consignment <b>C-1047</b> reached the warehouse. Later, cartons are found missing. Everyone turns
            to the CCTV footage on the 3PL's servers.
          </p>
          <p>
            But the 3PL controls those servers. <b>Is the footage genuine, and is it all there?</b>
          </p>
          <IncidentForm c={c} />
        </>
      );
    case 3:
      return (
        <>
          <p>
            The {cp} re-hashes the stored footage and compares it with the anchors on <b>its own ledger copy</b>, which
            it co-signed minute by minute, long before any dispute.
          </p>
          <p>
            The vendor has no node. It receives an evidence package (records, proofs and anchors) and checks the anchors
            with the {cp}.
          </p>
          <button className="btn" disabled={c.busy || !s.incident} onClick={c.a.exportPackage}>
            3PL exports the evidence package
          </button>
          {s.export && (
            <p className="hint">
              Package: {plural(s.export.segments, 'record')} with proofs, {Math.ceil(s.export.packageBytes / 1024)} KB of
              hashes and signatures.
            </p>
          )}
          <VerifyButtons c={c} />
        </>
      );
    case 4:
      return (
        <>
          <p>
            Someone with access to the recorder replaces ten seconds of video: a splice, or a loop of earlier footage.
          </p>
          <Target c={c} />
          <div className="btn-col">
            <Attack c={c} label="Splice in different video" mode="modify" />
            <Attack c={c} label="Loop earlier footage over it" mode="replay" />
          </div>
          <p>Verification pinpoints the exact segment. The rest of the footage still stands.</p>
          <VerifyButtons c={c} />
        </>
      );
    case 5:
      return (
        <>
          <p>
            The insider deletes a segment, or a whole minute. A signed camera cannot show what is missing: the owner
            holds the only record.
          </p>
          <Target c={c} />
          <div className="btn-col">
            <Attack c={c} label="Delete this segment" mode="delete" />
            <Attack c={c} label="Delete the whole minute" mode="delete_minute" />
          </div>
          <p>
            The anchor says <b>six segments existed</b> in that minute. Fewer are present, so the gap shows.
          </p>
          <VerifyButtons c={c} />
        </>
      );
    case 6:
      return (
        <>
          <p>
            The insider now uses <b>the camera's own key</b>: the altered segment is re-hashed and re-signed, so its
            signature is valid again.
          </p>
          <Target c={c} />
          <div className="btn-col">
            <Attack c={c} label="Splice and re-sign with the camera key" mode="modify" cover />
            <Attack c={c} label="Delete and renumber around the gap" mode="delete" cover />
          </div>
          <p>
            It still fails. The Merkle root for that minute was co-signed by the {cp} at capture, and the insider cannot
            change that.
          </p>
          <VerifyButtons c={c} />
        </>
      );
    case 7: {
      const r = c.rewriteResult;
      return (
        <>
          <p>Last resort: the 3PL tries to remove the anchor itself from the ledger.</p>
          <Target c={c} />
          <button className="btn btn-attack" disabled={c.busy || !c.picked} onClick={c.a.rewrite}>
            3PL tries to archive the anchor
          </button>
          {r && (
            <div className="reject">
              <b>✗ Rejected by the ledger</b>
              <div>
                Needs: {r.required.map((p) => partyName(s, p)).join(' + ') || 'a stakeholder'}
                <br />
                Signed: {r.signed.map((p) => partyName(s, p)).join(', ')}
                <br />
                Missing: <b>{r.missing.map((p) => partyName(s, p)).join(', ') || '—'}</b>
              </div>
            </div>
          )}
          <p>Every anchor is co-signed. No single party can alter or remove one.</p>
        </>
      );
    }
    default: {
      const v = s.lastVerify;
      return (
        <>
          <p>
            The network answers one question: <b>can this footage be trusted?</b> It does not decide who is at fault.
            Footage that every party accepts does that.
          </p>
          {v && (
            <div className={`reject ${v.ok ? 'good' : ''}`}>
              <b>{v.ok ? '✓ Trusted' : '✗ Not trusted'}</b>
              <div>{v.verdict}</div>
            </div>
          )}
          <ul className="values">
            <li>
              <b>{VALUE.complete}.</b> A deleted segment or minute shows as a gap against the ledger.
            </li>
            <li>
              <b>{VALUE.witness}.</b> The {cp} recorded each minute before any dispute existed; re-signed footage still
              fails.
            </li>
            <li>
              <b>{VALUE.hashes}.</b> The vendor verified from hashes and proofs. The video never left the 3PL.
            </li>
          </ul>
          <VerifyButtons c={c} />
          <button className="btn" disabled={c.busy || s.vendor.member} onClick={c.a.addVendor}>
            {s.vendor.member ? 'Vendor is a member' : 'Optional: add the vendor as a member'}
          </button>
        </>
      );
    }
  }
}

export function SceneRail(props: Ctx & { scene: number; setScene: (n: number) => void }) {
  const { scene, setScene, ...c } = props;
  return (
    <aside className="rail" aria-label="Scenes">
      <ol>
        {SCENES.map((sc, i) => {
          const n = i + 1;
          const open = n === scene;
          return (
            <li key={n} className={`${open ? 'open' : ''} ${n < scene ? 'done' : ''}`}>
              <button className="scene-head" onClick={() => setScene(n)} aria-expanded={open}>
                <b>{n}</b>
                <span>{sc.title}</span>
              </button>
              {open && (
                <div className="scene-body">
                  {sc.value && <div className="proves">Shows: {sc.value}</div>}
                  {body(n, c)}
                  {n < SCENES.length && (
                    <button className="next" onClick={() => setScene(n + 1)}>
                      Next: {SCENES[n].title} →
                    </button>
                  )}
                </div>
              )}
            </li>
          );
        })}
      </ol>
    </aside>
  );
}
