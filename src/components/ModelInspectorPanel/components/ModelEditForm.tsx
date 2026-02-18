import { FC } from 'react';
import { ExternalLink } from 'lucide-react';
import type { GgufModel, InferenceConfig } from '../../../types';
import { formatParamCount, getHuggingFaceUrl } from '../../../utils/format';
import { openUrl } from '../../../services/platform';
import { Icon } from '../../ui/Icon';
import { Input } from '../../ui/Input';
import { InferenceParametersForm } from '../../InferenceParametersForm';

interface ModelEditFormProps {
  model: GgufModel;
  editedQuantization: string;
  editedFilePath: string;
  editedInferenceDefaults: InferenceConfig | undefined;
  onQuantizationChange: (quant: string) => void;
  onFilePathChange: (path: string) => void;
  onInferenceDefaultsChange: (config: InferenceConfig) => void;
}

/**
 * Edit mode form for model metadata.
 * Shows editable inputs for quantization and file path,
 * with read-only display for other fields.
 */
export const ModelEditForm: FC<ModelEditFormProps> = ({
  model,
  editedQuantization,
  editedFilePath,
  editedInferenceDefaults,
  onQuantizationChange,
  onFilePathChange,
  onInferenceDefaultsChange,
}) => {

  return (
    <>
      <section className="mb-xl">
        <h3 className="m-0 mb-base text-sm font-semibold text-text-secondary uppercase tracking-[0.05em]">Model Information</h3>
      <div className="flex flex-col gap-md">
        <div className="flex justify-between items-start gap-base">
          <span className="text-text-muted text-sm shrink-0">Size:</span>
          <span className="text-text text-sm text-right break-words">{formatParamCount(model.paramCountB, model.expertUsedCount, model.expertCount)}</span>
        </div>
        {model.architecture && (
          <div className="flex justify-between items-start gap-base">
            <span className="text-text-muted text-sm shrink-0">Architecture:</span>
            <span className="text-text text-sm text-right break-words">{model.architecture}</span>
          </div>
        )}
        <div className="flex justify-between items-start gap-base">
          <span className="text-text-muted text-sm shrink-0">Quantization:</span>
          <Input
            type="text"
            className="py-sm px-md bg-background-input border-2 border-border-focus rounded-base text-text text-sm min-w-[200px] flex-1 transition duration-200 focus:outline-none focus:border-primary focus:shadow-[0_0_0_3px_rgba(59,130,246,0.1)]"
            value={editedQuantization}
            onChange={(e) => onQuantizationChange(e.target.value)}
            placeholder="e.g., Q4_0"
          />
        </div>
        {model.contextLength && (
          <div className="flex justify-between items-start gap-base">
            <span className="text-text-muted text-sm shrink-0">Context Length:</span>
            <span className="text-text text-sm text-right break-words">{model.contextLength.toLocaleString()}</span>
          </div>
        )}
        <div className="flex justify-between items-start gap-base">
          <span className="text-text-muted text-sm shrink-0">Path:</span>
          <Input
            type="text"
            className="py-sm px-md bg-background-input border-2 border-border-focus rounded-base text-text text-sm min-w-[200px] flex-1 transition duration-200 focus:outline-none focus:border-primary focus:shadow-[0_0_0_3px_rgba(59,130,246,0.1)] font-mono text-xs"
            value={editedFilePath}
            onChange={(e) => onFilePathChange(e.target.value)}
            placeholder="File path"
          />
        </div>
        {model.hfRepoId && (
          <div className="flex justify-between items-start gap-base">
            <span className="text-text-muted text-sm shrink-0">HuggingFace:</span>
            <span className="text-text text-sm text-right break-words flex items-center gap-sm">
              <span className="font-mono text-sm text-text">{model.hfRepoId}</span>
              <button
                className="bg-transparent border-none cursor-pointer text-[1rem] p-[2px_4px] rounded-sm opacity-70 transition-all duration-200 shrink-0 hover:opacity-100 hover:bg-background-hover hover:scale-110 active:scale-95"
                onClick={() => {
                  const url = getHuggingFaceUrl(model.hfRepoId);
                  if (url) openUrl(url);
                }}
                title="Open on HuggingFace"
                aria-label="Open on HuggingFace"
              >
                <Icon icon={ExternalLink} size={14} />
              </button>
            </span>
          </div>
        )}
      </div>
    </section>
    
    <InferenceParametersForm
      value={editedInferenceDefaults}
      onChange={onInferenceDefaultsChange}
    />
    </>
  );
};
