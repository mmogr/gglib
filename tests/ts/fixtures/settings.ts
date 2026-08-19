/**
 * Global settings, as `GET /api/settings` actually sends them.
 *
 * All 23 fields are always present. The endpoint answers with
 * `gglib_app_services::types::AppSettings` — not `gglib_core::Settings`,
 * which is persisted and never serialized to a client — and no field of it
 * uses `skip_serializing_if`, so "nothing configured" crosses the wire as
 * `null` and never as an absent key. A fixture naming five keys was
 * describing a response no endpoint produces.
 *
 * The `null` baseline here is the fresh-install state, which is what the
 * hooks resolving their own fallbacks need to be tested against.
 */
import type { AppSettings } from '../../../src/types';

const UNSET: AppSettings = {
  defaultDownloadPath: null,
  defaultContextSize: null,
  proxyPort: null,
  llamaBasePort: null,
  maxDownloadQueueSize: null,
  showMemoryFitIndicators: null,
  maxToolIterations: null,
  maxStagnationSteps: null,
  defaultModelId: null,
  inferenceDefaults: null,
  inferenceProfiles: null,
  setupCompleted: null,
  titleGenerationPrompt: null,
  bindHost: null,
  shareLan: null,
  proxyApiKey: null,
  trustClientSampling: null,
  proxyLoopDetection: null,
  toolCallRepair: null,
  agenticSampling: null,
  proxyAutostart: null,
  closeToTray: null,
  startAtLogin: null,
};

/** Settings with `overrides` applied over a fresh install. */
export function appSettings(overrides: Partial<AppSettings> = {}): AppSettings {
  return { ...UNSET, ...overrides };
}
