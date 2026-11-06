/**
 * Rendering the app's places as the app does: the real routes under the
 * real providers, over a memory router started at `path`. What a test stubs
 * is the transport (through `vi.mock`) and the registries' contents; what it
 * drives is the screen, by address or by click.
 *
 * `renderApp` mounts the routes under the providers the layout gives them,
 * without the header and the dialogs the layout draws over every place; a
 * test of those renders `App` itself. The router it makes is the app's, so
 * `app/menuBridge` and anything else that navigates from outside React
 * reaches it.
 */

import type { ReactNode } from 'react';
import { Outlet, RouterProvider } from 'react-router';
import { render } from '@testing-library/react';

import { createAppRouter, type AppRouter } from '../../../src/app/router';
import { ConfirmProvider } from '../../../src/contexts/ConfirmContext';
import { SettingsProvider } from '../../../src/contexts/SettingsContext';
import { ToastProvider, useToastContext } from '../../../src/contexts/ToastContext';

/** The toasts, which `ToastProvider` holds but does not draw. */
function Toasts() {
  const { toasts } = useToastContext();
  return <div data-testid="toasts">{toasts.map((t) => t.message).join(' | ')}</div>;
}

/** The providers every place has, around the place the address names. */
export function PlaceProviders({ children }: { children: ReactNode }) {
  return (
    <ToastProvider>
      <ConfirmProvider>
        <SettingsProvider showToast={() => {}}>
          {children}
          <Toasts />
        </SettingsProvider>
      </ConfirmProvider>
    </ToastProvider>
  );
}

function Layout() {
  return (
    <PlaceProviders>
      <Outlet />
    </PlaceProviders>
  );
}

export interface RenderedApp extends ReturnType<typeof render> {
  router: AppRouter;
  /** The address now, path and search. */
  at: () => string;
}

/** Render the place at `path`, with the real routes and providers. */
export function renderApp(path: string): RenderedApp {
  const router = createAppRouter(<Layout />, [path]);
  const view = render(<RouterProvider router={router} />);
  return { ...view, router, at: () => `${router.state.location.pathname}${router.state.location.search}` };
}
