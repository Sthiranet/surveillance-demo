// Turns what the server reports into the grid the stage draws: cameras × minutes × six 10-second slots.
import type { SegmentStatus, SegmentVerdict, SegmentView, StateView, VerifyResponse } from './api/client';
import { ms } from './ui';

export const MINUTE = 60_000;
/** Minutes on screen, and the length of the default incident window. */
export const WINDOW_MINUTES = 10;
/** The default incident window ends this many minutes after recording began (the seeded history). */
export const STORY_END_MINUTES = 30;

/** A 10-second slot on a camera's timeline. `minute` counts from the start of the chain; batch = minute + 1. */
export type SlotRef = { cam: string; minute: number; index: number };

export type SlotState = SegmentStatus | 'stored' | 'gap' | 'future';

export type Slot = SlotRef & {
  start: number;
  /** What the recorder holds for this slot right now, if anything. */
  seg?: SegmentView;
  /** The verifier's finding for this slot, once the window was verified. */
  verdict?: SegmentVerdict;
  state: SlotState;
};

export type Window = { from: number; to: number; minutes: number[] };

/** The minutes on screen: the incident window once there is one, otherwise the latest ten minutes. */
export function windowOf(s: StateView): Window {
  const start = ms(s.clock.start);
  let from: number, to: number;
  if (s.incident) {
    [from, to] = [ms(s.incident.from), ms(s.incident.to)];
  } else {
    const sealed = ms(s.clock.sealedTo);
    to = ms(s.clock.now) > sealed ? sealed + MINUTE : sealed;
    from = Math.max(start, to - WINDOW_MINUTES * MINUTE);
  }
  const minutes: number[] = [];
  for (let t = from; t < to; t += MINUTE) minutes.push((t - start) / MINUTE);
  return { from, to, minutes };
}

export type Footage = Map<number, SegmentView>;

export const byStart = (segments: SegmentView[] | undefined): Footage =>
  new Map((segments ?? []).map((x) => [ms(x.start), x]));

export function verdictsByStart(result: VerifyResponse | null, cam: string): Map<number, SegmentVerdict> {
  const c = result?.cams.find((x) => x.cam === cam);
  return new Map((c?.segments ?? []).map((x) => [ms(x.start), x]));
}

export function slotsOf(
  s: StateView,
  cam: string,
  minute: number,
  footage: Footage,
  verdicts: Map<number, SegmentVerdict>,
): Slot[] {
  const base = ms(s.clock.start) + minute * MINUTE;
  const now = ms(s.clock.now);
  const step = s.segmentSeconds * 1000;
  return Array.from({ length: s.segmentsPerBatch }, (_, index) => {
    const start = base + index * step;
    const seg = footage.get(start);
    const verdict = verdicts.get(start);
    const state: SlotState = verdict ? verdict.status : seg ? 'stored' : start >= now ? 'future' : 'gap';
    return { cam, minute, index, start, seg, verdict, state };
  });
}

export const isBad = (st: SlotState) => st !== 'ok' && st !== 'stored' && st !== 'future' && st !== 'gap';
