/**
 * ⧉ beside the launch line: the whole command on the clipboard as one shell
 * line (`commandLine`, the line drawn, quoted for a shell), and "Copied"
 * beside it for a moment.
 *
 * The "Copied" is the tray's pattern (`tray/Endpoint`), held here until one
 * shared inline Copied replaces both.
 *
 * @module inspector/local/CopyCommand
 */

import { useEffect, useState, type FC } from 'react';
import { CopyValue } from '../../components/CopyValue';
import type { LaunchPlan } from '../../types/generated/LaunchPlan';
import { commandLine } from './commandLine';

/** How long "Copied" stays up, in ms. */
const COPIED_MS = 1500;

export const CopyCommand: FC<{ plan: Pick<LaunchPlan, 'program' | 'args'> }> = ({ plan }) => {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return undefined;
    const timer = setTimeout(() => setCopied(false), COPIED_MS);
    return () => clearTimeout(timer);
  }, [copied]);
  return (
    <span className="flex shrink-0 items-center gap-xs">
      {copied && (
        <span role="status" className="text-xs text-text-secondary">
          Copied
        </span>
      )}
      <CopyValue value={commandLine(plan)} label="Copy as command" onCopied={() => setCopied(true)} />
    </span>
  );
};
