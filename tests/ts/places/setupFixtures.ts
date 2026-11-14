/**
 * The daemon's answers setup reads, every field present: the setup status
 * of a Mac nothing has run on yet, an engine's status as
 * `GET /api/config/system/engines/{kind}` sends it, and the shortlist's
 * suggestion as the recommend route sends it.
 */

import type { ProxyStatus } from '../../../src/services/transport/types/proxy';
import type { EngineStatus, PinState, RecommendationDto, RuntimeKind, SetupStatus } from '../../../src/types/setup';

const GIB = 1024 ** 3;

/** The state the daemon serves for an engine (`EngineState::of`). */
export function engineState(installed: boolean, broken: string | null): EngineStatus['state'] {
  if (!installed) return 'not_installed';
  return broken === null ? 'installed' : 'not_working';
}

/** The setup status of a Mac nothing has run on, the accelerator On. */
export function setupStatus(overrides: Partial<SetupStatus> = {}): SetupStatus {
  return {
    setupCompleted: false,
    llamaInstalled: false,
    llamaPin: { standing: 'missing', installedRelease: null, targetRelease: 'b11429', built: null, rebuilds: false, updateDownloads: true },
    llamaServing: null,
    llamaCanDownload: true,
    llamaPlatformDescription: 'macOS ARM64 (Metal)',
    gpuInfo: {
      hasMetal: true,
      hasNvidia: false,
      hasVulkan: false,
      cudaVersion: null,
      vulkanHeadersInstalled: false,
      vulkanGlslcInstalled: false,
      vulkanSpirvHeadersInstalled: false,
    },
    modelsDirectory: {
      path: '/Users/me/.local/share/llama_models',
      source: 'default',
      default_path: '/Users/me/.local/share/llama_models',
      exists: false,
      writable: true,
      state: 'absent',
    },
    pythonAvailable: true,
    fastDownloads: 'on',
    systemMemory: { totalRamBytes: 48 * GIB, gpuMemoryBytes: 36 * GIB, isUnifiedMemory: true, hasNvidiaGpu: false },
    ...overrides,
  };
}

const RELEASE: Record<RuntimeKind, string> = { llama: 'b11429', stable_diffusion: 'master-948-228c707' };
const WORD: Record<RuntimeKind, string> = { llama: 'llama', stable_diffusion: 'sd' };

/** `kind`'s status: missing with the Metal download, unless `installed`. */
export function engine(kind: RuntimeKind, installed = false, overrides: Partial<EngineStatus> = {}, pin: Partial<PinState> = {}): EngineStatus {
  return {
    kind,
    installed,
    broken: null,
    state: engineState(installed, overrides.broken ?? null),
    binaryPath: `/data/${kind}`,
    install: installed ? { built: false, release: RELEASE[kind], platform: 'Metal', date: '2026-10-26T00:00:00Z' } : null,
    recordError: null,
    reports: null,
    pin: {
      standing: installed ? 'atPin' : 'missing',
      installedRelease: installed ? RELEASE[kind] : null,
      targetRelease: RELEASE[kind],
      built: installed ? false : null,
      rebuilds: false,
      updateDownloads: !installed,
      ...pin,
    },
    standing: installed ? `Pinned release: ${RELEASE[kind]}, which is the one installed.` : null,
    download: 'macOS ARM64 (Metal)',
    downloadUnavailable: null,
    warning: null,
    serving: null,
    installCommand: `gglib config ${WORD[kind]} install`,
    ...overrides,
  };
}

export const REC: RecommendationDto = {
  repo: 'unsloth/Qwen3-8B-GGUF',
  rationale: 'strong tool calling at 8B',
  requiredBytes: Math.round(6.5 * GIB),
  budgetBytes: Math.round(43.2 * GIB),
  budgetSource: 'unifiedMemory',
  budgetLabel: 'unified memory',
  headroomBytes: Math.round(36.7 * GIB),
  context: 32768,
};

export const UP: ProxyStatus = { running: true, state: 'on', port: 8080, base_url: 'http://127.0.0.1:8080/v1', key: 'none_needed', pinned_model: null, pinned_to: null, pinned_model_id: null };
export const DOWN: ProxyStatus = { running: false, state: 'off', port: null, base_url: null, key: null, pinned_model: null, pinned_to: null, pinned_model_id: null };
