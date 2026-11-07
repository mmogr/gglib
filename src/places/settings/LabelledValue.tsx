import type { FC } from 'react';

/**
 * One label/value row. Values are mono so paths and hashes line up.
 *
 * Deliberately not called `Row`. It was, inside `SystemSettings.tsx`, which
 * also imports `Stack` from `components/primitives` — where `Row` is a flex
 * container. Two different ideas under one name, one import away from being
 * confused for each other.
 */
export const LabelledValue: FC<{ label: string; value: string; mono?: boolean }> = ({
  label,
  value,
  mono = true,
}) => (
  <div className="flex items-baseline justify-between gap-md">
    <span className="text-xs text-text-muted shrink-0">{label}</span>
    <span
      className={`text-xs text-text-secondary text-right break-all ${mono ? 'font-mono tabular-nums' : ''}`}
    >
      {value}
    </span>
  </div>
);
