/**
 * The sidebar: what gglib is, in the pitch's order.
 *
 * "gglib" atop, except in the Mac app, where that strip is the traffic
 * lights' alone (`TitleStrip`); then Library with how many models it holds,
 * the endpoint line (the signature) and Devices, each a `PlaceRow` with its
 * icon and its state's shape as a badge on it; the chats under all three
 * (`ChatList`, which scrolls while the rest stays); Settings and Diagnostics
 * pinned at the foot, their names on the same edge, Diagnostics in
 * tertiary text. The place that is open is the address, so
 * the open row is read from it, and every row navigates through
 * `app/paths`. Settings opens the settings place on General, Diagnostics
 * the diagnostics place on Live.
 *
 * @module sidebar/Sidebar
 */

import { useLocation, useNavigate } from 'react-router';
import type { FC } from 'react';
import { Library } from 'lucide-react';
import { paths } from '../app/paths';
import { isDesktop } from '../services/platform';
import { useModelLibrary } from '../services/modelRegistry';
import { DevicesLine } from './DevicesLine';
import { EndpointLine } from './EndpointLine';
import { ChatList } from './ChatList';
import { PlaceRow } from './PlaceRow';

/** Which sidebar row the address at `pathname` belongs to. */
export type SidebarPlace = 'library' | 'endpoint' | 'devices' | 'chats' | 'settings' | 'diagnostics' | null;

export function placeOf(pathname: string): SidebarPlace {
  const [, first] = pathname.split('/');
  if (first === 'library' || first === 'models' || first === 'far') return 'library';
  if (first === 'endpoint') return 'endpoint';
  if (first === 'devices') return 'devices';
  if (first === 'chat') return 'chats';
  if (first === 'settings') return 'settings';
  if (first === 'diagnostics') return 'diagnostics';
  return null;
}

const MAC_DESKTOP = isDesktop() && /Mac/i.test(navigator.platform);

/**
 * The strip atop the sidebar, which drags the window. On a macOS desktop the
 * window has no title bar and the traffic lights sit on this strip, so it is
 * taller there and holds nothing else: the menu bar already names gglib, and
 * a wordmark under the lights crowds them. Elsewhere it holds "gglib".
 */
export const TitleStrip: FC<{ mac: boolean }> = ({ mac }) =>
  mac ? (
    <div data-tauri-drag-region className="h-[52px] shrink-0" />
  ) : (
    <div data-tauri-drag-region className="flex h-[40px] shrink-0 items-end px-base pb-sm">
      <span data-tauri-drag-region className="text-sm font-semibold text-text select-none">
        gglib
      </span>
    </div>
  );

export function Sidebar() {
  const location = useLocation();
  const navigate = useNavigate();
  const place = placeOf(location.pathname);
  const { models } = useModelLibrary();

  return (
    <nav aria-label="gglib" className="flex h-full flex-col">
      <TitleStrip mac={MAC_DESKTOP} />

      <div className="flex flex-col gap-[2px]">
        <PlaceRow
          name="Library"
          icon={Library}
          word={models.length > 0 ? <span className="text-text-muted">{models.length}</span> : undefined}
          current={place === 'library'}
          onClick={() => navigate(paths.library())}
        />
        <EndpointLine current={place === 'endpoint'} />
        <DevicesLine current={place === 'devices'} />
      </div>

      <ChatList />

      <div className="flex shrink-0 flex-col gap-[2px] border-t border-border py-sm">
        <PlaceRow name="Settings" current={place === 'settings'} onClick={() => navigate(paths.settings())} />
        <PlaceRow
          name="Diagnostics"
          tertiary
          current={place === 'diagnostics'}
          onClick={() => navigate(paths.diagnostics())}
        />
      </div>
    </nav>
  );
}
