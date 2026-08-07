import { FC, useEffect, useState } from 'react';
import type { InferenceProfile } from '../../../types';
import { Skeleton } from '../../primitives';
import { Select } from '../../ui/Select';
import { useSamplingExplanation } from '../hooks/useSamplingExplanation';
import { InfoRow } from './InfoRow';
import { MetadataSection } from './MetadataSection';
import {
  PARAM_LABELS,
  caveats,
  describeSource,
  formatParamValue,
  resolvedValue,
} from '../../../utils/samplingProvenance';

interface SamplingProvenanceSectionProps {
  modelId: number;
  /** Configured profiles, from AppSettings. An empty list hides the selector. */
  profiles: InferenceProfile[];
  /** Invalidation signal — pass the model's stored defaults. */
  refreshKey?: unknown;
}

/**
 * What the model's sampling parameters actually resolve to, and which layer
 * supplied each one.
 *
 * Supersedes the stored-defaults view this replaced: a stored value that wins
 * appears here as `per-model defaults (user-set)`, and a stored value that
 * loses is finally visible as having lost. Every model gets a section, since
 * a model with nothing stored still resolves to something.
 *
 * Provenance is a fact about the model, not a state, so it borrows no state
 * colour — the same call the CLI's table makes.
 */
export const SamplingProvenanceSection: FC<SamplingProvenanceSectionProps> = ({
  modelId,
  profiles,
  refreshKey,
}) => {
  const [profileName, setProfileName] = useState<string | null>(null);

  // A profile selected for one model should not silently carry to the next.
  useEffect(() => setProfileName(null), [modelId]);

  const { explanation, isLoading, hasError } = useSamplingExplanation(
    modelId,
    profileName,
    refreshKey,
  );

  const selector = profiles.length > 0 && (
    <InfoRow label="Profile">
      <Select
        size="sm"
        aria-label="Inference profile"
        value={profileName ?? ''}
        onChange={(e) => setProfileName(e.target.value || null)}
        className="max-w-[12rem]"
      >
        <option value="">None</option>
        {profiles.map((profile) => (
          <option key={profile.name} value={profile.name}>
            {profile.name}
          </option>
        ))}
      </Select>
    </InfoRow>
  );

  // A null explanation with loading finished is a definitive "no data"
  // (e.g. a backend without the explain route resolves to null) — showing
  // skeletons for it would spin forever.
  if (hasError || (!explanation && !isLoading)) {
    return (
      <MetadataSection title="Sampling">
        {selector}
        <InfoRow label="Status">
          <span className="text-text-muted">Sampling provenance unavailable.</span>
        </InfoRow>
      </MetadataSection>
    );
  }

  if (!explanation) {
    return (
      <MetadataSection title="Sampling">
        {selector}
        {/* Deliberately unlabelled: a placeholder carrying real parameter
            names would claim to show a resolution that has not arrived. */}
        <div className="col-span-2">
          <Skeleton variant="text" count={4} />
        </div>
      </MetadataSection>
    );
  }

  const ctx = { profile: explanation.profile, isReasoning: explanation.isReasoning };

  return (
    <>
      <MetadataSection title="Sampling" className={isLoading ? 'opacity-60' : undefined}>
        {selector}
        {explanation.sources.map((entry) => (
          <InfoRow key={entry.param} label={PARAM_LABELS[entry.param] ?? entry.param}>
            <span className="tabular-nums">
              {formatParamValue(entry.param, resolvedValue(explanation.resolved, entry.param))}
            </span>
            <span className="text-text-muted"> {describeSource(entry, ctx)}</span>
          </InfoRow>
        ))}
      </MetadataSection>

      <div className="mt-md flex flex-col gap-xs text-xs text-text-muted">
        {caveats(explanation.trustClientSampling).map((note) => (
          <span key={note}>{note}</span>
        ))}
      </div>
    </>
  );
};
