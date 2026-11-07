/**
 * How it runs: the launch gglib would make, as the launch line and then
 * one row per decision (`runRows`), each with whose choice it was and why
 * on hover. On a loaded model it is How it ran: the decisions its launch
 * recorded (`LaunchFacts.narration`), as launched, where a change applies
 * at the next load. Change turns a row into its control (`Change`); a row
 * the person set reads `set by you · Reset`. One footnote ends it: a value
 * the model's author published that gglib overrides, and the two rungs the
 * rows cannot show (operator flags, client sampling).
 *
 * @module inspector/local/HowItRuns
 */

import { useState, type FC } from 'react';
import { Select } from '../../components/ui/Select';
import type { GgufModel, InferenceProfile, LaunchExplanation, ModelDetail } from '../../types';
import type { LaunchNarration } from '../../types/generated/LaunchNarration';
import { caveats, describePublished } from '../../utils/samplingProvenance';
import { CHANGEABLE, Change, reset, type Update } from './Change';
import { LaunchLine } from './LaunchLine';
import { Row } from './Row';
import {
  REASONING_KEYS,
  SAMPLING_KEYS,
  argsValue,
  argsWhy,
  chosenBy,
  contextNote,
  labelOf,
  runRows,
  samplingValue,
  setByYou,
  settingsLine,
  type RunRow,
} from './rows';

interface HowItRunsProps {
  model: GgufModel;
  detail: ModelDetail | null;
  explanation: LaunchExplanation | null;
  /** The explanation could not be read. */
  failed: boolean;
  /** Set when the model is loaded: the rows are its launch's. */
  narration?: LaunchNarration | null;
  profiles: InferenceProfile[];
  profile: string | null;
  onProfile: (name: string | null) => void;
  onUpdateModel: Update;
  /** A change was saved: read the explanation again. */
  onChanged: () => void;
}

const Caption: FC<{ title: string; aside: string }> = ({ title, aside }) => (
  <div className="flex items-baseline justify-between">
    <h3 className="m-0 text-xs font-semibold text-text">{title}</h3>
    <span className="text-xs text-text-muted">{aside}</span>
  </div>
);

export const HowItRuns: FC<HowItRunsProps> = (props) => {
  const { model, detail, explanation, failed, narration, profiles, profile, onProfile, onUpdateModel, onChanged } = props;
  const [changing, setChanging] = useState<string | null>(null);
  const sampling = explanation?.sampling ?? null;
  const plan = explanation?.plan ?? null;
  const loaded = narration !== undefined;

  const control = (kind: string) =>
    changing !== kind ? null : (
      <Change
        kind={kind}
        model={model}
        detail={detail}
        sampling={sampling}
        servesAt={plan?.fit?.servesAt ?? null}
        note={loaded ? 'Applies at next load.' : undefined}
        onUpdateModel={onUpdateModel}
        onDone={(saved) => {
          setChanging(null);
          if (saved) onChanged();
        }}
      />
    );
  const changeable = (kind: string) => (CHANGEABLE.has(kind) ? () => setChanging(kind) : undefined);
  const resetter = (kind: string, set: boolean) => {
    const updates = set ? reset(kind, model) : null;
    return updates ? () => void onUpdateModel(model.id, updates).then(onChanged) : undefined;
  };

  if (loaded) {
    // Nothing recorded (or not read yet): an empty section is not drawn.
    if (!narration?.decisions.length) return null;
    return (
      <section aria-label="How it ran" className="flex flex-col gap-xs">
        <Caption title="How it ran" aside="as launched" />
        <dl className="m-0">
          {narration.decisions.map((d) => {
            const kind = d.label === 'flags' ? 'templates' : d.label;
            return (
              <Row
                key={d.label}
                label={labelOf(d.label)}
                value={d.value}
                why={d.source ?? undefined}
                onChange={changeable(kind)}
                control={control(kind)}
              />
            );
          })}
        </dl>
      </section>
    );
  }

  if (!plan || !sampling) {
    return (
      <section aria-label="How it runs" className="flex flex-col gap-xs">
        <Caption title="How it runs" aside="" />
        {failed && <p className="m-0 text-xs text-text-secondary">How this model would load is unavailable.</p>}
      </section>
    );
  }

  const rows = runRows(plan, model.imageFamily !== undefined);
  const value = (row: RunRow) => {
    if (row.key === 'ctx') return [argsValue(row.args), contextNote(plan.fit)].filter(Boolean).join(' · ');
    if (row.key === 'images') return row.args.length ? argsValue(row.args) : 'None';
    if (row.key === 'sampling') return samplingValue(sampling);
    if (row.key === 'reasoning') {
      const own = settingsLine(sampling, REASONING_KEYS).set;
      return [row.args.length ? argsValue(row.args) : null, own].filter(Boolean).join(' · ') || "the template's own";
    }
    if (row.key === 'parts') {
      const missing = model.missingComponents.map((role) => `${role} missing`);
      return [row.args.length ? argsValue(row.args) : null, ...missing].filter(Boolean).join(' · ') || 'None';
    }
    return argsValue(row.args);
  };
  const isSet = (row: RunRow) => {
    if (row.key === 'ctx') return plan.fit?.source === 'model_server_defaults';
    if (row.key === 'sampling') return setByYou(sampling, SAMPLING_KEYS);
    if (row.key === 'reasoning') return setByYou(sampling, REASONING_KEYS);
    return row.args.some((a) => a.source === 'you') && (row.key === 'templates' || row.key === 'mtp');
  };

  return (
    <section aria-label="How it runs" className="flex flex-col gap-xs">
      <Caption title="How it runs" aside={chosenBy(plan.args)} />
      <LaunchLine plan={plan} />
      <dl className="m-0">
        {rows.map((row) => (
          <Row
            key={row.key}
            label={row.label}
            value={
              <>
                {value(row)}
                {row.key === 'sampling' && profiles.length > 0 && (
                  <Select
                    size="sm"
                    aria-label="As profile"
                    value={profile ?? ''}
                    onChange={(e) => onProfile(e.target.value || null)}
                    className="ml-sm inline-block w-auto"
                  >
                    <option value="">as profile: none</option>
                    {profiles.map((p) => (
                      <option key={p.name} value={p.name}>
                        {`as profile: ${p.name}`}
                      </option>
                    ))}
                  </Select>
                )}
              </>
            }
            why={row.args.length ? argsWhy(row.args) : undefined}
            onReset={resetter(row.key, isSet(row))}
            onChange={changeable(row.key)}
            control={control(row.key)}
          />
        ))}
      </dl>
      <p className="m-0 text-xs text-text-muted">
        {[
          ...sampling.published.filter((p) => p.state === 'overridden').map((p) => `${describePublished(p)}.`),
          ...caveats(sampling.trustClientSampling),
        ].join(' ')}
      </p>
    </section>
  );
};
