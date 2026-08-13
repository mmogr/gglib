/**
 * ProxyLaunchPanel.
 *
 * What the runtime decided when it launched the running model, and why.
 *
 * The GUI counterpart to the CLI startup banner: gglib auto-sizes the RAM
 * cache, quantizes the KV cache, enables speculative decoding, picks a tool-call
 * dialect parser and resolves the context through a four-level chain, and until
 * this panel existed a GUI user's only evidence of any of it was the README.
 *
 * Every row is rendered exactly as the backend ordered and worded it — see
 * `gglib_core::domain::LaunchNarration`. Nothing is re-derived, re-sorted or
 * re-labelled here, so this panel and the banner cannot drift into describing
 * the same launch differently.
 *
 * @module components/ProxyLaunchPanel
 */

import type { FC } from 'react';
import type { LaunchNarration } from '../services/transport/types/dashboard';

export interface ProxyLaunchPanelProps {
  /** `null`/`undefined` before the first request resolves a model. */
  launch?: LaunchNarration | null;
}

/** Bytes as GiB with one decimal, matching the backend's `format_gib`. */
function formatWeights(bytes: number): string {
  return `${(bytes / 1_073_741_824).toFixed(1)} GiB`;
}

/**
 * The model identity line: `qwen3-30b-a3b · Q4_K_M · 17.2 GB`.
 *
 * Unknown quantization and unknown size drop out rather than rendering as
 * filler, mirroring `LaunchNarration::headline` on the backend.
 */
export function headline(launch: LaunchNarration): string {
  const parts = [launch.model_name];
  if (launch.quantization) {
    parts.push(launch.quantization);
  }
  if (launch.weights_bytes > 0) {
    parts.push(formatWeights(launch.weights_bytes));
  }
  return parts.join(' · ');
}

export const ProxyLaunchPanel: FC<ProxyLaunchPanelProps> = ({ launch }) => {
  if (!launch) {
    return <p className="text-sm text-text-muted">No model resolved yet.</p>;
  }

  return (
    <div className="flex flex-col gap-sm">
      <p className="text-sm font-semibold text-text">{headline(launch)}</p>

      <div className="flex flex-col gap-xs p-md rounded-base bg-surface-elevated">
        {launch.decisions.map((decision) => (
          <div key={decision.label} className="flex items-baseline gap-md">
            <span className="text-xs text-text-muted w-16 shrink-0">{decision.label}</span>
            <span className="text-sm text-text font-mono tabular-nums">{decision.value}</span>
            {/*
              The provenance is why this panel exists rather than a config
              dump, but it is the secondary read — dimmed so the eye takes the
              decisions first and the reasons second.
            */}
            {decision.source && (
              <span className="text-xs text-text-muted ml-auto">({decision.source})</span>
            )}
          </div>
        ))}
      </div>
    </div>
  );
};

export default ProxyLaunchPanel;
