/**
 * Inference profiles settings panel.
 *
 * Lists the configured sampling profiles and provides add / edit / delete.
 * Self-contained — it loads and saves settings itself rather than threading
 * state through `SettingsModal`, matching the `McpServersPanel` pattern.
 *
 * Every mutation writes the whole list back, which is what the API expects:
 * `inferenceProfiles` replaces the stored list, while omitting the key leaves
 * it untouched.
 */

import { FC, useCallback, useEffect, useState } from "react";
import { getSettings, updateSettings } from "../../services/transport/api/settings";
import type { InferenceConfig, InferenceProfile } from "../../types";
import { Button } from "../ui/Button";
import { Banner } from '../ui/Banner';
import { Stack, EmptyState } from "../primitives";
import { InferenceProfileEditor } from "./InferenceProfileEditor";

/** Human-readable summary of the parameters a profile actually sets. */
function summarize(config: InferenceConfig): string {
  const labels: Record<string, string> = {
    temperature: "temperature",
    topP: "top-p",
    topK: "top-k",
    maxTokens: "max-tokens",
    repeatPenalty: "repeat-penalty",
    presencePenalty: "presence-penalty",
    minP: "min-p",
    dryMultiplier: "dry-multiplier",
    dryBase: "dry-base",
    dryAllowedLength: "dry-allowed-length",
    dryPenaltyLastN: "dry-penalty-last-n",
  };
  const parts = Object.entries(labels)
    .filter(([key]) => {
      const value = config[key as keyof InferenceConfig];
      return value !== undefined && value !== null;
    })
    .map(([key, label]) => `${label}=${config[key as keyof InferenceConfig]}`);
  return parts.length ? parts.join("  ") : "no parameters set";
}

/**
 * The CLI's starter templates (`gglib config profile install-templates`),
 * transcribed from `gglib_core::domain::inference_profile::builtin_templates`.
 * Templates, not behaviour: installing them only seeds the user's own list.
 * Sparse on purpose — a template that filled every field would silently
 * override per-model tuning. Drift is guarded by a contract test that reads
 * the Rust source.
 */
export const STARTER_PROFILES: InferenceProfile[] = [
  {
    name: 'coding',
    description: 'Low-variance sampling for code generation and tool use.',
    config: { temperature: 0.2, topP: 0.9 },
    listInModels: false,
  },
  {
    name: 'chat',
    description: 'Balanced sampling for conversational use.',
    config: { temperature: 0.7, topP: 0.95 },
    listInModels: true,
  },
  {
    name: 'creative',
    description: 'Wider sampling for brainstorming and prose.',
    config: { temperature: 1.1, topP: 0.98 },
    listInModels: false,
  },
];

export const InferenceProfiles: FC = () => {
  const [profiles, setProfiles] = useState<InferenceProfile[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** `null` = not editing; `""` = creating; otherwise the name being edited. */
  const [editing, setEditing] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const settings = await getSettings();
      setProfiles(settings.inferenceProfiles ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  /**
   * Persist a new list. The server validates and is the authority, so a
   * rejection is surfaced verbatim and local state is left untouched rather
   * than optimistically showing something that was not saved.
   */
  const persist = useCallback(async (next: InferenceProfile[]) => {
    setSaving(true);
    setError(null);
    try {
      const settings = await updateSettings({ inferenceProfiles: next });
      setProfiles(settings.inferenceProfiles ?? []);
      setEditing(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }, []);

  const handleSave = useCallback(
    (profile: InferenceProfile) => {
      const index = profiles.findIndex((p) => p.name === editing);
      const next =
        index >= 0
          ? profiles.map((p, i) => (i === index ? profile : p))
          : [...profiles, profile];
      void persist(next);
    },
    [profiles, editing, persist],
  );

  const handleDelete = useCallback(
    (name: string) => {
      void persist(profiles.filter((p) => p.name !== name));
    },
    [profiles, persist],
  );

  /**
   * Seed the CLI's starter templates, skip-existing — the same merge
   * `gglib config profile install-templates` performs (without --force).
   */
  const handleInstallTemplates = useCallback(() => {
    const missing = STARTER_PROFILES.filter(
      (t) => !profiles.some((p) => p.name === t.name),
    );
    if (missing.length === 0) return;
    void persist([...profiles, ...missing]);
  }, [profiles, persist]);

  const templatesMissing = STARTER_PROFILES.some(
    (t) => !profiles.some((p) => p.name === t.name),
  );

  if (loading) {
    return <p className="text-sm text-text-secondary">Loading profiles…</p>;
  }

  if (editing !== null) {
    const initial = profiles.find((p) => p.name === editing);
    return (
      <Stack gap="md">
        {error && (
          <Banner variant="danger">
            {error}
          </Banner>
        )}
        <InferenceProfileEditor
          initial={initial}
          takenNames={profiles.filter((p) => p.name !== editing).map((p) => p.name)}
          onSave={handleSave}
          onCancel={() => setEditing(null)}
        />
      </Stack>
    );
  }

  return (
    <Stack gap="md">
      <p className="text-sm text-text-secondary">
        Named sampling profiles apply to every model. A client selects one per request by
        asking for <code>&lt;model&gt;:&lt;profile&gt;</code> — so a coding agent and a chat
        UI can share one model with different sampling.
      </p>

      {error && (
        <Banner variant="danger">
          {error}
        </Banner>
      )}

      {profiles.length === 0 ? (
        <EmptyState
          title="No inference profiles"
          description="Create one to give chat and coding clients different sampling on the same model."
        />
      ) : (
        <Stack gap="sm">
          {profiles.map((profile) => (
            <div
              key={profile.name}
              className="p-md bg-surface rounded-md flex items-start justify-between gap-md"
            >
              <div className="min-w-0">
                <div className="flex items-center gap-sm">
                  <span className="font-semibold">{profile.name}</span>
                  {profile.listInModels && (
                    <span className="text-xs px-sm py-0.5 rounded-full bg-primary-subtle text-primary">
                      in model picker
                    </span>
                  )}
                </div>
                {profile.description && (
                  <p className="text-sm text-text-secondary">{profile.description}</p>
                )}
                <p className="text-xs text-text-secondary font-mono mt-xs break-words">
                  {summarize(profile.config)}
                </p>
              </div>
              <div className="flex gap-sm shrink-0">
                <Button
                  variant="secondary"
                  size="sm"
                  disabled={saving}
                  onClick={() => setEditing(profile.name)}
                >
                  Edit
                </Button>
                <Button
                  variant="dangerGhost"
                  size="sm"
                  disabled={saving}
                  onClick={() => handleDelete(profile.name)}
                >
                  Delete
                </Button>
              </div>
            </div>
          ))}
        </Stack>
      )}

      <div className="flex items-center gap-sm">
        <Button disabled={saving} onClick={() => setEditing("")}>
          Add profile
        </Button>
        {templatesMissing ? (
          <Button
            variant="secondary"
            disabled={saving}
            title="Seed the coding / chat / creative starter profiles; existing names are kept"
            onClick={handleInstallTemplates}
          >
            Install starter profiles
          </Button>
        ) : (
          <span className="text-xs text-text-muted">Starter profiles installed</span>
        )}
      </div>
    </Stack>
  );
};
