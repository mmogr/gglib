import { FC, useState } from 'react';
import { Checkbox } from '../../ui/Checkbox';
import { CAPABILITY_FLAGS, type CapabilityFlagName } from '../../../types';
import { setModelCapabilities } from '../../../services/transport/api/models/local';

interface InspectorCapabilitiesProps {
  modelId: number;
  /** The capability bitfield from the model detail. Absent/0 = pass-through. */
  capabilities: number | undefined;
  /** Called after a successful change so the owner can reload the detail. */
  onChanged: () => void;
  onError: (message: string) => void;
}

const FLAG_COPY: Record<CapabilityFlagName, { label: string; description: string }> = {
  supportsSystemRole: {
    label: 'Supports system role',
    description: 'Off: system messages are converted to user messages before dispatch.',
  },
  requiresStrictTurns: {
    label: 'Requires strict turns',
    description: 'On: consecutive same-role messages are merged before dispatch.',
  },
  supportsToolCalls: {
    label: 'Supports tool calls',
    description: 'Off: tool definitions (tools/tool_choice) are stripped from requests.',
  },
  supportsReasoning: {
    label: 'Supports reasoning',
    description: 'Informational only — reasoning extraction follows the "reasoning" tag above.',
  },
};

/**
 * Capability flags editor — the GUI face of `gglib model capabilities`.
 * Each toggle PATCHes one flag; the server returns the updated model and
 * the owner reloads. All flags unset means pass-through mode.
 */
export const InspectorCapabilities: FC<InspectorCapabilitiesProps> = ({
  modelId,
  capabilities,
  onChanged,
  onError,
}) => {
  const [saving, setSaving] = useState(false);
  const bits = capabilities ?? 0;

  const toggle = async (flag: CapabilityFlagName, value: boolean) => {
    setSaving(true);
    try {
      await setModelCapabilities(modelId, { [flag]: value });
      onChanged();
    } catch (err) {
      onError(err instanceof Error ? err.message : String(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="mb-xl">
      <h3 className="m-0 mb-xs text-sm font-semibold text-text">Capabilities</h3>
      <p className="m-0 mb-xs text-xs text-text-muted">
        How the pipeline shapes messages for this model. Set at import from the GGUF; correct
        them here when detection got it wrong. Separate from the tags above — re-detecting tags
        does not change these.
      </p>
      <p className="m-0 mb-base text-xs text-text-muted">
        {bits === 0
          ? 'All unset: messages pass through untouched. Ticking any one box switches this model to enforced shaping, at which point the three boxes left unticked start being applied as “no”.'
          : 'Enforced: every unticked box is applied as “no”, not as “unknown”. Untick all four to return to pass-through.'}
      </p>
      <div className="flex flex-col gap-sm">
        {(Object.keys(CAPABILITY_FLAGS) as CapabilityFlagName[]).map((flag) => (
          <Checkbox
            key={flag}
            checked={(bits & CAPABILITY_FLAGS[flag]) !== 0}
            disabled={saving}
            onChange={(e) => void toggle(flag, e.target.checked)}
            label={FLAG_COPY[flag].label}
            description={FLAG_COPY[flag].description}
          />
        ))}
      </div>
    </section>
  );
};
