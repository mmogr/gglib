/**
 * A context size as the inspector writes it. The launch plan's sentences
 * are not here: the daemon serves them (`plan.fit.sentence`,
 * `plan.unloads.sentence`), the words `gglib model explain` prints.
 *
 * @module readings/launchFit
 */

/** A context size: `32K` when it is whole thousands of 1,024, else the number. */
export const tokens = (n: number): string => (n % 1024 === 0 ? `${n / 1024}K` : n.toLocaleString('en-US'));
