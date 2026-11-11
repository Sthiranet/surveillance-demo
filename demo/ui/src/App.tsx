import { useQueries, useQuery, useQueryClient } from '@tanstack/react-query';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { api, keys, type ArchiveAttempt, type CounterpartyRole, type TamperMode, type Verifier, type VerifyResponse } from './api/client';
import { NetworkStrip, TopBar } from './components/Chrome';
import { Detail } from './components/Detail';
import { Timelines, Verdict, Zoom } from './components/Stage';
import { SceneRail, type Actions } from './components/Story';
import { MINUTE, byStart, isBad, slotsOf, verdictsByStart, windowOf, type Slot, type SlotRef } from './model';
import { iso, ms, useRun, useToast } from './ui';

const sleep = (t: number) => new Promise((r) => setTimeout(r, t));

/** Where each attack scene points by default, as [camera, minutes after the start, segment in that minute]. */
const SCENE_TARGET: Record<number, [string, number, number]> = {
  4: ['CAM-CAGE', 26, 1],
  5: ['CAM-DOCK', 23, 3],
  6: ['CAM-DOCK', 21, 2],
  7: ['CAM-CAGE', 26, 1],
};

/** Live updates: the server pushes the state version over SSE; any change refetches what is on screen. */
function useLiveUpdates() {
  const qc = useQueryClient();
  const [connected, setConnected] = useState(false);
  useEffect(() => {
    const es = new EventSource('/api/stream');
    let last: string | null = null;
    es.addEventListener('version', (e) => {
      setConnected(true);
      const v = (e as MessageEvent<string>).data;
      if (last !== null && v !== last) void qc.invalidateQueries();
      last = v;
    });
    es.onerror = () => setConnected(false); // the browser reconnects by itself; polling covers the gap
    return () => es.close();
  }, [qc]);
  return connected;
}

export function App() {
  const qc = useQueryClient();
  const run = useRun();
  const toast = useToast();
  const live = useLiveUpdates();
  const state = useQuery({ queryKey: keys.state, queryFn: api.state, refetchInterval: live ? false : 1500 });
  const s = state.data;
  const camIds = useMemo(() => (s?.cameras ?? []).map((c) => c.id), [s?.cameras]);
  const footage = useQueries({
    queries: camIds.map((cam) => ({ queryKey: keys.footage(cam), queryFn: () => api.footage(cam) })),
  });

  const [scene, setSceneRaw] = useState(1);
  const [target, setTarget] = useState<SlotRef>({ cam: 'CAM-DOCK', minute: 29, index: 0 });
  const [result, setResult] = useState<VerifyResponse | null>(null);
  const [verifying, setVerifying] = useState<string | null>(null);
  const [rewriteResult, setRewriteResult] = useState<ArchiveAttempt | null>(null);
  const [busy, setBusy] = useState(false);
  const [detail, setDetail] = useState(false);

  useEffect(() => {
    if (state.isError) toast('Lost connection to the demo server');
  }, [state.isError, state.errorUpdatedAt, toast]);

  const win = useMemo(() => (s ? windowOf(s) : { from: 0, to: 0, minutes: [] }), [s]);

  // cameras × minutes × slots, from what the recorder holds and (once verified) what the verifier found.
  const footageData = footage.map((q) => q.data);
  const footageStamp = footage.map((q) => q.dataUpdatedAt).join();
  const grid = useMemo(() => {
    const g = new Map<string, Map<number, Slot[]>>();
    if (!s) return g;
    camIds.forEach((cam, i) => {
      const stored = byStart(footageData[i]?.segments);
      const verdicts = verdictsByStart(result, cam);
      g.set(cam, new Map(win.minutes.map((m) => [m, slotsOf(s, cam, m, stored, verdicts)])));
    });
    return g;
    // `footageStamp` stands for `footageData`, whose array identity changes on every render.
  }, [s, win, result, camIds, footageStamp]);

  // Keep the selection inside the window on screen.
  const inWindow = win.minutes.includes(target.minute);
  const shown: SlotRef = inWindow || !win.minutes.length ? target : { ...target, minute: win.minutes[win.minutes.length - 1] };
  const slots = grid.get(shown.cam)?.get(shown.minute) ?? [];
  const picked = slots.find((x) => x.index === shown.index);

  const setScene = useCallback(
    (n: number) => {
      setSceneRaw(n);
      const t = SCENE_TARGET[n];
      if (t && win.minutes.includes(t[1])) setTarget({ cam: t[0], minute: t[1], index: t[2] });
    },
    [win.minutes],
  );

  if (!s) {
    return <div className="empty" style={{ margin: 40 }}>{state.isError ? 'Cannot reach the demo server.' : 'Loading…'}</div>;
  }

  /** Runs one presenter action; failures show as a toast and the screen refetches either way. */
  const act = async (fn: () => Promise<void>) => {
    setBusy(true);
    await run(fn);
    await qc.invalidateQueries();
    setBusy(false);
  };
  const clearFindings = () => {
    setResult(null);
    setRewriteResult(null);
  };

  const a: Actions = {
    recordMinute: () =>
      act(async () => {
        const start = ms(s.clock.start);
        const minute = Math.floor((ms(s.clock.now) - start) / MINUTE);
        if (!s.incident) setTarget((t) => ({ ...t, minute, index: 0 }));
        for (let i = 0; i < s.segmentsPerBatch; i++) {
          qc.setQueryData(keys.state, await api.record(1));
          await qc.invalidateQueries({ queryKey: ['footage'] });
          await sleep(220); // let the minute fill segment by segment
        }
      }),
    raiseIncident: (from, to) =>
      act(async () => {
        const inc = await api.incident(iso(from), iso(to));
        clearFindings();
        toast(`Incident ${inc.id} raised on consignment ${inc.consignment}`, 'warn');
      }),
    exportPackage: () =>
      act(async () => {
        const x = await api.exportPackage();
        toast(`Evidence package handed to the vendor: ${x.segments} records, ${Math.ceil(x.packageBytes / 1024)} KB. No video beyond the incident window.`, 'info');
      }),
    verify: (v: Verifier) =>
      act(async () => {
        setResult(null);
        setVerifying(v === 'CPTY' ? `The ${s.counterparty.name.toLowerCase()}` : 'The vendor');
        try {
          const [r] = await Promise.all([api.verify(v), sleep(900)]);
          setResult(r);
          // Jump to the first thing that is wrong, so the zoom shows it.
          const bad = r.cams.flatMap((c) => c.segments.filter((x) => isBad(x.status)).map((x) => ({ cam: c.cam, x })))[0];
          if (bad) setTarget({ cam: bad.cam, minute: bad.x.batchSeq - 1, index: bad.x.index });
        } finally {
          setVerifying(null);
        }
      }),
    tamper: (mode: TamperMode, cover: boolean) =>
      act(async () => {
        if (!picked?.seg) throw new Error('Nothing is stored in this slot. Pick another segment.');
        const r = await api.tamper(picked.cam, picked.seg.seq, mode, cover);
        clearFindings();
        toast(`Recorder altered: ${r.description}`, 'warn');
      }),
    rewrite: () =>
      act(async () => {
        setRewriteResult(await api.rewrite('WH3PL', shown.cam, shown.minute + 1));
      }),
    addVendor: () =>
      act(async () => {
        qc.setQueryData(keys.state, await api.addVendor());
        toast('Vendor added: it now holds a copy of every camera chain and co-signs new anchors', 'info');
      }),
  };

  const fresh = () => {
    clearFindings();
    setSceneRaw(1);
    setTarget({ cam: 'CAM-DOCK', minute: 29, index: 0 });
    qc.removeQueries({ predicate: (q) => q.queryKey[0] !== 'state' });
  };
  const onReset = () =>
    act(async () => {
      qc.setQueryData(keys.state, await api.reset());
      fresh();
    });
  const onRole = (role: CounterpartyRole) =>
    act(async () => {
      qc.setQueryData(keys.state, await api.setRole(role));
      fresh();
    });
  const onAuto = (on: boolean) =>
    act(async () => {
      qc.setQueryData(keys.state, await api.setAuto(on));
    });

  const batchVerdict = result?.cams.find((c) => c.cam === shown.cam)?.batches.find((b) => b.seq === shown.minute + 1);

  return (
    <>
      <TopBar s={s} onAuto={onAuto} onRole={onRole} onReset={onReset} />
      <div className="main">
        <SceneRail
          scene={scene}
          setScene={setScene}
          s={s}
          picked={picked}
          result={result}
          rewriteResult={rewriteResult}
          busy={busy}
          a={a}
        />
        <section className="stage">
          <Verdict s={s} result={result} verifying={verifying} />
          <Timelines s={s} win={win} grid={grid} result={result} target={shown} onPick={setTarget} />
          <Zoom s={s} slots={slots} target={shown} onPick={setTarget} />
          {!detail && (
            <button className="btn detail-toggle" onClick={() => setDetail(true)}>
              Show cryptographic detail
            </button>
          )}
        </section>
        {detail && <Detail s={s} target={shown} verdict={batchVerdict} onClose={() => setDetail(false)} />}
      </div>
      <NetworkStrip s={s} />
    </>
  );
}
