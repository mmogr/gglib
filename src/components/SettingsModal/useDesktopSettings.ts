/**
 * State for the always-on proxy toggles in the settings form.
 *
 * Kept out of `SettingsModal` because that component already carries the
 * whole form's state: three more `useState` pairs, three more load lines and
 * three more entries in the submit payload would have grown a file that is
 * over the project's complexity budget already. The group owns its own state
 * and hands back a ready-made slice of the update request instead.
 *
 * @module components/SettingsModal/useDesktopSettings
 */

import { useCallback, useEffect, useState } from 'react';
import type { AppSettings, UpdateSettingsRequest } from '../../types';

export interface DesktopSettingsValues {
  proxyAutostart: boolean;
  closeToTray: boolean;
  startAtLogin: boolean;
}

const DEFAULTS: DesktopSettingsValues = {
  proxyAutostart: false,
  closeToTray: false,
  startAtLogin: false,
};

export interface UseDesktopSettingsResult {
  values: DesktopSettingsValues;
  /** Update one toggle. */
  setValue: <K extends keyof DesktopSettingsValues>(
    key: K,
    value: DesktopSettingsValues[K],
  ) => void;
  /** Restore the built-in defaults (all off), for the form's Reset action. */
  reset: () => void;
  /** The slice of the update request these toggles own. */
  updates: Pick<UpdateSettingsRequest, 'proxyAutostart' | 'closeToTray' | 'startAtLogin'>;
}

/**
 * Track the three always-on proxy toggles, seeded from persisted settings.
 *
 * Each setting is tri-state on the wire (`true` / `false` / unset), but a
 * checkbox is binary — unset reads as off, matching how the backend treats a
 * missing value.
 */
export function useDesktopSettings(settings: AppSettings | null): UseDesktopSettingsResult {
  const [values, setValues] = useState<DesktopSettingsValues>(DEFAULTS);

  useEffect(() => {
    if (settings) {
      setValues({
        proxyAutostart: settings.proxyAutostart === true,
        closeToTray: settings.closeToTray === true,
        startAtLogin: settings.startAtLogin === true,
      });
    }
  }, [settings]);

  const setValue = useCallback(
    <K extends keyof DesktopSettingsValues>(key: K, value: DesktopSettingsValues[K]) => {
      setValues((previous) => ({ ...previous, [key]: value }));
    },
    [],
  );

  const reset = useCallback(() => setValues(DEFAULTS), []);

  return { values, setValue, reset, updates: values };
}
