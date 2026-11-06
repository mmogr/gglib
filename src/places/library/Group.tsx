/**
 * A group of the library's rows under its 12 px label, and the one line a
 * group or the library says in place of rows.
 *
 * @module places/library/Group
 */

import type { FC, ReactNode } from 'react';

export const Group: FC<{ label: string; aside?: ReactNode; children: ReactNode }> = ({ label, aside, children }) => (
  <section aria-label={label} className="flex flex-col pb-md">
    <h3 className="m-0 flex items-center justify-between px-md py-xs text-xs font-semibold text-text-secondary">
      <span>{label}</span>
      {aside}
    </h3>
    {children}
  </section>
);

export const Note: FC<{ children: ReactNode }> = ({ children }) => (
  <p className="m-0 px-md py-xs text-xs text-text-secondary">{children}</p>
);
