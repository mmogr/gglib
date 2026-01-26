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
import styles from './DeepResearchToggle.module.css';

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
      <div className={`${styles.toggleContainer} ${className || ''}`}>
        <div className={styles.runningIndicator}>
          <Icon icon={Loader2} size={12} className={styles.runningSpinner} />
          <span>Researching...</span>
        </div>
        {canWrapUp && (
          <button
            className={styles.wrapUpButton}
            onClick={onWrapUp}
            title="Wrap up research early (synthesize now)"
            type="button"
          >
            <span className={styles.wrapUpIcon}>
              <Icon icon={FastForward} size={12} />
            </span>
            <span>Wrap Up</span>
          </button>
        )}
        {onStop && (
          <button
            className={styles.stopButton}
            onClick={onStop}
            title="Stop research"
            type="button"
          >
            <span className={styles.stopIcon}>
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
    <div className={`${styles.toggleContainer} ${className || ''}`}>
      <div className={disabled && disabledReason ? styles.tooltip : undefined}>
        <button
          className={styles.toggleButton}
          onClick={onToggle}
          data-active={isEnabled}
          disabled={disabled}
          title={isEnabled ? 'Disable deep research mode' : 'Enable deep research mode'}
          type="button"
        >
          <span className={styles.toggleIcon}>
            <Icon icon={isEnabled ? Sparkles : Search} size={14} />
          </span>
          <span className={styles.toggleLabel}>
            Deep Research{isEnabled ? ' ✓' : ''}
          </span>
        </button>
        {disabled && disabledReason && (
          <span className={styles.tooltipText}>{disabledReason}</span>
        )}
      </div>
    </div>
  );
};

export default DeepResearchToggle;
