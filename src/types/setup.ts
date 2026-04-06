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

/** Vulkan build-readiness status. */
export interface VulkanStatus {
  hasLoader: boolean;
  hasHeaders: boolean;
  hasGlslc: boolean;
  readyForBuild: boolean;
  missing: MissingPackage[];
}
