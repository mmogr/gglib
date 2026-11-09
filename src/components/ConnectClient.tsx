/**
 * Connect a client: the three values a client asks for, and the recipe
 * per client that `gglib up` prints.
 *
 * Base URL ⧉ (the daemon's address), the API key line ("none needed · type
 * any text", or "required" with Show: the key is revealed only when asked,
 * EP-5), and the Model ID ⧉ with a picker of the library's chat models,
 * or, for a pin the picker does not offer (an image model), that model
 * named alone with no picker.
 * ▸ Recipes are the daemon's for that model (`GET /api/proxy/clients`),
 * each line copyable, said as the CLI says them (`readings/connect`): a
 * key the endpoint checks is named as the stored secret, never printed.
 * An answer for a model no longer picked is dropped.
 *
 * @module components/ConnectClient
 */

import { useEffect, useState, type FC } from 'react';
import { KEY_WORDS } from './status/words';
import { CopyValue } from './CopyValue';
import { Button } from './ui/Button';
import { useProxyKey } from '../hooks/useProxyKey';
import { recipesShown } from '../readings/connect';
import { getTransport } from '../services/transport';
import type { GgufModel } from '../types';
import type { ClientRecipes } from '../types/generated/ClientRecipes';
import type { KeyState } from '../types/generated/KeyState';
import { formatError } from '../utils/errors';

/** "Show" beside a required key: the key appears only once asked for, and goes with Hide. */
export const KeyReveal: FC = () => {
  const [shown, setShown] = useState(false);
  const key = useProxyKey(shown);
  return (
    <>
      {shown && key && (
        <>
          <code className="font-mono text-text">{key}</code>
          <CopyValue value={key} label="Copy the key" />
        </>
      )}
      <Button variant="link" size="sm" className="text-xs font-normal" onClick={() => setShown((s) => !s)}>
        {shown ? 'Hide' : 'Show'}
      </Button>
    </>
  );
};

interface ConnectClientProps {
  baseUrl: string;
  keyState: KeyState | null;
  /** The chat models a client may name. */
  models: GgufModel[];
  /** The model picked, by gglib id; `null` before there is one. */
  modelId: number | null;
  onModel: (modelId: number) => void;
  /** A pin that is not among `models`: the only model a client can name, so it is named alone, with no picker, and stands for `modelId`. */
  only?: { id: number; name: string };
}

const Line: FC<{ label: string; children: React.ReactNode }> = ({ label, children }) => (
  <div className="flex items-center gap-md text-sm">
    <span className="w-20 shrink-0 text-xs text-text-secondary">{label}</span>
    <span className="flex min-w-0 items-center gap-xs text-text-secondary">{children}</span>
  </div>
);

export const ConnectClient: FC<ConnectClientProps> = ({ baseUrl, keyState, models, modelId: picked, onModel, only }) => {
  const modelId = only?.id ?? picked;
  const [answer, setAnswer] = useState<{ for: number; recipes: ClientRecipes } | { for: number; failed: string } | null>(null);

  useEffect(() => {
    if (modelId === null) return undefined;
    let current = true;
    getTransport()
      .getProxyClients(modelId)
      .then((recipes) => current && setAnswer({ for: modelId, recipes }))
      .catch((err: unknown) => current && setAnswer({ for: modelId, failed: formatError(err) }));
    return () => {
      current = false;
    };
  }, [modelId, baseUrl]);

  const held = answer !== null && answer.for === modelId ? answer : null;
  const recipes = held && 'recipes' in held ? held.recipes : null;
  const shownId = recipes?.model_id ?? models.find((m) => m.id === modelId)?.openaiId ?? null;

  return (
    <section aria-label="Connect a client" className="flex flex-col gap-sm px-md pb-md">
      <h3 className="m-0 text-xs font-semibold text-text-secondary">Connect a client</h3>
      <Line label="Base URL">
        <code className="truncate font-mono text-text">{baseUrl}</code>
        <CopyValue value={baseUrl} label="Copy the base URL" />
      </Line>
      <Line label="API key">
        {keyState === 'required' ? (
          <>
            <span>{KEY_WORDS.required.toLowerCase()} ·</span>
            <KeyReveal />
          </>
        ) : (
          <span>{`${KEY_WORDS.none_needed.toLowerCase()} · type any text`}</span>
        )}
      </Line>
      <Line label="Model ID">
        {shownId !== null && (
          <>
            <code className="truncate font-mono text-text">{shownId}</code>
            <CopyValue value={shownId} label="Copy the Model ID" />
          </>
        )}
        {only ? (
          <span className="text-xs text-text">{only.name}</span>
        ) : (
          <select
            aria-label="Model to connect with"
            className="rounded-base bg-surface-elevated px-sm py-xs text-xs text-text"
            value={modelId ?? ''}
            onChange={(e) => onModel(Number(e.target.value))}
          >
            {modelId === null && <option value="">any library name</option>}
            {models.map((m) => (
              <option key={m.id} value={m.id}>
                {m.name}
              </option>
            ))}
          </select>
        )}
      </Line>
      {held && 'failed' in held && <p className="m-0 text-xs text-text-secondary">{held.failed}</p>}
      {recipes && (
        <details>
          <summary className="cursor-pointer text-sm text-text">Recipes</summary>
          <div className="flex flex-col gap-md pl-md pt-sm">
            {recipesShown(recipes).map((recipe) => (
              <div key={recipe.client} className="flex flex-col gap-xs" aria-label={recipe.client} role="group">
                <p className="m-0 text-sm text-text">
                  {recipe.client} <span className="text-xs text-text-secondary">{recipe.hint}</span>
                </p>
                {recipe.lines.map((line) => (
                  <Line key={line.label} label={line.label}>
                    <code className="truncate font-mono text-text">{line.value}</code>
                    <CopyValue value={line.value} label={`Copy ${recipe.client} ${line.label}`} />
                  </Line>
                ))}
              </div>
            ))}
          </div>
        </details>
      )}
    </section>
  );
};
