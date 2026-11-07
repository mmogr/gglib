/**
 * Settings › About: the build gglib is running, and stopping gglib.
 *
 * Interim until S3 (`feat(gui)/settings-engines-about`): the stop is today's
 * "Stop the daemon" with its confirm and its "Shutdown requested" line; S3
 * makes it Stop gglib… and the service-not-running screen.
 *
 * @module places/settings/AboutSection
 */

import { FC, useState } from 'react';
import { Button } from '../../components/ui/Button';
import { Banner } from '../../components/ui/Banner';
import { Stack } from '../../components/primitives';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { getTransport } from '../../services/transport';
import { BuildInfo } from './BuildInfo';

export const AboutSection: FC = () => {
  const { confirm } = useConfirmContext();
  const [shuttingDown, setShuttingDown] = useState(false);
  const [shutdownRequested, setShutdownRequested] = useState(false);

  /**
   * `gglib daemon stop`. Lives here rather than in the tray panel, which
   * deliberately keeps window-level actions on the native menu — and unlike
   * that menu's Quit, this reaches the daemon from the web UI too, where
   * there is otherwise no way to stop it.
   *
   * A failed request is not an error: the daemon is shutting down as it
   * replies, so losing the response is the expected outcome.
   */
  const handleShutdown = async () => {
    const confirmed = await confirm({
      title: 'Stop the daemon?',
      description:
        'Stops every running model and shuts down the backend this interface talks to. ' +
        'Nothing on disk is affected, but you will need to start gglib again to use it.',
      confirmLabel: 'Stop',
      variant: 'danger',
    });
    if (!confirmed) return;

    setShuttingDown(true);
    try {
      await getTransport().shutdownDaemon();
    } catch {
      // Expected — the connection dies with the daemon.
    } finally {
      setShuttingDown(false);
      setShutdownRequested(true);
    }
  };

  return (
    <Stack gap="xl">
      <BuildInfo />

      <section>
        <h3 className="m-0 mb-xs text-sm font-semibold text-text">Daemon</h3>
        <p className="m-0 mb-base text-xs text-text-muted">
          Everything here runs against the gglib daemon. Stopping it stops every running model
          with it, and this interface loses its backend until the daemon is started again — from
          a terminal with <code className="font-mono">gglib web</code>, which starts it and opens
          this interface in your browser, or by reopening the desktop app.
        </p>
        {shutdownRequested && (
          <Banner variant="info" className="mb-base">
            Shutdown requested. This interface will stop responding.
          </Banner>
        )}
        <Button
          variant="dangerGhost"
          size="sm"
          isLoading={shuttingDown}
          disabled={shuttingDown || shutdownRequested}
          onClick={() => void handleShutdown()}
        >
          Stop the daemon
        </Button>
      </section>
    </Stack>
  );
};
