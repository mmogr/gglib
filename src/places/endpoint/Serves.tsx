/**
 * Serves ▾: every model, or only one.
 *
 * The choices are Every model, then each loaded model, then the rest of
 * the library's chat models. Choosing restarts the endpoint, pinned to the
 * model or plain (`useEndpointActions().serve`). The pin is the daemon's
 * (`ProxyStatus.pinned_model`, a model's name), matched to the library by
 * that name.
 *
 * @module places/endpoint/Serves
 */

import type { FC } from 'react';
import type { GgufModel } from '../../types';

interface ServesProps {
  /** The models loaded now, by id and name. */
  loaded: { modelId: number; name: string }[];
  library: GgufModel[];
  pinnedModel: string | null;
  disabled: boolean;
  onServe: (modelId: number | null) => void;
}

export const Serves: FC<ServesProps> = ({ loaded, library, pinnedModel, disabled, onServe }) => {
  const loadedIds = new Set(loaded.map((l) => l.modelId));
  const rest = library.filter((m) => m.imageFamily === undefined && !loadedIds.has(m.id));
  const pinned = pinnedModel === null ? undefined : [...loaded.map((l) => ({ id: l.modelId, name: l.name })), ...library].find((m) => m.name === pinnedModel);
  return (
    <section aria-label="Serves" className="flex items-center gap-md px-md pb-md text-sm">
      <label htmlFor="endpoint-serves" className="text-xs font-semibold text-text-secondary">
        Serves
      </label>
      <select
        id="endpoint-serves"
        className="rounded-base bg-surface-elevated px-sm py-xs text-sm text-text"
        value={pinned === undefined ? '' : String(pinned.id)}
        disabled={disabled}
        onChange={(e) => onServe(e.target.value === '' ? null : Number(e.target.value))}
      >
        <option value="">Every model</option>
        {loaded.length > 0 && (
          <optgroup label="Loaded">
            {loaded.map((l) => (
              <option key={l.modelId} value={l.modelId}>{`Only ${l.name}`}</option>
            ))}
          </optgroup>
        )}
        {rest.length > 0 && (
          <optgroup label="On this Mac">
            {rest.map((m) => (
              <option key={m.id} value={m.id}>{`Only ${m.name}`}</option>
            ))}
          </optgroup>
        )}
      </select>
    </section>
  );
};
