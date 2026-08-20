/**
 * Setup wizard types.
 * Types for the first-run system setup status and provisioning endpoints.
 */

import type { DependencyDto } from './generated/DependencyDto';
import type { DiagnosticsDto } from './generated/DiagnosticsDto';
import type { RecommendationDto } from './generated/RecommendationDto';
import type { ResolvedPathsDto } from './generated/ResolvedPathsDto';

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

/** Models directory status. */
export interface ModelsDirectory {
  path: string;
  exists: boolean;
  writable: boolean;
}

/** System memory summary. */
export interface SystemMemory {
  totalRamBytes: number;
  gpuMemoryBytes?: number | null;
  isAppleSilicon: boolean;
}

/** Combined setup status returned by the setup-status endpoint. */
export interface SetupStatus {
  setupCompleted: boolean;
  llamaInstalled: boolean;
  llamaCanDownload: boolean;
  llamaPlatformDescription?: string | null;
  gpuInfo: GpuInfo;
  modelsDirectory: ModelsDirectory;
  pythonAvailable: boolean;
  fastDownloadReady: boolean;
  systemMemory?: SystemMemory | null;
}

/** SSE progress event for llama installation. */
export interface LlamaInstallProgress {
  downloaded: number;
  total: number;
}


/** What gglib recorded when it built the binary. Absent for a prebuilt install. */
export interface LlamaBuildInfo {
  version: string;
  commitSha: string;
  /** RFC 3339. */
  buildDate: string;
  acceleration: string;
  cmakeFlags: string[];
}

/** Native capabilities the binary reports for itself when probed. */
export interface LlamaRuntimeCapabilities {
  /** llama.cpp build number, absent when the binary could not be identified. */
  build?: number | null;
  commit?: string | null;
  versionLine: string;
  /** Capability flags, pre-rendered for display — do not parse. */
  flags: string;
}

/**
 * The installed llama.cpp, as reported by `gglib config llama status`.
 *
 * `build` and `runtime` are two different notions of version and can legitimately
 * disagree: the first is what gglib recorded at build time, the second is what
 * the binary says about itself. A hand-installed or prebuilt binary has no
 * `build` at all.
 */
export interface LlamaStatus {
  installed: boolean;
  binaryPath: string;
  configPath: string;
  healthy: boolean;
  healthError?: string | null;
  build?: LlamaBuildInfo | null;
  buildError?: string | null;
  runtime?: LlamaRuntimeCapabilities | null;
}

/** How far behind upstream the llama.cpp source checkout is. */
export interface LlamaUpdateCheck {
  installed: boolean;
  /** False for a prebuilt install: there is no repository to compare. */
  repoPresent: boolean;
  /**
   * Whether a comparison actually ran. When false, `commitsBehind` is 0
   * because nothing was compared — never present that as "up to date".
   */
  comparable: boolean;
  currentVersion?: string | null;
  currentAcceleration?: string | null;
  buildDate?: string | null;
  commitsBehind: number;
  recentCommits: string[];
}

/** What an uninstall removed. */
export interface LlamaUninstallOutcome {
  wasInstalled: boolean;
  removedPaths: string[];
}

/** Build pipeline phases, in order. */
export type BuildPhase =
  | 'dependency_check'
  | 'clone_or_update_repo'
  | 'configure'
  | 'compile'
  | 'install_binaries';

/** Streaming build events, shared by install-from-source and update. */
export type BuildEvent =
  | { type: 'phase_started'; phase: BuildPhase }
  | { type: 'log'; message: string }
  | { type: 'progress'; current: number; total: number }
  | { type: 'phase_completed'; phase: BuildPhase }
  | { type: 'completed'; version: string; acceleration: string }
  | { type: 'failed'; message: string };

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
