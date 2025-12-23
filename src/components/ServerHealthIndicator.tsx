/**
 * ServerHealthIndicator Component
 *
 * Pure presentational component for server health status.
 * No polling - reads from ServerRegistry hook.
 * Works anywhere: ModelList, server panel, chat header.
 */

import { useServerHealth } from '../services/serverRegistry';
import { getHealthDisplay } from '../types';

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
  const { dot, label, title } = getHealthDisplay(health);

  return (
    <span
      className={`inline-flex items-center gap-1.5 ${className || ''}`}
      title={title}
      aria-label={`Server health: ${label}`}
    >
      <span aria-hidden="true">{dot}</span>
      {showLabel && <span>{label}</span>}
    </span>
  );
}

export default ServerHealthIndicator;
