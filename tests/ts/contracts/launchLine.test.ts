/**
 * The inspector's launch line is the command line the CLI prints.
 * `LaunchPlan::command_line` writes it; `gglib-cli`'s launch-display tests
 * record a few plans beside the line it makes of each in
 * `contracts/models/launch_line.json` (an ordinary launch, values that need
 * quoting for a shell, `$` among them, an image model's); the page's formatter must make the
 * same line of the same plan.
 *
 * Nothing here reads Rust source.
 */

import { describe, it, expect } from 'vitest';

import { commandLine } from '../../../src/inspector/local/commandLine';
import type { LaunchPlan } from '../../../src/types/generated/LaunchPlan';

import { rust } from './rustSource';

const RECORDED = JSON.parse(rust('contracts/models/launch_line.json')) as { plan: LaunchPlan; line: string }[];

describe('the launch line is the command line the CLI records', () => {
  it('records more than one plan, quoting among them', () => {
    expect(RECORDED.length).toBeGreaterThan(1);
    expect(RECORDED.some((r) => r.line.includes("'"))).toBe(true);
    // A `$` a shell would expand is among them, quoted.
    expect(RECORDED.some((r) => r.line.includes(" '$HOME'"))).toBe(true);
    // And a value of `=,+%:@` a shell reads as itself, left as it is.
    expect(RECORDED.some((r) => r.line.includes(' k=v,a+b%c:d@e'))).toBe(true);
  });

  it.each(RECORDED.map((r) => [r.line, r] as const))('%s', (_line, recorded) => {
    expect(commandLine(recorded.plan)).toBe(recorded.line);
  });
});
