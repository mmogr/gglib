import type { ProvenanceParamKey, SuppressedEffort } from './reasoning';

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

/**
 * A model as the library list sends it — `GET /api/models`.
 *
 * Named `GgufModel` at the call sites and `GuiModel` in Rust. Eight fields
 * the mirror made optional are required on the wire: `id`, `capabilities`,
 * `tags` and `isServing` are always sent, and `architecture`,
 * `quantization`, `contextLength` and `hfRepoId` are always-present
 * nullables rather than absent keys. Every read site already tests
 * truthiness or `!= null`, so the narrowing is free at the call sites and
 * only fixtures had to grow.
 *
 * The three MoE fields stay optional: they carry `skip_serializing_if`, so a
 * dense model genuinely omits them.
 */
import type { GuiModel } from './generated/GuiModel';
export type GgufModel = GuiModel;

/**
 * Full detail for a single model — `GET /api/models/:id/detail`.
 *
 * Named `ModelDetail` at the call sites and `ModelDetailDto` in Rust; the
 * alias keeps the GUI's spelling without inventing a second shape.
 *
 * It is *not* an extension of `GuiModel`, though the hand-written mirror said
 * so twice: `ModelDetailDto` carries neither `serverDefaults` nor
 * `benchmarkSummary`, so the inheritance advertised two fields the endpoint
 * has never sent. Nothing read them.
 */
import type { ModelDetailDto } from './generated/ModelDetailDto';
export type ModelDetail = ModelDetailDto;


/**
 * One serve session's launch options — the body of `POST /api/servers/start`.
 *
 * Launch-only fields are declared here; the sampling half is
 * {@link SparseInferenceConfig}, so every field the wire's `InferenceConfig`
 * carries is available and none has to be hand-transcribed.
 *
 * It used to be a hand-kept *subset*, and the subset had drifted: nine
 * `InferenceConfig` fields — `frequencyPenalty`, the two dynatemp knobs,
 * `topNSigma`, the four DRY knobs and `seed` — were absent, so a control the
 * modal rendered was a control the launch silently discarded. Extending the
 * generated shape is what stops that recurring: a field added in Rust arrives
 * here without an edit.
 */
export interface ServeConfig extends SparseInferenceConfig {
  id: number;
  contextLength?: number;
  mlock?: boolean;
  port?: number;
  jinja?: boolean;
  /** Number of MTP draft tokens. undefined = auto-detect from tags; 0 = disable. */
  specDraftNMax?: number;
  /** Minimum acceptance probability for MTP draft tokens (default 0.75). */
  specDraftPMin?: number;
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

/**
 * Global settings — `GET`/`PUT /api/settings`.
 *
 * All 23 fields are required keys carrying `T | null`, not optional keys.
 * Nothing in `gglib-core`'s `Settings` uses `skip_serializing_if`, so the
 * response always names every field; "nothing configured" is a `null`, never
 * an absent key. The mirror's `?:` described a shape the endpoint does not
 * send, and every read site is already `settings?.x`-style, so adopting the
 * true one changes no call site.
 *
 * The *request* shape is different and stays hand-written: `PUT` treats an
 * absent key as "leave unchanged" and an explicit `null` as "clear", which is
 * the `double_option` three-state — see {@link UpdateSettingsRequest}.
 */
import type { AppSettings } from './generated/AppSettings';
export type { AppSettings };

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


