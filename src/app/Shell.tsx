/**
 * The window: a sidebar and the place beside it.
 *
 * The sidebar names what gglib is (the library, the endpoint with its
 * address, the devices) on every screen; the place is whatever the address
 * says (the router's outlet). There is no header: the window frame holds no
 * data, and on macOS the traffic lights sit over the sidebar's top strip.
 *
 * Under 720 px the sidebar is a drawer behind a menu button, closed by a
 * move to another place, by the scrim, or by Escape, so a narrow web window
 * keeps every destination. The one divider in the window is the sidebar's
 * right edge.
 *
 * @module app/Shell
 */

import { useEffect, useState } from 'react';
import { Outlet, useLocation } from 'react-router';
import { Menu } from 'lucide-react';
import { Sidebar } from '../sidebar/Sidebar';
import { IconButton } from '../components/ui/IconButton';
import { Icon } from '../components/ui/Icon';

export function Shell() {
  const [drawerOpen, setDrawerOpen] = useState(false);
  const location = useLocation();

  // A move to another place closes the drawer; the place is what was wanted.
  useEffect(() => {
    setDrawerOpen(false);
  }, [location.pathname]);

  useEffect(() => {
    if (!drawerOpen) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setDrawerOpen(false);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, [drawerOpen]);

  return (
    <div className="flex h-screen overflow-hidden bg-background text-text">
      <aside className="hidden wide:flex w-sidebar-width shrink-0 flex-col border-r border-border bg-background-secondary">
        <Sidebar />
      </aside>

      <div className="wide:hidden fixed top-sm left-sm z-dropdown">
        <IconButton
          variant="ghost"
          size="sm"
          label={drawerOpen ? 'Close the sidebar' : 'Open the sidebar'}
          aria-expanded={drawerOpen}
          onClick={() => setDrawerOpen((open) => !open)}
        >
          <Icon icon={Menu} size={18} />
        </IconButton>
      </div>

      {drawerOpen && (
        <div className="wide:hidden fixed inset-0 z-modal flex" data-testid="sidebar-drawer">
          <aside className="w-sidebar-width max-w-[85vw] flex flex-col border-r border-border bg-background-secondary pt-xl">
            <Sidebar />
          </aside>
          <div className="flex-1 bg-black/40" aria-hidden onClick={() => setDrawerOpen(false)} />
        </div>
      )}

      <main className="flex-1 min-w-0 min-h-0 flex overflow-hidden">
        <Outlet />
      </main>
    </div>
  );
}
