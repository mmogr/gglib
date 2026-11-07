/**
 * Loaded: what the endpoint holds in memory and what each model is doing,
 * from the dashboard's resident slots.
 *
 * A row is the model's name and its state word (Working, Loaded, or
 * "Reading 61%" with a thin bar while a prompt is read), and under it the
 * launch's flag line. Hover offers Unload and Chat; ⋯ offers Chat with it,
 * Change launch options… (the Load with options sheet), Pin the endpoint to
 * it, Show log and Unload. Clicking a row inspects the model beside the
 * place. The queue and the room for a second model are lines of their own.
 *
 * Unload asks the daemon whatever this window believes (TX-12): the rows
 * are the dashboard's, and the server list may not have heard yet.
 *
 * @module places/endpoint/Loaded
 */

import type { FC, KeyboardEvent } from 'react';
import { useNavigate } from 'react-router';
import { MoreHorizontal } from 'lucide-react';
import { HANDED, setChatHandoff } from '../../app/chatHandoff';
import { paths } from '../../app/paths';
import { RESIDENCY_MARK, StatusMark } from '../../components/status/StatusMark';
import { Button } from '../../components/ui/Button';
import { Icon } from '../../components/ui/Icon';
import type { DashboardSnapshot } from '../../services/transport/types/dashboard';
import { cn } from '../../utils/cn';
import { Group, Note } from '../library/Group';
import { Menu } from '../library/Menu';
import { loadedRows, queuedLines, roomLine, type LoadedRow } from './lines';

interface LoadedProps {
  snapshot: DashboardSnapshot;
  /** The model inspected beside the place, if any. */
  inspected: number | null;
  onUnload: (modelId: number) => void;
  onPin: (modelId: number) => void;
  /** A pin change is on its way: Pin waits for it. */
  pinning: boolean;
}

const Row: FC<{ row: LoadedRow; selected: boolean } & Omit<LoadedProps, 'snapshot' | 'inspected'>> = ({ row, selected, onUnload, onPin, pinning }) => {
  const navigate = useNavigate();
  const inspect = () => navigate(paths.endpointInspect(row.modelId));
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget || (event.key !== 'Enter' && event.key !== ' ')) return;
    event.preventDefault();
    inspect();
  };
  // The port is handed on, since the server list may not have heard of it yet.
  const chat = () => {
    setChatHandoff({ modelId: row.modelId, port: row.port, modelName: row.name });
    navigate(paths.chat(row.modelId), { state: HANDED });
  };
  const unload = () => onUnload(row.modelId);
  return (
    <div
      role="option"
      aria-selected={selected}
      tabIndex={0}
      onClick={inspect}
      onKeyDown={onKeyDown}
      className={cn(
        'group relative mx-sm flex min-h-[52px] cursor-pointer items-center gap-md rounded-base px-sm py-sm outline-none',
        'hover:bg-background-hover focus-visible:bg-background-hover',
        selected && 'bg-surface-hover hover:bg-surface-hover',
      )}
    >
      <div className="flex min-w-0 flex-1 flex-col gap-xs">
        <span className="truncate text-sm text-text">{row.name}</span>
        {row.flagLine && <span className="truncate text-xs text-text-secondary tabular-nums">{row.flagLine}</span>}
        {row.reading !== null && (
          <span role="progressbar" aria-label={`${row.name} reading`} aria-valuenow={row.reading} className="block h-[2px] w-1/2 bg-border">
            <span className="block h-full bg-text-secondary" style={{ width: `${row.reading}%` }} />
          </span>
        )}
      </div>
      <span className="shrink-0 group-hover:invisible group-focus-within:invisible">
        <StatusMark kind={RESIDENCY_MARK[row.state]} word={row.word} />
      </span>
      <span
        className="absolute right-sm top-1/2 flex -translate-y-1/2 items-center gap-xs opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
        onClick={(e) => e.stopPropagation()}
      >
        <Button variant="ghost" size="sm" onClick={unload}>
          Unload
        </Button>
        <Button variant="ghost" size="sm" onClick={chat}>
          Chat
        </Button>
        <Menu
          iconOnly
          name={`More for ${row.name}`}
          label={<Icon icon={MoreHorizontal} size={14} />}
          items={[
            { label: 'Chat with it', onPick: chat },
            { label: 'Change launch options…', onPick: () => navigate(paths.endpointServe(row.modelId)) },
            { label: 'Pin the endpoint to it', onPick: () => onPin(row.modelId), disabled: pinning },
            { label: 'Show log', onPick: () => navigate(paths.log(row.modelId)), gapBefore: true },
            { label: 'Unload', onPick: unload, gapBefore: true, danger: true },
          ]}
        />
      </span>
    </div>
  );
};

export const Loaded: FC<LoadedProps> = ({ snapshot, inspected, onUnload, onPin, pinning }) => {
  const navigate = useNavigate();
  const rows = loadedRows(snapshot);
  const room = roomLine(snapshot);
  return (
    <Group label="Loaded" aside={room && <span className="font-normal">{room}</span>}>
      {rows.length === 0 ? (
        <Note>Nothing loaded. An app loads a model by asking for it by name, or load one now.</Note>
      ) : (
        <div role="listbox" aria-label="Loaded models">
          {rows.map((row) => (
            <Row key={row.modelId} row={row} selected={row.modelId === inspected} onUnload={onUnload} onPin={onPin} pinning={pinning} />
          ))}
        </div>
      )}
      {queuedLines(snapshot).map((line) => (
        <Note key={line}>{line}</Note>
      ))}
      <div className="px-sm">
        <Button variant="ghost" size="sm" onClick={() => navigate(paths.library())}>
          Load a model…
        </Button>
      </div>
    </Group>
  );
};
