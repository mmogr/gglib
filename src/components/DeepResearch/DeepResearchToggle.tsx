/**
 * Deep Research Toggle Component
 *
 * A toggle button for enabling/disabling deep research mode in the composer.
 * Also provides a "Stop Research" button when research is active.
 *
 * @module components/DeepResearch/DeepResearchToggle
 */

import React from 'react';
import { FastForward, Loader2, Search, Square, Sparkles } from 'lucide-react';
import { Icon } from '../ui/Icon';
import { cn } from '../../utils/cn';

export interface DeepResearchToggleProps {
  /** Whether deep research mode is enabled */
  isEnabled: boolean;
  /** Toggle deep research mode */
  onToggle: () => void;
  /** Whether research is currently running */
  isRunning?: boolean;
  /** Stop the current research */
  onStop?: () => void;
  /** Request early wrap-up (synthesize with current facts) */
  onWrapUp?: () => void;
  /** Whether the toggle is disabled */
  disabled?: boolean;
  /** Tooltip text when disabled */
  disabledReason?: string;
  /** Optional className */
  className?: string;
  /** Current research phase (for showing wrap-up button only in gathering) */
  researchPhase?: string;
}

/**
 * Toggle button for deep research mode.
 *
 * Shows:
 * - Toggle button (off) - "Deep Research"
 * - Toggle button (on) - "Deep Research ✓"
 * - Stop button when research is running
 */
export const DeepResearchToggle: React.FC<DeepResearchToggleProps> = ({
  isEnabled,
  onToggle,
  isRunning = false,
  onStop,
  onWrapUp,
  disabled = false,
  disabledReason,
  className,
  researchPhase,
}) => {
  // If research is running, show stop button
  if (isRunning) {
    // Only show wrap-up during gathering phase
    const canWrapUp = researchPhase === 'gathering' && onWrapUp;
    
    return (
      <div className={cn('flex items-center gap-2', className)}>
        <div className="flex items-center gap-1.5 py-1 px-2 bg-primary-subtle rounded text-[11px] text-primary-light">
          <Icon icon={Loader2} size={12} className="animate-spin" />
          <span>Researching...</span>
        </div>
        {canWrapUp && (
          <button
            className="flex items-center gap-1.5 py-1.5 px-3 bg-success-subtle border border-success-border rounded-md text-success text-xs font-medium cursor-pointer transition-all duration-200 hover:bg-success/20"
            onClick={onWrapUp}
            title="Wrap up research early (synthesize now)"
            type="button"
          >
            <span className="flex items-center justify-center">
              <Icon icon={FastForward} size={12} />
            </span>
            <span>Wrap Up</span>
          </button>
        )}
        {onStop && (
          <button
            className="flex items-center gap-1.5 py-1.5 px-3 bg-danger-subtle border border-danger-border rounded-md text-danger text-xs font-medium cursor-pointer transition-all duration-200 hover:bg-danger/20"
            onClick={onStop}
            title="Stop research"
            type="button"
          >
            <span className="flex items-center justify-center">
              <Icon icon={Square} size={12} />
            </span>
            <span>Stop</span>
          </button>
        )}
      </div>
    );
  }

  // Show toggle button
  return (
    <div className={cn('flex items-center gap-2', className)}>
      <div className={cn(disabled && disabledReason && 'relative group')}>
        <button
          className={cn(
            'flex items-center gap-1.5 py-1.5 px-3 border rounded-md text-xs font-medium cursor-pointer transition-all duration-200 whitespace-nowrap',
            'bg-surface-elevated border-border text-text-secondary',
            'hover:not-disabled:bg-surface-hover hover:not-disabled:border-border-hover hover:not-disabled:text-text',
            'disabled:opacity-50 disabled:cursor-not-allowed',
            'data-[active=true]:bg-primary-subtle data-[active=true]:border-primary-border data-[active=true]:text-primary-light',
            'data-[active=true]:hover:not-disabled:bg-primary/20 data-[active=true]:hover:not-disabled:border-primary',
          )}
          onClick={onToggle}
          data-active={isEnabled}
          disabled={disabled}
          title={isEnabled ? 'Disable deep research mode' : 'Enable deep research mode'}
          type="button"
        >
          <span className="flex items-center justify-center">
            <Icon icon={isEnabled ? Sparkles : Search} size={14} />
          </span>
          <span className="inline">
            Deep Research{isEnabled ? ' ✓' : ''}
          </span>
        </button>
        {disabled && disabledReason && (
          <span className="absolute bottom-full left-1/2 -translate-x-1/2 py-1.5 px-2.5 bg-background border border-border rounded text-[11px] text-text-secondary whitespace-nowrap opacity-0 invisible group-hover:opacity-100 group-hover:visible transition-[opacity,visibility] duration-200 pointer-events-none z-[100] mb-1">
            {disabledReason}
          </span>
        )}
      </div>
    </div>
  );
};

export default DeepResearchToggle;
