import { FC } from 'react';
import { Loader2, Play } from 'lucide-react';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';
import { Modal } from '../../ui/Modal';
import type { GgufModel, AppSettings } from '../../../types';
import { formatParamCount } from '../../../utils/format';

interface ServeModalProps {
  model: GgufModel;
  settings: AppSettings | null;
  // State
  customContext: string;
  customPort: string;
  jinjaOverride: boolean | null;
  isServing: boolean;
  hasAgentTag: boolean;
  // Handlers
  onContextChange: (value: string) => void;
  onPortChange: (value: string) => void;
  onJinjaChange: (value: boolean) => void;
  onJinjaReset: () => void;
  onClose: () => void;
  onStart: () => void;
}

/**
 * Modal for configuring and starting a model server.
 */
export const ServeModal: FC<ServeModalProps> = ({
  model,
  settings,
  customContext,
  customPort,
  jinjaOverride,
  isServing,
  hasAgentTag,
  onContextChange,
  onPortChange,
  onJinjaChange,
  onJinjaReset,
  onClose,
  onStart,
}) => {
  const effectiveJinjaEnabled = jinjaOverride === null ? hasAgentTag : jinjaOverride;
  const isAutoJinja = jinjaOverride === null && hasAgentTag;

  return (
    <Modal open={true} onClose={onClose} title="Start model server" size="md" preventClose={isServing}>
      <div className="modal-body">
        <div className="model-info">
          <strong>{model.name}</strong>
          <span className="model-size">{formatParamCount(model.param_count_b)}</span>
        </div>

        <div className="form-group">
          <label htmlFor="context-input">
            Context Length
            <span className="label-hint"> (optional)</span>
          </label>
          <input
            id="context-input"
            type="number"
            className="context-input"
            placeholder={
              settings?.default_context_size
                ? `Default: ${settings.default_context_size.toLocaleString()}`
                : model.context_length
                  ? `Model max: ${model.context_length.toLocaleString()}`
                  : 'Enter context length'
            }
            value={customContext}
            onChange={(e) => onContextChange(e.target.value)}
            disabled={isServing}
            min="1"
          />
          <p className="input-help">
            {model.context_length
              ? `Model's maximum: ${model.context_length.toLocaleString()} tokens`
              : 'No model context metadata available'}
          </p>
        </div>

        <div className="form-group">
          <label htmlFor="port-input">
            Port
            <span className="label-hint"> (optional)</span>
          </label>
          <input
            id="port-input"
            type="number"
            className="context-input"
            placeholder={
              settings?.llama_base_port
                ? `Auto (from ${settings.llama_base_port})`
                : 'Auto (from 9000)'
            }
            value={customPort}
            onChange={(e) => onPortChange(e.target.value)}
            disabled={isServing}
            min="1024"
            max="65535"
          />
          <p className="input-help">
            Leave empty to auto-allocate from base port
          </p>
        </div>

        {hasAgentTag && (
          <div className="jinja-alert" role="status">
            <div className="jinja-alert-title">Agent tag detected</div>
            <p>
              Jinja templates {jinjaOverride === false 
                ? 'would normally be auto-enabled for agent-tagged models, but you have disabled them for this launch.' 
                : 'will be enabled automatically for agent-tagged models to support structured prompts.'}
            </p>
            {jinjaOverride !== null && (
              <Button
                type="button"
                variant="ghost"
                size="sm"
                onClick={onJinjaReset}
                disabled={isServing}
              >
                Reset to auto-detect
              </Button>
            )}
          </div>
        )}

        <div className="form-group">
          <div className="form-label-row">
            <label htmlFor="jinja-toggle">Jinja Templates</label>
            <span className="jinja-mode-label">
              {isAutoJinja
                ? 'Auto (agent tag)'
                : (jinjaOverride === null
                  ? 'Disabled'
                  : (jinjaOverride ? 'Enabled manually' : 'Disabled manually'))}
            </span>
          </div>
          <div className="jinja-toggle-row">
            <input
              id="jinja-toggle"
              type="checkbox"
              checked={effectiveJinjaEnabled}
              onChange={(e) => onJinjaChange(e.target.checked)}
              disabled={isServing}
            />
            <div className="jinja-toggle-copy">
              <p>
                Enable llama.cpp's Jinja templating for instruction/agent models. Leave off for plain chat models.
              </p>
            </div>
          </div>
        </div>
      </div>

      <div className="modal-footer">
        <Button variant="ghost" onClick={onClose} disabled={isServing}>
          Cancel
        </Button>
        <Button
          variant="primary"
          onClick={onStart}
          disabled={isServing}
          leftIcon={!isServing ? <Icon icon={Play} size={14} /> : undefined}
        >
          {isServing ? (
            <>
              <Loader2 className="spinner" />
              Loading model...
            </>
          ) : (
            'Start Server'
          )}
        </Button>
      </div>
    </Modal>
  );
};
