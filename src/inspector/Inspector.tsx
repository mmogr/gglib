/**
 * The inspector: one subject at a time, on the right of a place, read from
 * the address (`inspectorSubject`). A model on this machine (`local/`), its
 * log (`log/`), the paired machine's model (`far/`) or a Hugging Face repo
 * (`hf/`). Nothing is drawn with no subject.
 *
 * A model the library does not hold says so in one line, once the library
 * has been read. Each subject gets a fresh inspector, so nothing picked for
 * one (a profile, a row being changed, a size) carries to the next.
 *
 * Its left edge drags its width (`useInspectorWidth`): 320 to 720 px,
 * remembered per viewer; the log opens at 60% of the window.
 *
 * @module inspector/Inspector
 */

import type { FC } from 'react';
import { useLocation } from 'react-router';
import { inspectorSubject } from '../app/paths';
import type { PairedModelsState } from '../hooks/usePairedModels';
import { useModel, useModelLibrary } from '../services/modelRegistry';
import { FarInspector } from './far/FarInspector';
import { HfInspector } from './hf/HfInspector';
import { LocalInspector, type LocalInspectorProps } from './local/LocalInspector';
import { LogInspector } from './log/LogInspector';
import { MAX_WIDTH, MIN_WIDTH, useInspectorWidth } from './useInspectorWidth';

export type InspectorProps = Omit<LocalInspectorProps, 'model'> & {
  /** The paired machine's rows, for its model. */
  paired: PairedModelsState;
};

export const Inspector: FC<InspectorProps> = ({ paired, ...local }) => {
  const subject = inspectorSubject(useLocation());
  const { loading } = useModelLibrary();
  const model = useModel(subject?.kind === 'model' ? subject.id : null);
  const { width, edge } = useInspectorWidth(subject?.kind === 'model' && subject.log);
  if (subject === null) return null;

  let body;
  if (subject.kind === 'far') body = <FarInspector key={subject.id} id={subject.id} paired={paired} />;
  else if (subject.kind === 'hf') body = <HfInspector key={subject.repo} repo={subject.repo} />;
  else if (subject.log) body = <LogInspector key={subject.id} modelId={subject.id} modelName={model?.name ?? `Model #${subject.id}`} servers={local.servers} />;
  else if (model) body = <LocalInspector key={model.id} model={model} {...local} />;
  else body = !loading ? <p className="m-0 p-md text-xs text-text-secondary">This model is not in the library.</p> : null;

  return (
    <aside aria-label="Inspector" className="relative flex shrink-0 flex-col bg-surface" style={{ width }}>
      <div
        role="separator"
        aria-orientation="vertical"
        aria-label="Inspector width"
        aria-valuenow={width}
        aria-valuemin={MIN_WIDTH}
        aria-valuemax={MAX_WIDTH}
        tabIndex={0}
        className="absolute inset-y-0 left-0 z-20 w-1 cursor-col-resize touch-none hover:bg-border focus-visible:bg-border focus-visible:outline-none"
        {...edge}
      />
      {/* The edge stays put while the subject scrolls. */}
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto">{body}</div>
    </aside>
  );
};
