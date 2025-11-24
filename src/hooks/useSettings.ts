import { useCallback, useEffect, useState } from "react";
import { AppSettings, UpdateSettingsRequest } from "../types";
import { fetchSettings, updateSettings } from "../services/settings";

export function useSettings() {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const result = await fetchSettings();
      setSettings(result);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setLoading(false);
    }
  }, []);

  const save = useCallback(async (updates: UpdateSettingsRequest) => {
    try {
      setSaving(true);
      setError(null);
      const result = await updateSettings(updates);
      setSettings(result);
      return result;
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
      throw err;
    } finally {
      setSaving(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  return {
    settings,
    loading,
    saving,
    error,
    refresh: load,
    save,
  };
}
