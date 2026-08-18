import type { ProvenanceParamKey } from './reasoning';

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

/**
 * `PATCH /api/models/{id}/capabilities` body — absent fields are left unchanged.
 *
 * `Partial` over the generated shape, deliberately. ts-rs renders an
 * `Option<T>` as a required `T | null` unless the field carries
 * `#[ts(optional)]`, but serde lets a missing `Option<T>` deserialize to
 * `None` on its own — no attribute involved — so the handler genuinely
 * accepts omission, and omission is what "left unchanged" means. The one call
 * site builds a single-key literal from a computed flag name, which the
 * generated shape rejects. Adding `#[ts(optional)]` to the four fields would
 * let this drop the wrapper.
 */
import type { SetCapabilitiesRequest as SetCapabilitiesBody } from './generated/SetCapabilitiesRequest';
export type SetCapabilitiesRequest = Partial<SetCapabilitiesBody>;

/** What a retag pass changed (`POST /api/models/{id}/retag`). */
export type { RetagResponse } from './generated/RetagResponse';

/** Commit-SHA update check (`GET /api/models/{id}/upgrade-check`). */
export type { UpgradeCheck } from './generated/UpgradeCheck';

/** Outcome of `POST /api/models/{id}/upgrade`. */
export type { UpgradeOutcome } from './generated/UpgradeOutcome';

/**
 * A named sampling profile, selectable per request as `<model>:<profile>`.
 *
 * Profiles are global: one `coding` profile applies to every model, which
 * works because a profile only displaces the parameters it names — the rest
 * fall through to the model's own defaults, and that is what makes one
 * profile safe across differing architectures.
 *
 * "Only the ones it names" is a fact about the *stored* profile, not about
 * this type: a saved profile round-trips through `InferenceConfig`, so it
 * carries all eighteen keys with `null` where nothing was set, and `null` is
 * how the resolver hears "say nothing about this one".
 * {@link SparseInferenceProfile} below is the shape that genuinely has fewer
 * keys — a starter template, or one mid-edit.
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
import type { ParamProvenanceDto } from './generated/ParamProvenanceDto';
export type ParamProvenance = ParamProvenanceDto & {
  /**
   * The key this entry describes in {@link SamplingExplanation.resolved}.
   *
   * Narrowed from the generated `string` by intersection — `string & Union`
   * collapses to the union — because three call sites index `PARAM_LABELS`,
   * `publishedByParam` and `formatProvenanceValue` with it, and Rust types the
   * field as a plain `String`. This is the one place the mirror was better
   * than the generated shape; narrowing it properly is a Rust-side change.
   *
   * The union is `ProvenanceParamKey`, which is *seventeen* members and not
   * `InferenceConfig`'s eighteen. `FieldSources::iter` — what `wire_key`
   * renders — names seventeen fields and has none for `seed`, so `seed` never
   * appears in `sources` and the explain table has no row for it.
   */
  param: ProvenanceParamKey;
};

/**
 * A model's resolved sampling parameters and where each value came from —
 * `GET /api/models/:id/explain`.
 *
 * Mirrors the backend `SamplingExplanationDto`. The values live in `resolved`
 * rather than beside each provenance entry, so `sources[i].param` is a key
 * into `resolved`.
 */
import type { SamplingExplanationDto } from './generated/SamplingExplanationDto';
export type SamplingExplanation = Omit<SamplingExplanationDto, 'sources' | 'published'> & {
  /** Per-parameter provenance, already in display order. */
  sources: ParamProvenance[];
  /**
   * What the model's own GGUF publishes, for the fields it publishes at all.
   *
   * Always sent — an empty array on almost every model. The mirror made it
   * optional to tolerate a backend predating the field; that backend is gone
   * and the key is unconditional.
   */
  published: PublishedDefault[];
};

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
 * One field's published value and what gglib does with it — the `published`
 * entries of `GET /api/models/:id/explain`.
 *
 * Since llama.cpp PR #17120 a `general.sampling.*` key becomes the server's
 * default for every field gglib does not name, so a `kind: 'unset'` provenance
 * means *the model's own number applies* on a model that published one, and
 * *the build's default applies* on one that did not. Those render identically
 * without this.
 */
import type { PublishedDefaultDto } from './generated/PublishedDefaultDto';
export type PublishedDefault = PublishedDefaultDto & {
  /**
   * Joins to {@link ParamProvenance.param}, but narrowed one member tighter:
   * `SamplingParamKey` and not `ProvenanceParamKey`, because a published
   * default is a number read out of a GGUF key and `reasoningEffort` has
   * none. In practice the set is smaller still — only the fields
   * `general.sampling.*` covers are ever published.
   */
  param: SamplingParamKey;
};

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
 * `topNSigma`, the four DRY knobs and `seed` — were absent. Eight of those
 * nine have a control on the serve modal, so a parameter the form rendered
 * was one the launch silently discarded; `seed` has no control anywhere and
 * was simply unreachable. Extending the generated shape is what stops that
 * recurring: a field added in Rust arrives here without an edit.
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
 * `GET /api/settings` answers with `gglib_app_services::types::AppSettings`
 * — not `gglib_core::Settings`, which is the persisted shape and never
 * crosses the wire — and no field of it uses `skip_serializing_if`, so the
 * response always names every one. "Nothing configured" is a `null`, never an
 * absent key. The mirror's `?:` described a shape the endpoint does not send,
 * and every read site is already `settings?.x`-style, so adopting the true one
 * changes no call site.
 *
 * The *request* shape is a different type for a real reason: `PUT` treats an
 * absent key as "leave unchanged" and an explicit `null` as "clear", which is
 * the `double_option` three-state — see {@link UpdateSettingsRequest}.
 */
import type { AppSettings } from './generated/AppSettings';
export type { AppSettings };

/**
 * `PUT /api/settings` body — the three-state update.
 *
 * Absent key = leave unchanged; explicit `null` = clear to default; a value =
 * set it. Every field carries `#[serde(default, with = "…double_option")]` on
 * the Rust side, and ts-rs renders that as `field?: T | null`, which is
 * exactly the shape — so unlike the other request bodies here, this one needs
 * no wrapper to be accurate.
 *
 * Two fields are narrowed, and only two. A settings update carries a
 * *partially specified* config — the settings form sends the parameters the
 * user touched — where the generated body asks for the complete eighteen-key
 * `InferenceConfig` a response would carry. `Omit` is safe here because
 * `UpdateSettingsRequest` is a plain object type; the intersection form is
 * only required where the target is a union of arms.
 */
import type { UpdateSettingsRequest as UpdateSettingsBody } from './generated/UpdateSettingsRequest';
export type UpdateSettingsRequest = Omit<
  UpdateSettingsBody,
  'inferenceDefaults' | 'inferenceProfiles'
> & {
  /** Global inference parameter defaults, as far as the form specified them. */
  inferenceDefaults?: SparseInferenceConfig | null;
  /**
   * Replaces the whole profile list. `null` clears it; omitting the key
   * leaves it untouched, so an unrelated settings update cannot drop
   * profiles it never knew about.
   */
  inferenceProfiles?: SparseInferenceProfile[] | null;
};

// ============================================================================
// System Memory Types (for "Will it fit?" indicators)
// ============================================================================

/**
 * What the host has to run a model on — `GET /api/system/memory`.
 *
 * `gpuMemoryBytes` is optional and *not* nullable, which is the correction:
 * it is the one field carrying `skip_serializing_if`, so a machine with no
 * readable GPU memory omits the key rather than sending `null`. The mirror
 * admitted both, so a reader had two absent-shapes to handle and only one
 * could ever arrive.
 */
import type { SystemMemoryInfoDto as SystemMemoryInfo } from './generated/SystemMemoryInfoDto';
export type { SystemMemoryInfo };

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
 *
 * Four fields the mirror made optional are required nullables — `author`,
 * `last_modified`, `parameters_b` and `description`. The search handler builds
 * every one of them, and `None` crosses as `null`.
 */
export type { HfModelSummary } from './generated/HfModelSummary';

/**
 * Sort field options for HuggingFace model search.
 */
import type { HfSortField } from './generated/HfSortField';
export type { HfSortField };

/**
 * Request for searching HuggingFace models.
 *
 * Every field is required here, which the one construction site already
 * satisfies — it names all seven. `sort_by` and `sort_ascending` had defaults
 * in the mirror that the caller never relied on.
 */
export type { HfSearchRequest } from './generated/HfSearchRequest';

/**
 * Response from HuggingFace model search.
 */
export type { HfSearchResponse } from './generated/HfSearchResponse';

/**
 * Information about a specific quantization variant.
 */
export type { HfQuantization } from './generated/HfQuantization';

/**
 * Response containing available quantizations for a model.
 */
export type { HfQuantizationsResponse } from './generated/HfQuantizationsResponse';

/**
 * Response for tool/function calling support detection.
 *
 * Used for both HuggingFace model metadata and local running server queries.
 */
export type { ToolSupportResponse } from './generated/ToolSupportResponse';

// ============================================================================
// Model Filter Options Types
// ============================================================================

/**
 * A range of numeric values with min and max.
 */
export type { RangeValues } from './generated/RangeValues';

/**
 * Filter options for the model library UI.
 *
 * All three ranges are required nullables. The mirror made `speed_range`
 * optional, which read as "a backend that has not benchmarked anything omits
 * the key" — it does not; it sends `null`, exactly as the other two do when
 * the library is empty.
 */
export type { ModelFilterOptions } from './generated/ModelFilterOptions';

/**
 * Sort field for `GET /api/models`.
 * Matches the backend `ModelSortBy` domain enum (snake_case).
 */
export type { ModelSortBy } from './generated/ModelSortBy';

/**
 * Sort direction for model list queries.
 */
export type { SortOrder } from './generated/SortOrder';


