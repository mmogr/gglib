/**
 * Settings › Tools' tool servers: a row each, then + Add a server, which
 * opens the one sheet Edit opens too.
 *
 * The rows, their actions and the sheet share the section's one list (ST-8):
 * two copies meant a save refreshed one and left the other stale, and an
 * Edit from the stale one sent a variable the server no longer kept. The
 * list is read when the section opens and again after every change made
 * here, so nothing offers a Refresh. The daemon announces no change to its
 * tool servers, so one made from a terminal shows the next time the section
 * opens. A read that fails is said on its own line, over
 * whatever is listed; a refused change is said where it was asked (the
 * sheet, or under its row), never here.
 *
 * @module places/settings/tools/ToolServers
 */

import { useState, type FC } from 'react';
import { Plus } from 'lucide-react';
import { StatusMark } from '../../../components/status/StatusMark';
import { Button } from '../../../components/ui/Button';
import { Icon } from '../../../components/ui/Icon';
import type { UseMcpServersResult } from '../../../hooks/useMcpServers';
import type { McpServerInfo } from '../../../services/transport';
import { ServerRow } from './ServerRow';
import { ServerSheet } from './ServerSheet';
import { useServerActions } from './useServerActions';

export const ToolServers: FC<{ mcp: UseMcpServersResult }> = ({ mcp }) => {
  const { servers, loading, error, addServer, updateServer, setAllowRemote } = mcp;
  const actions = useServerActions(mcp);
  // `null` closed; `'new'` adding; else the server being edited.
  const [sheet, setSheet] = useState<McpServerInfo | 'new' | null>(null);

  return (
    <section aria-label="Tool servers" className="flex flex-col gap-sm">
      <span className="text-sm font-semibold text-text">Tool servers</span>
      {error !== null && (
        <p role="alert" className="m-0 text-sm text-danger">
          {`Couldn't read the tool servers: ${error}`}
        </p>
      )}
      {loading ? (
        <StatusMark kind="changing" label="Asking gglib" />
      ) : servers.length === 0 ? (
        error === null && (
          <p className="m-0 text-sm text-text-secondary">No tool servers yet. Add one to give chats tools such as web search.</p>
        )
      ) : (
        <ul className="m-0 flex list-none flex-col p-0">
          {servers.map((info) => (
            <ServerRow key={info.server.id} info={info} actions={actions} setAllowRemote={setAllowRemote} onEdit={setSheet} />
          ))}
        </ul>
      )}
      <Button
        variant="secondary"
        size="sm"
        className="self-start"
        leftIcon={<Icon icon={Plus} size={14} />}
        onClick={() => setSheet('new')}
      >
        Add a server
      </Button>
      {sheet !== null && (
        <ServerSheet
          editing={sheet === 'new' ? undefined : sheet}
          onClose={() => setSheet(null)}
          onSave={async (data) => {
            if (sheet === 'new') await addServer(data);
            else await updateServer(sheet.server.id, data);
            setSheet(null);
          }}
        />
      )}
    </section>
  );
};
