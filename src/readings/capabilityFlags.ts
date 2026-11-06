/**
 * The capability bitflags, mirroring `gglib_core::domain::ModelCapabilities`
 * (a bare u32 on the wire). The contract test `capabilityFlags.test.tsx`
 * holds these values to `contracts/models/capability_flags.json`.
 *
 * @module readings/capabilityFlags
 */

export const CAPABILITY_FLAGS = {
  supportsSystemRole: 0b0001,
  requiresStrictTurns: 0b0010,
  supportsToolCalls: 0b0100,
  supportsReasoning: 0b1000,
} as const;

export type CapabilityFlagName = keyof typeof CAPABILITY_FLAGS;
