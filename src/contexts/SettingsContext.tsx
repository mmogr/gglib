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
import { keptCurrent } from '../services/bridgeEvents';
import { getTransport } from '../services/transport';
import { formatError } from '../utils/errors';

export type ShowToastFn = (message: string, type?: "success" | "error" | "info" | "warning") => void;

export interface SettingsContextValue {
  settings: AppSettings | null;
  /**
   * True until the daemon has first answered, a read or a save, with the
   * settings or with a failure. A failure leaves `settings` null: a reader
   * that needs the settings themselves waits for `settings`, not this.
   */
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
 *
 * The settings are kept current (`keptCurrent`): read on mount and again
 * each time the event stream opens, so a first read the daemon refused on a
 * cold start is followed by one that lands. A read applies its answer only
 * if no save began and no later read landed while it was out, so a read
 * overtaken by a save cannot put back what was there before (TX-2). A read
 * that fails is said in `error` and is not newer state: an earlier read
 * that lands after it is still applied.
 */
export const SettingsProvider: FC<SettingsProviderProps> = ({ children, showToast }) => {
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [kept] = useState(() =>
    keptCurrent<AppSettings>({
      read: () => getTransport().getSettings(),
      apply: (read) => {
        setSettings(read);
        setError(null);
        setLoading(false);
      },
      onFailure: (err) => {
        setError(formatError(err));
        setLoading(false);
      },
    })
  );

  // Asked for by a reader: what it says while the read is out is the asking, not an old failure.
  const load = useCallback(async () => {
    setError(null);
    await kept.refresh();
  }, [kept]);

  const save = useCallback(
    async (updates: UpdateSettingsRequest, options?: { quiet?: boolean }): Promise<AppSettings> => {
      const toast = options?.quiet ? undefined : showToast;
      try {
        setSaving(true);
        setError(null);
        kept.overtake();
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
        // The daemon has answered, even if the read the save overtook never will.
        setLoading(false);
        setSaving(false);
      }
    },
    [showToast, kept]
  );

  // Read now, and again each time the event stream opens.
  useEffect(() => {
    return kept.keep();
  }, [kept]);

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
