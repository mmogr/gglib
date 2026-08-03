import { FC } from 'react';
import { ToggleField } from './ToggleField';
import type { DesktopSettingsValues } from '../useDesktopSettings';

interface DesktopSettingsProps {
  values: DesktopSettingsValues;
  onChange: <K extends keyof DesktopSettingsValues>(
    key: K,
    value: DesktopSettingsValues[K],
  ) => void;
  saving: boolean;
}

/**
 * Toggles that turn the desktop app into an always-on proxy host.
 *
 * These three settings are only meaningful together: autostart brings the
 * proxy up, close-to-tray keeps it up when the window is closed, and
 * start-at-login brings the app up in the first place. They are grouped so the
 * relationship is visible rather than scattered through the form.
 *
 * They have no effect on `gglib web`, which is why the group says so — the Web
 * UI renders this same form, and a setting that silently does nothing where
 * you are reading it is worse than one you had to go looking for.
 */
export const DesktopSettings: FC<DesktopSettingsProps> = ({ values, onChange, saving }) => (
  <div className="flex flex-col gap-md">
    <ToggleField
      id="proxy-autostart-input"
      label="Start proxy when gglib launches"
      checked={values.proxyAutostart}
      onChange={(value) => onChange('proxyAutostart', value)}
      disabled={saving}
    >
      Brings the OpenAI-compatible endpoint up automatically, so clients such as VS Code
      Copilot can reach it without you switching the proxy on first.
    </ToggleField>

    <ToggleField
      id="close-to-tray-input"
      label="Close to tray"
      checked={values.closeToTray}
      onChange={(value) => onChange('closeToTray', value)}
      disabled={saving}
    >
      Closing the window hides gglib to the system tray instead of quitting, leaving the proxy
      serving. Quit explicitly from the tray menu to stop it.
    </ToggleField>

    <ToggleField
      id="start-at-login-input"
      label="Start gglib at login"
      checked={values.startAtLogin}
      onChange={(value) => onChange('startAtLogin', value)}
      disabled={saving}
    >
      Registers gglib with your operating system&apos;s autostart mechanism. Combined with the
      two settings above, the endpoint is available from the moment you log in.
    </ToggleField>

    <p className="text-text-secondary text-sm">
      These apply to the desktop app only. <code>gglib proxy</code> and <code>gglib serve</code>
      {' '}stay explicit foreground commands.
    </p>
  </div>
);
