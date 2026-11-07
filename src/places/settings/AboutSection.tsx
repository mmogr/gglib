/**
 * Settings › About: the build gglib is running, its documentation, and
 * Stop gglib….
 *
 * Stop gglib… asks first, then asks the daemon to stop (`gglib daemon
 * stop`). Unlike the tray menu's Stop gglib…, it reaches the daemon from a
 * browser tab too. The window then shows the service-not-running screen
 * (`app/ServiceDown`) once the daemon no longer answers; until then the
 * button says it is stopping. A stop the daemon answers with a failure (one
 * not running as the gglib daemon, a key it wants, an error) says why here
 * in its words, and the button comes back.
 *
 * @module places/settings/AboutSection
 */

import { FC, useState } from 'react';
import { ExternalLink } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import { useConfirmContext } from '../../contexts/ConfirmContext';
import { openUrl } from '../../services/platform';
import { getTransport } from '../../services/transport';
import { TransportError } from '../../services/transport/errors';
import { formatError } from '../../utils/errors';
import { BuildInfo } from './BuildInfo';

/** Where the documentation is: the native Help menu's gglib Documentation opens the same. */
const DOCUMENTATION = 'https://github.com/mmogr/gglib';

export const AboutSection: FC = () => {
  const { confirm } = useConfirmContext();
  const [stopping, setStopping] = useState(false);
  const [refusal, setRefusal] = useState<string | null>(null);

  const stop = async () => {
    const yes = await confirm({
      title: 'Stop gglib?',
      description:
        'This stops every running model and the endpoint. Any client using the endpoint will lose it. ' +
        'Nothing on disk is affected.',
      confirmLabel: 'Stop',
      variant: 'danger',
    });
    if (!yes) return;
    setStopping(true);
    setRefusal(null);
    try {
      await getTransport().shutdownDaemon();
    } catch (err) {
      // A failure with no HTTP answer is the connection dying with the
      // daemon as it stops, which is the stop working. Any answer that is a
      // failure (refused, not allowed, broken) is the daemon still there.
      if (TransportError.isTransportError(err)) {
        setRefusal(formatError(err));
        setStopping(false);
      }
    }
  };

  return (
    <div className="flex flex-col gap-lg">
      <BuildInfo />
      <Button
        variant="ghost"
        size="sm"
        className="self-start px-0 hover:bg-transparent"
        rightIcon={<Icon icon={ExternalLink} size={14} />}
        onClick={() => void openUrl(DOCUMENTATION)}
      >
        Documentation
      </Button>
      <div className="flex flex-col gap-sm">
        <Button
          variant="dangerGhost"
          size="sm"
          className="self-start"
          disabled={stopping}
          onClick={() => void stop()}
        >
          {stopping ? 'Stopping gglib…' : 'Stop gglib…'}
        </Button>
        {refusal && (
          <p role="alert" className="m-0 text-sm text-danger">
            {refusal}
          </p>
        )}
      </div>
    </div>
  );
};
