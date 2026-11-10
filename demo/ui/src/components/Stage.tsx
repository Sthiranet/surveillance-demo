// The stage: the verdict, the three camera timelines, and one minute zoomed in to its six segments.
import type { StateView, VerifyResponse } from '../api/client';
import { type Slot, type SlotRef, type Window, isBad } from '../model';
import { Hash, STATUS_LABEL, hm, hms, plural } from '../ui';
import { Pictogram, SCENE_LABEL } from './Pictogram';

export function Verdict({ s, result, verifying }: { s: StateView; result: VerifyResponse | null; verifying: string | null }) {
  if (verifying) {
    return (
      <div className="banner busy">
        <div className="icon spin">⟳</div>
        <div>
          <div className="title">{verifying} is verifying…</div>
          <div className="small">Re-hashing every segment in the window and comparing it with the anchors co-signed at capture.</div>
        </div>
      </div>
    );
  }
  if (!result) {
    const inc = s.incident;
    return (
      <div className="banner idle">
        <div className="icon">◷</div>
        <div>
          <div className="title">
            {inc ? `Incident ${inc.id} · consignment ${inc.consignment} · ${hm(inc.from)} to ${hm(inc.to)}` : 'Live network'}
          </div>
          <div className="small">
            {inc
              ? 'The footage in this window is in question. It has not been verified yet.'
              : 'Every 10-second segment is hashed and signed by its camera. Every minute is anchored and co-signed.'}
          </div>
        </div>
      </div>
    );
  }
  const how =
    result.method === 'ledger_copy'
      ? `${result.verifierName} checked the stored footage against its own ledger copy`
      : result.anchorsFrom === 'VENDOR'
        ? 'The vendor checked the evidence package against its own ledger copy'
        : `The vendor checked the evidence package; the anchors were confirmed by the ${s.counterparty.name.toLowerCase()}`;
  return (
    <div className={`banner ${result.ok ? 'ok' : 'bad'}`}>
      <div className="icon">{result.ok ? '✓' : '✗'}</div>
      <div>
        <div className="title">{result.ok ? 'This footage can be relied on' : 'This footage cannot be relied on'}</div>
        <div className="verdict-text">{result.verdict}</div>
        <div className="small muted">
          {how} · {plural(result.segmentsChecked, 'segment')} · {hms(result.at)}
        </div>
      </div>
    </div>
  );
}

const minuteState = (slots: Slot[]) => (slots.some((x) => isBad(x.state)) ? 'bad' : slots.every((x) => x.state === 'ok') ? 'ok' : '');

export function Timelines(props: {
  s: StateView;
  win: Window;
  grid: Map<string, Map<number, Slot[]>>;
  result: VerifyResponse | null;
  target: SlotRef;
  onPick: (ref: SlotRef) => void;
}) {
  const { s, win, grid, result, target } = props;
  const start = Date.parse(`${s.clock.start.slice(0, 23)}Z`);
  return (
    <div className="timelines">
      <div className="tl-head">
        <h3>
          Footage on the 3PL's recorder · {hm(win.from)} to {hm(win.to)}
        </h3>
        <div className="legend">
          {result ? (
            <>
              <span><i className="sw ok" /> genuine</span>
              <span><i className="sw modified" /> modified</span>
              <span><i className="sw replayed" /> looped</span>
              <span><i className="sw missing" /> missing</span>
            </>
          ) : (
            <>
              <span><i className="sw stored" /> stored, not yet verified</span>
              <span><i className="sw gap" /> nothing stored</span>
            </>
          )}
        </div>
      </div>
      {s.cameras.map((cam) => {
        const cv = result?.cams.find((c) => c.cam === cam.id);
        return (
          <div className="tl-row" key={cam.id}>
            <div className="tl-cam">
              <b>{cam.name}</b>
              <span className="mono small">{cam.id}</span>
              {cv && <span className={`pill ${cv.ok ? 'ok' : 'bad'}`}>{cv.ok ? '✓ genuine & complete' : '✗ not trustworthy'}</span>}
            </div>
            <div className="tl-track" style={{ gridTemplateColumns: `repeat(${win.minutes.length}, minmax(0, 1fr))` }}>
              {win.minutes.map((m) => {
                const slots = grid.get(cam.id)?.get(m) ?? [];
                const sel = target.cam === cam.id && target.minute === m;
                const t = start + m * 60_000;
                return (
                  <button
                    key={m}
                    className={`minute ${minuteState(slots)} ${sel ? 'selected' : ''}`}
                    title={`${cam.id} · ${hm(t)}`}
                    onClick={() => props.onPick({ cam: cam.id, minute: m, index: sel ? target.index : 0 })}
                  >
                    {slots.map((x) => (
                      <i key={x.index} className={`sliver ${x.state}`} />
                    ))}
                  </button>
                );
              })}
            </div>
          </div>
        );
      })}
      <div className="tl-row axis">
        <div className="tl-cam"></div>
        <div className="tl-track" style={{ gridTemplateColumns: `repeat(${win.minutes.length}, minmax(0, 1fr))` }}>
          {win.minutes.map((m, i) => (
            <span key={m} className="tick">
              {i % 5 === 0 ? hm(start + m * 60_000) : ''}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}

export function Zoom(props: { s: StateView; slots: Slot[]; target: SlotRef; onPick: (ref: SlotRef) => void }) {
  const { s, slots, target } = props;
  const cam = s.cameras.find((c) => c.id === target.cam);
  const first = slots[0];
  const picked = slots.find((x) => x.index === target.index);
  return (
    <div className="zoom">
      <div className="tl-head">
        <h3>
          {cam?.name} · minute {first ? hm(first.start) : ''} · six 10-second segments
        </h3>
        <span className="muted small">Click a segment to select it</span>
      </div>
      <div className="tiles">
        {slots.map((x) => {
          const scene = x.verdict ? x.verdict.scene : x.seg?.scene;
          const hash = x.verdict?.recordedHash ?? x.seg?.contentHash;
          const empty = x.state === 'missing' || x.state === 'gap' || x.state === 'future';
          return (
            <button
              key={x.index}
              className={`tile ${x.state} ${x.index === target.index ? 'selected' : ''}`}
              onClick={() => props.onPick({ cam: x.cam, minute: x.minute, index: x.index })}
            >
              <div className="tile-time">{hms(x.start)}</div>
              <div className="tile-art">{empty ? <span className="tile-void">{x.state === 'future' ? '…' : '∅'}</span> : <Pictogram scene={scene} />}</div>
              <div className="tile-scene">
                {x.state === 'future' ? 'not recorded yet' : empty ? 'nothing in storage' : scene ? SCENE_LABEL[scene] : 'no video'}
              </div>
              <div className="tile-meta">
                {x.seg ? `#${x.seg.seq}` : x.verdict ? `#${x.verdict.seq}` : ''} <Hash h={hash} n={6} />
              </div>
              <div className="tile-status">
                {x.verdict ? (
                  <span className={`pill ${x.state === 'ok' ? 'ok' : 'bad'}`}>
                    {x.state === 'ok' ? '✓' : '✗'} {STATUS_LABEL[x.verdict.status]}
                  </span>
                ) : x.seg ? (
                  <span className="pill plain">{x.seg.batchSeq ? '✎ signed · anchored' : '✎ signed · pending'}</span>
                ) : (
                  <span className="pill plain">—</span>
                )}
              </div>
            </button>
          );
        })}
      </div>
      {picked?.verdict?.note && (
        <div className="explain bad">
          <b>
            {hms(picked.start)} · {STATUS_LABEL[picked.verdict.status]}.
          </b>{' '}
          {picked.verdict.note}
        </div>
      )}
    </div>
  );
}
