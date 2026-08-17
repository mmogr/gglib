/**
 * Whether this model's chat template reads `reasoning_effort` — the fact the
 * effort control everywhere else is gated on, stated in one place.
 *
 * # The remedy for a stale answer is a measurement
 *
 * There is no manual override here, and that is deliberate. The answer comes
 * from llama-server's own `chat_template_caps`, computed by executing the
 * template with instrumented variable access (ADR 0007) — the only detector
 * that cannot disagree with the renderer is the renderer itself. A checkbox
 * letting an operator assert "yes it does" would let a stored opinion outrank
 * the thing being described, which is the mistake this arc exists to undo.
 *
 * So the action offered is a re-observation. gglib takes the reading once per
 * launch, from a server it has just confirmed healthy, which means:
 *
 *   - while the model runs, re-reading the row is the whole re-check — the
 *     observation is taken by a detached task after the launch returns, so an
 *     inspector opened during startup can be holding a row older than the
 *     server;
 *   - while it does not, nothing can change until it is started, and saying
 *     "start it" is more honest than a button that re-reads an answer nothing
 *     has updated.
 */

import { FC } from 'react';
import { Play, RefreshCw } from 'lucide-react';
import type { TemplateSupport } from '../../../types';
import { Button } from '../../ui/Button';
import { Icon } from '../../ui/Icon';

interface ReasoningSupportProps {
  /** Absent while the detail loads, and on a backend without the field. */
  support: TemplateSupport | undefined;
  /** Whether this model has a server running right now. */
  isRunning: boolean;
  /** Re-read the model detail. Only offered while the model runs. */
  onRecheck: () => void;
  /** Open the serve modal — the only way to take a first reading. */
  onStart: () => void;
  /** True while a re-read is in flight. */
  isRechecking: boolean;
}

/**
 * The three answers, in the words the rest of the GUI uses for them.
 *
 * `unknown` is phrased as a gap in gglib's knowledge and never as a property
 * of the model, because it is by far the most common row: caps are read from a
 * running server, so every model nobody has launched here answers this way.
 */
function describeSupport(support: TemplateSupport | undefined): string {
  switch (support) {
    case 'yes':
      return 'Reads reasoning effort — observed from the template when this model was last served.';
    case 'no':
      return 'Does not read reasoning effort — observed from the template when this model was last served. A level resolved for this model is removed before the request is sent, and gglib says so rather than letting it vanish.';
    default:
      return 'Not yet observed — start the model to check. Until then a level set for this model is sent as given, because an unmeasured template is not a template that refuses.';
  }
}

export const ReasoningSupport: FC<ReasoningSupportProps> = ({
  support,
  isRunning,
  onRecheck,
  onStart,
  isRechecking,
}) => (
  <section className="mb-xl">
    <h3 className="m-0 mb-xs text-sm font-semibold text-text">Reasoning effort</h3>
    {/*
      No state colour on any of the three. Whether a template reads a kwarg is
      a fact about the model, like its quantization — not a health state, and
      not a failure when the answer is no.
    */}
    <p className="m-0 mb-base text-xs text-text-muted">{describeSupport(support)}</p>
    {isRunning ? (
      <Button
        type="button"
        variant="secondary"
        size="sm"
        onClick={onRecheck}
        isLoading={isRechecking}
        leftIcon={!isRechecking ? <Icon icon={RefreshCw} size={13} /> : undefined}
      >
        Re-check
      </Button>
    ) : (
      <Button
        type="button"
        variant="secondary"
        size="sm"
        onClick={onStart}
        leftIcon={<Icon icon={Play} size={13} />}
      >
        Start the model to check
      </Button>
    )}
  </section>
);
