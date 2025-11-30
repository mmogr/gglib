import { useSettingsContext } from "../contexts/SettingsContext";

/**
 * Hook to access shared settings state.
 * 
 * This hook consumes the SettingsContext, ensuring all components
 * share the same settings instance. When settings are saved via this hook,
 * all other components using useSettings() will automatically receive
 * the updated values.
 * 
 * Must be used within a SettingsProvider.
 */
export function useSettings() {
  return useSettingsContext();
}
