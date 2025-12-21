import {
  createContext,
  FC,
  ReactNode,
  useCallback,
  useContext,
  useEffect,
  useState,
} from "react";
import { AppSettings, UpdateSettingsRequest } from "../types";
import { getSettings, updateSettings } from "../services/clients/settings";

export type ShowToastFn = (message: string, type?: "success" | "error" | "info" | "warning") => void;

interface SettingsContextValue {
  settings: AppSettings | null;
  loading: boolean;
  saving: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  save: (updates: UpdateSettingsRequest) => Promise<AppSettings>;
}

const SettingsContext = createContext<SettingsContextValue | null>(null);

interface SettingsProviderProps {
  children: ReactNode;
  showToast?: ShowToastFn;
}

/**
 * Provider component that manages shared settings state.
 * All components using useSettings() will share the same state instance.
 */
export const SettingsProvider: FC<SettingsProviderProps> = ({ children, showToast }) => {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    try {
      setLoading(true);
      setError(null);
      const result = await getSettings();
      setSettings(result);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      setError(message);
    } finally {
      setLoading(false);
    }
  }, []);

  const save = useCallback(
    async (updates: UpdateSettingsRequest): Promise<AppSettings> => {
      try {
        setSaving(true);
        setError(null);
        const result = await updateSettings(updates);
        setSettings(result);
        showToast?.("Settings applied", "success");
        return result;
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err);
        setError(message);
        showToast?.(message, "error");
        throw err;
      } finally {
        setSaving(false);
      }
    },
    [showToast]
  );

  // Load settings on mount
  useEffect(() => {
    load();
  }, [load]);

  const value: SettingsContextValue = {
    settings,
    loading,
    saving,
    error,
    refresh: load,
    save,
  };

  return (
    <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>
  );
};

/**
 * Hook to access the shared settings context.
 * Must be used within a SettingsProvider.
 */
export function useSettingsContext(): SettingsContextValue {
  const context = useContext(SettingsContext);
  if (!context) {
    throw new Error("useSettingsContext must be used within a SettingsProvider");
  }
  return context;
}
