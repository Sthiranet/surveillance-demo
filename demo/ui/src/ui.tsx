// Small presentational helpers shared by every part of the screen.
import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from 'react';
import type { Anchor, SegmentStatus, StateView } from './api/client';

// Demo time is simulated and shown as it is on the ledger (UTC), so 08:21:10 reads the same everywhere.
// Ledger timestamps carry microseconds; Date only guarantees milliseconds, so the rest is cut off.
export const ms = (iso: string) => Date.parse(`${iso.slice(0, 23)}Z`);
export const iso = (t: number) => new Date(t).toISOString().replace(/Z$/, '000Z');
export const hms = (t?: string | number | null) =>
  t == null ? '—' : (typeof t === 'number' ? new Date(t).toISOString() : t).slice(11, 19);
export const hm = (t?: string | number | null) => hms(t).slice(0, 5);
export const plural = (n: number, w: string) => `${n} ${w}${n === 1 ? '' : 's'}`;

export const partyName = (s: StateView, p: string) =>
  p === 'WH3PL' ? '3PL' : p === 'CPTY' ? s.counterparty.name : p === 'VENDOR' ? 'Vendor' : p;

export const STATUS_LABEL: Record<SegmentStatus, string> = {
  ok: 'Genuine',
  modified: 'Modified',
  missing: 'Missing',
  replayed: 'Looped',
  bad_signature: 'Bad signature',
};

export function Hash({ h, cls = '', n = 8 }: { h?: string | null; cls?: string; n?: number }) {
  if (!h) return <span className="muted">—</span>;
  return (
    <span className={`hash ${cls}`} title={h}>
      {h.slice(0, n)}…
    </span>
  );
}

export const Sigs = ({ s, list }: { s: StateView; list?: string[] }) => (
  <>
    {(list ?? []).map((p) => (
      <span key={p} className="sig">
        {partyName(s, p)}
      </span>
    ))}
  </>
);

export function AnchorCard({ s, a }: { s: StateView; a: Anchor }) {
  return (
    <div className="anchor-card">
      <div className="anchor-title">
        <span>
          Anchor · {a.chainId} · minute #{a.seq}
        </span>
        <span className="mono small muted">{a.contractId}</span>
      </div>
      <div className="kv">
        <span className="k">window</span>
        <span className="v">
          {hms(a.fromTime)} → {hms(a.toTime)}
        </span>
        <span className="k">segments</span>
        <span className="v">
          {a.eventCount} <span className="muted small">(#{Number(a.fromPos)} to #{Number(a.toPos)})</span>
        </span>
        <span className="k">merkle_root</span>
        <span className="v">
          <Hash h={a.merkleRoot} cls="accent" n={16} />
        </span>
        <span className="k">prev_head</span>
        <span className="v">
          <Hash h={a.prevHead} cls="dim" n={16} />{' '}
          <span className="muted small">{a.seq === 1 ? '(genesis)' : `(head of #${a.seq - 1})`}</span>
        </span>
        <span className="k">head</span>
        <span className="v">
          <Hash h={a.head} cls="accent" n={16} />
        </span>
      </div>
      <div className="sigs">
        <span className="muted small">Signed by </span>
        <Sigs s={s} list={a.signatories} />
      </div>
    </div>
  );
}

// ── Toast ──
type ToastKind = 'error' | 'info' | 'warn';
const ToastCtx = createContext<(msg: string, kind?: ToastKind) => void>(() => {});
export const useToast = () => useContext(ToastCtx);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [t, setT] = useState<{ msg: string; kind: ToastKind; show: boolean }>({ msg: '', kind: 'error', show: false });
  const timer = useRef<number | undefined>(undefined);
  const show = useCallback((msg: string, kind: ToastKind = 'error') => {
    setT({ msg, kind, show: true });
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setT((x) => ({ ...x, show: false })), 4200);
  }, []);
  return (
    <ToastCtx.Provider value={show}>
      {children}
      <div id="toast" role="status" className={`${t.kind} ${t.show ? 'show' : ''}`}>
        {t.msg}
      </div>
    </ToastCtx.Provider>
  );
}

/** Wraps an action so a failure shows as a toast instead of an unhandled rejection. */
export function useRun() {
  const toast = useToast();
  return useCallback(
    async <T,>(fn: () => Promise<T>): Promise<T | undefined> => {
      try {
        return await fn();
      } catch (e) {
        toast(e instanceof Error ? e.message : String(e));
        return undefined;
      }
    },
    [toast],
  );
}
