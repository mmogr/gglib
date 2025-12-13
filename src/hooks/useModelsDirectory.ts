import { useCallback, useEffect, useState } from "react";
import { ModelsDirectoryInfo } from "../types";
import {
  getModelsDirectory,
  setModelsDirectory,
} from "../services/clients/system";

export function useModelsDirectory() {
  const [info, setInfo] = useState<ModelsDirectoryInfo | null>(null);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const result = await getModelsDirectory();
      setInfo(result);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setLoading(false);
    }
  }, []);

  const save = useCallback(async (path: string) => {
    try {
      setSaving(true);
      setError(null);
      await setModelsDirectory(path);
      // Reload the directory info after setting
      const result = await getModelsDirectory();
      setInfo(result);
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
    info,
    loading,
    saving,
    error,
    refresh: load,
    save,
  };
}
