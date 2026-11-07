/**
 * The launch line: the command gglib would run, as `LaunchPlan::command_line`
 * writes it (`crates/gglib-core/src/domain/launch_plan.rs`). The program,
 * then one token per argument, a value that holds a space, a quote or a tab
 * (or is empty) in single quotes. `tests/ts/contracts/launchLine.test.ts`
 * holds `commandLine` to the lines the CLI's tests record.
 *
 * @module inspector/local/commandLine
 */

import type { LaunchArg } from '../../types/generated/LaunchArg';
import type { LaunchPlan } from '../../types/generated/LaunchPlan';

/** `value` as a shell token: as it is, or in single quotes when it needs them. */
export function shellToken(value: string): string {
  return value === '' || /[ '"\t]/.test(value) ? `'${value.replaceAll("'", "'\\''")}'` : value;
}

/** One argument as the line writes it: its flag, then its value when it takes one. */
export function argText(arg: LaunchArg): string {
  return arg.value == null ? arg.flag : `${arg.flag} ${shellToken(arg.value)}`;
}

/** The whole line, tokens joined by single spaces. */
export function commandLine(plan: Pick<LaunchPlan, 'program' | 'args'>): string {
  return [plan.program, ...plan.args.map(argText)].join(' ');
}
