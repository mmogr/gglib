/**
 * The `reasoning_effort` control, the three answers a surface can have
 * about whether it applies, and the levels it offers.
 *
 * A model whose template's list is known is offered only the levels on it,
 * and an unknown list offers all six. A saved level the list leaves out stays
 * the control's value, with a note: an unlisted level can still work, so the
 * note never says it will fail and nothing changes it.
 *
 * Split out of `SamplingForm` because it is the only field there
 * that is *conditional on the model*: every sampling parameter reaches the
 * sampler on every model, while an effort level reaches only a chat template
 * that reads the variable. That condition — and the fact that it has three
 * states, not two — is the whole content of this file.
 */

import { FC } from 'react';
import { Info } from 'lucide-react';
import type { TemplateSupport } from '../../types';
import type { ReasoningEffort } from '../../types/generated/ReasoningEffort';
import {
  REASONING_EFFORT_LEVELS,
  offeredLevels,
  type ReasoningEffortLevel,
} from '../../constants/reasoningEffort';
import { Icon } from '../ui/Icon';
import { Select } from '../ui/Select';

const DEFAULT_FIELD_ID = 'inference-param-reasoningEffort';

export interface ReasoningEffortFieldProps {
  /** The level currently set on this layer, or undefined when unset. */
  value: ReasoningEffortLevel | undefined;
  onChange: (level: ReasoningEffortLevel | undefined) => void;
  disabled: boolean;
  /**
   * DOM id for the control, so a second surface can render this without
   * colliding with the global form's. Defaults to the global form's id.
   */
  id?: string;
  /**
   * What this surface knows about the model's template.
   *
   * `undefined` means *this surface has no model* — the global settings
   * form, which edits a default that will meet every model in the library.
   * That is a property of the surface, not an unobserved model, and it gets
   * its own caption rather than borrowing the unobserved one.
   */
  support?: TemplateSupport;
  /**
   * The levels the model's template lists. `undefined` when the list is
   * unknown, and on a surface with no model: every level is offered.
   */
  levels?: readonly ReasoningEffort[];
}

/** The note under a level the template's list leaves out. */
function unlistedNote(value: ReasoningEffortLevel, listed: readonly ReasoningEffortLevel[]): string {
  const lists = listed.length > 0 ? ` (it lists ${listed.join(', ')})` : ' (it lists none)';
  return `This model's template does not list ${value}${lists}. The level is kept and still sent.`;
}

/**
 * What to say under the control, per state. Never silent: an effort level that
 * may or may not be read is exactly the case a caption exists for.
 */
function caption(support: TemplateSupport | undefined): string {
  switch (support) {
    case 'yes':
      return "This model's template reads reasoning effort, so a level set here is honoured.";
    case 'unknown':
      // The state most models are in. Worded as a gap in gglib's knowledge,
      // never as a property of the model — the level is still sent.
      return 'Not yet observed — start the model to find out whether its template reads this. A level set here is sent either way.';
    default:
      // No model in scope. Name the condition instead of the model, the same
      // move `fallbackCaption` makes for a value it cannot attribute.
      return 'Applies to models whose template declares reasoning effort; others ignore it.';
  }
}

/**
 * The `no` case: no control, and a sentence saying why.
 *
 * Hiding the control alone would be indistinguishable from the field not
 * existing, which is how a user concludes gglib cannot do this at all. The
 * composer's Tools ▸ is the precedent — unknown shows the checklist,
 * a definite "no" says "This model can't call tools".
 *
 * No state colour: whether a template reads a kwarg is a fact about the model,
 * not a degraded state, and `SamplingProvenanceSection` makes the same call
 * for the same reason.
 */
const UnsupportedNote: FC = () => (
  <div className="flex flex-col gap-[0.4rem]">
    <span className="text-sm font-semibold text-text">Reasoning Effort</span>
    <span className="flex items-start gap-xs text-xs text-text-muted">
      <Icon icon={Info} size={13} className="mt-[2px] shrink-0" />
      <span>
        This model&apos;s template does not read a reasoning level, so there is none to set here.
        The reasoning budget below still applies.
      </span>
    </span>
  </div>
);

export const ReasoningEffortField: FC<ReasoningEffortFieldProps> = ({
  value,
  onChange,
  disabled,
  support,
  levels,
  id = DEFAULT_FIELD_ID,
}) => {
  const captionId = `${id}-description`;
  const noteId = `${id}-unlisted`;

  // The one state that hides the control. `unknown` deliberately does not:
  // only a measured `no` says the template ignores the level (ADR 0007
  // decision 3), so hiding on `unknown` would gate the control on every model
  // nobody has launched — which is most of them.
  if (support === 'no') return <UnsupportedNote />;

  const offered = offeredLevels(levels);
  // Only a known list can leave a level out. The saved level keeps its option,
  // or the select would read blank for a value it still holds.
  const unlisted = value !== undefined && !offered.includes(value);
  const options = unlisted ? REASONING_EFFORT_LEVELS.filter((level) => level === value || offered.includes(level)) : offered;

  return (
    <div className="flex flex-col gap-[0.4rem]">
      <label htmlFor={id} className="text-sm font-semibold text-text">
        Reasoning Effort
      </label>
      <span className="block max-w-[220px]">
        <Select
          id={id}
          size="sm"
          disabled={disabled}
          value={value ?? ''}
          aria-describedby={unlisted ? `${captionId} ${noteId}` : captionId}
          onChange={(e) => onChange((e.target.value || undefined) as ReasoningEffortLevel | undefined)}
        >
          {/*
            Blank is not a seventh rung. Choosing it sends no key at all, which
            leaves the template's own default in place — measured as `medium` on
            gpt-oss. gglib offers no `none` level anywhere for exactly this
            reason: it would promise no thinking and deliver the template's
            preference.
          */}
          <option value="">Template&apos;s own default (send nothing)</option>
          {options.map((level) => (
            <option key={level} value={level}>
              {level}
            </option>
          ))}
        </Select>
      </span>
      <span id={captionId} className="text-xs text-text-muted italic">
        {caption(support)}
      </span>
      {unlisted && (
        <span className="flex items-start gap-xs text-xs text-text-muted">
          <Icon icon={Info} size={13} className="mt-[2px] shrink-0" />
          <span id={noteId}>{unlistedNote(value, offered)}</span>
        </span>
      )}
    </div>
  );
};
