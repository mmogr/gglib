/**
 * Rendering the app's places as the app does: the real routes under the
 * real providers, over a memory router started at `path`. What a test stubs
 * is the transport (through `vi.mock`) and the registries' contents; what it
 * drives is the screen, by address or by click.
 *
 * `renderApp` mounts the routes under the providers the layout gives them,
 * without the sidebar and the dialogs the layout draws over every place;
 * `renderShell` mounts them inside the shell, sidebar and all, for a test of
 * the sidebar itself; `renderGated` mounts them behind the setup gate, as
 * the layout does, under StrictMode; a test of the dialogs renders `App`. The router each
 * makes is the app's, so `app/menuBridge` and anything else that navigates
 * from outside React reaches it.
 */

import { StrictMode, type ReactNode } from 'react';
import { Outlet, RouterProvider } from 'react-router';
import { render } from '@testing-library/react';

import { createAppRouter, type AppRouter } from '../../../src/app/router';
import { SetupGate } from '../../../src/app/SetupGate';
import { Shell } from '../../../src/app/Shell';
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

function rendered(router: AppRouter, strict = false): RenderedApp {
  const provider = <RouterProvider router={router} />;
  const view = render(strict ? <StrictMode>{provider}</StrictMode> : provider);
  return { ...view, router, at: () => `${router.state.location.pathname}${router.state.location.search}` };
}

/** Render the place at `path`, with the real routes and providers. */
export function renderApp(path: string): RenderedApp {
  return rendered(createAppRouter(<Layout />, [path]));
}

/** The shell, sidebar and all, around the place at `path`. */
function ShellLayout() {
  return (
    <PlaceProviders>
      <Shell />
    </PlaceProviders>
  );
}

/** Render the shell at `path`: the sidebar beside the place the address names. */
export function renderShell(path: string): RenderedApp {
  return rendered(createAppRouter(<ShellLayout />, [path]));
}

/** The routes behind the setup gate, as the layout puts them. */
function GatedLayout() {
  return (
    <PlaceProviders>
      <SetupGate>
        <Outlet />
      </SetupGate>
    </PlaceProviders>
  );
}

/**
 * Render the place at `path` behind the setup gate: setup while the daemon
 * says it is not done. Under StrictMode, as `main.tsx` renders the app in
 * development, so an effect that cannot run twice fails here.
 */
export function renderGated(path: string): RenderedApp {
  return rendered(createAppRouter(<GatedLayout />, [path]), true);
}
