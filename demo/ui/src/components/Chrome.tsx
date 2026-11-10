// Top bar and network strip: always on screen.
import { useEffect, useRef, useState } from 'react';
import type { CounterpartyRole, StateView } from '../api/client';
import { hms, plural } from '../ui';

export function TopBar(props: {
  s: StateView;
  onAuto: (on: boolean) => void;
  onRole: (role: CounterpartyRole) => void;
  onReset: () => void;
}) {
  const { s } = props;
  const [armed, setArmed] = useState(false);
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const reset = () => {
    if (!armed) {
      setArmed(true);
      timer.current = window.setTimeout(() => setArmed(false), 3000);
      return;
    }
    window.clearTimeout(timer.current);
    setArmed(false);
    props.onReset();
  };
  return (
    <header className="topbar">
      <div className="brand">
        <span className="logo">◆</span>
        <span className="brand-name">Sthiranet</span>
        <span className="brand-sub">Warehouse video integrity · live demo</span>
      </div>
      <div className="top-actions">
        <div className="clock" title="Simulated time: it advances 10 seconds for every recorded segment">
          <span className="clock-k">Demo time</span>
          <span className="clock-v">{hms(s.clock.now)}</span>
        </div>
        <label className="switch" title="Every camera records a 10-second segment each second (×10); each minute is sealed and anchored">
          <input type="checkbox" checked={s.auto} onChange={(e) => props.onAuto(e.target.checked)} />
          <span className="slider"></span>
          <span>Auto-record ×10</span>
        </label>
        <label className="role" title="Who co-signs every anchor. Changing it rebuilds the network.">
          <span>Counterparty</span>
          <select value={s.counterparty.role} onChange={(e) => props.onRole(e.target.value as CounterpartyRole)}>
            {s.roles.map((r) => (
              <option key={r.role} value={r.role}>
                {r.name}
              </option>
            ))}
          </select>
        </label>
        <button className="btn btn-ghost" onClick={reset}>
          {armed ? 'Click again to reset' : 'Reset demo'}
        </button>
      </div>
    </header>
  );
}

export function NetworkStrip({ s }: { s: StateView }) {
  const stored = s.cameras.reduce((n, c) => n + c.stored, 0);
  const minutes = s.cameras[0]?.lastSeq ?? 0;
  const v = s.lastVerify;
  const x = s.export;
  return (
    <section className="network" aria-label="Network">
      <div className="node">
        <div className="node-title">
          <span className="node-name">
            3PL warehouse <small>{s.cameras[0]?.site}</small>
          </span>
          <span className="tag">submits</span>
        </div>
        <div className="node-line">
          <span className="k">Cameras</span> {s.cameras.map((c) => c.id.replace('CAM-', '')).join(' · ')}
        </div>
        <div className="node-line">
          <span className="k">Recorder</span> {plural(stored, 'segment')} of video. <b>It never leaves.</b>
        </div>
      </div>
      <div className="link">
        <div className="sync-pill">Private synchronizer</div>
        <div className="bar"></div>
        <div>hashes only · {plural(s.envelopes, 'encrypted envelope')}</div>
      </div>
      <div className="node witness">
        <div className="node-title">
          <span className="node-name">
            {s.counterparty.name} <small>counterparty</small>
          </span>
          <span className="tag">co-signs</span>
        </div>
        <div className="node-line">
          <span className="k">Witnessed</span> {plural(minutes, 'minute')} per camera, as captured
        </div>
        <div className="node-line">
          <span className="k">Holds</span> its own ledger copy. No video.
        </div>
      </div>
      <div className="link">
        <div className="sync-pill">{s.vendor.member ? 'Member' : 'Evidence package'}</div>
        <div className={`bar ${s.vendor.member ? '' : 'dashed'}`}></div>
        <div>{s.vendor.member ? 'co-signs new anchors' : 'records + proofs, on request'}</div>
      </div>
      <div className={`node ${s.vendor.member ? 'witness' : 'outside'}`}>
        <div className="node-title">
          <span className="node-name">
            Vendor <small>goods owner</small>
          </span>
          <span className="tag">{s.vendor.member ? 'co-signs' : 'verifies'}</span>
        </div>
        <div className="node-line">{s.vendor.description}</div>
        <div className="node-line">
          <span className="k">Package</span>{' '}
          {x ? `${plural(x.segments, 'record')} · ${Math.ceil(x.packageBytes / 1024)} KB of hashes` : 'none exported yet'}
        </div>
      </div>
      <div className={`last-verify ${v ? (v.ok ? 'ok' : 'bad') : ''}`}>
        <span className="k">Last verification</span>
        <b>{v ? (v.ok ? '✓ Trusted' : '✗ Not trusted') : '—'}</b>
        <span className="muted small">{v ? `${v.verifier === 'CPTY' ? s.counterparty.name : 'Vendor'} · ${hms(v.at)}` : 'not verified yet'}</span>
      </div>
    </section>
  );
}
