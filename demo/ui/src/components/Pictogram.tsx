// What a segment shows, as a small pictogram. Illustration only: the real product never looks at pixels.
import type { Scene } from '../api/client';

export const SCENE_LABEL: Record<Scene, string> = {
  truck_docking: 'Truck docking',
  pallet_unloading: 'Pallets unloaded',
  forklift: 'Forklift',
  worker: 'Worker in view',
  empty_aisle: 'Empty aisle',
};

const FLOOR = <path d="M3 40h42" strokeWidth="1.5" opacity=".45" />;

const ART: Record<Scene, React.ReactNode> = {
  truck_docking: (
    <>
      <rect x="5" y="16" width="22" height="18" rx="1.5" />
      <path d="M27 22h8l6 6v6h-14z" />
      <circle cx="13" cy="37" r="3.2" />
      <circle cx="34" cy="37" r="3.2" />
      {FLOOR}
    </>
  ),
  pallet_unloading: (
    <>
      <rect x="9" y="12" width="12" height="11" rx="1" />
      <rect x="23" y="12" width="12" height="11" rx="1" />
      <rect x="16" y="24" width="14" height="10" rx="1" />
      <path d="M7 36h34M11 36v4M24 36v4M37 36v4" />
      {FLOOR}
    </>
  ),
  forklift: (
    <>
      <path d="M12 34V17h9l5 9v8z" />
      <path d="M30 12v22M30 32h11" />
      <rect x="32" y="22" width="8" height="8" rx="1" />
      <circle cx="15" cy="37" r="3.2" />
      <circle cx="25" cy="37" r="3.2" />
      {FLOOR}
    </>
  ),
  worker: (
    <>
      <circle cx="24" cy="12" r="4.2" />
      <path d="M24 17v13M16 22h16M24 30l-6 10M24 30l6 10" />
      {FLOOR}
    </>
  ),
  empty_aisle: (
    <>
      <path d="M4 8v32M44 8v32M4 8l12 9M44 8l-12 9M16 17v15M32 17v15M4 40l12-8M44 40l-12-8M16 17h16M16 32h16" opacity=".8" />
    </>
  ),
};

export function Pictogram({ scene }: { scene?: Scene | null }) {
  return (
    <svg className="picto" viewBox="0 0 48 48" role="img" aria-label={scene ? SCENE_LABEL[scene] : 'No video'}>
      {scene ? (
        <g fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
          {ART[scene]}
        </g>
      ) : (
        <g fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" opacity=".7">
          <path d="M14 14l20 20M34 14L14 34" />
        </g>
      )}
    </svg>
  );
}
