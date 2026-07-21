/**
 * Form for creating or editing one inference profile.
 *
 * The central UX point is *sparseness*: an empty parameter field means "not
 * set", not "zero". Unset parameters fall through to the model's own defaults,
 * which is what lets a single `coding` profile apply safely across models with
 * different architectures. Each field says so when left blank.
 */

import { FC, useState } from "react";
import type { InferenceConfig, InferenceProfile } from "../../types";
import { Button } from "../ui/Button";
import { Input } from "../ui/Input";
import { Stack, Label } from "../primitives";

/** One editable sampling parameter. */
interface ParamSpec {
  key: keyof InferenceConfig;
  label: string;
  hint: string;
  step: string;
}

/**
 * The parameters a profile may set, in the order the CLI prints them so the
 * two surfaces read the same way.
 */
const PARAMS: ParamSpec[] = [
  { key: "temperature", label: "Temperature", hint: "0.0 – 2.0", step: "0.05" },
  { key: "topP", label: "Top-P", hint: "0.0 – 1.0", step: "0.05" },
  { key: "topK", label: "Top-K", hint: "positive integer", step: "1" },
  { key: "maxTokens", label: "Max tokens", hint: "positive integer", step: "1" },
  { key: "repeatPenalty", label: "Repeat penalty", hint: "typically 1.0 – 1.3", step: "0.05" },
  { key: "presencePenalty", label: "Presence penalty", hint: "0.0 – 2.0", step: "0.1" },
  { key: "minP", label: "Min-P", hint: "0.0 – 1.0", step: "0.01" },
];

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
  initial?: InferenceProfile;
  /** Names already in use, excluding the one being edited. */
  takenNames: string[];
  onSave: (profile: InferenceProfile) => void;
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
      PARAMS.map(({ key }) => {
        const value = config[key];
        return [key, value === undefined || value === null ? "" : String(value)];
      }),
    );
  });

  const nameError = profileNameError(name, takenNames);

  const handleSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    if (nameError) return;

    // Blank fields are omitted entirely, which is what makes the profile
    // sparse — a `0` would be a real override, an absent key falls through.
    const config: InferenceConfig = {};
    for (const { key } of PARAMS) {
      const raw = values[key]?.trim();
      if (!raw) continue;
      const parsed = Number(raw);
      if (Number.isFinite(parsed)) {
        (config as Record<string, number>)[key] = parsed;
      }
    }

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
          {PARAMS.map(({ key, label, hint, step }) => (
            <div key={key}>
              <Label htmlFor={`profile-${key}`}>{label}</Label>
              <Input
                id={`profile-${key}`}
                type="number"
                step={step}
                value={values[key] ?? ""}
                onChange={(e) => setValues((v) => ({ ...v, [key]: e.target.value }))}
                placeholder="model default"
                aria-describedby={`profile-${key}-help`}
              />
              <p id={`profile-${key}-help`} className="text-xs text-text-secondary mt-xs">
                {values[key]?.trim() ? hint : "Unset — uses the model's own default"}
              </p>
            </div>
          ))}
        </div>

        <label className="flex items-center gap-sm text-sm cursor-pointer">
          <input
            type="checkbox"
            checked={listInModels}
            onChange={(e) => setListInModels(e.target.checked)}
          />
          <span>
            Show in the model picker
            <span className="block text-xs text-text-secondary">
              Adds <code>&lt;model&gt;:{name || "name"}</code> to /v1/models. Leave off to keep
              the picker short — the profile still works when named directly.
            </span>
          </span>
        </label>

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
