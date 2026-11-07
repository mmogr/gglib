import { FC, useState } from 'react';
import { Checkbox } from '../../components/ui/Checkbox';
import { CAPABILITY_FLAGS, type CapabilityFlagName } from '../../readings/capabilityFlags';
import { setModelCapabilities } from '../../services/transport/api/models/local';
import { formatError } from '../../utils/errors';

interface AbilitiesProps {
  modelId: number;
  /** The capability bitfield from the model detail. Absent/0 = pass-through. */
  capabilities: number | undefined;
  /** Called after a successful change so the owner can reload the detail. */
  onChanged: () => void;
  onError: (message: string) => void;
}

/** Each flag's label: its name as words, so the contract test can read it back. */
const LABELS: Record<CapabilityFlagName, string> = {
  supportsSystemRole: 'Supports system role',
  requiresStrictTurns: 'Requires strict turns',
  supportsToolCalls: 'Supports tool calls',
  supportsReasoning: 'Supports reasoning',
};

/**
 * Details › Abilities: the four capability flags, the GUI face of `gglib
 * model capabilities`, and one footnote saying what the boxes mean together.
 * Each box PATCHes one flag; the owner reads the detail again. All four
 * unset is pass-through.
 */
export const Abilities: FC<AbilitiesProps> = ({ modelId, capabilities, onChanged, onError }) => {
  const [saving, setSaving] = useState(false);
  const bits = capabilities ?? 0;

  const toggle = async (flag: CapabilityFlagName, value: boolean) => {
    setSaving(true);
    try {
      await setModelCapabilities(modelId, { [flag]: value });
      onChanged();
    } catch (err) {
      onError(formatError(err));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex flex-col gap-xs">
      {(Object.keys(CAPABILITY_FLAGS) as CapabilityFlagName[]).map((flag) => (
        <Checkbox
          key={flag}
          checked={(bits & CAPABILITY_FLAGS[flag]) !== 0}
          disabled={saving}
          onChange={(e) => void toggle(flag, e.target.checked)}
          label={LABELS[flag]}
        />
      ))}
      <p className="m-0 text-xs text-text-secondary">
        {bits === 0
          ? 'All unset: messages pass through as sent. Ticking one makes every unticked box a no.'
          : 'Every unticked box is applied as no. Untick all four to pass messages through as sent.'}
      </p>
    </div>
  );
};
