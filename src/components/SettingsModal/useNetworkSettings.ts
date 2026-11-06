/**
 * State for the network-binding settings (bind host, LAN sharing) and the
 * endpoint's own (its address, allowed hosts and prompt cache on disk).
 *
 * Same rationale as `useDesktopSettings`: the group owns its own state and
 * hands back a ready-made slice of the update request, keeping the
 * over-budget `SettingsModal` from growing more `useState` pairs.
 *
 * @module components/SettingsModal/useNetworkSettings
 */

import { useCallback, useEffect, useState } from 'react';
import type { AppSettings, UpdateSettingsRequest } from '../../types';
import { changedFields } from './settingsRequest';

export interface NetworkSettingsValues {
  /** Literal IP the daemon binds; empty = compiled-in default (127.0.0.1). */
  bindHost: string;
  shareLan: boolean;
  /** Literal IP the endpoint binds; empty = loopback. */
  proxyHost: string;
  /** Host header values the endpoint accepts beyond its own, comma-separated; empty = none. */
  proxyAllowedHosts: string;
  proxyCacheOnDisk: boolean;
  /** The on-disk prompt cache's budget in GiB, as typed; empty = sized from free disk. */
  proxyCacheDiskGb: string;
}

const DEFAULTS: NetworkSettingsValues = {
  bindHost: '',
  shareLan: false,
  proxyHost: '',
  proxyAllowedHosts: '',
  proxyCacheOnDisk: false,
  proxyCacheDiskGb: '',
};

export interface UseNetworkSettingsResult {
  values: NetworkSettingsValues;
  /** Restore the built-in defaults, for the form's Reset action. */
  reset: () => void;
  setValue: <K extends keyof NetworkSettingsValues>(
    key: K,
    value: NetworkSettingsValues[K],
  ) => void;
  /** The fields changed since settings loaded, as update-request fields. */
  updates: Partial<
    Pick<
      UpdateSettingsRequest,
      | 'bindHost'
      | 'shareLan'
      | 'proxyHost'
      | 'proxyAllowedHosts'
      | 'proxyCacheOnDisk'
      | 'proxyCacheDiskGb'
    >
  >;
}

const valuesFrom = (settings: AppSettings): NetworkSettingsValues => ({
  bindHost: settings.bindHost ?? '',
  shareLan: settings.shareLan === true,
  proxyHost: settings.proxyHost ?? '',
  proxyAllowedHosts: (settings.proxyAllowedHosts ?? []).join(', '),
  proxyCacheOnDisk: settings.proxyCacheOnDisk === true,
  proxyCacheDiskGb: settings.proxyCacheDiskGb?.toString() ?? '',
});

/** The hosts a person typed, one per comma; an empty field is no list. */
const hostsFrom = (typed: string): string[] | null => {
  const hosts = typed
    .split(',')
    .map((host) => host.trim())
    .filter(Boolean);
  return hosts.length > 0 ? hosts : null;
};

/**
 * The GiB as typed, or `null` for an empty field. What is typed goes to the
 * daemon as typed rather than being swallowed into "auto" here: a zero is
 * refused by its validator, which says the floor; a negative or a fraction
 * is refused earlier, as not a whole number of GiB.
 */
const gibFrom = (typed: string): number | null => {
  const trimmed = typed.trim();
  return trimmed === '' ? null : Number(trimmed);
};

const requestFrom = (values: NetworkSettingsValues) => ({
  bindHost: values.bindHost.trim() || null,
  shareLan: values.shareLan,
  proxyHost: values.proxyHost.trim() || null,
  proxyAllowedHosts: hostsFrom(values.proxyAllowedHosts),
  proxyCacheOnDisk: values.proxyCacheOnDisk,
  proxyCacheDiskGb: gibFrom(values.proxyCacheDiskGb),
});

/**
 * Track the network-binding fields, seeded from persisted settings.
 * An emptied bind host means "back to the default", which is a `null`
 * (clear the row) on the wire, not a blank string.
 */
export function useNetworkSettings(settings: AppSettings | null): UseNetworkSettingsResult {
  const [values, setValues] = useState<NetworkSettingsValues>(DEFAULTS);

  useEffect(() => {
    if (settings) {
      setValues(valuesFrom(settings));
    }
  }, [settings]);

  const reset = useCallback(() => setValues(DEFAULTS), []);

  const setValue = useCallback(
    <K extends keyof NetworkSettingsValues>(key: K, value: NetworkSettingsValues[K]) => {
      setValues((previous) => ({ ...previous, [key]: value }));
    },
    [],
  );

  const loaded = settings ? valuesFrom(settings) : DEFAULTS;
  return {
    values,
    setValue,
    reset,
    updates: changedFields(requestFrom(values), requestFrom(loaded)),
  };
}
