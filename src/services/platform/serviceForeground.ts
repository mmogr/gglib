// TRANSPORT_EXCEPTION: Desktop-only, Windows' permission prompt in front
// Only the window the user clicked in can lend the right to come to the
// front, so this asks the app, not the daemon.
// UI code may import from platform/, but clients/ must NOT.

import { isDesktop } from './detect';
import { appLogger } from './logging/appLogger';

/**
 * Let the gglib service bring a window to the front
 * (`allow_service_foreground`), as a request is posted whose program asks
 * Windows for permission: the repair that installs Microsoft's Visual C++
 * runtime. Windows leaves a prompt asked for from the background on the
 * taskbar; the app's window, just clicked, can lend the service the right
 * to come to the front until the user's next input to any other program,
 * gglib's own window included. The prompt comes only after the service
 * has downloaded and checked the installer (about 25 MB), so a click or
 * key press in that time ends the right.
 *
 * Resolves with whether the grant was made, and never rejects: a grant
 * that fails leaves the request as it was, and the daemon's words still
 * say where to look. In a browser there is no app to ask, and nothing is
 * granted.
 */
export async function allowServiceForeground(): Promise<boolean> {
  if (!isDesktop()) return false;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return await invoke<boolean>('allow_service_foreground');
  } catch (err) {
    appLogger.warn('service.platform', 'The app could not let the service come to the front', { error: err });
    return false;
  }
}
