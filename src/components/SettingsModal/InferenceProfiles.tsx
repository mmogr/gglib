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
import {
  getSettings,
  installProfileTemplates,
  updateSettings,
} from "../../services/transport/api/settings";
import type { SparseInferenceConfig, SparseInferenceProfile } from "../../types";
import { INFERENCE_CONFIG_KEYS } from "../../constants/inferenceDefaults";
import { PARAM_LABELS } from "../../utils/samplingProvenance";
import { Button } from "../ui/Button";
import { Banner } from '../ui/Banner';
import { Stack, EmptyState } from "../primitives";
import { InferenceProfileEditor } from "./InferenceProfileEditor";
import { formatError } from "../../utils/errors";

/**
 * Human-readable summary of the parameters a profile actually sets.
 *
 * Iterates `INFERENCE_CONFIG_KEYS` rather than a label table of its own. This
 * was the second hand-kept list of the same eleven names — so a profile that
 * set `topNSigma` from the CLI displayed as though it had set nothing, which
 * is the same silence that let the editor drop the field on save.
 */
function summarize(config: SparseInferenceConfig): string {
  const parts = INFERENCE_CONFIG_KEYS.filter((key) => {
    const value = config[key];
    return value !== undefined && value !== null;
  }).map((key) => `${wireLabel(key)}=${config[key]}`);
  return parts.length ? parts.join("  ") : "no parameters set";
}

/**
 * `PARAM_LABELS` in the hyphenated lower case this list has always used —
 * "Top P" reads as `top-p` here, matching how the CLI prints a profile.
 * Derived rather than tabulated so there is one place a label is written.
 */
function wireLabel(key: (typeof INFERENCE_CONFIG_KEYS)[number]): string {
  const label = (PARAM_LABELS as Record<string, string | undefined>)[key];
  return label ? label.toLowerCase().replace(/\s+/g, "-") : key;
}

export const InferenceProfiles: FC = () => {
  const [profiles, setProfiles] = useState<SparseInferenceProfile[]>([]);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** `null` = not editing; `""` = creating; otherwise the name being edited. */
  const [editing, setEditing] = useState<string | null>(null);
  /** What the last starter-profile install did, once there has been one. */
  const [installed, setInstalled] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const settings = await getSettings();
      setProfiles(settings.inferenceProfiles ?? []);
    } catch (e) {
      setError(formatError(e));
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
  const persist = useCallback(async (next: SparseInferenceProfile[]) => {
    setSaving(true);
    setError(null);
    setInstalled(null);
    try {
      const settings = await updateSettings({ inferenceProfiles: next });
      setProfiles(settings.inferenceProfiles ?? []);
      setEditing(null);
    } catch (e) {
      setError(formatError(e));
    } finally {
      setSaving(false);
    }
  }, []);

  const handleSave = useCallback(
    (profile: SparseInferenceProfile) => {
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
   * Add the starter profiles. The daemon runs the install `gglib config
   * profile install-templates` runs, so the nine and what happens to a
   * profile already under one of their names are decided there: it is kept.
   */
  const handleInstallTemplates = useCallback(async () => {
    setSaving(true);
    setError(null);
    try {
      const done = await installProfileTemplates();
      setProfiles(done.settings.inferenceProfiles ?? []);
      setInstalled(
        done.installed.length === 0
          ? "Starter profiles are already installed"
          : `Installed: ${done.installed.join(", ")}` +
              (done.kept.length ? `. Kept yours: ${done.kept.join(", ")}` : ""),
      );
    } catch (e) {
      setError(formatError(e));
    } finally {
      setSaving(false);
    }
  }, []);

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
        <Button
          variant="secondary"
          disabled={saving}
          title="Add the starter profiles: three for sampling and six for reasoning effort. A profile you already have under one of their names is kept"
          onClick={() => void handleInstallTemplates()}
        >
          Install starter profiles
        </Button>
        {installed && <span className="text-xs text-text-muted">{installed}</span>}
      </div>
    </Stack>
  );
};
