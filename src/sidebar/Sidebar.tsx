/**
 * The sidebar: what gglib is, in the pitch's order.
 *
 * "gglib" atop, over the strip the traffic lights sit on; then Library with
 * how many models it holds, the endpoint line (two lines tall, the
 * signature), Devices; the chats under all three (`ChatList`, which scrolls
 * while the rest stays); Settings and Diagnostics pinned at the foot,
 * Diagnostics in tertiary text. The place that is open is the address, so
 * the open row is read from it, and every row navigates through
 * `app/paths`. Settings opens the settings place on General, Diagnostics
 * the diagnostics place on Live.
 *
 * @module sidebar/Sidebar
 */

import { useLocation, useNavigate } from 'react-router';
import type { ReactNode } from 'react';
import { Button } from '../components/ui/Button';
import { paths } from '../app/paths';
import { isDesktop } from '../services/platform';
import { useModelLibrary } from '../services/modelRegistry';
import { cn } from '../utils/cn';
import { DevicesLine } from './DevicesLine';
import { EndpointLine } from './EndpointLine';
import { ChatList } from './ChatList';
import { SPREAD } from './placement';

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

/**
 * On a macOS desktop the window has no title bar and the traffic lights sit
 * over this strip, so it is taller there and drags the window.
 */
const MAC_DESKTOP = isDesktop() && /Mac/i.test(navigator.platform);

interface RowProps {
  current: boolean;
  onClick: () => void;
  tertiary?: boolean;
  children: ReactNode;
}

/** One sidebar row: a neutral raised fill when it is the open place, no bar, no colour. */
function Row({ current, onClick, tertiary = false, children }: RowProps) {
  return (
    <Button
      variant="ghost"
      fullWidth
      className={cn(
        'h-auto gap-sm px-sm py-sm text-sm',
        SPREAD,
        tertiary ? 'text-text-muted' : 'text-text',
        current && 'bg-surface-hover',
      )}
      aria-current={current ? 'page' : undefined}
      onClick={onClick}
    >
      {children}
    </Button>
  );
}

export function Sidebar() {
  const location = useLocation();
  const navigate = useNavigate();
  const place = placeOf(location.pathname);
  const { models } = useModelLibrary();

  return (
    <nav aria-label="gglib" className="flex h-full flex-col">
      <div
        data-tauri-drag-region
        className={cn('flex items-end px-base pb-sm shrink-0', MAC_DESKTOP ? 'h-[52px]' : 'h-[40px]')}
      >
        <span data-tauri-drag-region className="text-sm font-semibold text-text select-none">
          gglib
        </span>
      </div>

      <div className="flex flex-col gap-xs px-sm">
        <Row current={place === 'library'} onClick={() => navigate(paths.library())}>
          <span>Library</span>
          {models.length > 0 && <span className="text-text-muted tabular-nums">{models.length}</span>}
        </Row>
      </div>

      <EndpointLine current={place === 'endpoint'} />
      <DevicesLine current={place === 'devices'} />

      <ChatList />

      <div className="flex shrink-0 flex-col gap-xs border-t border-border px-sm py-sm">
        <Row current={place === 'settings'} onClick={() => navigate(paths.settings())}>
          <span>Settings</span>
        </Row>
        <Row current={place === 'diagnostics'} tertiary onClick={() => navigate(paths.diagnostics())}>
          <span>Diagnostics</span>
        </Row>
      </div>
    </nav>
  );
}
