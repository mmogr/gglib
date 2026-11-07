/**
 * Settings › Engines: llama.cpp and stable-diffusion.cpp, one row each
 * (`EngineRow`, the GUI face of `gglib config llama` and `gglib config
 * sd`), then Faster downloads (`gglib config fast-downloads`). The system
 * checks are Diagnostics › System's.
 *
 * @module places/settings/EnginesSection
 */

import type { FC } from 'react';
import { EngineRow } from './EngineRow';
import { FasterDownloads } from './FasterDownloads';

export const EnginesSection: FC = () => (
  <>
    <EngineRow kind="llama" />
    <EngineRow kind="stable_diffusion" />
    <FasterDownloads />
  </>
);
