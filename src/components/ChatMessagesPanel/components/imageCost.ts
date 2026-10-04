/**
 * What an image in the composer costs, as its tile says it: the prompt
 * tokens its upload was estimated at, and their share of the model's
 * context when that is known.
 */
export function imageCost(tokens: number, contextLength: number | null): string {
  const cost = `~${tokens.toLocaleString('en-US')} tokens`;
  if (!contextLength) return cost;
  const share = (tokens / contextLength) * 100;
  return `${cost} · ${share < 1 ? '<1' : Math.round(share)}% of context`;
}
