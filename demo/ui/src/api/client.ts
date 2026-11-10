// Typed client for surveillance-director. Types come from its OpenAPI spec (`npm run gen:api`),
// so a backend change breaks this build instead of the live demo.
import createClient from 'openapi-fetch';
import type { components, paths } from './schema';

export type S = components['schemas'];
export type StateView = S['StateView'];
export type CameraView = S['CameraView'];
export type SegmentView = S['SegmentView'];
export type MerkleTreeNode = S['MerkleTree'];
export type Anchor = S['Anchor'];
export type VerifyResponse = S['VerifyResponse'];
export type CamVerdict = S['CamVerdict'];
export type SegmentVerdict = S['SegmentVerdict'];
export type BatchVerdict = S['BatchVerdict'];
export type ArchiveAttempt = S['ArchiveAttempt'];
export type Scene = S['Scene'];
export type SegmentStatus = S['SegmentStatus'];
export type TamperMode = S['TamperMode'];
export type CounterpartyRole = S['CounterpartyRole'];
export type Verifier = 'CPTY' | 'VENDOR';

const client = createClient<paths>({ baseUrl: '' });

type Result<T> = { data?: T; error?: unknown; response: Response };

async function call<T>(p: Promise<Result<T>>): Promise<T> {
  const { data, error, response } = await p;
  if (error !== undefined || data === undefined) {
    const msg = (error as { error?: string } | undefined)?.error;
    throw new Error(msg || response.statusText || 'Request failed');
  }
  return data;
}

const path = <T extends Record<string, string | number>>(p: T) => ({ params: { path: p } });

export const api = {
  state: () => call(client.GET('/api/state')),
  record: (segments: number) => call(client.POST('/api/record', { body: { segments } })),
  footage: (cam: string) => call(client.GET('/api/footage/{cam}', path({ cam }))),
  batch: (cam: string, seq: number) => call(client.GET('/api/batch/{cam}/{seq}', path({ cam, seq }))),
  ledger: (party: string) => call(client.GET('/api/ledger/{party}', path({ party }))),
  rewrite: (actor: string, chain: string, seq: number) =>
    call(client.POST('/api/ledger/rewrite', { body: { actor, chain, seq } })),
  tamper: (cam: string, seq: number, mode: TamperMode, cover: boolean) =>
    call(client.POST('/api/tamper', { body: { cam, seq, mode, cover } })),
  incident: (from: string, to: string) => call(client.POST('/api/incident', { body: { from, to, cams: [] } })),
  exportPackage: () => call(client.POST('/api/export')),
  verify: (verifier: Verifier) => call(client.POST('/api/verify', { body: { verifier } })),
  setRole: (role: CounterpartyRole) => call(client.POST('/api/counterparty', { body: { role } })),
  addVendor: () => call(client.POST('/api/vendor/member')),
  setAuto: (on: boolean) => call(client.POST('/api/auto', { body: { on } })),
  reset: () => call(client.POST('/api/reset')),
};

export const keys = {
  state: ['state'] as const,
  footage: (cam: string) => ['footage', cam] as const,
  batch: (cam: string, seq: number) => ['batch', cam, seq] as const,
  ledger: (party: string) => ['ledger', party] as const,
};
