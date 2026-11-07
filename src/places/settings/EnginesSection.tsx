/**
 * Settings › Engines: llama.cpp and stable-diffusion.cpp.
 *
 * Interim until S3 (`feat(gui)/settings-engines-about`): one `EngineCard`
 * per engine, the GUI face of `gglib config llama` and `gglib config sd`,
 * each card's state in `useEngine`.
 *
 * @module places/settings/EnginesSection
 */

import type { FC } from 'react';
import { EngineCard } from './EngineCard';

export const EnginesSection: FC = () => (
  <>
    <EngineCard kind="llama" />
    <EngineCard kind="stable_diffusion" />
  </>
);
