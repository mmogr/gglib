/**
 * Settings › Models: where the library lives and where downloads wait, how
 * many may queue, the context for a model without its own, the model `gglib
 * q` uses and the one a drawing uses, the sampling defaults and the
 * profiles. Each saves on change.
 *
 * The image model choice lists only models that draw, since the daemon
 * refuses any other; a stored id the library no longer holds as one keeps a
 * choice of its own, so opening the section never changes it.
 *
 * @module places/settings/ModelsSection
 */

import type { FC } from 'react';
import { CONTEXT_SIZE, MAX_DOWNLOAD_QUEUE_SIZE } from '../../constants/settingsDefaults';
import { drawingValuesFrom } from '../../readings/drawingValues';
import { useModelLibrary } from '../../services/modelRegistry';
import type { AppSettings, GgufModel } from '../../types';
import { ChoiceSetting, NumberSetting, TextSetting, numberOrNull, type Choice } from './fields';
import { InferenceProfiles } from './InferenceProfiles';
import { ModelsFolder } from './ModelsFolder';
import { SamplingDefaults } from './SamplingDefaults';
import { useSettings } from '../../hooks/useSettings';

const idText = (id: number | null) => id?.toString() ?? '';

const choiceOf = (model: GgufModel): Choice => ({
  value: model.id.toString(),
  label: model.quantization ? `${model.name} (${model.quantization})` : model.name,
});

/** The image models, and a stored id that is not one of them kept as its own choice. */
export function imageChoices(models: GgufModel[], settings: AppSettings | null): Choice[] {
  const drawing = models.filter((model) => model.imageFamily != null).map(choiceOf);
  const stored = settings ? drawingValuesFrom(settings).defaultImageModel : '';
  const stray = stored !== '' && !drawing.some((c) => c.value === stored);
  return [
    { value: '', label: 'None' },
    ...(stray ? [{ value: stored, label: `Model ${stored} (not an image model in this library)` }] : []),
    ...drawing,
  ];
}

export const ModelsSection: FC = () => {
  const { models, loading } = useModelLibrary();
  const { settings } = useSettings();
  return (
    <>
      <ModelsFolder />

      <TextSetting
        k="defaultDownloadPath"
        id="download-path-input"
        label="Download staging folder"
        defaultHint="the models folder"
        placeholder="the models folder"
        mono
        description="Where a download waits before it joins the library. Leave it empty to download straight into the models folder."
      />

      <NumberSetting
        k="maxDownloadQueueSize"
        id="max-queue-size-input"
        label="Download queue size"
        spec={MAX_DOWNLOAD_QUEUE_SIZE}
        description={`How many downloads may wait at once (${MAX_DOWNLOAD_QUEUE_SIZE.min}–${MAX_DOWNLOAD_QUEUE_SIZE.max}).`}
      />

      <NumberSetting
        k="defaultContextSize"
        id="context-size-input"
        label="Context for models without their own"
        spec={CONTEXT_SIZE}
        description="The context window of a model with no context of its own set."
      />

      <ChoiceSetting
        k="defaultModelId"
        id="default-model-select"
        label="Model for gglib q"
        description={<>The model <code>gglib q</code> asks when it names none.</>}
        disabled={loading}
        choices={[{ value: '', label: 'None' }, ...models.map(choiceOf)]}
        toText={idText}
        fromText={numberOrNull}
      />

      <ChoiceSetting
        k="defaultImageModelId"
        id="default-image-model-select"
        label="Image model for drawing"
        description="The model a drawing uses when it names none. Only models that draw are listed."
        disabled={loading}
        choices={imageChoices(models, settings)}
        toText={idText}
        fromText={numberOrNull}
      />

      <SamplingDefaults />

      <InferenceProfiles />
    </>
  );
};
