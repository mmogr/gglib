/**
 * Settings › General: whether gglib starts at login, whether the endpoint
 * comes up with it, and (in the desktop app, which has a menu bar) whether
 * closing the window keeps it running there. Each saves on change.
 *
 * @module places/settings/GeneralSection
 */

import type { FC } from 'react';
import { isDesktop } from '../../services/platform';
import { ToggleSetting } from './fields';

export const GeneralSection: FC = () => (
  <>
    <ToggleSetting k="startAtLogin" id="start-at-login-input" label="Start gglib at login">
      Registers gglib with your operating system&apos;s autostart mechanism, at once.
    </ToggleSetting>

    <ToggleSetting k="proxyAutostart" id="proxy-autostart-input" label="Start the endpoint when gglib launches">
      Brings the OpenAI-compatible endpoint up with the desktop app, so clients such as VS Code
      Copilot reach it without you turning it on. <code>gglib proxy</code> and{' '}
      <code>gglib serve</code> stay commands you run.
    </ToggleSetting>

    {isDesktop() && (
      <ToggleSetting k="closeToTray" id="close-to-tray-input" label="Keep running in the menu bar when the window closes">
        Closing the window hides gglib instead of quitting, and the endpoint keeps serving. Quit
        from the menu bar icon to stop it. On macOS it also leaves the Dock while hidden.
      </ToggleSetting>
    )}
  </>
);
