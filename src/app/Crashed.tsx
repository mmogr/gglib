/**
 * The screen a page that throws while it draws is replaced by.
 *
 * Without it React Router draws its developer placeholder, "Unexpected
 * Application Error!", and the window is dead (#1959, B1). The router puts
 * this screen in two places (`app/router`): inside the layout, in place of
 * the page that threw, so the sidebar stays and another place can be opened,
 * and going to one draws it; and in place of the whole window, when the
 * layout itself threw. It says what broke in the error's own words and
 * offers a reload, which starts the window again from its address.
 *
 * @module app/Crashed
 */

import type { FC } from 'react';
import { isRouteErrorResponse, useRouteError } from 'react-router';
import { Button } from '../components/ui/Button';
import { formatError } from '../utils/errors';

/** What the error says, in its own words. */
function describe(error: unknown): string {
  if (isRouteErrorResponse(error)) return `${error.status} ${error.statusText}`.trim();
  return formatError(error);
}

export const Crashed: FC = () => {
  const error = useRouteError();
  return (
    <main className="flex h-full min-h-screen flex-1 min-w-0 flex-col items-center justify-center gap-lg bg-background px-lg text-center text-text">
      <div role="alert" className="flex max-w-[560px] flex-col items-center gap-sm">
        <p className="m-0 text-lg">This page stopped working.</p>
        <p className="m-0 text-sm text-text-secondary break-words">{describe(error)}</p>
      </div>
      <Button variant="primary" onClick={() => window.location.reload()}>
        Reload
      </Button>
    </main>
  );
};
