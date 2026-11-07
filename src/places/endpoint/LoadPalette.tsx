/**
 * Load a model…: the endpoint's palette for loading one, opened from its
 * button under the Loaded rows.
 *
 * A filter on top (typed words narrow the rows as the library's search
 * does), then this machine's models not already loaded, each with its quant
 * and size and one line of fit (`paletteRows`, the reads `useFitReads`
 * makes), then the joined machine's models as On <machine>. ↑ ↓ Home End
 * move the highlight (the field keeps focus, `aria-activedescendant` names
 * the row); ⏎ or a click loads it as the inspector's one-click Load does,
 * the model's id and nothing typed (IN-7), and closes the palette, the
 * Loaded rows then saying Loading from the dashboard; ⌥⏎ opens Load with
 * options for it instead (`paths.endpointServe`). A far row loads on that
 * machine where it may, as the far inspector's Load does. A failed load is
 * one toast in the daemon's words. Esc closes and gives focus back to the
 * button; a click outside closes. The paired rows never hold up this
 * machine's (LB-6): they come from their own hook, already read.
 *
 * @module places/endpoint/LoadPalette
 */

import { useEffect, useId, useRef, useState, type FC, type KeyboardEvent } from 'react';
import { useNavigate } from 'react-router';
import { paths } from '../../app/paths';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { useToastContext } from '../../contexts/ToastContext';
import { useClickOutside } from '../../hooks/useClickOutside';
import type { PairedModelsState } from '../../hooks/usePairedModels';
import { useModelLibrary } from '../../services/modelRegistry';
import { getTransport } from '../../services/transport';
import { cn } from '../../utils/cn';
import { formatError } from '../../utils/errors';
import { farChoices, fitLine, localAside, localChoices } from './paletteRows';
import { useFitReads } from './useFitReads';

/** A row's height, and how many rows the list shows: the rows in view are read after the highlighted one. */
const ROW_PX = 52;
const IN_VIEW = 6;

interface LoadPaletteProps {
  /** The models loaded now, by id: the Loaded rows' own. */
  loaded: ReadonlySet<number>;
  paired: PairedModelsState;
}

/** One row of the list, this machine's or the paired machine's. */
type Choice = { kind: 'local'; id: number } | { kind: 'far'; id: number };

export const LoadPalette: FC<LoadPaletteProps> = ({ loaded, paired }) => {
  const navigate = useNavigate();
  const { showToast } = useToastContext();
  const { models } = useModelLibrary();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const [active, setActive] = useState(0);
  const [first, setFirst] = useState(0);
  const wrap = useRef<HTMLDivElement>(null);
  const button = useRef<HTMLButtonElement>(null);
  const listId = useId();

  const local = open ? localChoices(models, loaded, query) : [];
  const far = open ? farChoices(paired, query) : null;
  const choices: Choice[] = [
    ...local.map((m) => ({ kind: 'local' as const, id: m.id })),
    ...(far?.models ?? []).map((m) => ({ kind: 'far' as const, id: m.gglib_id })),
  ];
  const current = Math.min(active, choices.length - 1);
  const highlighted = choices[current] ?? null;
  const optionId = (c: Choice) => `${listId}-${c.kind}-${c.id}`;

  const inView = local.slice(first, first + IN_VIEW).map((m) => m.id);
  const wanted = highlighted?.kind === 'local' ? [highlighted.id, ...inView] : inView;
  const fit = useFitReads(wanted);

  const show = () => {
    fit.begin();
    setQuery('');
    setActive(0);
    setFirst(0);
    setOpen(true);
  };
  const close = (refocus: boolean) => {
    fit.end();
    setOpen(false);
    if (refocus) button.current?.focus();
  };
  useClickOutside(wrap, () => close(false), open);

  useEffect(() => {
    if (highlighted) document.getElementById(optionId(highlighted))?.scrollIntoView?.({ block: 'nearest' });
    // The highlighted row's id is what matters, not the object.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [highlighted?.kind, highlighted?.id]);

  const failed = (err: unknown) => showToast(`Didn't load: ${formatError(err)}`, 'error');
  const choose = (choice: Choice | null, withOptions: boolean) => {
    if (!choice) return;
    if (choice.kind === 'far') {
      if (far?.blocked || withOptions) return;
      close(true);
      getTransport()
        .loadPairedModel(choice.id)
        .then(() => paired.refetch(), failed);
      return;
    }
    close(true);
    if (withOptions) navigate(paths.endpointServe(choice.id));
    else getTransport().serveModel({ id: choice.id }).catch(failed);
  };

  const move = (to: number) => setActive(Math.max(0, Math.min(choices.length - 1, to)));
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const keys: Record<string, () => void> = {
      ArrowDown: () => move(current + 1),
      ArrowUp: () => move(current - 1),
      Home: () => move(0),
      End: () => move(choices.length - 1),
      Enter: () => choose(highlighted, event.altKey),
      Escape: () => close(true),
    };
    const act = keys[event.key];
    if (!act) return;
    event.preventDefault();
    act();
  };

  const row = (choice: Choice, index: number, name: string, line: string | null, aside: string, blocked = false) => (
    <div
      key={optionId(choice)}
      id={optionId(choice)}
      role="option"
      aria-selected={index === current}
      aria-disabled={blocked || undefined}
      onMouseDown={(e) => e.preventDefault()}
      onMouseMove={() => index !== current && setActive(index)}
      onClick={() => choose(choice, false)}
      className={cn(
        'flex min-h-[52px] items-center gap-md rounded-base px-md py-sm',
        blocked ? 'cursor-default' : 'cursor-pointer',
        index === current && 'bg-surface-hover',
      )}
    >
      <span className="flex min-w-0 flex-1 flex-col gap-xs">
        <span className={cn('truncate text-sm', blocked ? 'text-text-secondary' : 'text-text')}>{name}</span>
        {line && <span className="truncate text-xs text-text-secondary">{line}</span>}
      </span>
      {aside && <span className="shrink-0 text-xs text-text-secondary tabular-nums">{aside}</span>}
    </div>
  );

  return (
    <div className="relative px-sm" ref={wrap}>
      <Button ref={button} variant="ghost" size="sm" aria-haspopup="dialog" aria-expanded={open} onClick={() => (open ? close(false) : show())}>
        Load a model…
      </Button>
      {open && (
        <div
          role="dialog"
          aria-label="Load a model"
          className="absolute left-sm top-full z-popover mt-xs flex w-[min(560px,calc(100vw-32px))] flex-col gap-xs rounded-base bg-surface-elevated p-xs shadow-lg"
        >
          <Input
            autoFocus
            size="sm"
            role="combobox"
            aria-label="Load a model"
            aria-expanded
            aria-controls={listId}
            aria-autocomplete="list"
            aria-activedescendant={highlighted ? optionId(highlighted) : undefined}
            placeholder="Load a model…"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setActive(0);
            }}
            onKeyDown={onKeyDown}
          />
          <div
            id={listId}
            role="listbox"
            aria-label="Models to load"
            className="flex flex-col overflow-y-auto"
            style={{ maxHeight: ROW_PX * IN_VIEW }}
            onScroll={(e) => setFirst(Math.floor(e.currentTarget.scrollTop / ROW_PX))}
          >
            {local.map((m, i) => row({ kind: 'local', id: m.id }, i, m.name, fitLine(fit.reads.get(m.id)), localAside(m)))}
            {far && far.models.length > 0 && (
              <div role="group" aria-label={`On ${far.name}`} className="flex flex-col">
                <span role="presentation" className="px-md pb-xs pt-sm text-xs font-semibold text-text-secondary">{`On ${far.name}`}</span>
                {far.models.map((m, i) =>
                  row({ kind: 'far', id: m.gglib_id }, local.length + i, m.id, far.blocked ?? `Load on ${far.name}`, m.quantization ?? '', far.blocked !== null),
                )}
              </div>
            )}
          </div>
          {choices.length === 0 && (
            <p className="m-0 px-md py-xs text-xs text-text-secondary">{query.trim() ? 'No model matches.' : models.length === 0 ? 'No models on this Mac.' : 'Every model on this Mac is loaded.'}</p>
          )}
          <p className="m-0 border-t border-border px-md pt-xs text-xs text-text-muted">⏎ Load · ⌥⏎ Load with options…</p>
        </div>
      )}
    </div>
  );
};
