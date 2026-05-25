/**
 * Council mode toggle for the chat composer.
 *
 * When active, the next message submission routes to `suggestCouncil()`
 * instead of the standard agentic chat flow.
 *
 * @module components/Council/Composer/CouncilToggle
 */

import type { FC } from 'react';
import { Network, Users } from 'lucide-react';
import { cn } from '../../../utils/cn';
import { Icon } from '../../ui/Icon';
import type { CouncilEngine } from '../../../types';

export interface CouncilToggleProps {
  active: boolean;
  onToggle: () => void;
  disabled?: boolean;
  /** Which engine is currently selected — changes the icon and tooltip. */
  engine?: CouncilEngine;
}

export const CouncilToggle: FC<CouncilToggleProps> = ({ active, onToggle, disabled, engine = 'legacy' }) => {
  const isV2 = engine === 'v2';
  const titleOn  = isV2 ? 'Orchestrator v2 ON — click to disable' : 'Council mode ON — click to disable';
  const titleOff = isV2 ? 'Enable Orchestrator v2 (DAG engine)' : 'Enable Council of Agents';

  return (
    <button
      type="button"
      onClick={onToggle}
      disabled={disabled}
      title={active ? titleOn : titleOff}
      aria-pressed={active}
      className={cn(
        'relative flex items-center justify-center w-8 h-8 rounded-base border transition-all duration-150',
        'focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50',
        'disabled:opacity-50 disabled:cursor-not-allowed',
        active
          ? 'bg-primary/15 border-primary/40 text-primary'
          : 'bg-transparent border-border text-text-muted hover:bg-background-hover hover:text-text',
      )}
    >
      <Icon icon={isV2 ? Network : Users} size={16} />
      {isV2 && (
        <span className="absolute -top-[5px] -right-[5px] text-[8px] font-bold leading-none bg-primary text-white rounded-full px-[3px] py-[1px] pointer-events-none">
          v2
        </span>
      )}
    </button>
  );
};
