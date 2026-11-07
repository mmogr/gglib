/**
 * Settings › Engines: llama.cpp and stable-diffusion.cpp.
 *
 * Interim until S3 (`feat(gui)/settings-engines-about`): one `EngineCard`
 * per engine, the GUI face of `gglib config llama` and `gglib config sd`,
 * each card's state in `useEngine`. The system checks (`gglib config
 * check-deps`, `paths`, `fast-downloads status`) sit under them until
 * Diagnostics has its System tab (G1).
 *
 * @module places/settings/EnginesSection
 */

import type { FC } from 'react';
import { DiagnosticsPanel } from './DiagnosticsPanel';
import { EngineCard } from './EngineCard';

export const EnginesSection: FC = () => (
  <>
    <EngineCard kind="llama" />
    <EngineCard kind="stable_diffusion" />
    <DiagnosticsPanel />
  </>
);
