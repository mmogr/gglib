/**
 * `▸ Log` on a loaded model's inspector: the server's last line, in
 * tertiary mono, and a click opens the Log subject (`/models/:id/log`).
 *
 * @module inspector/local/LogLink
 */

import type { FC } from 'react';
import Anser from 'anser';
import { useNavigate } from 'react-router';
import { ChevronRight } from 'lucide-react';
import { paths } from '../../app/paths';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import { useServerLogs } from '../../hooks/useServerLogs';

export const LogLink: FC<{ modelId: number; port: number }> = ({ modelId, port }) => {
  const navigate = useNavigate();
  const { logs } = useServerLogs({ serverPort: port });
  const tail = logs.at(-1);
  const last = tail && Anser.ansiToText(tail.line).trim();
  return (
    <Button
      variant="ghost"
      size="sm"
      onClick={() => navigate(paths.log(modelId))}
      className="h-auto min-w-0 justify-start gap-xs p-0 text-left text-xs font-normal"
    >
      <Icon icon={ChevronRight} size={12} />
      <span className="shrink-0 font-semibold text-text">Log</span>
      {last && <span className="min-w-0 truncate font-mono text-text-muted">{last}</span>}
    </Button>
  );
};
