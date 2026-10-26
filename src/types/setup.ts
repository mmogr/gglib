/**
 * Setup wizard types.
 * Types for the first-run system setup status and provisioning endpoints.
 */

import type { DependencyDto } from './generated/DependencyDto';
import type { DiagnosticsDto } from './generated/DiagnosticsDto';
import type { ModelsDirectoryInfo } from './generated/ModelsDirectoryInfo';
import type { PinStanding } from './generated/PinStanding';
import type { PinState } from './generated/PinState';
import type { RuntimeKind } from './generated/RuntimeKind';
import type { RecommendationDto } from './generated/RecommendationDto';
import type { ResolvedPathsDto } from './generated/ResolvedPathsDto';
import type { SystemMemoryInfo } from './generated/SystemMemoryInfo';

/** GPU detection results. */
export interface GpuInfo {
  hasMetal: boolean;
  hasNvidia: boolean;
  hasVulkan: boolean;
  cudaVersion?: string | null;
  vulkanHeadersInstalled: boolean;
  vulkanGlslcInstalled: boolean;
  vulkanSpirvHeadersInstalled: boolean;
}

/**
 * Combined setup status returned by the setup-status endpoint.
 *
 * The models directory and the memory are the types the settings routes send,
 * so each has one shape wherever it is read.
 */
export interface SetupStatus {
  setupCompleted: boolean;
  llamaInstalled: boolean;
  /** Where the installed llama.cpp stands against its pinned release, and the two releases. */
  llamaPin: PinState;
  /** The chat model running on llama-server, when one is: no update is offered under it. */
  llamaServing?: string | null;
  llamaCanDownload: boolean;
  llamaPlatformDescription?: string | null;
  gpuInfo: GpuInfo;
  modelsDirectory: ModelsDirectoryInfo;
  pythonAvailable: boolean;
  fastDownloadReady: boolean;
  systemMemory?: SystemMemoryInfo | null;
}

/** Pre-built install phases, in order. */
export type InstallPhase =
  | 'check_availability'
  | 'fetch_release'
  | 'download'
  | 'extract'
  | 'cuda_runtime'
  | 'verify';

/**
 * Streaming pre-built install events.
 *
 * The daemon's install route streams these, and it is the one source of them
 * for the desktop app and a browser tab alike.
 *
 * `rateBps` and `etaSeconds` are absent until the backend's estimator has
 * warmed up, and absent is not zero — zero is a stalled transfer. Never derive
 * a rate here from successive `downloaded` values; the backend measures it.
 */
export type LlamaProgressEvent =
  | { type: 'phase_started'; phase: InstallPhase }
  | {
      type: 'progress';
      downloaded: number;
      total: number;
      rate_bps?: number;
      eta_seconds?: number;
    }
  | { type: 'phase_completed'; phase: InstallPhase }
  | { type: 'completed'; version: string }
  | { type: 'failed'; message: string };

/** How each install phase reads in the UI. */
export const INSTALL_PHASE_LABELS: Record<InstallPhase, string> = {
  check_availability: 'Checking platform availability…',
  fetch_release: 'Fetching release information…',
  download: 'Downloading llama.cpp binaries…',
  extract: 'Extracting binaries and libraries…',
  cuda_runtime: 'Downloading CUDA runtime libraries…',
  verify: 'Verifying installation…',
};

/**
 * How a phase reads for an install of `product`: the download names it, as
 * `InstallPhase::label_for` does in Rust; every other phase reads the same.
 * For `llama.cpp` it is {@link INSTALL_PHASE_LABELS}.
 */
export function installPhaseLabel(phase: InstallPhase, product = 'llama.cpp'): string {
  return phase === 'download' ? `Downloading ${product} binaries…` : INSTALL_PHASE_LABELS[phase];
}

/** What each engine is called: the name an install's labels and its messages use. */
export const ENGINE_PRODUCT: Record<RuntimeKind, string> = {
  llama: 'llama.cpp',
  stable_diffusion: 'stable-diffusion.cpp',
};

/**
 * Either engine as Settings shows it: what is installed and how, where it
 * stands against its pin, the download an install would take and what to
 * say about it, and the model running on it.
 */
export type { EngineStatus } from './generated/EngineStatus';
export type { EngineInstall } from './generated/EngineInstall';
/** What an engine's removal took. */
export type { UninstallOutcome } from './generated/UninstallOutcome';
export type { PinStanding, PinState, RuntimeKind };

/*
 * The diagnostics bundle behind the System tab, and the recommendation `gglib
 * up` offers on a first run, are generated from their Rust DTOs.
 *
 * Three fields are `String` in Rust with the union written only in a doc
 * comment, so ts-rs renders them as `string`. Each is intersected back over
 * the generated shape, the form `src/types/README.md` prescribes for a
 * narrowing — `X & { field: Union }`, so the narrowed member is the one that
 * survives.
 */

/** One system dependency's state, from `gglib config check-deps`. */
export type DependencyInfo = DependencyDto & {
  status: 'present' | 'missing' | 'optional';
};

/** Where everything resolves to — `gglib config paths`. */
export type ResolvedPaths = ResolvedPathsDto & {
  modelsSource: 'explicit' | 'envVar' | 'default';
};

/**
 * The full diagnostics bundle behind the System tab.
 *
 * `dependencies` is replaced rather than intersected: in
 * `DependencyDto[] & DependencyInfo[]`, `.map` and `.filter` type their
 * callback's element from the first constituent, so the panel's rows would see
 * the generated `status` and lose the union. `src/types/README.md` allows
 * `Omit` here because `DiagnosticsDto` is a plain object type, not a union of
 * arms. `paths` needs no such treatment — it is not an array, so intersecting
 * it narrows `modelsSource` correctly.
 */
export type Diagnostics = Omit<DiagnosticsDto, 'dependencies'> & {
  dependencies: DependencyInfo[];
  paths: ResolvedPaths;
};

type AssertNoUnlistedFields<T extends never> = T;
/**
 * Fails to compile if the wrapper's keys and `DiagnosticsDto`'s keys ever
 * differ in either direction: a narrowed field renamed or dropped in Rust, or
 * one added here that the generated shape does not carry. A field *added* to
 * `DiagnosticsDto` is not drift — `Omit` propagates it — so it stays quiet.
 * `Omit<T, K>` constrains `K` to `keyof any`, not `keyof T`, so the
 * key above cannot drift-check itself: a Rust rename would leave a phantom
 * field here and an un-narrowed one beside it, with the bindings still
 * matching their Rust.
 *
 * Unreferenced by design, and exported because `noUnusedLocals` is on and an
 * unused type alias is not exempt — the same shape as
 * `InferenceConfigKeysAreComplete` and `ReasoningLadderIsComplete`.
 */
export type DiagnosticsKeysAreComplete = AssertNoUnlistedFields<
  | Exclude<keyof Diagnostics, keyof DiagnosticsDto>
  | Exclude<keyof DiagnosticsDto, keyof Diagnostics>
>;

/**
 * A hardware-sized model suggestion — the shortlist `gglib up` picks from.
 *
 * The route returns `null` when nothing fits, which is a real answer: a
 * machine too small for the smallest candidate should be told so.
 */
export type ModelRecommendation = RecommendationDto & {
  budgetSource: 'vram' | 'unifiedMemory' | 'systemRam';
};
