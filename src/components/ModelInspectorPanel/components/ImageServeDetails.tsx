import type { FC } from 'react';
import { Settings } from 'lucide-react';
import type { ComponentLinkDto, GgufModel } from '../../../types';
import type { SdStatusState } from '../hooks/useSdStatus';
import { componentRoleLabel, imageFamilyLabel } from '../../../utils/imageFamily';
import { Banner } from '../../ui/Banner';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';

interface ImageServeDetailsProps {
  model: GgufModel;
  /** The model's linked components, from its detail; `undefined` while it is read. */
  components: ComponentLinkDto[] | undefined;
  /** The image runtime, as the modal read it. */
  runtime: SdStatusState;
  /** Open Settings where the image runtime is installed; no link when absent. */
  onOpenSystemSettings?: () => void;
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() || path;
}

/**
 * What loading an image model means, in the serve modal's place for the
 * context, template and sampling options it does not have: its family, the
 * files it draws with (each missing one named), and whether `sd-server` is
 * installed to load it. Loading places it beside the chat model when memory
 * allows; nothing draws with it yet.
 */
export const ImageServeDetails: FC<ImageServeDetailsProps> = ({
  model,
  components,
  runtime,
  onOpenSystemSettings,
}) => {
  const linked = (components ?? []).filter((c) => c.path);
  const installed = runtime.status?.install.installed;

  return (
    <div className="flex flex-col gap-md mb-lg">
      <dl className="m-0 grid grid-cols-[auto_1fr] gap-x-md gap-y-xs text-sm">
        <dt className="text-text-secondary">Family</dt>
        <dd className="m-0 text-text">{model.imageFamily ? imageFamilyLabel(model.imageFamily) : '—'}</dd>
        {linked.map((c) => (
          <div key={c.role} className="contents">
            <dt className="text-text-secondary">{componentRoleLabel(c.role)}</dt>
            <dd className={`m-0 font-mono text-xs break-all ${c.present ? 'text-text' : 'text-danger'}`}>
              {fileName(c.path ?? '')}
              {c.present ? '' : ' (file missing)'}
            </dd>
          </div>
        ))}
      </dl>

      {model.missingComponents.length > 0 && (
        <Banner variant="warning">
          Missing {model.missingComponents.map(componentRoleLabel).join(', ')}. Link{' '}
          {model.missingComponents.length === 1 ? 'it' : 'them'} in the inspector&apos;s Components
          row before loading this model.
        </Banner>
      )}

      {runtime.error && <Banner variant="danger">{runtime.error}</Banner>}
      {runtime.status && !installed && (
        <Banner
          variant="info"
          action={
            onOpenSystemSettings && (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                leftIcon={<Icon icon={Settings} size={14} />}
                onClick={onOpenSystemSettings}
              >
                Open Settings
              </Button>
            )
          }
        >
          The image runtime is not installed. Install it in Settings → System → Image runtime, or
          run <code className="font-mono">{runtime.status.installCommand}</code>.
        </Banner>
      )}
      {installed && (
        <p className="m-0 text-sm text-text-secondary">
          Loads on stable-diffusion.cpp {runtime.status?.install.release ?? ''}, beside the chat
          model when memory allows. Drawing with it is not available yet.
        </p>
      )}
    </div>
  );
};
