/**
 * Settings › Engines: llama.cpp and stable-diffusion.cpp, one row each
 * (`EngineRow`, the GUI face of `gglib config llama` and `gglib config
 * sd`), then Faster downloads (`gglib config fast-downloads`). The system
 * checks (`gglib config check-deps`, `paths`, `fast-downloads status`) sit
 * at the foot until Diagnostics has its System tab (G1).
 *
 * @module places/settings/EnginesSection
 */

import type { FC } from 'react';
import { DiagnosticsPanel } from './DiagnosticsPanel';
import { EngineRow } from './EngineRow';
import { FasterDownloads } from './FasterDownloads';

export const EnginesSection: FC = () => (
  <>
    <EngineRow kind="llama" />
    <EngineRow kind="stable_diffusion" />
    <FasterDownloads />
    <DiagnosticsPanel />
  </>
);
