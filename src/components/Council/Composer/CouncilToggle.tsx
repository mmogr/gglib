/**
 * Council mode toggle for the chat composer.
 *
 * When active, the next message submission routes to `suggestCouncil()`
 * instead of the standard agentic chat flow.
 *
 * @module components/Council/Composer/CouncilToggle
 */

import type { FC } from 'react';
import { Users } from 'lucide-react';
import { cn } from '../../../utils/cn';
import { Icon } from '../../ui/Icon';

export interface CouncilToggleProps {
  active: boolean;
  onToggle: () => void;
  disabled?: boolean;
}

export const CouncilToggle: FC<CouncilToggleProps> = ({ active, onToggle, disabled }) => (
  <button
    type="button"
    onClick={onToggle}
    disabled={disabled}
    title={active ? 'Council mode ON — click to disable' : 'Enable Council of Agents'}
    aria-pressed={active}
    className={cn(
      'flex items-center justify-center w-8 h-8 rounded-base border transition-all duration-150',
      'focus:outline-none focus-visible:ring-2 focus-visible:ring-primary/50',
      'disabled:opacity-50 disabled:cursor-not-allowed',
      active
        ? 'bg-primary/15 border-primary/40 text-primary'
        : 'bg-transparent border-border text-text-muted hover:bg-background-hover hover:text-text',
    )}
  >
    <Icon icon={Users} size={16} />
  </button>
);
