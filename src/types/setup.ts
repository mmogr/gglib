/**
 * Setup wizard types.
 * Types for the first-run system setup status and provisioning endpoints.
 */

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

/** Distro-specific install command for a missing Vulkan component. */
export interface InstallHint {
  distro: string;
  command: string;
}

/** A missing Vulkan build component with install hints. */
export interface MissingPackage {
  id: string;
  label: string;
  installHints: InstallHint[];
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

/** One system dependency's state, from `gglib config check-deps`. */
export interface DependencyInfo {
  name: string;
  status: 'present' | 'missing' | 'optional';
  /** Present only when `status` is `'present'`. */
  version?: string | null;
  description: string;
  required: boolean;
  installHint?: string | null;
}

/** Where everything resolves to — `gglib config paths`. */
export interface ResolvedPaths {
  dataRoot: string;
  resourceRoot: string;
  databasePath: string;
  llamaServerPath: string;
  modelsDir: string;
  /** How the models directory was chosen. */
  modelsSource: 'explicit' | 'envVar' | 'default';
}

/**
 * What acceleration a build would use. Detection refuses to fall back to CPU,
 * so a failure here is an answer with install hints, not a broken request.
 */
export interface AccelerationInfo {
  detected?: string | null;
  detectionError?: string | null;
}

/** The optional hf_xet download accelerator. Downloads work without it. */
export interface FastDownloadsInfo {
  provisioned: boolean;
  envDir: string;
  legacyPath: boolean;
  builder?: string | null;
  availableBuilder: string;
  error?: string | null;
}

/** The full diagnostics bundle behind the System tab. */
export interface Diagnostics {
  dependencies: DependencyInfo[];
  paths: ResolvedPaths;
  acceleration: AccelerationInfo;
  fastDownloads: FastDownloadsInfo;
}

/**
 * A hardware-sized model suggestion — the shortlist `gglib up` picks from.
 *
 * The route returns `null` when nothing fits, which is a real answer: a
 * machine too small for the smallest candidate should be told so.
 */
export interface ModelRecommendation {
  repo: string;
  quantization: string;
  /** Why this model, in the user's terms. */
  rationale: string;
  /** Weights plus KV cache at the candidate's context. */
  requiredBytes: number;
  budgetBytes: number;
  budgetSource: 'vram' | 'unifiedMemory' | 'systemRam';
  headroomBytes: number;
  context: number;
}
