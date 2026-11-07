/**
 * `▸ Log` on a loaded model's inspector: the label opens the Log subject
 * (`/models/:id/log`), and the server's last line follows it in tertiary
 * mono, cut with an ellipsis at the inspector's edge (whole on hover).
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
    <div role="group" aria-label="Log" className="flex min-w-0 items-center gap-sm text-xs">
      <Button
        variant="ghost"
        size="sm"
        className="h-auto shrink-0 gap-xs p-0 text-xs"
        leftIcon={<Icon icon={ChevronRight} size={12} />}
        onClick={() => navigate(paths.log(modelId))}
      >
        Log
      </Button>
      {last && (
        <span className="min-w-0 flex-1 truncate font-mono text-text-muted" title={last}>
          {last}
        </span>
      )}
    </div>
  );
};
