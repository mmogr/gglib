/**
 * The Jinja launch control, in the three states a launch actually has.
 *
 * # Why this stopped being a checkbox
 *
 * gglib emits `--jinja` to turn templating on, `--no-jinja` to turn it off,
 * and *neither* when nobody chose — and llama-server initialises `use_jinja`
 * to true, so emitting neither leaves templating **on**. Off, On and Defer are
 * three distinct launches, which is why `JinjaMode` in `gglib-core` is a
 * three-variant enum rather than the bool it replaced.
 *
 * A checkbox has two states, so it had to fold two of those together, and it
 * folded the wrong pair: an untouched control on an untagged model showed
 * *unticked* and read "Disabled", describing a launch that would run with Jinja
 * on. Its description then advised leaving it off "for plain chat models",
 * which was advice to do something the control could not do. Both are fixed
 * here by giving the third state its own option instead of hiding it behind
 * the absence of the other two.
 */

import { FC } from 'react';
import { Select } from '../../ui/Select';

interface JinjaModeFieldProps {
  /**
   * `null` = defer (send no flag), `true` = `--jinja`, `false` = `--no-jinja`.
   * Mirrors `ServeConfig.jinja`, which is `undefined` for the deferred case.
   */
  value: boolean | null;
  /** An agent-tagged model has `--jinja` added for it when the choice is deferred. */
  hasAgentTag: boolean;
  disabled: boolean;
  onChange: (value: boolean) => void;
  /** Return to the deferred state. Distinct from choosing `false`. */
  onReset: () => void;
}

/**
 * What this launch will actually do, spelled out.
 *
 * Every branch names Jinja as on or off first, because that is the question,
 * and the reason second. The deferred-and-untagged branch is the one that used
 * to lie: nothing is sent, and llama-server's own default is on.
 */
function describeLaunch(value: boolean | null, hasAgentTag: boolean): string {
  if (value === true) return 'On — --jinja, chosen for this launch';
  if (value === false) return 'Off — --no-jinja, chosen for this launch';
  return hasAgentTag
    ? 'On — --jinja, added automatically for the agent tag'
    : "On — no flag sent, and llama-server's own default is on";
}

export const JinjaModeField: FC<JinjaModeFieldProps> = ({
  value,
  hasAgentTag,
  disabled,
  onChange,
  onReset,
}) => (
  <div className="mb-lg">
    <div className="flex items-center justify-between gap-sm">
      <label htmlFor="jinja-mode" className="block mb-0 font-medium text-text">
        Jinja Templates
      </label>
      <span className="text-sm text-text-muted">{describeLaunch(value, hasAgentTag)}</span>
    </div>
    <Select
      id="jinja-mode"
      className="mt-sm max-w-[240px]"
      disabled={disabled}
      value={value === null ? 'auto' : value ? 'on' : 'off'}
      onChange={(e) => {
        if (e.target.value === 'auto') onReset();
        else onChange(e.target.value === 'on');
      }}
    >
      <option value="auto">Auto — let llama-server decide</option>
      <option value="on">On — always send --jinja</option>
      <option value="off">Off — always send --no-jinja</option>
    </Select>
    <p className="mt-sm mb-0 text-sm text-text-secondary">
      Jinja templating is what renders tool definitions and template kwargs into the prompt.
      Turning it <em>off</em> strips those; it is not the setting for a plain chat model, which
      simply has no tool calls to render. Note that llama.cpp also reads{' '}
      <code className="font-mono">LLAMA_ARG_JINJA</code> from the environment — an explicit choice
      here still wins, because arguments beat the environment.
    </p>
  </div>
);
