/**
 * The app's routes, and its one router.
 *
 * Every screen has an address (`app/paths`). The routes are the children of
 * one layout, which `App` supplies: the providers, the header and the dialogs
 * that sit over every place. The library is a layout route of its own over
 * `/library`, `/models/:id` (and its `/log`), `/far/:id` and
 * `/get/:owner/:name`, so picking a row changes the address without
 * remounting the page.
 *
 * The router instance is module-scoped so the native menu can navigate from
 * outside React (`app/menuBridge`). It is created after `takeDaemonToken()`
 * has taken the daemon's token out of the address, so the first location the
 * router reads carries no token. Tests create a memory router over the same
 * routes (`tests/ts/places/harness.tsx`).
 *
 * @module app/router
 */

import type { ReactNode } from 'react';
import { createBrowserRouter, createMemoryRouter, Navigate, type RouteObject } from 'react-router';

import LibraryPlace from '../places/library/LibraryPlace';
import { takeDaemonToken } from '../services/transport/api/daemonToken';
import { BenchmarkRoute } from './BenchmarkRoute';
import { ChatRoute } from './ChatRoute';
import { FarChatRoute } from './FarChatRoute';
import { paths } from './paths';

export const routes: RouteObject[] = [
  { index: true, element: <Navigate to={paths.library()} replace /> },
  {
    element: <LibraryPlace />,
    children: [{ path: 'library' }, { path: 'models/:id' }, { path: 'models/:id/log' }, { path: 'far/:id' }, { path: 'get/:owner/:name' }],
  },
  { path: 'chat/far/:id', element: <FarChatRoute /> },
  { path: 'chat/:modelId', element: <ChatRoute /> },
  { path: 'diagnostics/benchmarks', element: <BenchmarkRoute /> },
  { path: '*', element: <Navigate to={paths.library()} replace /> },
];

export type AppRouter = ReturnType<typeof createBrowserRouter>;

let current: { router: AppRouter; kind: 'browser' | 'memory' } | null = null;

/**
 * Create the router under `layout`, and make it the app's. In the browser it
 * reads the address bar, and there is only ever one: a second call answers
 * the first's router, because React runs a state initializer twice in
 * development and keeps the first result, and the app's router must be the
 * one on screen. Given `memory`, a new router starts at those entries each
 * time, for tests, and the one before it is disposed.
 */
export function createAppRouter(layout: ReactNode, memory?: string[]): AppRouter {
  if (memory === undefined && current?.kind === 'browser') return current.router;
  // Out of the address bar before the router reads it.
  takeDaemonToken();
  const tree: RouteObject[] = [{ path: '/', element: layout, children: routes }];
  if (current?.kind === 'memory') current.router.dispose();
  current = memory
    ? { router: createMemoryRouter(tree, { initialEntries: memory }), kind: 'memory' }
    : { router: createBrowserRouter(tree), kind: 'browser' };
  return current.router;
}

/** Forget the app's router, for a test that made a browser one. */
export function disposeAppRouter(): void {
  current?.router.dispose();
  current = null;
}

/** The app's router, for navigation from outside React. */
export function appRouter(): AppRouter {
  if (current === null) throw new Error('createAppRouter has not been called');
  return current.router;
}
