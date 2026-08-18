import type { ModelBenchmarkSummary } from './benchmark';
import type { ReasoningEffortLevel } from '../constants/reasoningEffort';
import type { ProvenanceParamKey, SuppressedEffort, TemplateSupport } from './reasoning';

// The reasoning-control wire shapes live in their own module — see its header
// for why — and are re-exported here so every caller keeps importing from
// `../types`.
export type { ProvenanceParamKey, SuppressedEffort, TemplateSupport } from './reasoning';

// ============================================================================
// Inference Configuration
// ============================================================================

/**
 * Inference parameters for LLM sampling.
 * All fields are optional to support partial configuration and fallback chains.
 * 
 * Hierarchy resolution (backend automatically applies):
 * 1. Request-level override (user specified for this request)
 * 2. Per-model defaults (stored in model.inferenceDefaults)
 * 3. Global settings (stored in AppSettings.inferenceDefaults)
 * 4. Hardcoded fallback (e.g., temperature = 0.7)
 */
import type { InferenceConfig } from './generated/InferenceConfig';
export type { InferenceConfig };

/**
 * A partially-specified {@link InferenceConfig} — what an edit form holds
 * mid-edit, and what a starter profile stores.
 *
 * The wire always sends all eighteen fields, `null` for the ones nothing has
 * set, which is why the generated type has no optional keys. A form is a
 * different thing: it needs to express "this field is not in play" by the
 * key being absent, and to `delete` a key when the user clears a control.
 */
export type SparseInferenceConfig = Partial<InferenceConfig>;

/** Capability bitflags, mirroring `gglib_core::domain::ModelCapabilities` (a bare u32 on the wire). */
export const CAPABILITY_FLAGS = {
  supportsSystemRole: 0b0001,
  requiresStrictTurns: 0b0010,
  supportsToolCalls: 0b0100,
  supportsReasoning: 0b1000,
} as const;

export type CapabilityFlagName = keyof typeof CAPABILITY_FLAGS;

/** `PATCH /api/models/{id}/capabilities` body — absent fields are left unchanged. */
export interface SetCapabilitiesRequest {
  supportsSystemRole?: boolean;
  requiresStrictTurns?: boolean;
  supportsToolCalls?: boolean;
  supportsReasoning?: boolean;
}

/** What a retag pass changed (`POST /api/models/{id}/retag`). */
export type { RetagResponse } from './generated/RetagResponse';

/** Commit-SHA update check (`GET /api/models/{id}/upgrade-check`). */
export interface UpgradeCheck {
  hasUpdate: boolean;
  currentSha?: string | null;
  latestSha: string;
}

/** Outcome of `POST /api/models/{id}/upgrade`. */
export interface UpgradeOutcome {
  updated: boolean;
  latestSha: string;
  filePath?: string | null;
}

/**
 * A named sampling profile, selectable per request as `<model>:<profile>`.
 *
 * Profiles are global: one `coding` profile applies to every model. They are
 * deliberately *sparse* — only the fields set here override, and everything
 * left undefined falls through to the model's own defaults, which is what
 * makes one profile safe to apply across differing model architectures.
 */
import type { InferenceProfile } from './generated/InferenceProfile';
export type { InferenceProfile };

/**
 * A profile whose config is only partly specified — a starter template, or one
 * being edited. The wire form carries all eighteen sampling fields; a template
 * that names two is not a lesser wire value, it is a different thing.
 */
export type SparseInferenceProfile = Omit<InferenceProfile, 'config'> & {
  config: SparseInferenceConfig;
};

// ============================================================================
// Sampling provenance
// ============================================================================

/**
 * A rung of the resolution ladder, in priority order.
 *
 * Mirrors the backend `SamplingLayerDto`
 * (`gglib-app-services/src/sampling_explain.rs`), which is the wire form of
 * `gglib_core::domain::SamplingLayer`.
 */
// Imported as well as re-exported: a bare `export … from` does not bind the
// name in this module, and declarations below refer to it.
import type { SamplingLayerDto as SamplingLayerName } from './generated/SamplingLayerDto';
export type { SamplingLayerName };

/**
 * What class of source supplied a resolved value.
 *
 * Mirrors the backend `ProvenanceKindDto`. `floorCoupled` is distinct from
 * `floor`: it means a layer claimed `temperature` and this parameter is tuned
 * against it, so lower layers were deliberately passed over rather than simply
 * having nothing to say. `suppressedByTemplate` means a layer named a value and
 * a request-shaping stage then threw it away, because the model's observed chat
 * template does not read that field (ADR 0007) — distinct from `unset`, where
 * nobody named anything at all.
 */
export type ProvenanceKind = 'layer' | 'floor' | 'floorCoupled' | 'unset' | 'suppressedByTemplate';

/**
 * The numeric parameters with a bounded range — the keys of `INFERENCE_PARAMS`
 * and `PARAM_LABELS`.
 *
 * Membership here is a claim that `tests/ts/contracts/settingsBounds.test.ts`
 * can check: every key needs a `{ default, min, max, step }` entry that the
 * test reads Rust's `validate_inference_config` and `with_hardcoded_defaults`
 * to verify. `reasoningBudgetTokens` qualifies — Rust bounds it at `>= -1` and
 * floors it at unset — and joins for exactly that reason.
 *
 * `reasoningEffort` does not, and cannot: it is a string enum with no bounds
 * and no numeric default, so an entry for it would be a fabricated row in a
 * table whose entire purpose is to be checkable. It lives in
 * {@link ProvenanceParamKey} instead.
 */
export type SamplingParamKey =
  | 'temperature'
  | 'topP'
  | 'topK'
  | 'presencePenalty'
  | 'repeatPenalty'
  | 'minP'
  | 'frequencyPenalty'
  | 'dynatempRange'
  | 'dynatempExponent'
  | 'topNSigma'
  | 'dryMultiplier'
  | 'dryBase'
  | 'dryAllowedLength'
  | 'dryPenaltyLastN'
  | 'maxTokens'
  | 'reasoningBudgetTokens';

/**
 * Where one resolved parameter's value came from.
 *
 * Mirrors the backend `ParamProvenanceDto`. `layer` is present only when
 * `kind` is `'layer'`.
 */
export interface ParamProvenance {
  /** The key this entry describes in {@link SamplingExplanation.resolved}. */
  param: ProvenanceParamKey;
  kind: ProvenanceKind;
  layer?: SamplingLayerName;
}

/**
 * A model's resolved sampling parameters and where each value came from —
 * `GET /api/models/:id/explain`.
 *
 * Mirrors the backend `SamplingExplanationDto`. The values live in `resolved`
 * rather than beside each provenance entry, so `sources[i].param` is a key
 * into `resolved`.
 */
export interface SamplingExplanation {
  /** The fully resolved configuration, after floors. */
  resolved: InferenceConfig;
  /** Per-parameter provenance, already in display order. */
  sources: ParamProvenance[];
  /** The profile applied, if one was named. */
  profile?: string | null;
  /** Whether the model carries the `reasoning` tag, which selects the floor. */
  isReasoning: boolean;
  /** Whether client-supplied sampling is trusted — a rung the table cannot show. */
  trustClientSampling: boolean;
  /**
   * What the model's own GGUF publishes, for the fields it publishes at all.
   *
   * Empty on almost every model, and absent entirely from a backend that
   * predates the field — so treat `undefined` as "nothing published", never as
   * an error.
   */
  published?: PublishedDefault[];
  /**
   * Where the model's stored defaults came from.
   *
   * `published` and `auto_detected` share a ladder rung — both are unreviewed,
   * so both rank below global settings — which means `ParamProvenance.layer`
   * alone cannot name its own source. Without this, a recipe fetched from the
   * model author renders as gglib's reasoning-tag guess.
   */
  defaultsOrigin?: DefaultsOriginName | null;
  /**
   * The `reasoningEffort` this model's template would ignore, when the stored
   * configuration resolves one it does not read.
   *
   * The suppression is in {@link sources} too, as a `suppressedByTemplate`
   * entry — but that entry has overwritten the rung that asked, and `resolved`
   * carries nothing, so between them a reader learns only that *something* was
   * dropped. This is the level and the layer.
   *
   * Conditional, not historical: the endpoint explains stored configuration, so
   * nothing has been sent. Word it as what *would* happen on a request.
   * Absent on a backend that predates the field, which reads as "no suppression
   * to report".
   */
  effortSuppressed?: SuppressedEffort | null;
}

/**
 * Where a model's stored `inferenceDefaults` came from.
 *
 * - `user` — set by a person. Outranks global settings.
 * - `auto_detected` — gglib's `reasoning`-tag guess. Ranks below.
 * - `published` — the author's `generation_config.json`, read at import. Ranks
 *   exactly where `auto_detected` does; what differs is the evidence.
 * - `measured` — a tune sweep's winner on this hardware, ranked with those two.
 *
 * Snake_case: this mirrors `gglib_core::domain::DefaultsOrigin` itself, which
 * is the one spelling every endpoint reporting the column now uses.
 */
import type { DefaultsOrigin as DefaultsOriginName } from './generated/DefaultsOrigin';
export type { DefaultsOriginName };

/**
 * What gglib does with one field's published recommendation.
 *
 * Mirrors the backend `PublishedStateDto`. There is no `notPublished` arm: a
 * field with no author recommendation is simply absent from the list.
 *
 * - `deferred` — gglib names nothing, so llama.cpp applies the model's number.
 * - `restated` — gglib sends the same number the model published.
 * - `overridden` — gglib sends a different number. The only one that warns.
 * - `unreadable` — the published value could not be parsed, so gglib cannot say
 *   what it displaced. Renders as unknown, never as an override.
 */
export type PublishedState = 'deferred' | 'restated' | 'overridden' | 'unreadable';

/**
 * One field's published value and what gglib does with it — the `published`
 * entries of `GET /api/models/:id/explain`.
 *
 * Since llama.cpp PR #17120 a `general.sampling.*` key becomes the server's
 * default for every field gglib does not name, so a `kind: 'unset'` provenance
 * means *the model's own number applies* on a model that published one, and
 * *the build's default applies* on one that did not. Those render identically
 * without this.
 */
export interface PublishedDefault {
  /** Joins to {@link ParamProvenance.param}. */
  param: SamplingParamKey;
  /** The GGUF key carrying it, e.g. `general.sampling.penalty_repeat`. */
  key: string;
  state: PublishedState;
  /** What the model author published. Absent only when `state` is `unreadable`. */
  published?: number;
  /** What gglib sends instead. Present only when `state` is `overridden`. */
  sending?: number;
}

// ============================================================================
// Server Configuration
// ============================================================================

/**
 * Per-model server defaults.
 * Overrides global settings for specific llama-server launch parameters.
 */
import type { ServerConfig } from './generated/ServerConfig';
export type { ServerConfig };

// ============================================================================
// Model Types
// ============================================================================

export interface GgufModel {
  /** Capability bitfield — see CAPABILITY_FLAGS. Absent/0 = unknown, pass-through. */
  capabilities?: number;
  id?: number;
  name: string;
  filePath: string;
  paramCountB: number;
  architecture?: string;
  quantization?: string;
  contextLength?: number;
  expertCount?: number;
  expertUsedCount?: number;
  expertSharedCount?: number;
  addedAt: string;
  hfRepoId?: string;
  tags?: string[];
  // Server status
  isServing?: boolean;
  port?: number;
  // Inference defaults
  inferenceDefaults?: InferenceConfig;
  /**
   * Where `inferenceDefaults` came from — see {@link DefaultsOriginName}.
   *
   * Everything but `user` ranks below the global inference defaults in the
   * resolution hierarchy; see the `InferenceConfig` doc comment above.
   */
  defaultsOrigin?: DefaultsOriginName | null;
  // Per-model server defaults (overrides global settings for launch params)
  serverDefaults?: ServerConfig;
  // Benchmark summary (cached from benchmark_summaries table)
  benchmarkSummary?: ModelBenchmarkSummary;
}

/**
 * Full detail for a single model — superset of GgufModel.
 * Returned by `GET /api/models/:id/detail` and mirrors the backend `ModelDetailDto`.
 * Adds HuggingFace provenance, download timestamps, and raw GGUF metadata.
 */
export interface ModelDetail extends GgufModel {
  /** Original filename on HuggingFace (e.g. "Meta-Llama-3-8B-Instruct-Q4_K_M.gguf"). */
  hfFilename?: string;
  /** Git commit SHA of the HF repo snapshot at download time. */
  hfCommitSha?: string;
  /** ISO-8601 timestamp of when the model was first downloaded. */
  downloadDate?: string;
  /** ISO-8601 timestamp of the last update-check for this model. */
  lastUpdateCheck?: string;
  /** Raw GGUF key-value metadata pairs (may be large). */
  metadata: Record<string, string>;
  /**
   * Whether this model's chat template reads `reasoning_effort`.
   *
   * Only the model *detail* carries it: the observation is taken from
   * `GET /props` at launch and stored on the row, and the list endpoint does
   * not publish it. Absent on a backend that predates the field — which reads
   * as `'unknown'`, the one default that cannot hide the control from a model
   * that supports it.
   */
  reasoningEffortSupport?: TemplateSupport;
}


export interface ServeConfig {
  id: number;
  contextLength?: number;
  mlock?: boolean;
  port?: number;
  jinja?: boolean;
  /** Number of MTP draft tokens. undefined = auto-detect from tags; 0 = disable. */
  specDraftNMax?: number;
  /** Minimum acceptance probability for MTP draft tokens (default 0.75). */
  specDraftPMin?: number;
  // Inference parameters for this serve session
  temperature?: number;
  topP?: number;
  topK?: number;
  maxTokens?: number;
  repeatPenalty?: number;
  presencePenalty?: number;
  minP?: number;
  /**
   * Session reasoning controls, forwarded to `inference_params`.
   *
   * Listed because the serve modal offers them: this struct is a hand-kept
   * subset of {@link InferenceConfig}, not a projection of it, so a field the
   * modal renders and this omits is a control the user can set and the launch
   * silently discards. (Nine of `InferenceConfig`'s fields are in exactly that
   * position already — see `tests/ts/contracts/startServerRequest.test.ts`.)
   */
  reasoningEffort?: ReasoningEffortLevel;
  reasoningBudgetTokens?: number;
}

/**
 * A running server as `GET /api/servers` reports it.
 *
 * Mirrors `gglib_app_services::types::ServerInfo`, which carries no
 * `rename_all` — these keys really are snake_case, unlike the camelCase
 * `AppEvent` frames describing the same servers. Not what the UI renders
 * from: that is `ServerViewModel`, built off the event registry, and it has
 * a `status` this shape has no answer for.
 */
export type { ServerInfo } from './generated/ServerInfo';

/**
 * `source` widens to `string` here, from the hand-written
 * `'explicit' | 'environment' | 'default'`. Its Rust field is a plain `String`
 * produced by `format_source()`, so the narrow union was a claim the wire does
 * not make. Its only consumer indexes a `Record<string, string>`, so the
 * widening costs nothing; narrowing it properly is a Rust-side change.
 */
export type { ModelsDirectoryInfo } from './generated/ModelsDirectoryInfo';

export interface AppSettings {
  defaultDownloadPath?: string | null;
  defaultContextSize?: number | null;
  proxyPort?: number | null;
  llamaBasePort?: number | null;
  maxDownloadQueueSize?: number | null;
  titleGenerationPrompt?: string | null;
  showMemoryFitIndicators?: boolean | null;
  /** Maximum iterations for tool calling agentic loop (default: 25) */
  maxToolIterations?: number | null;
  /** Default model ID for quick commands (e.g., `gglib question`) */
  defaultModelId?: number | null;
  /** Global inference parameter defaults */
  inferenceDefaults?: InferenceConfig | null;
  /** Named sampling profiles, selectable per request as `<model>:<profile>` */
  inferenceProfiles?: InferenceProfile[] | null;
  /** Whether the setup wizard has been completed */
  setupCompleted?: boolean | null;
  /**
   * Bearer token the proxy requires on `/v1/*` and `/mcp`; `/health` stays
   * open. Unset leaves the endpoint unauthenticated, which is the default for
   * a loopback bind. The proxy sets this itself the first time it binds a
   * non-loopback host, so the GUI dashboard reads it from here to authenticate
   * against the proxy it started.
   */
  proxyApiKey?: string | null;
  /**
   * Whether a client's own sampling parameters (temperature, topP, topK,
   * presencePenalty, repeatPenalty, minP) are honoured by the proxy at all.
   * `false`/unset (the default) drops them from the resolution hierarchy —
   * only `maxTokens` still comes from the request. Most clients that talk to
   * this proxy send fixed sampling values with no user-facing control behind
   * them (VS Code Copilot's LLM Gateway, for one), so trusting them by
   * default would let that boilerplate silently outrank this server's own
   * per-model and global defaults.
   */
  trustClientSampling?: boolean | null;
  /**
   * Whether the proxy's turn-level loop/stagnation guard runs on
   * /v1/chat/completions. Unset/`true` (the default) means active: a
   * conversation whose replayed history repeats the same tool-call batch or
   * assistant response beyond the threshold is rejected with a clean 400
   * before any model work. `false` disables the guard — the escape hatch for
   * a client that legitimately repeats identical requests.
   */
  proxyLoopDetection?: boolean | null;
  /**
   * Whether the desktop app starts the proxy as soon as it launches. Desktop
   * app only — `gglib proxy` and `gglib serve` stay explicit foreground
   * commands.
   */
  proxyAutostart?: boolean | null;
  /** Whether closing the desktop window hides to the tray instead of quitting. */
  closeToTray?: boolean | null;
  /** Whether the desktop app registers itself to launch on login. */
  startAtLogin?: boolean | null;
  /**
   * Maximum consecutive no-progress agent steps before the loop stops.
   * Shared by the built-in agent loop and the proxy's turn-level guard.
   */
  maxStagnationSteps?: number | null;
  /** Literal IP the daemon binds. Unset = the compiled-in 127.0.0.1. */
  bindHost?: string | null;
  /** Whether the daemon binds beyond loopback. Unset/false = localhost-only. */
  shareLan?: boolean | null;
  /**
   * Whether a structured-output turn gets its temperature capped when no
   * human chose one. Unset/true = active; anything set by a person stands.
   */
  agenticSampling?: boolean | null;
  /**
   * Whether a tool call that fails schema validation is re-issued with
   * `tool_choice: "required"`. Unset/`true` (the default) means active.
   * `false` forwards the malformed call as the model produced it — which is
   * what you want when you are measuring a model's own behaviour rather than
   * using it.
   */
  toolCallRepair?: boolean | null;
}

export interface UpdateSettingsRequest {
  defaultDownloadPath?: string | null | undefined;
  defaultContextSize?: number | null | undefined;
  proxyPort?: number | null | undefined;
  llamaBasePort?: number | null | undefined;
  maxDownloadQueueSize?: number | null | undefined;
  titleGenerationPrompt?: string | null | undefined;
  showMemoryFitIndicators?: boolean | null | undefined;
  /** Maximum iterations for tool calling agentic loop (default: 25) */
  maxToolIterations?: number | null | undefined;
  /** Default model ID for quick commands (e.g., `gglib question`) */
  defaultModelId?: number | null | undefined;
  /** Global inference parameter defaults */
  inferenceDefaults?: SparseInferenceConfig | null | undefined;
  /**
   * Replaces the whole profile list. `null` clears it; omitting the key
   * leaves it untouched, so an unrelated settings update cannot drop
   * profiles it never knew about.
   */
  inferenceProfiles?: SparseInferenceProfile[] | null | undefined;
  /** Whether the setup wizard has been completed */
  setupCompleted?: boolean | null | undefined;
  /** See `AppSettings.proxyApiKey`. */
  proxyApiKey?: string | null | undefined;
  /** See `AppSettings.trustClientSampling`. */
  trustClientSampling?: boolean | null | undefined;
  /** See `AppSettings.proxyLoopDetection`. */
  proxyLoopDetection?: boolean | null | undefined;
  /** See `AppSettings.proxyAutostart`. */
  proxyAutostart?: boolean | null | undefined;
  /** See `AppSettings.closeToTray`. */
  closeToTray?: boolean | null | undefined;
  /** See `AppSettings.startAtLogin`. */
  startAtLogin?: boolean | null | undefined;
  /** See `AppSettings.maxStagnationSteps`. */
  maxStagnationSteps?: number | null | undefined;
  /** See `AppSettings.bindHost`. */
  bindHost?: string | null | undefined;
  /** See `AppSettings.shareLan`. */
  shareLan?: boolean | null | undefined;
  /** See `AppSettings.agenticSampling`. */
  agenticSampling?: boolean | null | undefined;
  /** See `AppSettings.toolCallRepair`. */
  toolCallRepair?: boolean | null | undefined;
}

// ============================================================================
// System Memory Types (for "Will it fit?" indicators)
// ============================================================================

/**
 * System memory information for model fit calculations.
 */
export interface SystemMemoryInfo {
  /** Total system RAM in bytes */
  totalRamBytes: number;
  /** GPU memory in bytes (VRAM for discrete GPUs, or unified memory portion for Apple Silicon) */
  gpuMemoryBytes?: number | null;
  /** Whether the system has Apple Silicon with unified memory */
  isAppleSilicon: boolean;
  /** Whether the system has an NVIDIA GPU */
  hasNvidiaGpu: boolean;
}

/**
 * Fit status for a model quantization based on available memory.
 */
export type FitStatus = 'fits' | 'tight' | 'wont_fit' | 'unknown';

// ============================================================================
// Server Health Types (for monitoring server lifecycle)
// ============================================================================

/**
 * Health status for a running server.
 * Maps to gglib-core::ports::server_health::ServerHealthStatus.
 * Uses 'status' as discriminant to match Rust serde(tag = "status").
 */
import type { ServerHealthStatus } from './generated/ServerHealthStatus';
export type { ServerHealthStatus };

/**
 * Semantic tone for a health state. Callers map this to a token colour
 * rather than baking a colour (or a coloured emoji) into the model layer.
 */
export type HealthTone = 'healthy' | 'degraded' | 'failed' | 'unknown';

/**
 * Structured detail for a model runtime failure.
 * Maps to gglib-core::ports::model_runtime::RuntimeErrorEnvelope.
 */
import type { RuntimeErrorEnvelope as RuntimeErrorInfo } from './generated/RuntimeErrorEnvelope';
export type { RuntimeErrorInfo };

/**
 * Get display info for a health status (tone, label, title).
 */
export function getHealthDisplay(health?: ServerHealthStatus): { tone: HealthTone; label: string; title: string } {
  if (!health) {
    return { tone: 'unknown', label: 'Unknown', title: 'No health data yet' };
  }

  switch (health.status) {
    case 'healthy':
      return { tone: 'healthy', label: 'Healthy', title: 'Server responding normally' };

    case 'degraded':
      return {
        tone: 'degraded',
        label: 'Degraded',
        title: health.reason ?? 'Health checks reporting degraded state',
      };

    case 'unreachable':
      return {
        tone: 'failed',
        label: 'Unreachable',
        title: health.lastError ?? 'Health endpoint not reachable',
      };

    case 'processdied':
      return {
        tone: 'failed',
        label: 'Process died',
        title: 'Server process appears to have exited',
      };

    default:
      return { tone: 'unknown', label: 'Unknown', title: 'Unrecognized health state' };
  }
}

// ============================================================================
// Download Types (re-exported from transport types for convenience)
// ============================================================================

export type {
  DownloadStatus,
  ShardInfo,
  DownloadQueueItem,
  DownloadQueueStatus,
  DownloadCompletionInfo,
} from '../services/transport/types/downloads';

export type {
  DownloadSummary,
  DownloadEvent,
} from '../services/transport/types/events';

// ============================================================================
// HuggingFace Browser Types
// ============================================================================

/**
 * Summary of a HuggingFace model from the search API.
 */
export interface HfModelSummary {
  /** Model ID (e.g., "TheBloke/Llama-2-7B-GGUF") */
  id: string;
  /** Human-readable model name (derived from id) */
  name: string;
  /** Author/organization (e.g., "TheBloke") */
  author?: string | null;
  /** Total download count */
  downloads: number;
  /** Like count */
  likes: number;
  /** Last modified timestamp */
  last_modified?: string | null;
  /** Total parameter count in billions (from safetensors.total) */
  parameters_b?: number | null;
  /** Model description/README excerpt */
  description?: string | null;
  /** Model tags */
  tags: string[];
}

/**
 * Sort field options for HuggingFace model search.
 */
import type { HfSortField } from './generated/HfSortField';
export type { HfSortField };

/**
 * Request for searching HuggingFace models.
 */
export interface HfSearchRequest {
  /** Search query (model name) */
  query?: string | null;
  /** Minimum parameters in billions */
  min_params_b?: number | null;
  /** Maximum parameters in billions */
  max_params_b?: number | null;
  /** Page number (0-indexed) */
  page: number;
  /** Results per page (default 30) */
  limit: number;
  /** Sort field (default: downloads) */
  sort_by?: HfSortField;
  /** Sort direction: true = ascending, false = descending (default: false/descending) */
  sort_ascending?: boolean;
}

/**
 * Response from HuggingFace model search.
 */
export interface HfSearchResponse {
  /** Models matching the search criteria */
  models: HfModelSummary[];
  /** Whether more results are available */
  has_more: boolean;
  /** Current page number (0-indexed) */
  page: number;
  /** Total count of matching models (if available) */
  total_count?: number | null;
}

/**
 * Information about a specific quantization variant.
 */
export interface HfQuantization {
  /** Quantization name (e.g., "Q4_K_M", "Q8_0") */
  name: string;
  /** File path within the repository */
  file_path: string;
  /** File size in bytes */
  size_bytes: number;
  /** File size in MB (for display) */
  size_mb: number;
  /** Whether this is a sharded model (multiple files) */
  is_sharded: boolean;
  /** Number of shards if sharded */
  shard_count?: number | null;
}

/**
 * Response containing available quantizations for a model.
 */
export type { HfQuantizationsResponse } from './generated/HfQuantizationsResponse';

/**
 * Response for tool/function calling support detection.
 *
 * Used for both HuggingFace model metadata and local running server queries.
 */
export interface ToolSupportResponse {
  /** Whether the model supports tool/function calling */
  supports_tool_calls: boolean;
  /** Confidence level of the detection (0.0 to 1.0) */
  confidence: number;
  /** Detected tool calling format (e.g., "hermes", "llama3", "mistral") */
  detected_format?: string | null;
}

// ============================================================================
// Model Filter Options Types
// ============================================================================

/**
 * A range of numeric values with min and max.
 */
export interface RangeValues {
  min: number;
  max: number;
}

/**
 * Filter options for the model library UI.
 * Contains aggregate data about available models for building dynamic filter controls.
 */
export interface ModelFilterOptions {
  /** All distinct quantization types present in the library */
  quantizations: string[];
  /** Minimum and maximum parameter counts (in billions) */
  param_range: RangeValues | null;
  /** Minimum and maximum context lengths */
  context_range: RangeValues | null;
  /** Token-generation speed range (t/s) derived from benchmark data.
   *  Only present when at least one model has been benchmarked. */
  speed_range?: RangeValues | null;
}

/**
 * Sort field for `GET /api/models`.
 * Matches the backend `ModelSortBy` domain enum (snake_case).
 */
export type { ModelSortBy } from './generated/ModelSortBy';

/**
 * Sort direction for model list queries.
 */
export type { SortOrder } from './generated/SortOrder';


