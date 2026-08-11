/**
 * State for the network-binding settings (bind host, LAN sharing).
 *
 * Same rationale as `useDesktopSettings`: the group owns its own state and
 * hands back a ready-made slice of the update request, keeping the
 * over-budget `SettingsModal` from growing more `useState` pairs.
 *
 * @module components/SettingsModal/useNetworkSettings
 */

import { useCallback, useEffect, useState } from 'react';
import type { AppSettings, UpdateSettingsRequest } from '../../types';

export interface NetworkSettingsValues {
  /** Literal IP the daemon binds; empty = compiled-in default (127.0.0.1). */
  bindHost: string;
  shareLan: boolean;
}

const DEFAULTS: NetworkSettingsValues = {
  bindHost: '',
  shareLan: false,
};

export interface UseNetworkSettingsResult {
  values: NetworkSettingsValues;
  /** Restore the built-in defaults, for the form's Reset action. */
  reset: () => void;
  setValue: <K extends keyof NetworkSettingsValues>(
    key: K,
    value: NetworkSettingsValues[K],
  ) => void;
  /** The slice of the update request these fields own. */
  updates: Pick<UpdateSettingsRequest, 'bindHost' | 'shareLan'>;
}

/**
 * Track the network-binding fields, seeded from persisted settings.
 * An emptied bind host means "back to the default", which is a `null`
 * (clear the row) on the wire, not a blank string.
 */
export function useNetworkSettings(settings: AppSettings | null): UseNetworkSettingsResult {
  const [values, setValues] = useState<NetworkSettingsValues>(DEFAULTS);

  useEffect(() => {
    if (settings) {
      setValues({
        bindHost: settings.bindHost ?? '',
        shareLan: settings.shareLan === true,
      });
    }
  }, [settings]);

  const reset = useCallback(() => setValues(DEFAULTS), []);

  const setValue = useCallback(
    <K extends keyof NetworkSettingsValues>(key: K, value: NetworkSettingsValues[K]) => {
      setValues((previous) => ({ ...previous, [key]: value }));
    },
    [],
  );

  return {
    values,
    setValue,
    reset,
    updates: {
      bindHost: values.bindHost.trim() || null,
      shareLan: values.shareLan,
    },
  };
}
