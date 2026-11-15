/**
 * A place that throws while it draws (#1959, B1). The window shows gglib's
 * own screen, inside the layout, so the sidebar stays and another place can
 * be opened; going to one draws it. A layout that throws gets the same
 * screen in place of the whole window. Never React Router's developer
 * placeholder ("Unexpected Application Error!"), which left the window dead.
 */

import { describe, it, expect, vi, afterEach } from 'vitest';
import { act, render, screen } from '@testing-library/react';
import '@testing-library/jest-dom';
import { Outlet, RouterProvider } from 'react-router';

import { createAppRouter, disposeAppRouter } from '../../../src/app/router';

vi.mock('../../../src/places/devices/DevicesPlace', () => ({
  default: () => {
    throw new Error('the devices list was undefined');
  },
}));
vi.mock('../../../src/places/endpoint/EndpointPlace', () => ({ default: () => <main aria-label="Endpoint" /> }));

function Layout() {
  return (
    <div data-testid="layout">
      <Outlet />
    </div>
  );
}

function Broken(): never {
  throw new Error('the layout could not start');
}

describe('a place that throws', () => {
  afterEach(() => {
    disposeAppRouter();
    vi.restoreAllMocks();
  });

  it('shows gglib’s own screen inside the layout, and another place opens from it', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    const router = createAppRouter(<Layout />, ['/devices']);
    render(<RouterProvider router={router} />);

    const crashed = await screen.findByRole('alert');
    expect(crashed).toHaveTextContent('This page stopped working.');
    expect(crashed).toHaveTextContent('the devices list was undefined');
    expect(screen.getByTestId('layout')).toContainElement(crashed);
    expect(screen.queryByText(/Unexpected Application Error/)).not.toBeInTheDocument();

    await act(() => router.navigate('/endpoint'));
    expect(screen.getByRole('main', { name: 'Endpoint' })).toBeInTheDocument();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('shows the same screen in place of a layout that throws', async () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    render(<RouterProvider router={createAppRouter(<Broken />, ['/endpoint'])} />);

    const crashed = await screen.findByRole('alert');
    expect(crashed).toHaveTextContent('This page stopped working.');
    expect(crashed).toHaveTextContent('the layout could not start');
    expect(screen.getByRole('button', { name: 'Reload' })).toBeInTheDocument();
    expect(screen.queryByText(/Unexpected Application Error/)).not.toBeInTheDocument();
  });
});
