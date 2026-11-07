/**
 * The sheet's Parts line for an image model, in place of the context,
 * templates, drafting and sampling it does not have: its family and the
 * files it draws with (a missing one named), then the image runtime. With
 * stable-diffusion.cpp missing, Install runs the daemon's install here (the
 * one System settings runs, `useEngine`), its progress in place; once it is
 * there, the line says what loading means today. `ready` tells the sheet
 * whether Load can go.
 *
 * @module inspector/ImageParts
 */

import { useEffect, type FC } from 'react';
import { useEngine } from '../components/SettingsModal/useEngine';
import { InstallProgress } from '../components/InstallProgress';
import { StatusMark } from '../components/status/StatusMark';
import { Button } from '../components/ui/Button';
import type { ComponentLinkDto, GgufModel } from '../types';
import { ENGINE_PRODUCT } from '../types/setup';
import { componentRoleLabel, imageFamilyLabel } from '../utils/imageFamily';
import { fileName } from './local/rows';

interface ImagePartsProps {
  model: GgufModel;
  /** The model's linked components, from its detail; `undefined` while it is read. */
  components: ComponentLinkDto[] | undefined;
  /** Whether the parts and the runtime are all there to load it. */
  onReady: (ready: boolean) => void;
}

const SD = ENGINE_PRODUCT.stable_diffusion;

export const ImageParts: FC<ImagePartsProps> = ({ model, components, onReady }) => {
  const sd = useEngine('stable_diffusion');
  const installed = sd.status?.installed === true;
  const ready = installed && model.missingComponents.length === 0;
  useEffect(() => onReady(ready), [ready, onReady]);

  const parts = [
    model.imageFamily ? imageFamilyLabel(model.imageFamily) : null,
    ...(components ?? []).filter((c) => c.path).map((c) => `${componentRoleLabel(c.role)} ${fileName(c.path ?? '')}${c.present ? '' : ' (file missing)'}`),
    ...model.missingComponents.map((role) => `▲ ${componentRoleLabel(role)} missing`),
  ].filter(Boolean);

  return (
    <div role="group" aria-label="Parts" className="grid grid-cols-[6.5rem_minmax(0,1fr)] items-baseline gap-x-md gap-y-xs text-xs">
      <span className="text-text-secondary">Parts</span>
      <span className="break-words text-text">{parts.join(' · ')}</span>
      <span />
      <span className="flex flex-col gap-xs text-text-secondary">
        {sd.statusError && (
          <span className="flex items-baseline gap-xs text-text">
            <StatusMark kind="failed" label="Failed" />
            {sd.statusError}
          </span>
        )}
        {sd.status && !installed && !sd.installing && (
          <span className="flex items-center gap-sm">
            {`Drawing needs ${SD}.`}
            <Button variant="link" size="sm" className="text-xs font-normal" onClick={sd.runInstall}>
              Install
            </Button>
          </span>
        )}
        {sd.installing && <InstallProgress progress={sd.progress} product={SD} />}
        {sd.installError && <span className="text-text">{`Didn't install: ${sd.installError}`}</span>}
        {installed &&
          `Loads on ${[SD, sd.status?.install?.release].filter(Boolean).join(' ')}, beside the chat model when memory allows. Drawing with it is not available yet.`}
      </span>
    </div>
  );
};
