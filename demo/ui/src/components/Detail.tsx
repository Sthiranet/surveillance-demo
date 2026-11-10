// "Show cryptographic detail": the Merkle tree and anchor of the selected minute, each party's
// ledger copy, what the synchronizer saw, and the activity log.
import { useQuery } from '@tanstack/react-query';
import { useState } from 'react';
import { api, keys, type BatchVerdict, type StateView } from '../api/client';
import type { SlotRef } from '../model';
import { AnchorCard, Hash, Sigs, hm, hms, partyName, plural } from '../ui';
import { MerkleTree } from './MerkleTree';

type Tab = 'minute' | 'ledger' | 'activity';

function Checked({ s, b }: { s: StateView; b: BatchVerdict }) {
  const row = (ok: boolean, text: string) => (
    <li className={ok ? 'ok' : 'bad'}>
      <span>{ok ? '✓' : '✗'}</span> {text}
    </li>
  );
  return (
    <>
      <h4>Recomputed from the 3PL's storage</h4>
      <MerkleTree tree={b.tree} labels={b.leafLabels} bad={new Set(b.badLeaves)} rootBad={!b.rootMatch} />
      <div className="kv">
        <span className="k">anchored root</span>
        <span className="v">
          <Hash h={b.anchoredRoot} cls="accent" n={20} />
        </span>
        <span className="k">recomputed</span>
        <span className="v">
          <Hash h={b.recomputedRoot} cls={b.rootMatch ? 'good' : 'bad'} n={20} />
        </span>
        <span className="k">segments</span>
        <span className="v">
          {b.presentCount} present of {b.expectedCount} anchored
        </span>
        <span className="k">co-signed by</span>
        <span className="v">
          <Sigs s={s} list={b.signatories} />
        </span>
      </div>
      <ul className="checks">
        {row(b.countMatch, `Every anchored segment is present (${b.presentCount} of ${b.expectedCount})`)}
        {row(b.rootMatch, 'The records hash to the Merkle root that was co-signed at capture')}
        {row(b.badLeaves.length === 0, 'Every stored video matches its signed record')}
        {row(b.headMatch, 'The chain of heads up to this minute matches the ledger')}
      </ul>
    </>
  );
}

function Minute({ s, target, verdict }: { s: StateView; target: SlotRef; verdict?: BatchVerdict }) {
  const seq = target.minute + 1;
  const q = useQuery({ queryKey: keys.batch(target.cam, seq), queryFn: () => api.batch(target.cam, seq), retry: false });
  const d = q.data;
  return (
    <div className="detail-body">
      <p className="lead">
        <b>{target.cam}</b> · minute #{seq}. Six segment records are each hashed into a leaf; the leaves fold into one
        root; only that root goes on the ledger.
      </p>
      {verdict ? (
        <Checked s={s} b={verdict} />
      ) : d ? (
        <>
          <h4>As sealed by the 3PL's node</h4>
          <MerkleTree tree={d.batch.tree} labels={d.batch.items.map((i) => `#${Number(i.pos)}`)} />
        </>
      ) : (
        <div className="empty">{q.isError ? 'This minute is not sealed yet.' : 'Loading…'}</div>
      )}
      {d && (
        <>
          <h4>On the ledger</h4>
          <AnchorCard s={s} a={d.anchor} />
        </>
      )}
      <div className="formulae mono small">
        leaf = H(0x00 ‖ salt ‖ record) · node = H(0x01 ‖ left ‖ right)
        <br />
        head = H(0x02 ‖ prev_head ‖ root ‖ meta)
      </div>
    </div>
  );
}

function Ledger({ s, cam }: { s: StateView; cam: string }) {
  const [party, setParty] = useState('CPTY');
  const q = useQuery({ queryKey: keys.ledger(party), queryFn: () => api.ledger(party) });
  const chain = q.data?.chains.find((c) => c.chainId === cam);
  return (
    <div className="detail-body">
      <div className="chips">
        <span className="label">Ledger copy of</span>
        {['WH3PL', 'CPTY', 'VENDOR'].map((p) => (
          <button key={p} className={`chip ${party === p ? 'active' : ''}`} onClick={() => setParty(p)}>
            {partyName(s, p)}
          </button>
        ))}
      </div>
      {!q.data ? (
        <div className="empty">Loading…</div>
      ) : !chain ? (
        <div className="empty">
          {partyName(s, party)}'s node holds no copy of {cam}: it is not a stakeholder, so it cannot even see these
          anchors.
        </div>
      ) : (
        <>
          <p className="lead">
            {partyName(s, party)}'s own copy of chain <b>{cam}</b>: {plural(chain.anchors.length, 'anchor')}, each signed
            by {chain.members.map((m) => partyName(s, m)).join(' + ')}. Camera key agreed at genesis:{' '}
            <Hash h={chain.attributes.device_key} n={12} />
          </p>
          <div className="blocks">
            {chain.anchors
              .slice(-6)
              .reverse()
              .map((a) => (
                <div className="block" key={a.seq}>
                  <div className="block-title">
                    <span>#{a.seq}</span>
                    <span>
                      {hm(a.fromTime)}–{hm(a.toTime)}
                    </span>
                  </div>
                  <div className="row">
                    <span className="k">root</span> <Hash h={a.merkleRoot} cls="accent" />
                  </div>
                  <div className="row">
                    <span className="k">head</span> <Hash h={a.head} />
                  </div>
                  <div className="row">
                    <span className="k">segments</span> {a.eventCount}
                  </div>
                </div>
              ))}
          </div>
        </>
      )}
      <h4>What the synchronizer saw</h4>
      <p className="lead">Recipients and sizes of encrypted envelopes. Never a hash, never a frame.</p>
      <table className="grid">
        <thead>
          <tr>
            <th>Update</th>
            <th>Recipients</th>
            <th>Bytes</th>
            <th>Ciphertext</th>
          </tr>
        </thead>
        <tbody>
          {(q.data?.synchronizer ?? []).slice(0, 8).map((e) => (
            <tr key={e.updateId}>
              <td className="mono">{e.updateId}</td>
              <td>{e.recipients.map((p) => partyName(s, p)).join(', ')}</td>
              <td>{e.bytes}</td>
              <td className="mono">{e.ciphertext}…</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ActivityLog({ s }: { s: StateView }) {
  return (
    <ol className="activity">
      {s.activity.map((x, i) => (
        <li key={`${x.at}-${i}`} className={x.level}>
          <div className="meta">
            <span className="who">{partyName(s, x.actor)}</span>
            <span>{hms(x.at)}</span>
          </div>
          {x.text}
        </li>
      ))}
    </ol>
  );
}

export function Detail(props: { s: StateView; target: SlotRef; verdict?: BatchVerdict; onClose: () => void }) {
  const [tab, setTab] = useState<Tab>('minute');
  const tabs: [Tab, string][] = [
    ['minute', 'Selected minute'],
    ['ledger', 'Ledger copies'],
    ['activity', 'Activity'],
  ];
  return (
    <aside className="detail" aria-label="Cryptographic detail">
      <div className="detail-head">
        <h3>Cryptographic detail</h3>
        <button className="btn btn-ghost btn-sm" onClick={props.onClose}>
          Hide
        </button>
      </div>
      <nav className="tabs">
        {tabs.map(([id, label]) => (
          <button key={id} className={tab === id ? 'active' : ''} onClick={() => setTab(id)}>
            {label}
          </button>
        ))}
      </nav>
      <div className="detail-scroll">
        {tab === 'minute' && <Minute s={props.s} target={props.target} verdict={props.verdict} />}
        {tab === 'ledger' && <Ledger s={props.s} cam={props.target.cam} />}
        {tab === 'activity' && <ActivityLog s={props.s} />}
      </div>
    </aside>
  );
}
