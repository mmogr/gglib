/**
 * One tool server as a row: its name, its state as a shape and a word, and
 * how many tools it offers while it runs. A hover (or focus) shows Test,
 * Start or Stop, Edit and ⋯ Remove…, each greyed with why where it cannot
 * act. A server gglib cannot run (an SSE one) offers only Remove…. A stdio
 * server whose command no longer resolves says "Needs relink" with Fix.
 *
 * The name opens the row: the command it runs, its tools, and Devices may
 * use it where the server can be opened to paired devices. How the row's
 * last action went (Test's tools or the reason it failed, a refusal, Fix's
 * path) is said under it.
 *
 * @module places/settings/tools/ServerRow
 */

import { useState, type FC } from 'react';
import { ChevronDown, ChevronRight, MoreHorizontal } from 'lucide-react';
import { StatusMark } from '../../../components/status/StatusMark';
import { Button } from '../../../components/ui/Button';
import { Chip } from '../../../components/ui/Chip';
import { Icon } from '../../../components/ui/Icon';
import type { UseMcpServersResult } from '../../../hooks/useMcpServers';
import type { McpServerInfo, McpTool } from '../../../services/transport';
import { canOpenToDevices, isServerRunning } from '../../../utils/mcp';
import { Menu } from '../../../components/ui/Menu';
import { AllowRemoteCheckbox } from './AllowRemoteCheckbox';
import { serverState } from './serverState';
import type { RowOutcome, ServerActions } from './useServerActions';

/** Why Start is greyed for a server not used in chats. */
export const NOT_IN_CHATS = 'Not used in chats: Edit it and tick Use in chats to start it';
/** Why Remove… is greyed while the server runs. */
export const STOP_FIRST = 'Stop it before removing it';
/** Why Test is greyed while another row's test runs. */
export const ONE_TEST = 'Another test is running';

const count = (n: number) => `${n} tool${n === 1 ? '' : 's'}`;

const Tools: FC<{ tools: McpTool[] }> = ({ tools }) => (
  <span className="flex flex-wrap gap-xs">
    {tools.map((tool) => (
      <Chip key={tool.name} size="sm" className="font-mono font-normal" title={tool.description ?? undefined}>
        {tool.name}
      </Chip>
    ))}
  </span>
);

/** How the row's last action went. */
const Outcome: FC<{ outcome: RowOutcome }> = ({ outcome }) => {
  if (outcome.kind === 'refused') return <p className="m-0 text-xs text-danger">{outcome.message}</p>;
  if (outcome.kind === 'fixed') return <p className="m-0 text-xs text-text-secondary">{`Resolved: ${outcome.path}`}</p>;
  const { result } = outcome;
  if (!result.ok) return <p className="m-0 text-xs text-danger">{`Connection failed — ${result.error ?? ''}`}</p>;
  return (
    <div className="flex flex-col gap-xs">
      <p className="m-0 text-xs text-text-secondary">{`Connected — ${count(result.tools.length)} offered`}</p>
      {result.tools.length > 0 && <Tools tools={result.tools} />}
    </div>
  );
};

interface ServerRowProps {
  info: McpServerInfo;
  actions: ServerActions;
  setAllowRemote: UseMcpServersResult['setAllowRemote'];
  onEdit: (info: McpServerInfo) => void;
}

export const ServerRow: FC<ServerRowProps> = ({ info, actions, setAllowRemote, onEdit }) => {
  const [open, setOpen] = useState(false);
  const { server } = info;
  const state = serverState(info);
  const running = isServerRunning(info);
  const busy = actions.busy.has(server.id);
  const relink = !server.is_valid;
  const outcome = actions.outcome(server.id);
  const command = [server.config.command, ...(server.config.args ?? [])].filter(Boolean).join(' ');

  return (
    <li aria-label={server.name} className="flex flex-col gap-xs rounded-base px-sm py-xs">
      <div className="group relative flex min-h-[36px] items-center gap-md">
        <Button
          variant="ghost"
          size="sm"
          aria-expanded={open}
          className="min-w-0 flex-1 justify-start gap-sm px-0 text-sm font-normal text-text hover:bg-transparent"
          leftIcon={<Icon icon={open ? ChevronDown : ChevronRight} size={14} />}
          onClick={() => setOpen((o) => !o)}
        >
          <span className="truncate">{server.name}</span>
          {running && <span className="ml-sm text-xs text-text-secondary tabular-nums">{count(info.tools.length)}</span>}
        </Button>
        <span className="shrink-0 group-hover:invisible group-focus-within:invisible">
          <StatusMark kind={state.kind} word={state.word} />
        </span>
        <span className="absolute right-0 top-1/2 flex -translate-y-1/2 items-center gap-xs opacity-0 group-hover:opacity-100 group-focus-within:opacity-100">
          <Button
            variant="ghost"
            size="sm"
            disabled={busy || actions.testing}
            title={actions.testing && !busy ? ONE_TEST : 'Start a throwaway instance, list its tools, then stop it'}
            onClick={() => void actions.test(info)}
          >
            Test
          </Button>
          {running ? (
            <Button variant="ghost" size="sm" disabled={busy} onClick={() => void actions.stop(info)}>
              Stop
            </Button>
          ) : (
            <Button
              variant="ghost"
              size="sm"
              disabled={busy || !server.enabled}
              title={server.enabled ? undefined : NOT_IN_CHATS}
              onClick={() => void actions.start(info)}
            >
              Start
            </Button>
          )}
          <Button variant="ghost" size="sm" disabled={busy} onClick={() => onEdit(info)}>
            Edit
          </Button>
          <Menu
            iconOnly
            name={`More for ${server.name}`}
            label={<Icon icon={MoreHorizontal} size={14} />}
            items={[
              {
                label: 'Remove…',
                danger: true,
                disabled: busy || running,
                title: running ? STOP_FIRST : undefined,
                onPick: () => void actions.remove(info),
              },
            ]}
          />
        </span>
      </div>
      {relink && (
        <div className="flex items-center gap-sm pl-lg">
          <span title={server.last_error ?? undefined}>
            <StatusMark kind="attention" word="Needs relink" />
          </span>
          <Button
            variant="secondary"
            size="sm"
            disabled={busy}
            title="Find the command's executable again"
            onClick={() => void actions.fix(info)}
          >
            Fix
          </Button>
        </div>
      )}
      {(state.reason !== null || (relink && server.last_error)) && (
        <p className="m-0 pl-lg text-xs text-danger">{state.reason ?? server.last_error}</p>
      )}
      {open && (
        <div className="flex flex-col gap-sm pl-lg">
          {command && <code className="break-all font-mono text-xs text-text-secondary">{command}</code>}
          {running && info.tools.length > 0 && <Tools tools={info.tools} />}
          {canOpenToDevices(server) && (
            <AllowRemoteCheckbox server={server} label="Devices may use it" onToggle={setAllowRemote} />
          )}
        </div>
      )}
      {outcome && (
        <div className="pl-lg">
          <Outcome outcome={outcome} />
        </div>
      )}
    </li>
  );
};
