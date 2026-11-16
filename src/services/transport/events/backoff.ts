/**
 * Exponential backoff with jitter for reconnection.
 */
export class Backoff {
  private ms: number;
  private readonly minMs: number;
  private readonly maxMs: number;

  constructor(minMs = 500, maxMs = 30000) {
    this.ms = minMs;
    this.minMs = minMs;
    this.maxMs = maxMs;
  }

  next(): number {
    const jitter = Math.floor(Math.random() * 250);
    const out = Math.min(this.ms, this.maxMs) + jitter;
    this.ms = Math.min(this.ms * 2, this.maxMs);
    return out;
  }

  /** Back to the first wait, the one it was made with. */
  reset(): void {
    this.ms = this.minMs;
  }
}
