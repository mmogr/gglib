/**
 * Form for creating or editing one inference profile.
 *
 * The central UX point is *sparseness*: an empty parameter field means "not
 * set", not "zero". Unset parameters fall through to the model's own defaults,
 * which is what lets a single `coding` profile apply safely across models with
 * different architectures. Each field says so when left blank.
 */

import { FC, useState } from "react";
import { Checkbox } from '../ui/Checkbox';
import type {
  InferenceConfig,
  SamplingParamKey,
  SparseInferenceConfig,
  SparseInferenceProfile,
} from "../../types";
import { INFERENCE_CONFIG_KEYS, INFERENCE_PARAMS } from "../../constants/inferenceDefaults";
import type { ReasoningEffortLevel } from "../../constants/reasoningEffort";
import { PARAM_LABELS } from "../../utils/samplingProvenance";
import { ReasoningEffortField } from "../InferenceParametersForm/ReasoningEffortField";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Stack, Label } from "../primitives";

/**
 * Prose hint per numeric parameter.
 *
 * A `Record<SamplingParamKey, string>` rather than a hand-kept array: the
 * exhaustive key type is what makes a new sampling parameter a compile error
 * here instead of a field the editor silently drops. Labels and bounds come
 * from the shared tables — `PARAM_LABELS` and `INFERENCE_PARAMS`, the same
 * ones `InferenceParametersForm` reads — so this file no longer keeps its own
 * copy of either.
 */
const HINTS: Record<SamplingParamKey, string> = {
  temperature: "0.0 – 2.0",
  topP: "0.0 – 1.0",
  topK: "positive integer",
  maxTokens: "positive integer",
  repeatPenalty: "typically 1.0 – 1.3",
  presencePenalty: "0.0 – 2.0",
  minP: "0.0 – 1.0",
  frequencyPenalty: "0.0 – 2.0",
  dynatempRange: "0 disables dynamic temperature",
  dynatempExponent: "typically 1.0",
  topNSigma: "-1 disables; typically 1 – 4",
  dryMultiplier: "0 disables; typically 0.8",
  dryBase: "at least 1.0; typically 1.75",
  dryAllowedLength: "0 or more",
  dryPenaltyLastN: "-1 = whole context",
  reasoningBudgetTokens: "-1 = no cap",
};

/**
 * The numeric parameters a profile may set, in the order the CLI prints them
 * so the two surfaces read the same way.
 *
 * Derived from `INFERENCE_CONFIG_KEYS` rather than written out, so the order
 * and the membership both follow the wire type.
 */
const PROFILE_PARAM_KEYS = INFERENCE_CONFIG_KEYS.filter(
  (key): key is SamplingParamKey => key !== "seed" && key !== "reasoningEffort",
);

type AssertNoUnlistedFields<T extends never> = T;
/**
 * Fails to compile if `InferenceConfig` grows a field this editor does not
 * handle.
 *
 * The exclusions are the point. `reasoningEffort` is an enum and gets its own
 * control below rather than a number input. `seed` is deliberately absent
 * from every profile surface — a profile is reused across every request that
 * selects it, so a seed here would pin them all to one output, which is why
 * `gglib config profile set` has no `--seed` either. Everything else must be
 * editable, because the save path rebuilds the config from this list and
 * anything missing is dropped: eleven of eighteen were, until this list
 * stopped being hand-kept.
 */
export type ProfileParamsAreComplete = AssertNoUnlistedFields<
  Exclude<keyof InferenceConfig, SamplingParamKey | "reasoningEffort" | "seed">
>;

/**
 * Client-side name check, mirroring `gglib_core::domain::validate_name`.
 *
 * Purely for immediate feedback — the server validates independently and is
 * the authority. Keep the two in step if the server rule changes.
 */
export function profileNameError(name: string, taken: string[]): string | null {
  if (!name) return "Name is required.";
  if (name.length > 32) return "Name must be 32 characters or fewer.";
  if (!/^[a-z0-9-]+$/.test(name)) {
    return "Use lowercase letters, digits and '-' only.";
  }
  if (name.startsWith("-") || name.endsWith("-")) {
    return "Name cannot start or end with '-'.";
  }
  if (["interactive", "native"].includes(name)) {
    return `'${name}' is reserved.`;
  }
  if (taken.includes(name)) return `A profile named '${name}' already exists.`;
  return null;
}

interface InferenceProfileEditorProps {
  /** The profile being edited, or undefined when creating a new one. */
  initial?: SparseInferenceProfile;
  /** Names already in use, excluding the one being edited. */
  takenNames: string[];
  onSave: (profile: SparseInferenceProfile) => void;
  onCancel: () => void;
}

export const InferenceProfileEditor: FC<InferenceProfileEditorProps> = ({
  initial,
  takenNames,
  onSave,
  onCancel,
}) => {
  const [name, setName] = useState(initial?.name ?? "");
  const [description, setDescription] = useState(initial?.description ?? "");
  const [listInModels, setListInModels] = useState(initial?.listInModels ?? false);
  // Kept as strings so a half-typed "0." does not get coerced mid-edit.
  const [values, setValues] = useState<Record<string, string>>(() => {
    const config = initial?.config ?? {};
    return Object.fromEntries(
      PROFILE_PARAM_KEYS.map((key) => {
        const value = config[key];
        return [key, value === undefined || value === null ? "" : String(value)];
      }),
    );
  });
  const [reasoningEffort, setReasoningEffort] = useState<ReasoningEffortLevel | undefined>(
    initial?.config?.reasoningEffort ?? undefined,
  );

  const nameError = profileNameError(name, takenNames);

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    if (nameError) return;

    // Blank fields are omitted entirely, which is what makes the profile
    // sparse — a `0` would be a real override, an absent key falls through.
    const config: SparseInferenceConfig = {};
    for (const key of PROFILE_PARAM_KEYS) {
      const raw = values[key]?.trim();
      if (!raw) continue;
      const parsed = Number(raw);
      if (Number.isFinite(parsed)) {
        (config as Record<string, number>)[key] = parsed;
      }
    }
    // Outside the numeric loop: an effort level is an enum, and `Number()`
    // would turn it into `NaN` and drop it.
    if (reasoningEffort) config.reasoningEffort = reasoningEffort;

    onSave({
      name,
      description: description.trim() || null,
      config,
      listInModels,
    });
  };

  return (
    <form onSubmit={handleSubmit}>
      <Stack gap="md">
        <div>
          <Label htmlFor="profile-name">Name</Label>
          <Input
            id="profile-name"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="coding"
            aria-invalid={nameError !== null}
            aria-describedby="profile-name-help"
          />
          <p id="profile-name-help" className="text-xs text-text-secondary mt-xs">
            {nameError ?? `Clients select it as <model>:${name || "name"}`}
          </p>
        </div>

        <div>
          <Label htmlFor="profile-description">Description</Label>
          <Input
            id="profile-description"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="Low-variance sampling for code generation"
          />
        </div>

        <div className="grid grid-cols-2 gap-md">
          {PROFILE_PARAM_KEYS.map((key) => (
            <div key={key}>
              <Label htmlFor={`profile-${key}`}>{PARAM_LABELS[key]}</Label>
              <Input
                id={`profile-${key}`}
                type="number"
                step={INFERENCE_PARAMS[key].step}
                min={INFERENCE_PARAMS[key].min}
                max={INFERENCE_PARAMS[key].max}
                value={values[key] ?? ""}
                onChange={(e) => setValues((v) => ({ ...v, [key]: e.target.value }))}
                placeholder="model default"
                aria-describedby={`profile-${key}-help`}
              />
              <p id={`profile-${key}-help`} className="text-xs text-text-secondary mt-xs">
                {values[key]?.trim() ? HINTS[key] : "Unset — uses the model's own default"}
              </p>
            </div>
          ))}
        </div>

        {/*
          No `support` prop: a profile is not attached to a model, so this
          surface cannot know whether any given template reads the level —
          the same condition the global settings form is in.
        */}
        <ReasoningEffortField
          id="profile-reasoningEffort"
          value={reasoningEffort}
          onChange={setReasoningEffort}
          disabled={false}
        />

        <Checkbox
          checked={listInModels}
          onChange={(e) => setListInModels(e.target.checked)}
          label="Show in the model picker"
          description={
            <>
              Adds <code>&lt;model&gt;:{name || "name"}</code> to /v1/models. Leave off to keep
              the picker short — the profile still works when named directly.
            </>
          }
        />

        <div className="flex gap-sm justify-end">
          <Button type="button" variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
          <Button type="submit" disabled={nameError !== null}>
            {initial ? "Save profile" : "Create profile"}
          </Button>
        </div>
      </Stack>
    </form>
  );
};
