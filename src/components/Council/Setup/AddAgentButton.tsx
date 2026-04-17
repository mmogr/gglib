/**
 * Button to add a blank agent scaffold to the council.
 *
 * Renders a dashed-border card that matches the agent card grid layout.
 *
 * @module components/Council/Setup/AddAgentButton
 */

import type { FC } from 'react';
import { cn } from '../../../utils/cn';

interface AddAgentButtonProps {
  onClick: () => void;
  disabled?: boolean;
}

export const AddAgentButton: FC<AddAgentButtonProps> = ({ onClick, disabled }) => (
  <button
    type="button"
    onClick={onClick}
    disabled={disabled}
    className={cn(
      'rounded-base border-2 border-dashed border-border p-md',
      'flex items-center justify-center gap-sm',
      'text-sm text-text-muted transition-colors',
      'hover:border-primary hover:text-primary',
      'disabled:opacity-50 disabled:cursor-not-allowed disabled:hover:border-border disabled:hover:text-text-muted',
    )}
    aria-label="Add agent"
  >
    <span className="text-lg leading-none">+</span>
    Add Agent
  </button>
);
