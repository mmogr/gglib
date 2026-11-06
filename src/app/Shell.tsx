/**
 * The window: a sidebar and the place beside it.
 *
 * The sidebar names what gglib is (the library, the endpoint with its
 * address, the devices) on every screen; the place is whatever the address
 * says (the router's outlet). There is no header: the window frame holds no
 * data, and on macOS the traffic lights sit over the sidebar's top strip.
 *
 * Under 720 px the sidebar is a drawer behind a menu button in a strip
 * above the place, so the button covers nothing. The drawer closes on any
 * move (a new place, or a dialog opened over this one), on the scrim, on
 * its own close button, or on an Escape no dialog has already handled, so
 * a narrow web window keeps every destination. The one divider in the
 * window is the sidebar's right edge.
 *
 * @module app/Shell
 */

import { useEffect, useState } from 'react';
import { Outlet, useLocation } from 'react-router';
import { Menu, X } from 'lucide-react';
import { Sidebar } from '../sidebar/Sidebar';
import { IconButton } from '../components/ui/IconButton';
import { Icon } from '../components/ui/Icon';

export function Shell() {
  const [drawerOpen, setDrawerOpen] = useState(false);
  const location = useLocation();

  // Any navigation closes the drawer: a new place, or a dialog opened over
  // this one by a search parameter. What was wanted is now on screen.
  useEffect(() => {
    setDrawerOpen(false);
  }, [location.key]);

  useEffect(() => {
    if (!drawerOpen) return;
    const onKey = (event: KeyboardEvent) => {
      // A dialog consumes Escape on a capture listener and marks it handled;
      // the same keypress must not also take the drawer it was opened from.
      if (event.defaultPrevented) return;
      if (event.key === 'Escape') setDrawerOpen(false);
    };
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [drawerOpen]);

  return (
    <div className="flex h-screen overflow-hidden bg-background text-text">
      <aside className="hidden wide:flex w-sidebar-width shrink-0 flex-col border-r border-border bg-background-secondary">
        <Sidebar />
      </aside>

      {drawerOpen && (
        <div className="wide:hidden fixed inset-0 z-modal flex" data-testid="sidebar-drawer">
          <aside className="relative w-sidebar-width max-w-[85vw] flex flex-col border-r border-border bg-background-secondary">
            <div className="absolute top-sm right-sm">
              <IconButton variant="ghost" size="sm" label="Close the sidebar" onClick={() => setDrawerOpen(false)}>
                <Icon icon={X} size={16} />
              </IconButton>
            </div>
            <Sidebar />
          </aside>
          <div
            className="flex-1 bg-black/40"
            aria-hidden
            data-testid="sidebar-scrim"
            onClick={() => setDrawerOpen(false)}
          />
        </div>
      )}

      <main className="flex-1 min-w-0 min-h-0 flex flex-col overflow-hidden">
        <div className="wide:hidden h-10 shrink-0 flex items-center px-sm border-b border-border">
          <IconButton
            variant="ghost"
            size="sm"
            label="Open the sidebar"
            aria-expanded={drawerOpen}
            onClick={() => setDrawerOpen(true)}
          >
            <Icon icon={Menu} size={18} />
          </IconButton>
        </div>
        <div className="flex-1 min-h-0 flex overflow-hidden">
          <Outlet />
        </div>
      </main>
    </div>
  );
}
