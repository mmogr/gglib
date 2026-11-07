/**
 * The Log subject: a model's llama-server output, at `/models/:id/log`.
 *
 * The port is the running server's, or the last one the server registry
 * heard for the model: a stop or a crash names no port, so the registry
 * keeps the last, and that server's log stays on the daemon. With neither,
 * one sentence says why: the model has not run since gglib started, or, for
 * Show log on a failed load (`LogFrom`), the daemon named no port for that
 * start, so its output cannot be read. ‹ goes back to the model; × closes
 * the inspector.
 *
 * @module inspector/log/LogInspector
 */

import type { FC } from 'react';
import { useLocation, useNavigate } from 'react-router';
import { paths } from '../../app/paths';
import type { ServerViewModel } from '../../hooks/useServers';
import { useServerState } from '../../services/serverEvents';
import ConsoleLogPanel, { LogHeader } from './ConsoleLogPanel';

/** What a navigation to the log carries: it came from a failed load's Show log. */
export interface LogFrom {
  failedLoad: true;
}

interface LogInspectorProps {
  modelId: number;
  /** The model's name in the library; its id until the library has it. */
  modelName: string;
  servers: ServerViewModel[];
}

export const LogInspector: FC<LogInspectorProps> = ({ modelId, modelName, servers }) => {
  const navigate = useNavigate();
  const failedLoad = (useLocation().state as Partial<LogFrom> | null)?.failedLoad === true;
  const last = useServerState(modelId);
  const port = servers.find((s) => s.modelId === modelId)?.port ?? last?.port;
  const header = { modelName, onBack: () => navigate(paths.model(modelId)), onClose: () => navigate(paths.library()) };

  if (port === undefined)
    return (
      <div className="flex flex-col">
        <LogHeader {...header} />
        <p className="m-0 px-md text-xs text-text-secondary">
          {failedLoad
            ? 'The output of this start is not available: the daemon named no port for it.'
            : `${modelName} has not run since gglib started; its log starts when it loads.`}
        </p>
      </div>
    );
  return <ConsoleLogPanel key={port} serverPort={port} {...header} />;
};
