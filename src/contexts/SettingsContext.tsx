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
import { getTransport } from '../services/transport';
import { formatError } from '../utils/errors';

export type ShowToastFn = (message: string, type?: "success" | "error" | "info" | "warning") => void;

export interface SettingsContextValue {
  settings: AppSettings | null;
  loading: boolean;
  saving: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  /**
   * Write `updates`. `quiet` raises no toast either way, for a caller that
   * says how the write went beside the field it came from (the settings
   * place); a failure still rejects.
   */
  save: (updates: UpdateSettingsRequest, options?: { quiet?: boolean }) => Promise<AppSettings>;
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
      const result = await getTransport().getSettings();
      setSettings(result);
    } catch (err) {
      const message = formatError(err);
      setError(message);
    } finally {
      setLoading(false);
    }
  }, []);

  const save = useCallback(
    async (updates: UpdateSettingsRequest, options?: { quiet?: boolean }): Promise<AppSettings> => {
      const toast = options?.quiet ? undefined : showToast;
      try {
        setSaving(true);
        setError(null);
        const result = await getTransport().updateSettings(updates);
        setSettings(result);
        toast?.("Settings applied", "success");
        return result;
      } catch (err) {
        const message = formatError(err);
        setError(message);
        toast?.(message, "error");
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
