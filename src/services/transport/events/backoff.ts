/**
 * Exponential backoff with jitter for reconnection.
 */
export class Backoff {
  private ms = 500;
  private readonly maxMs: number;

  constructor(minMs = 500, maxMs = 30000) {
    this.ms = minMs;
    this.maxMs = maxMs;
  }

  next(): number {
    const jitter = Math.floor(Math.random() * 250);
    const out = Math.min(this.ms, this.maxMs) + jitter;
    this.ms = Math.min(this.ms * 2, this.maxMs);
    return out;
  }

  reset(): void {
    this.ms = 500;
  }
}
