/**
 * The router's creation: after the daemon's token has left the address, and
 * with `/` and anything unknown going to the library.
 */

import { describe, it, expect, afterEach } from 'vitest';
import { render, waitFor } from '@testing-library/react';
import { Outlet, RouterProvider } from 'react-router';

import { appRouter, createAppRouter, disposeAppRouter } from '../../../src/app/router';

describe('the app router', () => {
  afterEach(() => {
    disposeAppRouter();
    window.history.replaceState(null, '', '/');
    window.localStorage.clear();
  });

  it('is created after the token has left the address, so its first location carries none', () => {
    window.history.replaceState(null, '', '/library#token=s3cr3t');

    const router = createAppRouter(<Outlet />);

    expect(router.state.location.hash).toBe('');
    expect(router.state.location.pathname).toBe('/library');
    expect(window.location.hash).toBe('');
    expect(window.localStorage.getItem('gglib_daemon_token')).toBe('s3cr3t');
  });

  it('is one router however many times the browser asks, so the one on screen is the one the menu drives', () => {
    // React runs a state initializer twice in development and keeps the
    // first result; the second call must answer the same router.
    const first = createAppRouter(<Outlet />);
    const second = createAppRouter(<Outlet />);

    expect(second).toBe(first);
    expect(appRouter()).toBe(first);
  });

  it('takes / and an unknown address to the library', async () => {
    for (const start of ['/', '/nowhere/at/all']) {
      const router = createAppRouter(<Outlet />, [start]);
      const view = render(<RouterProvider router={router} />);
      await waitFor(() => expect(router.state.location.pathname).toBe('/library'));
      view.unmount();
    }
  });
});
