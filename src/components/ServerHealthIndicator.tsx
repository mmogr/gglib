/**
 * ServerHealthIndicator Component
 *
 * Pure presentational component for server health status.
 * No polling - reads from ServerRegistry hook.
 * Works anywhere: ModelList, server panel, chat header.
 */

import { useServerHealth } from '../services/serverRegistry';
import { getHealthDisplay, type HealthTone } from '../types';
import { cn } from '../utils/cn';

/** Token-backed dot colour per health tone. */
const toneDot: Record<HealthTone, string> = {
  healthy: 'bg-success',
  degraded: 'bg-warning',
  failed: 'bg-danger',
  unknown: 'bg-offline',
};

interface Props {
  /** Model ID to monitor (server registry key) */
  modelId: string | number;
  /** Optional CSS class */
  className?: string;
  /** Show label text alongside indicator */
  showLabel?: boolean;
}

export function ServerHealthIndicator({ modelId, className, showLabel = false }: Props) {
  const health = useServerHealth(modelId);
  const { tone, label, title } = getHealthDisplay(health);

  return (
    <span
      className={cn('inline-flex items-center gap-sm', className)}
      title={title}
      aria-label={`Server health: ${label}`}
    >
      <span className={cn('w-2 h-2 rounded-full shrink-0', toneDot[tone])} aria-hidden="true" />
      {showLabel && <span>{label}</span>}
    </span>
  );
}

export default ServerHealthIndicator;
