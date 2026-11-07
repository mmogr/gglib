/**
 * Settings › General: whether the endpoint comes up when gglib starts and,
 * in the desktop app only, whether the app starts at login and whether
 * closing its window keeps it running in the menu bar. Each saves on change.
 *
 * The two desktop settings are read by the desktop app alone: the login item
 * is registered when the app starts (`src-tauri/src/autostart.rs`), and the
 * menu bar choice is read at each close (`src-tauri/src/main.rs`). A web page
 * has neither, so it does not draw them; `gglib config settings set
 * --start-at-login` and `--close-to-tray` still write them.
 *
 * @module places/settings/GeneralSection
 */

import type { FC } from 'react';
import { isDesktop } from '../../services/platform';
import { ToggleSetting } from './fields';
import { ON_DESKTOP_START, ON_GGLIB_START } from './notes';

export const GeneralSection: FC = () => (
  <>
    {isDesktop() && (
      <ToggleSetting k="startAtLogin" id="start-at-login-input" label="Start gglib at login" note={ON_DESKTOP_START}>
        Registers the desktop app with your operating system&apos;s login items.
      </ToggleSetting>
    )}

    <ToggleSetting
      k="proxyAutostart"
      id="proxy-autostart-input"
      label="Start the endpoint when gglib launches"
      note={ON_GGLIB_START}
    >
      Brings the OpenAI-compatible endpoint up when gglib starts, so clients such as VS Code Copilot
      reach it without you turning it on. <code>gglib proxy</code> and <code>gglib serve</code> stay
      commands you run.
    </ToggleSetting>

    {isDesktop() && (
      <ToggleSetting k="closeToTray" id="close-to-tray-input" label="Keep running in the menu bar when the window closes">
        Closing the window hides gglib instead of quitting, and the endpoint keeps serving. Quit
        from the menu bar icon to stop it. On macOS it also leaves the Dock while hidden.
      </ToggleSetting>
    )}
  </>
);
